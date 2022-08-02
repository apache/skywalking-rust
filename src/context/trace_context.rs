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

use super::{
    span::Span,
    system_time::{fetch_time, TimePeriod},
    tracer::{Tracer, WeakTracer},
};
use crate::{
    common::random_generator::RandomGenerator,
    context::propagation::context::PropagationContext,
    error::LOCK_MSG,
    skywalking_proto::v3::{
        RefType, SegmentObject, SegmentReference, SpanLayer, SpanObject, SpanType,
    },
};
use std::{
    fmt::Formatter,
    mem::take,
    sync::{Arc, RwLock},
};

#[derive(Default)]
pub(crate) struct SpanStack {
    pub(crate) finialized: RwLock<Vec<SpanObject>>,
    pub(crate) active: RwLock<Vec<SpanObject>>,
}

impl SpanStack {
    pub(crate) fn with_finialized<T>(&self, f: impl FnOnce(&Vec<SpanObject>) -> T) -> T {
        f(&self.finialized.try_read().expect(LOCK_MSG))
    }

    pub(crate) fn with_finialized_mut<T>(&self, f: impl FnOnce(&mut Vec<SpanObject>) -> T) -> T {
        f(&mut *self.finialized.try_write().expect(LOCK_MSG))
    }

    pub(crate) fn with_active<T>(&self, f: impl FnOnce(&Vec<SpanObject>) -> T) -> T {
        f(&*self.active.try_read().expect(LOCK_MSG))
    }

    pub(crate) fn with_active_mut<T>(&self, f: impl FnOnce(&mut Vec<SpanObject>) -> T) -> T {
        f(&mut *self.active.try_write().expect(LOCK_MSG))
    }

    fn pop_active(&self, index: usize) -> Option<SpanObject> {
        self.with_active_mut(|stack| {
            if stack.len() > index + 1 {
                None
            } else {
                stack.pop()
            }
        })
    }

    /// Close span. We can't use closed span after finalize called.
    pub(crate) fn finalize_span(&self, index: usize) {
        let span = self.pop_active(index);
        if let Some(mut span) = span {
            span.end_time = fetch_time(TimePeriod::End);
            self.with_finialized_mut(|spans| spans.push(span));
        } else {
            panic!("Finalize span isn't the active span");
        }
    }
}

#[must_use = "call `create_entry_span` after `TracingContext` created."]
pub struct TracingContext {
    trace_id: String,
    trace_segment_id: String,
    service: String,
    service_instance: String,
    next_span_id: i32,
    span_stack: Arc<SpanStack>,
    primary_endpoint_name: String,
    tracer: WeakTracer,
}

impl std::fmt::Debug for TracingContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let span_objects: Vec<SpanObject>;
        f.debug_struct("TracingContext")
            .field("trace_id", &self.trace_id)
            .field("trace_segment_id", &self.trace_segment_id)
            .field("service", &self.service)
            .field("service_instance", &self.service_instance)
            .field("next_span_id", &self.next_span_id)
            .field(
                "finialized_spans",
                match self.span_stack.finialized.try_read() {
                    Ok(spans) => {
                        span_objects = spans.clone();
                        &span_objects
                    }
                    Err(_) => &"<locked>",
                },
            )
            .finish()
    }
}

impl TracingContext {
    /// Generate a new trace context.
    pub(crate) fn new(
        service_name: impl ToString,
        instance_name: impl ToString,
        tracer: WeakTracer,
    ) -> Self {
        TracingContext {
            trace_id: RandomGenerator::generate(),
            trace_segment_id: RandomGenerator::generate(),
            service: service_name.to_string(),
            service_instance: instance_name.to_string(),
            next_span_id: Default::default(),
            span_stack: Default::default(),
            primary_endpoint_name: Default::default(),
            tracer,
        }
    }

    #[inline]
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    #[inline]
    pub fn trace_segment_id(&self) -> &str {
        &self.trace_segment_id
    }

    #[inline]
    pub fn service(&self) -> &str {
        &self.service
    }

    #[inline]
    pub fn service_instance(&self) -> &str {
        &self.service_instance
    }

    fn next_span_id(&self) -> i32 {
        self.next_span_id
    }

    #[inline]
    fn inc_next_span_id(&mut self) -> i32 {
        let span_id = self.next_span_id;
        self.next_span_id += 1;
        span_id
    }

    pub fn last_span(&self) -> Option<SpanObject> {
        self.span_stack
            .with_finialized(|spans| spans.last().cloned())
    }

    fn with_spans_mut<T>(&mut self, f: impl FnOnce(&mut Vec<SpanObject>) -> T) -> T {
        f(&mut *self.span_stack.finialized.try_write().expect(LOCK_MSG))
    }

    pub(crate) fn with_active_span_stack<T>(&self, f: impl FnOnce(&Vec<SpanObject>) -> T) -> T {
        self.span_stack.with_active(f)
    }

    pub(crate) fn with_active_span_stack_mut<T>(
        &mut self,
        f: impl FnOnce(&mut Vec<SpanObject>) -> T,
    ) -> T {
        self.span_stack.with_active_mut(f)
    }

    pub(crate) fn with_active_span<T>(&self, f: impl FnOnce(&SpanObject) -> T) -> Option<T> {
        self.with_active_span_stack(|stack| stack.last().map(f))
    }

    pub(crate) fn with_active_span_mut<T>(
        &mut self,
        f: impl FnOnce(&mut SpanObject) -> T,
    ) -> Option<T> {
        self.with_active_span_stack_mut(|stack| stack.last_mut().map(f))
    }

    /// Create a new entry span, which is an initiator of collection of spans.
    /// This should be called by invocation of the function which is triggered
    /// by external service.
    ///
    /// Typically called when no context has
    /// been propagated and a new trace is to be started.
    pub fn create_entry_span(&mut self, operation_name: &str) -> Span {
        let span = Span::new_obj(
            self.inc_next_span_id(),
            self.peek_active_span_id().unwrap_or(-1),
            operation_name.to_string(),
            String::default(),
            SpanType::Entry,
            SpanLayer::Http,
            false,
        );

        let index = self.push_active_span(span);
        Span::new(index, Arc::downgrade(&self.span_stack))
    }

    /// Create a new entry span, which is an initiator of collection of spans.
    /// This should be called by invocation of the function which is triggered
    /// by external service.
    ///
    /// They should be propagated on `sw8` header in HTTP request with encoded
    /// form. You can retrieve decoded context with
    /// `skywalking::context::propagation::encoder::encode_propagation`
    pub fn create_entry_span_with_propagation(
        &mut self,
        operation_name: &str,
        propagation: &PropagationContext,
    ) -> Span {
        let mut span = self.create_entry_span(operation_name);
        span.with_span_object_mut(|span| {
            span.refs.push(SegmentReference {
                ref_type: RefType::CrossProcess as i32,
                trace_id: self.trace_id().to_owned(),
                parent_trace_segment_id: propagation.parent_trace_segment_id.clone(),
                parent_span_id: propagation.parent_span_id,
                parent_service: propagation.parent_service.clone(),
                parent_service_instance: propagation.parent_service_instance.clone(),
                parent_endpoint: propagation.destination_endpoint.clone(),
                network_address_used_at_peer: propagation.destination_address.clone(),
            });
        });
        span
    }

    /// Create a new exit span, which will be created when tracing context will
    /// generate new span for function invocation.
    /// Currently, this SDK supports RPC call. So we must set `remote_peer`.
    ///
    /// # Panics
    ///
    /// Panic if entry span not existed.
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
        Span::new(index, Arc::downgrade(&self.span_stack))
    }

    /// Create a new local span.
    ///
    /// # Panics
    ///
    /// Panic if entry span not existed.
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
        Span::new(index, Arc::downgrade(&self.span_stack))
    }

    /// Capture a snapshot for cross-thread propagation.
    pub fn capture(&self) -> ContextSnapshot {
        ContextSnapshot {
            trace_id: self.trace_id().to_owned(),
            trace_segment_id: self.trace_segment_id().to_owned(),
            span_id: self.peek_active_span_id().unwrap_or(-1),
            parent_endpoint: self.primary_endpoint_name.clone(),
        }
    }

    /// Build the reference between this segment and a cross-thread segment.
    pub fn continued(&mut self, snapshot: ContextSnapshot) {
        if snapshot.is_valid() {
            self.trace_id = snapshot.trace_id.clone();

            let tracer = self.upgrade_tracer();

            let segment_ref = SegmentReference {
                ref_type: RefType::CrossThread as i32,
                trace_id: snapshot.trace_id,
                parent_trace_segment_id: snapshot.trace_segment_id,
                parent_span_id: snapshot.span_id,
                parent_service: tracer.service_name().to_owned(),
                parent_service_instance: tracer.instance_name().to_owned(),
                parent_endpoint: snapshot.parent_endpoint,
                network_address_used_at_peer: Default::default(),
            };

            self.with_active_span_mut(|span| {
                span.refs.push(segment_ref);
            });
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

    pub(crate) fn peek_active_span_id(&self) -> Option<i32> {
        self.with_active_span(|span| span.span_id)
    }

    fn push_active_span(&mut self, span: SpanObject) -> usize {
        self.primary_endpoint_name = span.operation_name.clone();
        self.with_active_span_stack_mut(|stack| {
            stack.push(span);
            stack.len() - 1
        })
    }

    fn upgrade_tracer(&self) -> Tracer {
        self.tracer.upgrade().expect("Tracer has dropped")
    }
}

impl Drop for TracingContext {
    /// Convert to segment object, and send to tracer for reporting.
    ///
    /// # Panics
    ///
    /// Panic if tracer is dropped.
    fn drop(&mut self) {
        self.upgrade_tracer().finalize_context(self)
    }
}

#[derive(Debug)]
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
