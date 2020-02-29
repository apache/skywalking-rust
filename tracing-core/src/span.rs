use std::time::SystemTime;

use crate::Context;

/// Span is one of the tracing concept, representing a time duration.
/// Typically, it represent a method invocation or a RPC.
pub trait Span {
    fn start(&mut self);
    fn start_with_timestamp(&mut self, timestamp: SystemTime);
    fn is_entry(&self) -> bool;
    fn is_exit(&self) -> bool;
    fn span_id(&self) -> i32;
}

pub struct TracingSpan {
    /// The operation name represents the logic process of this span
    operation_name: String,
    span_id: i32,
    paren_span_id: i32,
    /// The timestamp of the span start time
    start_time: u64,
    /// The timestamp of the span end time
    end_time: u64,
    /// As an entry span
    is_entry: bool,
    /// As an exit span
    is_exit: bool,
}

impl TracingSpan {
    /// Create a new entry span
    pub fn new_entry_span(operation_name: String, context: &mut dyn Context, parent: Option<Box<dyn Span>>) -> Box<dyn Span> {
        let mut span = TracingSpan::_new(operation_name, context, parent);
        span.is_entry = true;
        Box::new(span)
    }

    /// Create a new exit span
    pub fn new_exit_span(operation_name: String, context: &mut dyn Context, parent: Option<Box<dyn Span>>) -> Box<dyn Span> {
        let mut span = TracingSpan::_new(operation_name, context, parent);
        span.is_exit = true;
        Box::new(span)
    }

    /// Create a new local span
    pub fn new_local_span(operation_name: String, context: &mut dyn Context, parent: Option<Box<dyn Span>>) -> Box<dyn Span> {
        let span = TracingSpan::_new(operation_name, context, parent);
        Box::new(span)
    }

    /// Create a span and set the limited internal values
    fn _new(operation_name: String, context: &mut dyn Context, parent: Option<Box<dyn Span>>) -> Self {
        TracingSpan {
            operation_name: operation_name.clone(),
            span_id: context.next_span_id(),
            paren_span_id: match parent {
                // -1 means no parent span
                None => { -1 }
                Some(parent) => { parent.span_id() }
            },
            start_time: 0,
            end_time: 0,
            is_entry: false,
            is_exit: false,
        }
    }
}

impl Span for TracingSpan {
    fn start(&mut self) {
        self.start_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => { n.as_millis() }
            Err(_) => self.start_time as u128,
        } as u64;
    }

    fn start_with_timestamp(&mut self, timestamp: SystemTime) {
        self.start_time = match timestamp.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => { n.as_millis() }
            Err(_) => self.start_time as u128,
        } as u64;
    }

    fn is_entry(&self) -> bool {
        self.is_entry
    }

    fn is_exit(&self) -> bool {
        self.is_exit
    }

    fn span_id(&self) -> i32 {
        self.span_id
    }
}

#[cfg(test)]
mod span_tests {
    use std::time::SystemTime;

    use crate::span::*;
    use crate::TracingContext;

    #[test]
    fn test_span_new() {
        let mut context = TracingContext::new();
        let mut span = TracingSpan::_new(String::from("op1"), &mut context, None);
        assert_eq!(span.paren_span_id, -1);
        assert_eq!(span.span_id, 0);
        assert_eq!(span.start_time, 0);
        span.start();
        assert_ne!(span.start_time, 0);

        let mut span2 = TracingSpan::_new(String::from("op2"), &mut context, Some(Box::new(span)));
        assert_eq!("op2", span2.operation_name);
        assert_eq!(span2.paren_span_id, 0);
        assert_eq!(span2.span_id, 1);
        span2.start_with_timestamp(SystemTime::now());
        assert_ne!(span2.start_time, 0);
    }

    #[test]
    fn test_new_entry_span() {
        let mut context = TracingContext::new();
        let mut span = TracingSpan::new_entry_span(String::from("op1"), &mut context, None);
        assert_eq!(span.is_entry(), true)
    }
}



