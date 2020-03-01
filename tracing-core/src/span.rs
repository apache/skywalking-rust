use std::time::SystemTime;

use crate::log::LogEvent;
use crate::Tag;

/// Span is one of the tracing concept, representing a time duration.
/// Span typically used in the certain scope, Typically, it represent a method invocation or a RPC.
pub trait Span {
    /// Start the span with the current system time
    fn start(&mut self);
    /// Start the span by using given time point.
    fn start_with_timestamp(&mut self, timestamp: SystemTime);
    /// Add a new tag to the span
    fn tag(&mut self, tag: Tag);
    /// Add a log event to the span
    fn log(&mut self, log: LogEvent);
    /// Indicate error occurred during the span execution.
    fn error_occurred(&mut self);
    /// Set the component id represented by this span.
    /// Component id is pre-definition in the SkyWalking OAP backend component-libraries.yml file.
    /// Read [Component library settings](https://github.com/apache/skywalking/blob/master/docs/en/guides/Component-library-settings.md) documentation for more details
    fn set_component_id(&mut self, component_id: i32);
    /// End the span with the current system time.
    /// End just means sealing the end time, still need to call Context::finish_span to officially finish span and archive it for further reporting.
    fn end(&mut self);
    /// End the span by using given time point.
    /// End just means sealing the end time, still need to call Context::finish_span to officially finish span and archive it for further reporting.
    fn end_with_timestamp(&mut self, timestamp: SystemTime);

    /// All following are status reading methods.

    /// Return true if the span has been set end time
    fn is_ended(&self) -> bool;
    /// Return true if the span is an entry span
    fn is_entry(&self) -> bool;
    /// Return true if the span is an exit span
    fn is_exit(&self) -> bool;
    /// Return span id.
    fn span_id(&self) -> i32;
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
    logs: Vec<LogEvent>,
}

/// Tracing Span is only created inside TracingContext.
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
            logs: Vec::new(),
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

    fn tag(&mut self, tag: Tag) {
        self.tags.push(tag);
    }

    fn log(&mut self, log: LogEvent) {
        self.logs.push(log);
    }

    fn error_occurred(&mut self) {
        self.error_occurred = true;
    }

    fn set_component_id(&mut self, component_id: i32) {
        self.component_id = Some(component_id);
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

    fn is_ended(&self) -> bool {
        self.end_time != 0
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
    use std::time::SystemTime;

    use crate::log::{EventField, LogEvent};
    use crate::span::*;
    use crate::Tag;

    #[test]
    fn test_span_new() {
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
        let span = TracingSpan::new_entry_span(String::from("op1"), 0, 1);
        assert_eq!(span.is_entry(), true)
    }

    #[test]
    fn test_span_with_tags() {
        let mut span = TracingSpan::new_entry_span(String::from("op1"), 0, 1);
        span.tag(Tag::new(String::from("tag1"), String::from("value1")));
        span.tag(Tag::new(String::from("tag2"), String::from("value2")));

        let tags = span.tags();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags.get(0).unwrap().key(), "tag1")
    }

    #[test]
    fn test_span_with_logs() {
        let mut span = TracingSpan::_new(String::from("op1"), 0, -1);

        span.log(LogEvent::new(123, Box::new([
            { EventField::new(String::from("event1"), String::from("event description")) },
            { EventField::new(String::from("event2"), String::from("event description")) },
        ])));

        assert_eq!(span.logs.len(), 1);
    }
}



