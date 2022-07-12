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

use super::system_time::{fetch_time, TimePeriod};
use super::tracer::WeakTracer;
use crate::common::random_generator::RandomGenerator;
use crate::context::propagation::context::PropagationContext;
use crate::skywalking_proto::v3::{
    KeyStringValuePair, Log, RefType, SegmentObject, SegmentReference, SpanLayer, SpanObject,
    SpanType,
};
use std::borrow::Borrow;
use std::cell::{RefCell, Cell};
use std::collections::LinkedList;
use std::fmt::Formatter;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::sync::Arc;

/// Span is a concept that represents trace information for a single RPC.
/// The Rust SDK supports Entry Span to represent inbound to a service
/// and Exit Span to represent outbound from a service.
///
/// # Example
///
/// ```
/// use skywalking::context::trace_context::TracingContext;
///
/// async fn handle_request() {
///     let mut ctx = TracingContext::default("svc", "ins");
///     {
///         // Generate an Entry Span when a request
///         // is received. An Entry Span is generated only once per context.
///         let span = ctx.create_entry_span("operation1").unwrap();
///         
///         // Something...
///         
///         {
///             // Generates an Exit Span when executing an RPC.
///             let span2 = ctx.create_exit_span("operation2", "remote_peer").unwrap();
///             
///             // Something...
///
///             ctx.finalize_span(span2);
///         }
///
///         ctx.finalize_span(span);
///     }
/// }
/// ```
#[derive(Clone)]
pub struct Span {
    span_internal: Rc<RefCell<SpanObject>>,
}

impl std::fmt::Debug for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Span")
            .field("span_internal", &self.span_internal)
            .finish()
    }
}

const SKYWALKING_RUST_COMPONENT_ID: i32 = 11000;

impl Span {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        span_id: i32,
        parent_span_id: i32,
        operation_name: String,
        remote_peer: String,
        span_type: SpanType,
        span_layer: SpanLayer,
        skip_analysis: bool,
    ) -> Self {
        let span_internal = SpanObject {
            span_id,
            parent_span_id,
            start_time: fetch_time(TimePeriod::Start),
            end_time: 0, // not set
            refs: Vec::<SegmentReference>::new(),
            operation_name,
            peer: remote_peer,
            span_type: span_type as i32,
            span_layer: span_layer as i32,
            component_id: SKYWALKING_RUST_COMPONENT_ID,
            is_error: false,
            tags: Vec::<KeyStringValuePair>::new(),
            logs: Vec::<Log>::new(),
            skip_analysis,
        };

        Span {
            span_internal: Rc::new(RefCell::new(span_internal)),
        }
    }

    /// Close span. It only registers end time to the span.
    pub fn close(&mut self) {
        self.with_span_object_mut(|span| span.end_time = fetch_time(TimePeriod::End));
    }

    pub fn with_span_object<T>(&self, f: impl FnOnce(&SpanObject) -> T) -> T {
        f(&self.span_internal.deref().borrow())
    }

    pub fn with_span_object_mut<T>(&mut self, f: impl FnOnce(&mut SpanObject) -> T) -> T {
        f(&mut self.span_internal.deref().borrow_mut())
    }

    pub fn span_id(&self) -> i32 {
        self.with_span_object(|span| span.span_id)
    }

    /// Add logs to the span.
    pub fn add_log<K, V, I>(&mut self, message: I)
    where
        K: ToString,
        V: ToString,
        I: IntoIterator<Item = (K, V)>,
    {
        let log = Log {
            time: fetch_time(TimePeriod::Log),
            data: message
                .into_iter()
                .map(|v| {
                    let (key, value) = v;
                    KeyStringValuePair {
                        key: key.to_string(),
                        value: value.to_string(),
                    }
                })
                .collect(),
        };
        self.with_span_object_mut(|span| span.logs.push(log));
    }

    /// Add tag to the span.
    pub fn add_tag(&mut self, tag: (&str, &str)) {
        let (key, value) = tag;
        self.with_span_object_mut(|span| {
            span.tags.push(KeyStringValuePair {
                key: key.to_string(),
                value: value.to_string(),
            })
        })
    }

    fn add_segment_reference(&mut self, segment_reference: SegmentReference) {
        self.with_span_object_mut(|span| span.refs.push(segment_reference));
    }

    fn unwrap_inner(self) -> SpanObject {
        Rc::try_unwrap(self.span_internal).unwrap().into_inner()
    }

    // fn downgrade(&self) -> WeakSpan {
    //     WeakSpan {
    //         span_internal: Rc::downgrade(&self.span_internal),
    //     }
    // }
}

// pub(crate) struct WeakSpan {
//     span_internal: Weak<RefCell<SpanObject>>,
// }

// impl WeakSpan {
//     pub fn span_id(&self) -> Option<i32> {
//         self.upgrade().map(|span| span.span_id())
//     }

//     fn upgrade(&self) -> Option<Span> {
//         self.span_internal
//             .upgrade()
//             .map(|span_internal| Span { span_internal })
//     }
// }

struct Inner {
    trace_id: String,
    trace_segment_id: String,
    service: String,
    service_instance: String,
    next_span_id: Cell<i32>,
    spans: RefCell<Vec<Span>>,
    segment_link: Option<PropagationContext>,
    active_span_stack: RefCell<LinkedList<Span>>,
    primary_endpoint_name: RefCell<String>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TracingContext")
            .field("trace_id", &self.trace_id)
            .field("trace_segment_id", &self.trace_segment_id)
            .field("service", &self.service)
            .field("service_instance", &self.service_instance)
            .field("next_span_id", &self.next_span_id)
            .field("spans", &self.spans)
            .finish()
    }
}

#[derive(Clone)]
pub struct TracingContext {
    inner: Rc<Inner>,
    tracer: WeakTracer,
}

impl std::fmt::Debug for TracingContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
std::fmt::Debug::fmt(&self.inner,f)
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
            inner: Rc::new(Inner {
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
            inner: Rc::new(Inner {
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

    #[inline]
    fn next_span_id(&self) -> i32 {
        self.inner.next_span_id.get()
    } 

    fn inc_next_span_id(&self) {
        self.inner.next_span_id.set(self.next_span_id() + 1)
    } 

    fn with_spans<T>(&self, f: impl FnOnce(&Vec<Span>) -> T) -> T {
        f(&self.inner.spans.borrow())
    }

    fn with_spans_mut<T>(&mut self, f: impl FnOnce(&mut Vec<Span>) -> T) -> T {
        f(&mut self.inner.spans.borrow_mut())
    }

    fn with_active_span_stack<T>(&self, f: impl FnOnce(&LinkedList<Span>) -> T) -> T {
        f(&self.inner.active_span_stack.borrow())
    }

    fn with_active_span_stack_mut<T>(&mut self, f: impl FnOnce(&mut LinkedList<Span>) -> T) -> T {
        f(&mut self.inner.active_span_stack.borrow_mut())
    }

    fn with_primary_endpoint_name<T>(&self, f: impl FnOnce(&String) -> T) -> T {
        f(&self.inner.primary_endpoint_name.borrow())
    }

    fn with_primary_endpoint_name_mut<T>(&mut self, f: impl FnOnce(&mut String) -> T) -> T {
        f(&mut self.inner.primary_endpoint_name.borrow_mut())
    }

    /// A wrapper of create entry span, which close generated span automatically.
    /// Note that, we may use async operation in closure. But that is not unstable feature in 2021/12.
    /// <https://github.com/rust-lang/rust/issues/62290>
    /// So we should create and close spans manually in general.
    pub fn entry<F: FnMut(&Span)>(
        &mut self,
        operation_name: &str,
        mut process_fn: F,
    ) -> crate::Result<()> {
        match self.create_entry_span(operation_name) {
            Ok(mut span) => {
                process_fn(&span);
                span.close();
                Ok(())
            }
            Err(message) => Err(message),
        }
    }

    /// Create a new entry span, which is an initiator of collection of spans.
    /// This should be called by invocation of the function which is triggered by
    /// external service.
    pub fn create_entry_span(&mut self, operation_name: &str) -> crate::Result<Span> {
        if self.next_span_id() >= 1 {
            return Err(crate::Error::CreateSpan("entry span have already exist."));
        }

        let parent_span_id = self.peek_active_span_id().unwrap_or(-1);

        let mut span = Span::new(
            self.inner.next_span_id.get(),
            parent_span_id,
            operation_name.to_string(),
            String::default(),
            SpanType::Entry,
            SpanLayer::Http,
            false,
        );

        if let Some(segment_link) = &self.inner.segment_link {
            span.add_segment_reference(SegmentReference {
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
        self.inc_next_span_id();
        self.push_active_span(&span);
        Ok(span)
    }

    /// A wrapper of create exit span, which close generated span automatically.
    /// Note that, we may use async operation in closure. But that is not unstable feature in 2021/12.
    /// <https://github.com/rust-lang/rust/issues/62290>
    /// So we should create and close spans manually in general.
    pub fn exit<F: FnMut(&Span)>(
        &mut self,
        operation_name: &str,
        remote_peer: &str,
        mut process_fn: F,
    ) -> crate::Result<()> {
        match self.create_exit_span(operation_name, remote_peer) {
            Ok(mut span) => {
                process_fn(&span);
                span.close();
                Ok(())
            }
            Err(message) => Err(message),
        }
    }

    /// Create a new exit span, which will be created when tracing context will generate
    /// new span for function invocation.
    /// Currently, this SDK supports RPC call. So we must set `remote_peer`.
    pub fn create_exit_span(
        &mut self,
        operation_name: &str,
        remote_peer: &str,
    ) -> crate::Result<Span> {
        if self.next_span_id() == 0 {
            return Err(crate::Error::CreateSpan("entry span must be existed."));
        }

        let parent_span_id = self.peek_active_span_id().unwrap_or(-1);

        let span = Span::new(
            self.inner.next_span_id.get(),
            parent_span_id,
            operation_name.to_string(),
            remote_peer.to_string(),
            SpanType::Exit,
            SpanLayer::Http,
            false,
        );
        self.inc_next_span_id();
        self.push_active_span(&span);
        Ok(span)
    }

    // Create a new local span.
    pub fn create_local_span(&mut self, operation_name: &str) -> crate::Result<Span> {
        if self.next_span_id() == 0 {
            return Err(crate::Error::CreateSpan("entry span must be existed."));
        }

        let parent_span_id = self.peek_active_span_id().unwrap_or(-1);

        let span = Span::new(
            self.inner.next_span_id.get(),
            parent_span_id,
            operation_name.to_string(),
            Default::default(),
            SpanType::Local,
            SpanLayer::Unknown,
            false,
        );
        self.inc_next_span_id();
        self.push_active_span(&span);
        Ok(span)
    }

    /// Close span. We can't use closed span after finalize called.
    pub fn finalize_span(&mut self, mut span: Span) {
        span.close();
        self.with_spans_mut(|spans| spans.push(span));
        self.pop_active_span();
    }

    /// It converts tracing context into segment object.
    /// This conversion should be done before sending segments into OAP.
    pub fn convert_segment_object(self) -> SegmentObject {
        let mut objects = Vec::with_capacity(self.with_spans(|spans| spans.len()));

        let trace_id = self.trace_id().to_owned();
        let trace_segment_id = self.trace_segment_id().to_owned();
        let service = self.service().to_owned();
        let service_instance = self.service_instance().to_owned();
        let inner = self.unwrap_inner();

        for span in inner.spans.into_inner() {
            objects.push(span.unwrap_inner());
        }

        SegmentObject {
            trace_id,
            trace_segment_id,
            spans: objects,
            service,
            service_instance,
            is_size_limited: false,
        }
    }

    pub fn capture(&self) -> ContextSnapshot {
        ContextSnapshot {
            trace_id: self.inner.trace_id.clone(),
            trace_segment_id: self.inner.trace_segment_id.clone(),
            span_id: self.peek_active_span_id().unwrap_or(-1),
            parent_endpoint: self.with_primary_endpoint_name(|endpoint| endpoint.clone()),
        }
    }

    pub fn continued(&mut self, snapshot: ContextSnapshot) {
        if snapshot.is_valid() {
            let tracer = self.tracer.upgrade().unwrap();
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
            let mut span = self.peek_active_span().unwrap();
            span.add_segment_reference(segment_ref);
            // TraceSegmentRef segmentRef = new TraceSegmentRef(snapshot);
            // this.segment.ref(segmentRef);
            // this.activeSpan().ref(segmentRef);
            // this.segment.relatedGlobalTrace(snapshot.getTraceId());
            // this.correlationContext.continued(snapshot);
            // this.extensionContext.continued(snapshot);
            // this.extensionContext.handle(this.activeSpan());
        }
    }

    pub(crate) fn peek_active_span_id(&self) -> Option<i32> {
        self.peek_active_span().map(|span| span.span_id())
    }

    pub(crate) fn peek_active_span(&self) -> Option<Span> {
        self.with_active_span_stack(|stack| stack.back().cloned())
    }

    fn push_active_span(&mut self, span: &Span) {
        self.with_primary_endpoint_name_mut(|endpoint| *endpoint = span.with_span_object(|span| span.operation_name.clone()));
        self.with_active_span_stack_mut(|stack| stack.push_back(span.clone()));
    }

    fn pop_active_span(&mut self) {
        self.with_active_span_stack_mut(|stack| stack.pop_back());
    }

    fn unwrap_inner(self) -> Inner {
        Rc::try_unwrap(self.inner).unwrap()
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
