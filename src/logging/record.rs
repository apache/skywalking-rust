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

//! Log record items.

use crate::{
    common::system_time::{fetch_time, TimePeriod},
    skywalking_proto::v3::{
        log_data_body::Content, JsonLog, KeyStringValuePair, LogData, LogDataBody, LogTags,
        TextLog, TraceContext, YamlLog,
    },
    trace::{span::Span, trace_context::TracingContext},
};
use std::time::{SystemTime, UNIX_EPOCH};

/// Log record type of [LogRecord];
pub enum RecordType {
    /// Text type.
    Text,
    /// Json type.
    Json,
    /// Yaml type.
    Yaml,
}

impl Default for RecordType {
    fn default() -> Self {
        Self::Text
    }
}

/// The builder of [LogData];
#[derive(Default)]
pub struct LogRecord {
    time: Option<SystemTime>,
    is_ignore_time: bool,
    endpoint: String,
    tags: Vec<(String, String)>,
    trace_id: Option<String>,
    trace_segment_id: Option<String>,
    span_id: Option<i32>,
    record_type: RecordType,
    content: String,
}

impl LogRecord {
    /// New default [LogRecord];
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Use custom time rather than now time.
    #[inline]
    pub fn custom_time(mut self, time: SystemTime) -> Self {
        self.time = Some(time);
        self
    }

    /// Not set the log time, OAP server would use the received timestamp as
    /// log's timestamp, or relies on the OAP server analyzer.
    #[inline]
    pub fn ignore_time(mut self) -> Self {
        self.is_ignore_time = true;
        self
    }

    /// The logic name represents the endpoint, which logs belong.
    #[inline]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// The available tags. OAP server could provide search/analysis
    /// capabilities based on these.
    pub fn add_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.push((key.into(), value.into()));
        self
    }

    /// The available tags. OAP server could provide search/analysis
    /// capabilities based on these.
    pub fn add_tags<K, V, I>(mut self, tags: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        self.tags
            .extend(tags.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// Logs with trace context.
    pub fn with_tracing_context(mut self, tracing_context: &TracingContext) -> Self {
        self.trace_id = Some(tracing_context.trace_id().to_owned());
        self.trace_segment_id = Some(tracing_context.trace_segment_id().to_owned());
        self
    }

    /// The span should be unique in the whole segment.
    pub fn with_span(mut self, span: &Span) -> Self {
        self.span_id = Some(span.with_span_object(|span| span.span_id));
        self
    }

    /// A type to match analyzer(s) at the OAP server.
    /// The data could be analyzed at the client side, but could be partial.
    pub fn record_type(mut self, record_type: RecordType) -> Self {
        self.record_type = record_type;
        self
    }

    /// Log content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    pub(crate) fn convert_to_log_data(
        self,
        service_name: String,
        instance_name: String,
    ) -> LogData {
        let timestamp = if self.is_ignore_time {
            0
        } else {
            match self.time {
                Some(time) => time
                    .duration_since(UNIX_EPOCH)
                    .map(|dur| dur.as_millis() as i64)
                    .unwrap_or_default(),
                None => fetch_time(TimePeriod::Log),
            }
        };
        let trace_context = match (self.trace_id, self.trace_segment_id, self.span_id) {
            (Some(trace_id), Some(trace_segment_id), Some(span_id)) => Some(TraceContext {
                trace_id,
                trace_segment_id,
                span_id,
            }),
            _ => None,
        };
        let tags = if self.tags.is_empty() {
            None
        } else {
            let data = self
                .tags
                .into_iter()
                .map(|(key, value)| KeyStringValuePair { key, value })
                .collect();
            Some(LogTags { data })
        };

        LogData {
            timestamp,
            service: service_name,
            service_instance: instance_name,
            endpoint: self.endpoint,
            body: Some(LogDataBody {
                r#type: "".to_owned(),
                content: match self.record_type {
                    RecordType::Text => Some(Content::Text(TextLog { text: self.content })),
                    RecordType::Json => Some(Content::Json(JsonLog { json: self.content })),
                    RecordType::Yaml => Some(Content::Yaml(YamlLog { yaml: self.content })),
                },
            }),
            trace_context,
            tags,
            layer: Default::default(),
        }
    }
}
