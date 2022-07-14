// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements.  See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License.  You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

use super::span::Span;
use super::system_time::{fetch_time, TimePeriod};
use super::tracer::{Tracer, WeakTracer};
use crate::common::random_generator::RandomGenerator;
use crate::context::propagation::context::PropagationContext;
use crate::error::LOCK_MSG;
use crate::skywalking_proto::v3::{
    KeyStringValuePair, Log, RefType, SegmentObject, SegmentReference, SpanLayer, SpanObject,
    SpanType,
};
use std::borrow::Borrow;
use std::cell::{Cell, RefCell};
use std::collections::LinkedList;
use std::fmt::Formatter;
use std::mem::take;
use std::ops::Deref;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, Weak};

struct Inner {
    trace_id: String,
    trace_segment_id: String,
    service: String,
    service_instance: String,
    next_span_id: AtomicI32,
    spans: Mutex<Vec<SpanObject>>,
    active_span_stack: Mutex<Vec<SpanObject>>,
    segment_link: Option<PropagationContext>,
    primary_endpoint_name: Mutex<String>,
}

#[derive(Clone)]
pub struct TracingContext {
    inner: Arc<Inner>,
    tracer: WeakTracer,
}

impl std::fmt::Debug for TracingContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TracingContext")
            .field("trace_id", &self.inner.trace_id)
            .field("trace_segment_id", &self.inner.trace_segment_id)
            .field("service", &self.inner.service)
            .field("service_instance", &self.inner.service_instance)
            .field("next_span_id", &self.inner.next_span_id)
            .field("spans", &self.inner.spans)
            .finish()
    }
}

impl TracingContext {
    /// Generate a new trace context. Typically called when no context has
    /// been propagated and a new trace is to be started.
    pub(crate) fn new(
        service_name: impl ToString,
        instance_name: impl ToString,
        tracer: WeakTracer,
    ) -> Self {
        TracingContext {
            inner: Arc::new(Inner {
                trace_id: RandomGenerator::generate(),
                trace_segment_id: RandomGenerator::generate(),
                service: service_name.to_string(),
                service_instance: instance_name.to_string(),
                next_span_id: Default::default(),
                spans: Default::default(),
                segment_link: None,
                active_span_stack: Default::default(),
                primary_endpoint_name: Default::default(),
            }),
            tracer,
        }
    }

    /// Generate a new trace context using the propagated context.
    /// They should be propagated on `sw8` header in HTTP request with encoded form.
    /// You can retrieve decoded context with `skywalking::context::propagation::encoder::encode_propagation`
    pub(crate) fn from_propagation_context(
        service_name: impl ToString,
        instance_name: impl ToString,
        context: PropagationContext,
        tracer: WeakTracer,
    ) -> Self {
        TracingContext {
            inner: Arc::new(Inner {
                trace_id: context.parent_trace_id.clone(),
                trace_segment_id: RandomGenerator::generate(),
                service: service_name.to_string(),
                service_instance: instance_name.to_string(),
                next_span_id: Default::default(),
                spans: Default::default(),
                segment_link: Some(context),
                active_span_stack: Default::default(),
                primary_endpoint_name: Default::default(),
            }),
            tracer,
        }
    }

    #[inline]
    pub fn trace_id(&self) -> &str {
        &self.inner.trace_id
    }

    #[inline]
    pub fn trace_segment_id(&self) -> &str {
        &self.inner.trace_segment_id
    }

    #[inline]
    pub fn service(&self) -> &str {
        &self.inner.service
    }

    #[inline]
    pub fn service_instance(&self) -> &str {
        &self.inner.service_instance
    }

    fn next_span_id(&self) -> i32 {
        self.inner.next_span_id.load(Ordering::Relaxed)
    }

    #[inline]
    fn inc_next_span_id(&self) -> i32 {
        self.inner.next_span_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn with_spans<T>(&self, f: impl FnOnce(&Vec<SpanObject>) -> T) -> T {
        f(&*self.inner.spans.try_lock().expect(LOCK_MSG))
    }

    fn with_spans_mut<T>(&mut self, f: impl FnOnce(&mut Vec<SpanObject>) -> T) -> T {
        f(&mut *self.inner.spans.try_lock().expect(LOCK_MSG))
    }

    pub(crate) fn with_active_span_stack<T>(&self, f: impl FnOnce(&Vec<SpanObject>) -> T) -> T {
        f(&*self.inner.active_span_stack.try_lock().expect(LOCK_MSG))
    }

    pub(crate) fn with_active_span_stack_mut<T>(
        &mut self,
        f: impl FnOnce(&mut Vec<SpanObject>) -> T,
    ) -> T {
        f(&mut *self.inner.active_span_stack.try_lock().expect(LOCK_MSG))
    }

    pub(crate) fn try_with_active_span_stack<T>(
        &self,
        f: impl FnOnce(&Vec<SpanObject>) -> T,
    ) -> Option<T> {
        self.inner
            .active_span_stack
            .try_lock()
            .ok()
            .map(|stack| f(&*stack))
    }

    pub(crate) fn with_active_span<T>(&self, f: impl FnOnce(&SpanObject) -> T) -> Option<T> {
        self.with_active_span_stack(|stack| stack.last().map(|span| f(span)))
    }

    fn with_primary_endpoint_name<T>(&self, f: impl FnOnce(&String) -> T) -> T {
        f(&*self.inner.primary_endpoint_name.try_lock().expect(LOCK_MSG))
    }

    fn with_primary_endpoint_name_mut<T>(&mut self, f: impl FnOnce(&mut String) -> T) -> T {
        f(&mut *self.inner.primary_endpoint_name.try_lock().expect(LOCK_MSG))
    }

    /// Create a new entry span, which is an initiator of collection of spans.
    /// This should be called by invocation of the function which is triggered by
    /// external service.
    pub fn create_entry_span(&mut self, operation_name: &str) -> Span {
        if self.next_span_id() >= 1 {
            panic!("entry span have already exist.");
        }

        let mut span = Span::new_obj(
            self.inc_next_span_id(),
            self.peek_active_span_id().unwrap_or(-1),
            operation_name.to_string(),
            String::default(),
            SpanType::Entry,
            SpanLayer::Http,
            false,
        );

        if let Some(segment_link) = &self.inner.segment_link {
            span.refs.push(SegmentReference {
                ref_type: RefType::CrossProcess as i32,
                trace_id: self.inner.trace_id.clone(),
                parent_trace_segment_id: segment_link.parent_trace_segment_id.clone(),
                parent_span_id: segment_link.parent_span_id,
                parent_service: segment_link.parent_service.clone(),
                parent_service_instance: segment_link.parent_service_instance.clone(),
                parent_endpoint: segment_link.destination_endpoint.clone(),
                network_address_used_at_peer: segment_link.destination_address.clone(),
            });
        }

        let index = self.push_active_span(span);
        Span::new(index, self.downgrade())
    }

    /// Create a new exit span, which will be created when tracing context will generate
    /// new span for function invocation.
    /// Currently, this SDK supports RPC call. So we must set `remote_peer`.
    pub fn create_exit_span(&mut self, operation_name: &str, remote_peer: &str) -> Span {
        if self.next_span_id() == 0 {
            panic!("entry span must be existed.");
        }

        let span = Span::new_obj(
            self.inc_next_span_id(),
            self.peek_active_span_id().unwrap_or(-1),
            operation_name.to_string(),
            remote_peer.to_string(),
            SpanType::Exit,
            SpanLayer::Http,
            false,
        );

        let index = self.push_active_span(span);
        Span::new(index, self.downgrade())
    }

    // Create a new local span.
    pub fn create_local_span(&mut self, operation_name: &str) -> Span {
        if self.next_span_id() == 0 {
            panic!("entry span must be existed.");
        }

        let span = Span::new_obj(
            self.inc_next_span_id(),
            self.peek_active_span_id().unwrap_or(-1),
            operation_name.to_string(),
            Default::default(),
            SpanType::Local,
            SpanLayer::Unknown,
            false,
        );

        let index = self.push_active_span(span);
        Span::new(index, self.downgrade())
    }

    /// Close span. We can't use closed span after finalize called.
    pub(crate) fn finalize_span(&mut self, index: usize) -> Result<(), ()> {
        let span = self.pop_active_span(index);
        if let Some(mut span) = span {
            span.end_time = fetch_time(TimePeriod::End);
            self.with_spans_mut(|spans| spans.push(span));
            Ok(())
        } else {
            Err(())
        }
    }

    /// It converts tracing context into segment object.
    /// This conversion should be done before sending segments into OAP.
    ///
    /// Notice: The spans will taked, so this method shouldn't be called twice.
    pub(crate) fn convert_segment_object(&mut self) -> SegmentObject {
        let trace_id = self.trace_id().to_owned();
        let trace_segment_id = self.trace_segment_id().to_owned();
        let service = self.service().to_owned();
        let service_instance = self.service_instance().to_owned();
        let spans = self.with_spans_mut(|spans| take(spans));

        SegmentObject {
            trace_id,
            trace_segment_id,
            spans,
            service,
            service_instance,
            is_size_limited: false,
        }
    }

    // pub fn capture(&self) -> ContextSnapshot {
    //     ContextSnapshot {
    //         trace_id: self.inner.trace_id.clone(),
    //         trace_segment_id: self.inner.trace_segment_id.clone(),
    //         span_id: self.peek_active_span_id().unwrap_or(-1),
    //         parent_endpoint: self.with_primary_endpoint_name(|endpoint| endpoint.clone()),
    //     }
    // }

    // pub fn continued(&mut self, snapshot: ContextSnapshot) {
    //     if snapshot.is_valid() {
    //         let tracer = self.tracer.upgrade().unwrap();
    //         let segment_ref = SegmentReference {
    //             ref_type: RefType::CrossThread as i32,
    //             trace_id: snapshot.trace_id,
    //             parent_trace_segment_id: snapshot.trace_segment_id,
    //             parent_span_id: snapshot.span_id,
    //             parent_service: tracer.service_name().to_owned(),
    //             parent_service_instance: tracer.instance_name().to_owned(),
    //             parent_endpoint: snapshot.parent_endpoint,
    //             network_address_used_at_peer: Default::default(),
    //         };
    //         let mut span = self.peek_active_span().unwrap();
    //         span.add_segment_reference(segment_ref);
    //         // TraceSegmentRef segmentRef = new TraceSegmentRef(snapshot);
    //         // this.segment.ref(segmentRef);
    //         // this.activeSpan().ref(segmentRef);
    //         // this.segment.relatedGlobalTrace(snapshot.getTraceId());
    //         // this.correlationContext.continued(snapshot);
    //         // this.extensionContext.continued(snapshot);
    //         // this.extensionContext.handle(this.activeSpan());
    //     }
    // }

    pub(crate) fn peek_active_span_id(&self) -> Option<i32> {
        self.with_active_span(|span| span.span_id)
    }

    fn push_active_span(&mut self, span: SpanObject) -> usize {
        self.with_primary_endpoint_name_mut(|endpoint| *endpoint = span.operation_name.clone());
        self.with_active_span_stack_mut(|stack| {
            stack.push(span);
            stack.len() - 1
        })
    }

    fn pop_active_span(&mut self, index: usize) -> Option<SpanObject> {
        self.with_active_span_stack_mut(|stack| {
            if stack.len() > index + 1 {
                None
            } else {
                stack.pop()
            }
        })
    }

    fn downgrade(&self) -> WeakTracingContext {
        WeakTracingContext {
            inner: Arc::downgrade(&self.inner),
            tracer: self.tracer.clone(),
        }
    }

    // fn unwrap_inner(self) -> Inner {
    //     Arc::try_unwrap(self.inner).ok().expect(MORE_RC_MSG)
    // }

    fn upgrade_tracer(&self) -> Tracer {
        self.tracer.upgrade().expect("Tracer has dropped")
    }
}

impl Drop for TracingContext {
    fn drop(&mut self) {
        self.upgrade_tracer().finalize_context(self)
    }
}

#[derive(Clone)]
pub(crate) struct WeakTracingContext {
    inner: Weak<Inner>,
    tracer: WeakTracer,
}

impl WeakTracingContext {
    pub(crate) fn upgrade(&self) -> Option<TracingContext> {
        self.inner.upgrade().map(|inner| TracingContext {
            inner,
            tracer: self.tracer.clone(),
        })
    }
}

pub struct ContextSnapshot {
    trace_id: String,
    trace_segment_id: String,
    span_id: i32,
    parent_endpoint: String,
}

impl ContextSnapshot {
    pub fn is_from_current(&self, context: &TracingContext) -> bool {
        !self.trace_segment_id.is_empty() && self.trace_segment_id == context.trace_segment_id()
    }

    pub fn is_valid(&self) -> bool {
        !self.trace_segment_id.is_empty() && self.span_id > -1 && !self.trace_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait AssertSend: Send {}

    impl AssertSend for TracingContext {}
}
