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

/// Log represents an event happened during the span duration.
/// It is much heavier than tag. Usually this is only used in the error case to log the detailed error message.
/// Log Entity is a creation once object. Can't be change once it is created.
pub struct LogEvent {
    timestamp: i64,
    /// Any extra fields to describe the event.
    fields: Box<[EventField]>,
}

pub struct EventField {
    name: String,
    value: String,
}

impl LogEvent {
    pub fn new(timestamp: i64, fields: Box<[EventField]>) -> Self {
        LogEvent {
            timestamp,
            fields,
        }
    }
}

impl EventField {
    pub fn new(name: String, value: String) -> Self {
        EventField {
            name,
            value,
        }
    }
}

#[cfg(test)]
mod log_tests {
    use crate::skywalking::core::log::{EventField, LogEvent};

    #[test]
    fn test_log_new() {
        let fields = [
            { EventField::new(String::from("event1"), String::from("event description")) },
            { EventField::new(String::from("event2"), String::from("event description")) },
        ];
        let event = LogEvent::new(123, Box::new(fields));
        assert_eq!(event.timestamp, 123);
        assert_eq!(event.fields.len(), 2);
        assert_eq!(event.fields[0].name, "event1");
        assert_eq!(event.fields[1].value, "event description");
    }
}

