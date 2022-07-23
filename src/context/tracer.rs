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

use super::propagation::context::PropagationContext;
use crate::{
    context::trace_context::TracingContext, reporter::DynReporter, reporter::Reporter,
    skywalking_proto::v3::SegmentObject,
};
use std::future::{self, Future};
use std::mem::take;
use std::pin::Pin;
use std::sync::Weak;
use std::task::{Context, Poll};
use std::{collections::LinkedList, sync::Arc};
use tokio::sync::{OnceCell, RwLock};
use tokio::task::JoinError;
use tokio::{
    sync::{
        mpsc::{self},
        Mutex,
    },
    task::JoinHandle,
};

static GLOBAL_TRACER: OnceCell<Tracer> = OnceCell::const_new();

/// Set the global tracer.
pub fn set_global_tracer(tracer: Tracer) {
    if GLOBAL_TRACER.set(tracer).is_err() {
        panic!("global tracer has setted")
    }
}

/// Get the global tracer.
pub fn global_tracer() -> &'static Tracer {
    GLOBAL_TRACER.get().expect("global tracer haven't setted")
}

/// Create trace conetxt by global tracer.
pub fn create_trace_context() -> TracingContext {
    global_tracer().create_trace_context()
}

/// Create trace conetxt from propagation by global tracer.
pub fn create_trace_context_from_propagation(context: PropagationContext) -> TracingContext {
    global_tracer().create_trace_context_from_propagation(context)
}

/// Start to reporting by global tracer, quit when shutdown_signal received.
///
/// Accept a `shutdown_signal` argument as a graceful shutdown signal.
pub fn reporting() -> Reporting {
    global_tracer().reporting()
}

struct Inner {
    service_name: String,
    instance_name: String,
    segment_sender: RwLock<Option<mpsc::UnboundedSender<SegmentObject>>>,
    segment_receiver: Mutex<mpsc::UnboundedReceiver<SegmentObject>>,
    reporter: Box<Mutex<DynReporter>>,
}

/// Skywalking tracer.
#[derive(Clone)]
pub struct Tracer {
    inner: Arc<Inner>,
}

impl Tracer {
    /// New with service info and reporter.
    pub fn new(
        service_name: impl ToString,
        instance_name: impl ToString,
        reporter: impl Reporter + Send + Sync + 'static,
    ) -> Self {
        let (segment_sender, segment_receiver) = mpsc::unbounded_channel();

        Self {
            inner: Arc::new(Inner {
                service_name: service_name.to_string(),
                instance_name: instance_name.to_string(),
                segment_sender: RwLock::new(Some(segment_sender)),
                segment_receiver: Mutex::new(segment_receiver),
                reporter: Box::new(Mutex::new(reporter)),
            }),
        }
    }

    pub fn service_name(&self) -> &str {
        &self.inner.service_name
    }

    pub fn instance_name(&self) -> &str {
        &self.inner.instance_name
    }

    /// Create trace conetxt.
    pub fn create_trace_context(&self) -> TracingContext {
        TracingContext::new(
            &self.inner.service_name,
            &self.inner.instance_name,
            self.downgrade(),
        )
    }

    /// Create trace conetxt from propagation.
    pub fn create_trace_context_from_propagation(
        &self,
        context: PropagationContext,
    ) -> TracingContext {
        TracingContext::from_propagation_context(
            &self.inner.service_name,
            &self.inner.instance_name,
            context,
            self.downgrade(),
        )
    }

    /// Finalize the trace context.
    pub(crate) fn finalize_context(&self, context: &mut TracingContext) {
        let segment_object = context.convert_segment_object();
        let segment_sender = match self.inner.segment_sender.try_read() {
            Ok(segment_sender) => segment_sender,
            Err(_) => {
                tracing::error!("segment object sender is locked");
                return;
            }
        };

        if segment_sender
            .as_ref()
            .and_then(|segment_sender| segment_sender.send(segment_object).ok())
            .is_none()
        {
            tracing::error!("segment object channel has closed");
        }
    }

    /// Start to reporting, quit when shutdown_signal received.
    ///
    /// Accept a `shutdown_signal` argument as a graceful shutdown signal.
    pub fn reporting(&self) -> Reporting {
        Reporting {
            tracer: self.clone(),
            handle: None,
            shutdown_signal: None,
        }
    }

    async fn do_reporting(self, shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static) {
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            loop {
                let mut segment_receiver = self.inner.segment_receiver.lock().await;
                let mut segments = LinkedList::new();

                tokio::select! {
                    segment = segment_receiver.recv() => {
                        drop(segment_receiver);

                        if let Some(segment) = segment {
                            // TODO Implement batch collect in future.
                            segments.push_back(segment);
                            Self::report_segment_object(&self.inner.reporter, segments).await;
                        } else {
                            break;
                        }
                    }
                    _ =  shutdown_rx.recv() => break,
                }
            }

            // Flush.
            let mut segment_receiver = self.inner.segment_receiver.lock().await;
            let mut segments = LinkedList::new();
            while let Ok(segment) = segment_receiver.try_recv() {
                segments.push_back(segment);
            }
            Self::report_segment_object(&self.inner.reporter, segments).await;
        });

        shutdown_signal.await;

        if shutdown_tx.send(()).is_err() {
            tracing::error!("Shutdown signal send failed");
        }
        if let Err(e) = handle.await {
            tracing::error!("Tokio handle join failed: {:?}", e);
        }
    }

    async fn report_segment_object(
        reporter: &Mutex<DynReporter>,
        segments: LinkedList<SegmentObject>,
    ) {
        if let Err(e) = reporter.lock().await.collect(segments).await {
            tracing::error!("Collect failed: {:?}", e);
        }
    }

    fn do_sync_reporting(self) {
        loop {
            let mut segment_receiver = self.inner.segment_receiver.blocking_lock();
            let mut segments = LinkedList::new();

            let segment = segment_receiver.blocking_recv();
            drop(segment_receiver);

            if let Some(segment) = segment {
                // TODO Implement batch collect in future.
                segments.push_back(segment);
                Self::sync_report_segment_object(&self.inner.reporter, segments);
            } else {
                break;
            }
        }
    }

    fn sync_report_segment_object(
        reporter: &Mutex<DynReporter>,
        segments: LinkedList<SegmentObject>,
    ) {
        if let Err(e) = reporter.blocking_lock().sync_collect(segments) {
            tracing::error!("Collect failed: {:?}", e);
        }
    }

    pub fn close(&self) {
        take(&mut *self.inner.segment_sender.blocking_write());
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

pub struct Reporting {
    tracer: Tracer,
    handle: Option<JoinHandle<()>>,
    shutdown_signal: Option<Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>>,
}

impl Reporting {
    pub fn with_graceful_shutdown(
        mut self,
        shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    ) -> Self {
        self.shutdown_signal = Some(Box::pin(shutdown_signal));
        self
    }

    pub fn spawn(mut self) -> Self {
        self.do_spawn();
        self
    }

    fn do_spawn(&mut self) {
        let tracer = self.tracer.clone();
        let shutdown_signal = take(&mut self.shutdown_signal);
        let handle = match shutdown_signal {
            Some(shutdown_signal) => tokio::spawn(tracer.do_reporting(shutdown_signal)),
            None => tokio::spawn(tracer.do_reporting(future::pending())),
        };
        self.handle = Some(handle);
    }

    pub fn wait(self) {
        self.tracer.do_sync_reporting();
    }
}

impl Future for Reporting {
    type Output = Result<(), JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.handle.is_none() {
            self.do_spawn();
        }
        Pin::new(self.handle.as_mut().unwrap()).poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait AssertSend: Send {}

    impl AssertSend for Tracer {}
}
