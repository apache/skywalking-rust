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
    common::{
        system_time::{fetch_time, TimePeriod},
        wait_group::WaitGroup,
    },
    proto::v3::{SpanLayer, SpanObject, SpanType},
    trace::trace_context::{SpanStack, SpanUid},
};
use std::{
    fmt::{self, Formatter},
    mem::take,
    sync::{Arc, Weak},
};

/// [AbstractSpan] contains methods handle [SpanObject].
pub trait AbstractSpan {
    /// Get immutable span object reference.
    fn span_object(&self) -> &SpanObject;

    /// Mutable with inner span object.
    fn span_object_mut(&mut self) -> &mut SpanObject;

    /// Get span id.
    fn span_id(&self) -> i32 {
        self.span_object().span_id
    }

    /// Add logs to the span.
    fn add_log<K, V, I>(&mut self, message: I)
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        self.span_object_mut().add_log(message)
    }

    /// Add tag to the span.
    fn add_tag(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.span_object_mut().add_tag(key, value)
    }
}

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
    uid: SpanUid,
    obj: Option<SpanObject>,
    wg: WaitGroup,
    stack: Arc<SpanStack>,
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Span")
            .field(
                "data",
                match self.obj {
                    Some(ref obj) => obj,
                    None => &"<none>",
                },
            )
            .finish()
    }
}

const SKYWALKING_RUST_COMPONENT_ID: i32 = 11000;

impl Span {
    pub(crate) fn new(uid: SpanUid, obj: SpanObject, wg: WaitGroup, stack: Arc<SpanStack>) -> Self {
        Self {
            uid,
            obj: Some(obj),
            wg,
            stack,
        }
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

    fn is_active_span(&self) -> bool {
        let active_spans = &*self.stack.active();
        active_spans
            .last()
            .map(|span| span.uid() == self.uid)
            .unwrap_or_default()
    }

    /// The [Span] finish at current tracing context, but the current span is
    /// still alive, until [AsyncSpan] dropped.
    ///
    /// This method must be called:
    ///
    /// 1. In original thread (tracing context).
    /// 2. Current span is active span.
    ///
    /// During alive, tags, logs and attributes of the span could be changed, in
    /// any thread.
    ///
    /// # Panics
    ///
    /// Current span could by active span.
    pub fn prepare_for_async(mut self) -> AsyncSpan {
        if !self.is_active_span() {
            panic!("current span isn't active span");
        }

        self.wg.add(1);

        AsyncSpan {
            uid: self.uid,
            wg: self.wg.clone(),
            obj: take(&mut self.obj),
            stack: Arc::downgrade(&self.stack),
        }
    }
}

impl Drop for Span {
    /// Set the end time as current time, pop from context active span stack,
    /// and push to context spans.
    fn drop(&mut self) {
        self.stack.finalize_span(self.uid, take(&mut self.obj));
    }
}

impl AbstractSpan for Span {
    #[inline]
    fn span_object(&self) -> &SpanObject {
        self.obj.as_ref().unwrap()
    }

    #[inline]
    fn span_object_mut(&mut self) -> &mut SpanObject {
        self.obj.as_mut().unwrap()
    }
}

/// Generated by [Span::prepare_for_async], tags, logs and attributes of the
/// span could be changed, in any thread.
///
/// It could be finished when dropped.
#[must_use = "assign a variable name to guard the active span not be dropped immediately."]
pub struct AsyncSpan {
    uid: SpanUid,
    obj: Option<SpanObject>,
    wg: WaitGroup,
    stack: Weak<SpanStack>,
}

impl fmt::Debug for AsyncSpan {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncSpan")
            .field(
                "data",
                match self.obj {
                    Some(ref obj) => obj,
                    None => &"<none>",
                },
            )
            .finish()
    }
}

impl Drop for AsyncSpan {
    /// Set the end time as current time.
    fn drop(&mut self) {
        self.stack
            .upgrade()
            .expect("TracingContext has dropped")
            .finalize_async_span(self.uid, take(&mut self.obj).unwrap());

        self.wg.done();
    }
}

impl AbstractSpan for AsyncSpan {
    #[inline]
    fn span_object(&self) -> &SpanObject {
        self.obj.as_ref().unwrap()
    }

    #[inline]
    fn span_object_mut(&mut self) -> &mut SpanObject {
        self.obj.as_mut().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait AssertSend: Send + 'static {}

    impl AssertSend for Span {}

    impl AssertSend for AsyncSpan {}
}
