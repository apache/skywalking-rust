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

//! Span is an important and common concept in distributed tracing system. Learn
//! Span from Google Dapper Paper.

use crate::{
    common::system_time::{fetch_time, TimePeriod},
    error::LOCK_MSG,
    skywalking_proto::v3::{SpanLayer, SpanObject, SpanType},
    trace::trace_context::SpanStack,
};
use std::{
    fmt::Formatter,
    sync::{Arc, Weak},
};

/// Span is a concept that represents trace information for a single RPC.
/// The Rust SDK supports Entry Span to represent inbound to a service
/// and Exit Span to represent outbound from a service.
///
/// # Example
///
/// ```
/// use skywalking::trace::tracer::Tracer;
///
/// async fn handle_request(tracer: Tracer) {
///     let mut ctx = tracer.create_trace_context();
///
///     {
///         // Generate an Entry Span when a request is received.
///         // An Entry Span is generated only once per context.
///         // Assign a variable name to guard the span not to be dropped immediately.
///         let _span = ctx.create_entry_span("op1");
///
///         // Something...
///
///         {
///             // Generates an Exit Span when executing an RPC.
///             let _span2 = ctx.create_exit_span("op2", "remote_peer");
///
///             // Something...
///
///             // Auto close span2 when dropped.
///         }
///
///         // Auto close span when dropped.
///     }
///
///     // Auto report ctx when dropped.
/// }
/// ```
#[must_use = "assign a variable name to guard the span not be dropped immediately."]
pub struct Span {
    index: usize,
    stack: Weak<SpanStack>,
}

impl std::fmt::Debug for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let span_object: SpanObject;
        f.debug_struct("Span")
            .field(
                "data",
                match self.stack.upgrade() {
                    Some(stack) => match stack.active.try_read() {
                        Ok(spans) => match spans.get(self.index) {
                            Some(span) => {
                                span_object = span.clone();
                                &span_object
                            }
                            None => &"<hanged>",
                        },
                        Err(_) => &"<locked>",
                    },
                    None => &"<dropped>",
                },
            )
            .finish()
    }
}

const SKYWALKING_RUST_COMPONENT_ID: i32 = 11000;

impl Span {
    pub(crate) fn new(index: usize, stack: Weak<SpanStack>) -> Self {
        Self { index, stack }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_obj(
        span_id: i32,
        parent_span_id: i32,
        operation_name: String,
        remote_peer: String,
        span_type: SpanType,
        span_layer: SpanLayer,
        skip_analysis: bool,
    ) -> SpanObject {
        SpanObject {
            span_id,
            parent_span_id,
            start_time: fetch_time(TimePeriod::Start),
            operation_name,
            peer: remote_peer,
            span_type: span_type as i32,
            span_layer: span_layer as i32,
            component_id: SKYWALKING_RUST_COMPONENT_ID,
            skip_analysis,
            ..Default::default()
        }
    }

    fn upgrade_stack(&self) -> Arc<SpanStack> {
        self.stack.upgrade().expect("Context has dropped")
    }

    /// Immutable with inner span object.
    pub fn with_span_object<T>(&self, f: impl FnOnce(&SpanObject) -> T) -> T {
        self.upgrade_stack()
            .with_active(|stack| f(&stack[self.index]))
    }

    /// Mutable with inner span object.
    pub fn with_span_object_mut<T>(&mut self, f: impl FnOnce(&mut SpanObject) -> T) -> T {
        f(&mut (self.upgrade_stack().active.try_write().expect(LOCK_MSG))[self.index])
    }

    /// Get span id.
    pub fn span_id(&self) -> i32 {
        self.with_span_object(|span| span.span_id)
    }

    /// Add logs to the span.
    pub fn add_log<K, V, I>(&mut self, message: I)
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        self.with_span_object_mut(|span| span.add_log(message))
    }

    /// Add tag to the span.
    pub fn add_tag(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.with_span_object_mut(|span| span.add_tag(key, value))
    }
}

impl Drop for Span {
    /// Set the end time as current time, pop from context active span stack,
    /// and push to context spans.
    ///
    /// # Panics
    ///
    /// Panic if context is dropped or this span isn't the active span.
    fn drop(&mut self) {
        self.upgrade_stack().finalize_span(self.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait AssertSend: Send {}

    impl AssertSend for Span {}
}
