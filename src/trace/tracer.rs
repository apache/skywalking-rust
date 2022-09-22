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

//! Tracer items.

use crate::{
    reporter::{CollectItem, DynReport, Report},
    trace::trace_context::TracingContext,
};
use std::sync::{Arc, Weak};
use tokio::sync::OnceCell;

static GLOBAL_TRACER: OnceCell<Tracer> = OnceCell::const_new();

/// Set the global tracer.
pub fn set_global_tracer(tracer: Tracer) {
    if GLOBAL_TRACER.set(tracer).is_err() {
        panic!("global tracer has set")
    }
}

/// Get the global tracer.
pub fn global_tracer() -> &'static Tracer {
    GLOBAL_TRACER.get().expect("global tracer haven't set")
}

/// Create trace context by global tracer.
pub fn create_trace_context() -> TracingContext {
    global_tracer().create_trace_context()
}

struct Inner {
    service_name: String,
    instance_name: String,
    reporter: Box<DynReport>,
}

/// Skywalking tracer.
#[derive(Clone)]
pub struct Tracer {
    inner: Arc<Inner>,
}

impl Tracer {
    /// New with service info and reporter.
    pub fn new(
        service_name: impl Into<String>,
        instance_name: impl Into<String>,
        reporter: impl Report + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                service_name: service_name.into(),
                instance_name: instance_name.into(),
                reporter: Box::new(reporter),
            }),
        }
    }

    /// Get service name.
    pub fn service_name(&self) -> &str {
        &self.inner.service_name
    }

    /// Get instance name.
    pub fn instance_name(&self) -> &str {
        &self.inner.instance_name
    }

    /// Create trace context.
    pub fn create_trace_context(&self) -> TracingContext {
        TracingContext::new(
            &self.inner.service_name,
            &self.inner.instance_name,
            self.downgrade(),
        )
    }

    /// Finalize the trace context.
    pub(crate) fn finalize_context(&self, context: &mut TracingContext) {
        let segment_object = context.convert_to_segment_object();
        self.inner
            .reporter
            .report(CollectItem::Trace(Box::new(segment_object)));
    }

    fn downgrade(&self) -> WeakTracer {
        WeakTracer {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

#[derive(Clone)]
pub(crate) struct WeakTracer {
    inner: Weak<Inner>,
}

impl WeakTracer {
    pub(crate) fn upgrade(&self) -> Option<Tracer> {
        Weak::upgrade(&self.inner).map(|inner| Tracer { inner })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait AssertSend: Send {}

    impl AssertSend for Tracer {}
}
