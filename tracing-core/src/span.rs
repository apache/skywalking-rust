use std::time::SystemTime;

use crate::Tag;

/// Span is one of the tracing concept, representing a time duration.
/// Typically, it represent a method invocation or a RPC.
pub trait Span {
    fn start(&mut self);
    fn start_with_timestamp(&mut self, timestamp: SystemTime);
    fn is_entry(&self) -> bool;
    fn is_exit(&self) -> bool;
    fn span_id(&self) -> i32;
    fn tag(&mut self, tag: Tag);
    /// End means sealing the end time.
    /// Still need to call Context::archive
    fn end(&mut self);
    fn end_with_timestamp(&mut self, timestamp: SystemTime);
    fn is_ended(&mut self) -> bool;
    /// Return the replicated existing tags.
    fn tags(&self) -> Vec<Tag>;
}

pub struct TracingSpan {
    /// The operation name represents the logic process of this span
    operation_name: String,
    span_id: i32,
    parent_span_id: i32,
    /// The timestamp of the span start time
    start_time: u64,
    /// The timestamp of the span end time
    end_time: u64,
    /// As an entry span
    is_entry: bool,
    /// As an exit span
    is_exit: bool,
    /// The peer network address when as an RPC related span.
    /// Typically used in exit span, representing the target server address.
    peer: Option<String>,
    /// Tag this span in error status.
    error_occurred: bool,
    /// Component id is defined in the main repo to represent the library kind.
    component_id: Option<i32>,
    tags: Vec<Tag>,
}

impl TracingSpan {
    /// Create a new entry span
    pub fn new_entry_span(operation_name: String, span_id: i32, parent_span_id: i32) -> Box<dyn Span> {
        let mut span = TracingSpan::_new(operation_name, span_id, parent_span_id);
        span.is_entry = true;
        Box::new(span)
    }

    /// Create a new exit span
    pub fn new_exit_span(operation_name: String, span_id: i32, parent_span_id: i32) -> Box<dyn Span> {
        let mut span = TracingSpan::_new(operation_name, span_id, parent_span_id);
        span.is_exit = true;
        Box::new(span)
    }

    /// Create a new local span
    pub fn new_local_span(operation_name: String, span_id: i32, parent_span_id: i32) -> Box<dyn Span> {
        let span = TracingSpan::_new(operation_name, span_id, parent_span_id);
        Box::new(span)
    }

    /// Create a span and set the limited internal values
    fn _new(operation_name: String, span_id: i32, parent_span_id: i32) -> Self {
        TracingSpan {
            operation_name,
            span_id,
            parent_span_id,
            start_time: 0,
            end_time: 0,
            is_entry: false,
            is_exit: false,
            peer: None,
            error_occurred: false,
            component_id: None,
            tags: Vec::new(),
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

    fn tag(&mut self, tag: Tag) {
        self.tags.push(tag);
    }

    fn end(&mut self) {
        self.end_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => { n.as_millis() }
            Err(_) => self.start_time as u128,
        } as u64;
    }

    fn end_with_timestamp(&mut self, timestamp: SystemTime) {
        self.end_time = match timestamp.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => { n.as_millis() }
            Err(_) => self.start_time as u128,
        } as u64;
    }

    fn is_ended(&mut self) -> bool {
        self.end_time != 0
    }

    fn tags(&self) -> Vec<Tag> {
        let mut tags = Vec::new();
        for t in &self.tags {
            tags.push(t.clone());
        };
        tags
    }
}

#[cfg(test)]
mod span_tests {
    use std::rc::Rc;
    use std::time::SystemTime;

    use crate::{Context, Reporter, Tag, TracingContext};
    use crate::span::*;

    #[test]
    fn test_span_new() {
        let mut context = TracingContext::new(&MockRegister {}).unwrap();
        let mut span = TracingSpan::_new(String::from("op1"), 0, -1);
        assert_eq!(span.parent_span_id, -1);
        assert_eq!(span.span_id, 0);
        assert_eq!(span.start_time, 0);
        span.start();
        assert_ne!(span.start_time, 0);

        let mut span2 = TracingSpan::_new(String::from("op2"), 1, 0);
        assert_eq!("op2", span2.operation_name);
        assert_eq!(span2.parent_span_id, 0);
        assert_eq!(span2.span_id, 1);
        span2.start_with_timestamp(SystemTime::now());
        assert_ne!(span2.start_time, 0);
    }

    #[test]
    fn test_new_entry_span() {
        let context = TracingContext::new(&MockRegister {}).unwrap();
        let span = TracingSpan::new_entry_span(String::from("op1"), 0, 1);
        assert_eq!(span.is_entry(), true)
    }

    #[test]
    fn test_span_with_tags() {
        let context = TracingContext::new(&MockRegister {}).unwrap();
        let mut span = TracingSpan::new_entry_span(String::from("op1"), 0, 1);
        span.tag(Tag::new(String::from("tag1"), String::from("value1")));
        span.tag(Tag::new(String::from("tag2"), String::from("value2")));

        let tags = span.tags();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags.get(0).unwrap().key(), "tag1")
    }

    struct MockRegister {}

    impl Reporter for MockRegister {
        fn service_instance_id(&self) -> Option<i32> {
            Some(1)
        }

        fn report_trace(&self, finished_context: TracingContext) {
            unimplemented!()
        }
    }
}



