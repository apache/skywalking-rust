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

use crate::common::random_generator::RandomGenerator;
use crate::common::time::TimeFetcher;
use crate::context::propagation::context::PropagationContext;
use crate::skywalking_proto::v3::{
    KeyStringValuePair, Log, RefType, SegmentObject, SegmentReference, SpanLayer, SpanObject,
    SpanType,
};
use std::collections::LinkedList;
use std::fmt::Formatter;
use std::sync::Arc;

use super::system_time::UnixTimeStampFetcher;

/// Span is a concept that represents trace information for a single RPC.
/// The Rust SDK supports Entry Span to represent inbound to a service
/// and Exit Span to represent outbound from a service.
///
/// # Example
///
/// ```
/// use skywalking_rust::context::trace_context::TracingContext;
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
pub struct Span {
    span_internal: SpanObject,
    time_fetcher: Arc<dyn TimeFetcher + Sync + Send>,
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
        time_fetcher: Arc<dyn TimeFetcher + Sync + Send>,
    ) -> Self {
        let span_internal = SpanObject {
            span_id,
            parent_span_id,
            start_time: time_fetcher.get(),
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
            span_internal,
            time_fetcher,
        }
    }

    /// Close span. It only registers end time to the span.
    pub fn close(&mut self) {
        self.span_internal.end_time = self.time_fetcher.get();
    }

    pub fn span_object(&self) -> &SpanObject {
        &self.span_internal
    }

    pub fn span_object_mut(&mut self) -> &mut SpanObject {
        &mut self.span_internal
    }

    /// Add logs to the span.
    pub fn add_log(&mut self, message: Vec<(&str, &str)>) {
        let log = Log {
            time: self.time_fetcher.get(),
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
        self.span_internal.logs.push(log);
    }

    /// Add tag to the span.
    pub fn add_tag(&mut self, tag: (&str, &str)) {
        let (key, value) = tag;
        self.span_internal.tags.push(KeyStringValuePair {
            key: key.to_string(),
            value: value.to_string(),
        });
    }

    fn add_segment_reference(&mut self, segment_reference: SegmentReference) {
        self.span_internal.refs.push(segment_reference);
    }
}

pub struct TracingContext {
    pub trace_id: String,
    pub trace_segment_id: String,
    pub service: String,
    pub service_instance: String,
    pub next_span_id: i32,
    pub spans: Vec<Box<Span>>,
    time_fetcher: Arc<dyn TimeFetcher + Sync + Send>,
    segment_link: Option<PropagationContext>,
    active_span_id_stack: LinkedList<i32>,
}

impl std::fmt::Debug for TracingContext {
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

impl TracingContext {
    /// Generate a new trace context. Typically called when no context has
    /// been propagated and a new trace is to be started.
    pub fn default(service_name: &str, instance_name: &str) -> Self {
        let unix_time_fetcher = UnixTimeStampFetcher {};
        TracingContext::default_internal(Arc::new(unix_time_fetcher), service_name, instance_name)
    }

    pub fn default_internal(
        time_fetcher: Arc<dyn TimeFetcher + Sync + Send>,
        service_name: &str,
        instance_name: &str,
    ) -> Self {
        TracingContext {
            trace_id: RandomGenerator::generate(),
            trace_segment_id: RandomGenerator::generate(),
            service: String::from(service_name),
            service_instance: String::from(instance_name),
            next_span_id: 0,
            time_fetcher,
            spans: Vec::new(),
            segment_link: None,
            active_span_id_stack: LinkedList::new(),
        }
    }

    /// Generate a new trace context using the propagated context.
    /// They should be propagated on `sw8` header in HTTP request with encoded form.
    /// You can retrieve decoded context with `skywalking_rust::context::propagation::encoder::encode_propagation`
    pub fn from_propagation_context(
        service_name: &str,
        instance_name: &str,
        context: PropagationContext,
    ) -> Self {
        let unix_time_fetcher = UnixTimeStampFetcher {};
        TracingContext::from_propagation_context_internal(
            Arc::new(unix_time_fetcher),
            service_name,
            instance_name,
            context,
        )
    }

    pub fn from_propagation_context_internal(
        time_fetcher: Arc<dyn TimeFetcher + Sync + Send>,
        service_name: &str,
        instance_name: &str,
        context: PropagationContext,
    ) -> Self {
        TracingContext {
            trace_id: context.parent_trace_id.clone(),
            trace_segment_id: RandomGenerator::generate(),
            service: service_name.to_string(),
            service_instance: instance_name.to_string(),
            next_span_id: 0,
            time_fetcher,
            spans: Vec::new(),
            segment_link: Some(context),
            active_span_id_stack: LinkedList::new(),
        }
    }

    /// A wrapper of create entry span, which close generated span automatically.
    /// Note that, we may use async operation in closure. But that is not unstable feature in 2021/12.
    /// <https://github.com/rust-lang/rust/issues/62290>
    /// So we should create and close spans manually in general.
    pub fn entry<F: FnMut(&Span)>(
        &mut self,
        operation_name: &str,
        mut process_fn: F,
    ) -> Result<(), &str> {
        match self.create_entry_span(operation_name) {
            Ok(mut span) => {
                process_fn(span.as_ref());
                span.close();
                Ok(())
            }
            Err(message) => Err(message),
        }
    }

    /// Create a new entry span, which is an initiator of collection of spans.
    /// This should be called by invocation of the function which is triggered by
    /// external service.
    pub fn create_entry_span(&mut self, operation_name: &str) -> Result<Box<Span>, &'static str> {
        if self.next_span_id >= 1 {
            return Err("entry span have already exist.");
        }

        let parent_span_id = self.peek_active_span_id().unwrap_or(-1);

        let mut span = Box::new(Span::new(
            self.next_span_id,
            parent_span_id,
            operation_name.to_string(),
            String::default(),
            SpanType::Entry,
            SpanLayer::Http,
            false,
            self.time_fetcher.clone(),
        ));

        if self.segment_link.is_some() {
            span.add_segment_reference(SegmentReference {
                ref_type: RefType::CrossProcess as i32,
                trace_id: self.trace_id.clone(),
                parent_trace_segment_id: self
                    .segment_link
                    .as_ref()
                    .unwrap()
                    .parent_trace_segment_id
                    .clone(),
                parent_span_id: self.segment_link.as_ref().unwrap().parent_span_id,
                parent_service: self.segment_link.as_ref().unwrap().parent_service.clone(),
                parent_service_instance: self
                    .segment_link
                    .as_ref()
                    .unwrap()
                    .parent_service_instance
                    .clone(),
                parent_endpoint: self
                    .segment_link
                    .as_ref()
                    .unwrap()
                    .destination_endpoint
                    .clone(),
                network_address_used_at_peer: self
                    .segment_link
                    .as_ref()
                    .unwrap()
                    .destination_address
                    .clone(),
            });
        }
        self.next_span_id += 1;
        self.active_span_id_stack
            .push_back(span.span_internal.span_id);
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
    ) -> Result<(), &str> {
        match self.create_exit_span(operation_name, remote_peer) {
            Ok(mut span) => {
                process_fn(span.as_ref());
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
    ) -> Result<Box<Span>, &'static str> {
        if self.next_span_id == 0 {
            return Err("entry span must be existed.");
        }

        let parent_span_id = self.peek_active_span_id().unwrap_or(-1);

        let span = Box::new(Span::new(
            self.next_span_id,
            parent_span_id,
            operation_name.to_string(),
            remote_peer.to_string(),
            SpanType::Exit,
            SpanLayer::Http,
            false,
            self.time_fetcher.clone(),
        ));
        self.next_span_id += 1;
        self.active_span_id_stack
            .push_back(span.span_internal.span_id);
        Ok(span)
    }

    /// Close span. We can't use closed span after finalize called.
    pub fn finalize_span(&mut self, mut span: Box<Span>) {
        span.close();
        self.spans.push(span);
        self.active_span_id_stack.pop_back();
    }

    /// It converts tracing context into segment object.
    /// This conversion should be done before sending segments into OAP.
    pub fn convert_segment_object(&self) -> SegmentObject {
        let mut objects = Vec::<SpanObject>::new();

        for span in self.spans.iter() {
            objects.push(span.span_internal.clone());
        }

        SegmentObject {
            trace_id: self.trace_id.to_string(),
            trace_segment_id: self.trace_segment_id.to_string(),
            spans: objects,
            service: self.service.clone(),
            service_instance: self.service_instance.clone(),
            is_size_limited: false,
        }
    }

    pub(crate) fn peek_active_span_id(&self) -> Option<i32> {
        self.active_span_id_stack.back().copied()
    }
}
