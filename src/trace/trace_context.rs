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

//! TracingContext is the context of the tracing process. Span should only be
//! created through context, and be archived into the context after the span
//! finished.

use crate::{
    common::{
        random_generator::RandomGenerator,
        system_time::{fetch_time, TimePeriod},
    },
    error::LOCK_MSG,
    proto::v3::{RefType, SegmentObject, SegmentReference, SpanLayer, SpanObject, SpanType},
    trace::{
        propagation::context::PropagationContext,
        span::Span,
        tracer::{Tracer, WeakTracer},
    },
};
use parking_lot::{
    MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::{fmt::Formatter, mem::take, sync::Arc};

pub(crate) struct ActiveSpan {
    span_id: i32,

    /// For [TracingContext::continued] used.
    r#ref: Option<SegmentReference>,
}

impl ActiveSpan {
    fn new(span_id: i32) -> Self {
        Self {
            span_id,
            r#ref: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct SpanStack {
    pub(crate) finalized: RwLock<Vec<SpanObject>>,
    pub(crate) active: RwLock<Vec<ActiveSpan>>,
}

impl SpanStack {
    #[allow(dead_code)]
    pub(crate) fn finalized(&self) -> RwLockReadGuard<'_, Vec<SpanObject>> {
        self.finalized.try_read().expect(LOCK_MSG)
    }

    pub(crate) fn finalized_mut(&self) -> RwLockWriteGuard<'_, Vec<SpanObject>> {
        self.finalized.try_write().expect(LOCK_MSG)
    }

    pub(crate) fn active(&self) -> RwLockReadGuard<'_, Vec<ActiveSpan>> {
        self.active.try_read().expect(LOCK_MSG)
    }

    pub(crate) fn active_mut(&self) -> RwLockWriteGuard<'_, Vec<ActiveSpan>> {
        self.active.try_write().expect(LOCK_MSG)
    }

    fn pop_active(&self, index: usize) -> Option<ActiveSpan> {
        let mut stack = self.active_mut();
        if stack.len() > index + 1 {
            None
        } else {
            stack.pop()
        }
    }

    /// Close span. We can't use closed span after finalize called.
    pub(crate) fn finalize_span(&self, index: usize, mut obj: SpanObject) {
        let span = self.pop_active(index);
        if let Some(span) = span {
            if span.span_id == obj.span_id {
                obj.end_time = fetch_time(TimePeriod::End);

                if let Some(r#ref) = span.r#ref {
                    obj.refs.push(r#ref);
                }

                self.finalized_mut().push(obj);
                return;
            }
        }
        panic!("Finalize span isn't the active span");
    }
}

/// TracingContext is the context of the tracing process. Span should only be
/// created through context, and be archived into the context after the span
/// finished.
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
                "finalized_spans",
                match self.span_stack.finalized.try_read() {
                    Some(spans) => {
                        span_objects = spans.clone();
                        &span_objects
                    }
                    None => &"<locked>",
                },
            )
            .finish()
    }
}

impl TracingContext {
    /// Generate a new trace context.
    pub(crate) fn new(
        service_name: impl Into<String>,
        instance_name: impl Into<String>,
        tracer: WeakTracer,
    ) -> Self {
        TracingContext {
            trace_id: RandomGenerator::generate(),
            trace_segment_id: RandomGenerator::generate(),
            service: service_name.into(),
            service_instance: instance_name.into(),
            next_span_id: Default::default(),
            span_stack: Default::default(),
            primary_endpoint_name: Default::default(),
            tracer,
        }
    }

    /// Get trace id.
    #[inline]
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    /// Get trace segment id.
    #[inline]
    pub fn trace_segment_id(&self) -> &str {
        &self.trace_segment_id
    }

    /// Get service name.
    #[inline]
    pub fn service(&self) -> &str {
        &self.service
    }

    /// Get service instance.
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

    /// Clone the last finalized span.
    #[doc(hidden)]
    pub fn last_span(&self) -> Option<SpanObject> {
        RwLockReadGuard::try_map(self.span_stack.finalized(), |spans| spans.last())
            .ok()
            .as_deref()
            .cloned()
    }

    fn spans_mut(&mut self) -> RwLockWriteGuard<'_, Vec<SpanObject>> {
        self.span_stack.finalized.try_write().expect(LOCK_MSG)
    }

    pub(crate) fn active_span_stack(&self) -> RwLockReadGuard<'_, Vec<ActiveSpan>> {
        self.span_stack.active()
    }

    pub(crate) fn active_span_stack_mut(&mut self) -> RwLockWriteGuard<'_, Vec<ActiveSpan>> {
        self.span_stack.active_mut()
    }

    pub(crate) fn active_span(&self) -> Option<MappedRwLockReadGuard<'_, ActiveSpan>> {
        RwLockReadGuard::try_map(self.active_span_stack(), |stack| stack.last()).ok()
    }

    pub(crate) fn active_span_mut(&mut self) -> Option<MappedRwLockWriteGuard<'_, ActiveSpan>> {
        RwLockWriteGuard::try_map(self.active_span_stack_mut(), |stack| stack.last_mut()).ok()
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

        let index = self.push_active_span(&span);
        Span::new(index, span, self.span_stack.clone())
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
        self.trace_id = propagation.parent_trace_id.clone();
        span.span_object_mut().refs.push(SegmentReference {
            ref_type: RefType::CrossProcess as i32,
            trace_id: self.trace_id().to_owned(),
            parent_trace_segment_id: propagation.parent_trace_segment_id.clone(),
            parent_span_id: propagation.parent_span_id,
            parent_service: propagation.parent_service.clone(),
            parent_service_instance: propagation.parent_service_instance.clone(),
            parent_endpoint: propagation.destination_endpoint.clone(),
            network_address_used_at_peer: propagation.destination_address.clone(),
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

        let index = self.push_active_span(&span);
        Span::new(index, span, self.span_stack.clone())
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

        let index = self.push_active_span(&span);
        Span::new(index, span, self.span_stack.clone())
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

            if let Some(mut span) = self.active_span_mut() {
                span.r#ref = Some(segment_ref);
            }
        }
    }

    /// It converts tracing context into segment object.
    /// This conversion should be done before sending segments into OAP.
    ///
    /// Notice: The spans will be taken, so this method shouldn't be called
    /// twice.
    pub(crate) fn convert_to_segment_object(&mut self) -> SegmentObject {
        let trace_id = self.trace_id().to_owned();
        let trace_segment_id = self.trace_segment_id().to_owned();
        let service = self.service().to_owned();
        let service_instance = self.service_instance().to_owned();
        let spans = take(&mut *self.spans_mut());

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
        self.active_span().map(|span| span.span_id)
    }

    fn push_active_span(&mut self, span: &SpanObject) -> usize {
        self.primary_endpoint_name = span.operation_name.clone();
        let mut stack = self.active_span_stack_mut();
        stack.push(ActiveSpan::new(span.span_id));
        stack.len() - 1
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

/// Cross threads context snapshot.
#[derive(Debug)]
pub struct ContextSnapshot {
    trace_id: String,
    trace_segment_id: String,
    span_id: i32,
    parent_endpoint: String,
}

impl ContextSnapshot {
    /// Check if the snapshot is created from current context.
    pub fn is_from_current(&self, context: &TracingContext) -> bool {
        !self.trace_segment_id.is_empty() && self.trace_segment_id == context.trace_segment_id()
    }

    /// Check if the snapshot is valid.
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
