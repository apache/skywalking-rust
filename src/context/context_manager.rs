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

use crate::{context::trace_context::TracingContext, skywalking_proto::v3::SegmentObject};
use tokio::sync::OnceCell;

struct Service {
    service_name: String,
    instance_name: String,
}

/// Skywalking context manager.
pub struct ContextManager<F = fn(SegmentObject)> {
    notify: F,
    service: OnceCell<Service>,
}

impl<F> ContextManager<F> {
    /// New with notify, this method can construct static variable.
    ///
    /// Notify will be called in [`ContextManager::finalize_context`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use skywalking::context::context_manager::ContextManager;
    /// use skywalking::skywalking_proto::v3::SegmentObject;
    /// use tokio::runtime::Handle;
    /// use tokio::sync::{OnceCell, mpsc::{self, UnboundedSender, UnboundedReceiver}};
    ///
    /// static SENDER: OnceCell<UnboundedSender<SegmentObject>> = OnceCell::const_new();
    ///
    /// static CONTEXT_MANAGER: ContextManager = ContextManager::new(|segment_object| {
    ///     SENDER.get().unwrap().send(segment_object).unwrap();
    /// });
    ///
    /// let (sender, mut receiver) = mpsc::unbounded_channel();
    /// SENDER.set(sender).unwrap();
    ///
    /// tokio::spawn(async move {
    ///     receiver.recv().await;
    /// });
    /// ```
    pub const fn new(notify: F) -> Self {
        Self {
            notify,
            service: OnceCell::const_new(),
        }
    }

    /// Set the service name and instance name.
    ///
    /// # Panic
    ///
    /// Panic if call more than once.
    pub fn set_service(&self, service_name: impl ToString, instance_name: impl ToString) {
        if self
            .service
            .set(Service {
                service_name: service_name.to_string(),
                instance_name: instance_name.to_string(),
            })
            .is_err()
        {
            panic!("Has been called before");
        }
    }

    /// Create trace conetxt
    ///
    /// # Panic
    ///
    /// Panic if not call [`ContextManager::set_service`] before.
    pub fn create_trace_context(&self) -> TracingContext {
        let service = self
            .service
            .get()
            .expect("Should called `ContextManager::set_service` before");
        TracingContext::default(&service.service_name, &service.instance_name)
    }
}

impl<F: Fn(SegmentObject)> ContextManager<F> {
    /// Finalize the context and notify.
    pub fn finalize_context(&self, context: TracingContext) {
        let segment_object = context.convert_segment_object();
        (self.notify)(segment_object);
    }
}
