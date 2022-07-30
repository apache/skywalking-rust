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

use crate::skywalking_proto::v3::{SpanLayer, SpanObject, SpanType};
use std::fmt::Formatter;

use super::{
    system_time::{fetch_time, TimePeriod},
    trace_context::{TracingContext, WeakTracingContext},
};

/// Span is a concept that represents trace information for a single RPC.
/// The Rust SDK supports Entry Span to represent inbound to a service
/// and Exit Span to represent outbound from a service.
///
/// # Example
///
/// ```
/// use skywalking::context::tracer::Tracer;
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
    context: WeakTracingContext,
}

impl std::fmt::Debug for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("Span");
        match self.context.upgrade() {
            Some(context) => {
                let op = context.try_with_active_span_stack(|stack| {
                    d.field("data", &stack.get(self.index));
                });
                if op.is_none() {
                    d.field("data", &format_args!("<locked>"));
                }
            }
            None => {
                d.field("context", &format_args!("<dropped>"));
            }
        }
        d.finish()
    }
}

const SKYWALKING_RUST_COMPONENT_ID: i32 = 11000;

impl Span {
    pub(crate) fn new(index: usize, context: WeakTracingContext) -> Self {
        Self { index, context }
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

    fn upgrade_context(&self) -> TracingContext {
        self.context.upgrade().expect("Context has dropped")
    }

    // Notice: Perhaps in the future, `RwLock` can be used instead of `Mutex`, so
    // `with_*` can be nested. (Although I can't find the meaning of such use at
    // present.)
    pub fn with_span_object<T>(&self, f: impl FnOnce(&SpanObject) -> T) -> T {
        self.upgrade_context()
            .with_active_span_stack(|stack| f(&stack[self.index]))
    }

    pub fn with_span_object_mut<T>(&mut self, f: impl FnOnce(&mut SpanObject) -> T) -> T {
        self.upgrade_context()
            .with_active_span_stack_mut(|stack| f(&mut stack[self.index]))
    }

    pub fn span_id(&self) -> i32 {
        self.with_span_object(|span| span.span_id)
    }

    /// Add logs to the span.
    pub fn add_log<K, V, I>(&mut self, message: I)
    where
        K: ToString,
        V: ToString,
        I: IntoIterator<Item = (K, V)>,
    {
        self.with_span_object_mut(|span| span.add_log(message))
    }

    /// Add tag to the span.
    pub fn add_tag(&mut self, key: impl ToString, value: impl ToString) {
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
        if self.upgrade_context().finalize_span(self.index).is_err() {
            panic!("Dropped span isn't the active span");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait AssertSend: Send {}

    impl AssertSend for Span {}
}
