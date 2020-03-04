// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements.  See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License.  You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use base64::{decode, encode};

use crate::skywalking::agent::reporter::Reporter;
use crate::skywalking::core::{ID, Span};
use crate::skywalking::core::context_carrier::{Extractable, Injectable};
use crate::skywalking::core::id::IDGenerator;
use crate::skywalking::core::segment_ref::SegmentRef;
use crate::skywalking::core::span::TracingSpan;

/// Context represents the context of a tracing process.
/// All new span belonging to this tracing context should be created through this context.
pub trait Context {
    /// Create an entry span belonging this context
    fn create_entry_span(&mut self, operation_name: &str, parent_span_id: Option<i32>, extractor: Option<&dyn Extractable>) -> Box<dyn Span>;
    /// Create an exit span belonging this context
    fn create_exit_span(&mut self, operation_name: &str, parent_span_id: Option<i32>, peer: &str, injector: Option<&dyn Injectable>) -> Box<dyn Span>;
    /// Create an local span belonging this context
    fn create_local_span(&mut self, operation_name: &str, parent_span_id: Option<i32>) -> Box<dyn Span>;
    /// Finish the given span. The span is only being accept if it belongs to this context.
    /// Return err if the span was created by another context.
    fn finish_span(&mut self, span: Box<dyn Span>);
}

pub struct TracingContext {
    /// Span id sequence. Indicate the number of created spans.
    next_seq: i32,

    primary_trace_id: ID,
    segment_id: ID,
    self_generated_id: bool,
    entry_endpoint_name: Option<String>,
    first_ref: Option<SegmentRef>,
    service_instance_id: i32,

    finished_spans: Vec<Box<dyn Span>>,
}

impl TracingContext {
    /// Create a new instance
    pub fn new(service_instance_id: Option<i32>) -> Option<TracingContext> {
        match service_instance_id {
            None => { None }
            Some(id) => {
                Some(TracingContext {
                    next_seq: -1,
                    primary_trace_id: IDGenerator::new_id(id),
                    segment_id: IDGenerator::new_id(id),
                    self_generated_id: true,
                    entry_endpoint_name: None,
                    first_ref: None,
                    service_instance_id: id,
                    finished_spans: Vec::new(),
                }
                )
            }
        }
    }

    pub fn service_instance_id(&self) -> i32 {
        self.service_instance_id
    }

    pub fn first_ref(&self) -> &Option<SegmentRef> {
        &self.first_ref
    }

    pub fn entry_endpoint_name(&self) -> &Option<String> {
        &self.entry_endpoint_name
    }

    pub fn trace_id(&self) -> ID {
        self.primary_trace_id.clone()
    }

    pub fn segment_id(&self) -> ID {
        self.segment_id.clone()
    }

    /// Fetch the next id for new span
    fn next_span_id(&mut self) -> i32 {
        self.next_seq = self.next_seq + 1;
        self.next_seq
    }
}

/// Default implementation of Context
impl Context for TracingContext {
    fn create_entry_span(&mut self, operation_name: &str, parent_span_id: Option<i32>, extractor: Option<&dyn Extractable>) -> Box<dyn Span> {
        let mut entry_span = TracingSpan::new_entry_span(operation_name, self.next_span_id(), match parent_span_id {
            None => { -1 }
            Some(s) => { s }
        });

        if extractor.is_some() {
            match SegmentRef::from_text(extractor.unwrap().extract("sw6".to_string())) {
                Some(reference) => {
                    if self.self_generated_id {
                        self.self_generated_id = false;
                        self.primary_trace_id = reference.get_trace_id();
                    }
                    if self.first_ref.is_none() {
                        self.first_ref = Some(reference.clone());
                        self.entry_endpoint_name = Some(String::from(operation_name))
                    }
                    entry_span._add_ref(reference);
                }
                _ => {}
            }
        }
        Box::new(entry_span)
    }

    fn create_exit_span(&mut self, operation_name: &str, parent_span_id: Option<i32>, peer: &str, injector: Option<&dyn Injectable>) -> Box<dyn Span> {
        let exit_span = TracingSpan::new_exit_span(operation_name, self.next_span_id(), match parent_span_id {
            None => { -1 }
            Some(s) => { s }
        }, peer);

        if injector.is_some() {
            injector.unwrap().inject(String::from("sw6"), SegmentRef::for_across_process(self, &exit_span, &peer).serialize());
        }

        Box::new(exit_span)
    }

    fn create_local_span(&mut self, operation_name: &str, parent_span_id: Option<i32>) -> Box<dyn Span> {
        Box::new(TracingSpan::new_local_span(operation_name, self.next_span_id(), match parent_span_id {
            None => { -1 }
            Some(s) => { s }
        }))
    }

    fn finish_span(&mut self, mut span: Box<dyn Span>) {
        if !span.is_ended() {
            span.end();
        }
        self.finished_spans.push(span);
    }
}

#[cfg(test)]
mod context_tests {
    use std::sync::mpsc;
    use std::sync::mpsc::{Receiver, Sender};

    use crate::skywalking::core::{Context, ContextListener, Extractable, ID, Injectable, Tag, TracingContext};

    #[test]
    fn test_context_stack() {
        let reporter = MockReporter::new();
        let mut context = TracingContext::new(reporter.service_instance_id()).unwrap();
        let span1 = context.create_entry_span("op1", None, Some(&MockerHeader {}));
        {
            assert_eq!(span1.span_id(), 0);
            let mut span2 = context.create_local_span("op2", Some(span1.span_id()));
            span2.tag(Tag::new(String::from("tag1"), String::from("value1")));
            {
                assert_eq!(span2.span_id(), 1);
                let span3 = context.create_exit_span("op3", Some(span2.span_id()), "127.0.0.1:8080", Some(&HeaderCarrier {}));
                assert_eq!(span3.span_id(), 2);

                context.finish_span(span3);
            }
            context.finish_span(span2);
        }
        context.finish_span(span1);

        reporter.report_trace(Box::new(context));
        // context has moved into reporter. Can't be used again.

        let received_context = reporter.recv.recv().unwrap();
        assert_eq!(received_context.primary_trace_id == ID::new(3, 4, 5), true);
        assert_eq!(received_context.finished_spans.len(), 3);
    }

    #[test]
    fn test_no_context() {
        let context = TracingContext::new(None);
        assert_eq!(context.is_none(), true);
    }

    struct MockReporter {
        sender: Box<Sender<Box<TracingContext>>>,
        recv: Box<Receiver<Box<TracingContext>>>,
    }

    impl MockReporter {
        fn new() -> Self {
            let (tx, rx) = mpsc::channel();
            MockReporter {
                sender: Box::new(tx),
                recv: Box::new(rx),
            }
        }
    }

    impl ContextListener for MockReporter {
        fn service_instance_id(&self) -> Option<i32> {
            Some(1)
        }

        fn report_trace(&self, finished_context: Box<TracingContext>) {
            self.sender.send(finished_context);
        }
    }

    struct MockerHeader {}

    impl Extractable for MockerHeader {
        fn extract(&self, key: String) -> &str {
            "1-My40LjU=-MS4yLjM=-4-1-1-IzEyNy4wLjAuMTo4MDgw-Iy9wb3J0YWw=-MTIz"
        }
    }

    struct HeaderCarrier {}

    impl Injectable for HeaderCarrier {
        fn inject(&self, key: String, value: String) {
            assert_eq!(key, "sw6");
            assert_eq!(value.len() > 0, true);
        }
    }
}