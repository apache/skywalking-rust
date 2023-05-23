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

use skywalking::{
    logging::{
        logger::Logger,
        record::{LogRecord, RecordType},
    },
    proto::v3::{
        log_data_body::Content, JsonLog, KeyStringValuePair, LogData, LogDataBody, LogTags,
        TextLog, TraceContext,
    },
    trace::{span::AbstractSpan, tracer::Tracer},
};
use std::{
    collections::LinkedList,
    sync::{Arc, Mutex},
};

#[test]
fn log() {
    let reporter = Arc::new(MockReporter::default());
    let logger = Logger::new("service_name", "instance_name", reporter.clone());

    {
        logger.log(LogRecord::new());
        assert_eq!(
            reporter.pop(),
            LogData {
                timestamp: 10,
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                body: Some(LogDataBody {
                    r#type: "".to_owned(),
                    content: Some(Content::Text(TextLog {
                        text: "".to_owned()
                    }))
                }),
                ..Default::default()
            }
        );
    }

    {
        logger.log(LogRecord::new().ignore_time());
        assert_eq!(
            reporter.pop(),
            LogData {
                timestamp: 0,
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                body: Some(LogDataBody {
                    r#type: "".to_owned(),
                    content: Some(Content::Text(TextLog {
                        text: "".to_owned()
                    }))
                }),
                ..Default::default()
            }
        );
    }

    {
        logger.log(
            LogRecord::new()
                .endpoint("endpoint")
                .add_tag("foo", "foo")
                .add_tags([("bar", "bar")])
                .record_type(RecordType::Json)
                .content(r#"{"content": "something to log"}"#),
        );
        assert_eq!(
            reporter.pop(),
            LogData {
                timestamp: 10,
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                endpoint: "endpoint".to_owned(),
                tags: Some(LogTags {
                    data: vec![
                        KeyStringValuePair {
                            key: "foo".to_owned(),
                            value: "foo".to_owned()
                        },
                        KeyStringValuePair {
                            key: "bar".to_owned(),
                            value: "bar".to_owned()
                        },
                    ],
                }),
                body: Some(LogDataBody {
                    r#type: "".to_owned(),
                    content: Some(Content::Json(JsonLog {
                        json: r#"{"content": "something to log"}"#.to_owned()
                    }))
                }),
                ..Default::default()
            }
        );
    }
}

#[test]
fn integrate_trace() {
    let reporter = Arc::new(MockReporter::default());
    let tracer = Tracer::new("service_name", "instance_name", reporter.clone());
    let logger = Logger::new("service_name", "instance_name", reporter.clone());

    let mut ctx = tracer.create_trace_context();
    let span = ctx.create_entry_span("operation_name");

    logger.log(LogRecord::new().with_tracing_context(&ctx).with_span(&span));
    assert_eq!(
        reporter.pop(),
        LogData {
            timestamp: 10,
            service: "service_name".to_owned(),
            service_instance: "instance_name".to_owned(),
            trace_context: Some(TraceContext {
                trace_id: ctx.trace_id().to_owned(),
                trace_segment_id: ctx.trace_segment_id().to_owned(),
                span_id: span.span_id()
            }),
            body: Some(LogDataBody {
                r#type: "".to_owned(),
                content: Some(Content::Text(TextLog {
                    text: "".to_owned()
                }))
            }),
            ..Default::default()
        }
    );
}

#[derive(Default, Clone)]
struct MockReporter {
    items: Arc<Mutex<LinkedList<LogData>>>,
}

impl MockReporter {
    fn pop(&self) -> LogData {
        self.items.try_lock().unwrap().pop_back().unwrap()
    }
}

impl Report for MockReporter {
    fn report(&self, item: CollectItem) {
        match item {
            CollectItem::Log(data) => {
                self.items.try_lock().unwrap().push_back(*data);
            }
            _ => {}
        }
    }
}
