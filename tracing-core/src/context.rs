use crate::{Span, TracingSpan};

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
    finished_spans: Vec<Box<dyn Span>>,
}

impl TracingContext {
    /// Create a new instance
    pub fn new() -> Box<dyn Context> {
        Box::new(TracingContext {
            next_seq: -1,
            finished_spans: Vec::new(),
        })
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
    use crate::TracingContext;

    #[test]
    fn test_context_stack() {
        let mut context = TracingContext::new();
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
    }
}