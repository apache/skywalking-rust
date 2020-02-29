use std::rc::Rc;

use crate::{ID, Reporter, Span, TracingSpan};
use crate::id::IDGenerator;

/// Context represents the context of a tracing process.
/// All new span belonging to this tracing context should be created through this context.
pub trait Context {
    fn create_entry_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span>;
    fn create_exit_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span>;
    fn create_local_span(&mut self, operation_name: String, parent: Option<&Box<dyn Span>>) -> Box<dyn Span>;
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
    pub fn new(reporter: &dyn Reporter) -> Option<TracingContext> {
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
    use std::rc::Rc;
    use std::sync::mpsc;
    use std::sync::mpsc::{Receiver, Sender};

    use crate::{Context, Reporter, TracingContext};

    #[test]
    fn test_context_stack() {
        let reporter = MockReporter::new();
        let mut context = TracingContext::new(&reporter).unwrap();
        let span1 = context.create_entry_span(String::from("op1"), None);
        {
            assert_eq!(span1.span_id(), 0);
            let span2 = context.create_entry_span(String::from("op2"), Some(&span1));
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

    impl Reporter for MockReporter {
        fn service_instance_id(&self) -> Option<i32> {
            Some(1)
        }

        fn report_trace(&self, finished_context: TracingContext) {
            self.sender.send(finished_context);
        }
    }

    struct MockRegisterPending {}

    impl Reporter for MockRegisterPending {
        fn service_instance_id(&self) -> Option<i32> {
            None
        }

        fn report_trace(&self, finished_context: TracingContext) {
            unimplemented!()
        }
    }
}