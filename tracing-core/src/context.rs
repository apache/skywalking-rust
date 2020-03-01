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

use crate::{ContextListener, ID, Span};
use crate::id::IDGenerator;
use crate::span::TracingSpan;

/// Context represents the context of a tracing process.
/// All new span belonging to this tracing context should be created through this context.
pub trait Context {
    /// Create an entry span belonging this context
    fn create_entry_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span>;
    /// Create an exit span belonging this context
    fn create_exit_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span>;
    /// Create an local span belonging this context
    fn create_local_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span>;
    /// Finish the given span. The span is only being accept if it belongs to this context.
    /// Return err if the span was created by another context.
    fn finish_span(&mut self, span: Box<dyn Span>);
}

pub struct TracingContext {
    /// Span id sequence. Indicate the number of created spans.
    next_seq: i32,

    primary_trace_id: ID,
    self_generated_id: bool,
    finished_spans: Vec<Box<dyn Span>>,
}

impl TracingContext {
    /// Create a new instance
    pub fn new(reporter: &dyn ContextListener) -> Option<TracingContext> {
        let instance_id = reporter.service_instance_id();
        match instance_id {
            None => { None }
            Some(id) => {
                Some(TracingContext {
                    next_seq: -1,
                    primary_trace_id: IDGenerator::new_id(id),
                    self_generated_id: true,
                    finished_spans: Vec::new(),
                }
                )
            }
        }
    }

    /// Fetch the next id for new span
    fn next_span_id(&mut self) -> i32 {
        self.next_seq = self.next_seq + 1;
        self.next_seq
    }
}

/// Default implementation of Context
impl Context for TracingContext {
    fn create_entry_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span> {
        TracingSpan::new_entry_span(operation_name, self.next_span_id(), match parent {
            None => { -1 }
            Some(s) => { s.span_id() }
        })
    }

    fn create_exit_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span> {
        TracingSpan::new_exit_span(operation_name, self.next_span_id(), match parent {
            None => { -1 }
            Some(s) => { s.span_id() }
        })
    }

    fn create_local_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span> {
        TracingSpan::new_local_span(operation_name, self.next_span_id(), match parent {
            None => { -1 }
            Some(s) => { s.span_id() }
        })
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

    use crate::{Context, ContextListener, Tag, TracingContext};

    #[test]
    fn test_context_stack() {
        let reporter = MockReporter::new();
        let mut context = TracingContext::new(&reporter).unwrap();
        let span1 = context.create_entry_span(String::from("op1"), None);
        {
            assert_eq!(span1.span_id(), 0);
            let mut span2 = context.create_entry_span(String::from("op2"), Some(&span1));
            span2.tag(Tag::new(String::from("tag1"), String::from("value1")));
            {
                assert_eq!(span2.span_id(), 1);
                let mut span3 = context.create_entry_span(String::from("op3"), Some(&span2));
                assert_eq!(span3.span_id(), 2);

                context.finish_span(span3);
            }
            context.finish_span(span2);
        }
        context.finish_span(span1);

        reporter.report_trace(context);
        // context has moved into reporter. Can't be used again.

        let received_context = reporter.recv.recv().unwrap();
        assert_eq!(received_context.finished_spans.len(), 3);
    }

    #[test]
    fn test_no_context() {
        let context = TracingContext::new(&MockRegisterPending {});
        assert_eq!(context.is_none(), true);
    }

    struct MockReporter {
        sender: Box<Sender<TracingContext>>,
        recv: Box<Receiver<TracingContext>>,
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

        fn report_trace(&self, finished_context: TracingContext) {
            self.sender.send(finished_context);
        }
    }

    struct MockRegisterPending {}

    impl ContextListener for MockRegisterPending {
        fn service_instance_id(&self) -> Option<i32> {
            None
        }

        fn report_trace(&self, finished_context: TracingContext) {
            unimplemented!()
        }
    }
}