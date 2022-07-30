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
    context::trace_context::TracingContext,
    reporter::{DynReporter, Reporter},
    skywalking_proto::v3::SegmentObject,
};
use std::{
    collections::LinkedList,
    error::Error,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
    task::{Context, Poll},
};
use tokio::{
    sync::{
        mpsc::{self},
        Mutex, OnceCell,
    },
    task::JoinHandle,
};
use tonic::async_trait;

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
pub fn reporting(shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static) -> Reporting {
    global_tracer().reporting(shutdown_signal)
}

pub trait SegmentSender: Send + Sync + 'static {
    fn send(&self, segment: SegmentObject) -> Result<(), Box<dyn Error>>;
}

impl SegmentSender for () {
    fn send(&self, _segment: SegmentObject) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

impl SegmentSender for mpsc::UnboundedSender<SegmentObject> {
    fn send(&self, segment: SegmentObject) -> Result<(), Box<dyn Error>> {
        Ok(self.send(segment)?)
    }
}

#[async_trait]
pub trait SegmentReceiver: Send + Sync + 'static {
    async fn recv(&self) -> Result<Option<SegmentObject>, Box<dyn Error + Send>>;

    async fn try_recv(&self) -> Result<Option<SegmentObject>, Box<dyn Error + Send>>;
}

#[async_trait]
impl SegmentReceiver for () {
    async fn recv(&self) -> Result<Option<SegmentObject>, Box<dyn Error + Send>> {
        Ok(None)
    }

    async fn try_recv(&self) -> Result<Option<SegmentObject>, Box<dyn Error + Send>> {
        Ok(None)
    }
}

#[async_trait]
impl SegmentReceiver for Mutex<mpsc::UnboundedReceiver<SegmentObject>> {
    async fn recv(&self) -> Result<Option<SegmentObject>, Box<dyn Error + Send>> {
        Ok(self.lock().await.recv().await)
    }

    async fn try_recv(&self) -> Result<Option<SegmentObject>, Box<dyn Error + Send>> {
        use mpsc::error::TryRecvError;

        match self.lock().await.try_recv() {
            Ok(segment) => Ok(Some(segment)),
            Err(e) => match e {
                TryRecvError::Empty => Ok(None),
                TryRecvError::Disconnected => Err(Box::new(e)),
            },
        }
    }
}

struct Inner {
    service_name: String,
    instance_name: String,
    segment_sender: Box<dyn SegmentSender>,
    segment_receiver: Box<dyn SegmentReceiver>,
    reporter: Box<Mutex<DynReporter>>,
    is_reporting: AtomicBool,
    is_closed: AtomicBool,
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
        Self::new_with_channel(
            service_name,
            instance_name,
            reporter,
            (segment_sender, Mutex::new(segment_receiver)),
        )
    }

    /// New with service info, reporter, and custom channel.
    pub fn new_with_channel(
        service_name: impl ToString,
        instance_name: impl ToString,
        reporter: impl Reporter + Send + Sync + 'static,
        channel: (impl SegmentSender, impl SegmentReceiver),
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                service_name: service_name.to_string(),
                instance_name: instance_name.to_string(),
                segment_sender: Box::new(channel.0),
                segment_receiver: Box::new(channel.1),
                reporter: Box::new(Mutex::new(reporter)),
                is_reporting: Default::default(),
                is_closed: Default::default(),
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
        if self.inner.is_closed.load(Ordering::Relaxed) {
            tracing::warn!("tracer closed");
            return;
        }

        let segment_object = context.convert_segment_object();
        if let Err(err) = self.inner.segment_sender.send(segment_object) {
            tracing::error!(?err, "send segment object failed");
        }
    }

    /// Start to reporting, quit when shutdown_signal received.
    ///
    /// Accept a `shutdown_signal` argument as a graceful shutdown signal.
    ///
    /// # Panics
    ///
    /// Panic if call more than once.
    pub fn reporting(
        &self,
        shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    ) -> Reporting {
        if self.inner.is_reporting.swap(true, Ordering::Relaxed) {
            panic!("reporting already called");
        }

        Reporting {
            handle: tokio::spawn(self.clone().do_reporting(shutdown_signal)),
        }
    }

    async fn do_reporting(
        self,
        shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    ) -> crate::Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    segment = self.inner.segment_receiver.recv() => {
                        match segment {
                            Ok(Some(segment)) => {
                                // TODO Implement batch collect in future.
                                let mut segments = LinkedList::new();
                                segments.push_back(segment);
                                Self::report_segment_object(&self.inner.reporter, segments).await;
                            }
                            Ok(None) => break,
                            Err(err) => return Err(err.into()),
                        }
                    }
                    _ =  shutdown_rx.recv() => break,
                }
            }

            self.inner.is_closed.store(true, Ordering::Relaxed);

            // Flush.
            let mut segments = LinkedList::new();
            loop {
                match self.inner.segment_receiver.try_recv().await {
                    Ok(Some(segment)) => {
                        segments.push_back(segment);
                    }
                    Ok(None) => break,
                    Err(err) => return Err(err.into()),
                }
            }
            Self::report_segment_object(&self.inner.reporter, segments).await;

            Ok::<_, crate::Error>(())
        });

        shutdown_signal.await;

        if shutdown_tx.send(()).is_err() {
            tracing::error!("shutdown signal send failed");
        }

        handle.await??;

        Ok(())
    }

    async fn report_segment_object(
        reporter: &Mutex<DynReporter>,
        segments: LinkedList<SegmentObject>,
    ) {
        if let Err(err) = reporter.lock().await.collect(segments).await {
            tracing::error!(?err, "collect failed");
        }
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

/// Created by [Tracer::reporting].
pub struct Reporting {
    handle: JoinHandle<crate::Result<()>>,
}

impl Future for Reporting {
    type Output = crate::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.handle).poll(cx).map(|r| r?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future;

    trait AssertSend: Send {}

    impl AssertSend for Tracer {}

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn custom_channel() {
        let tracer = Tracer::new_with_channel("service_name", "instance_name", (), ((), ()));
        tracer.reporting(future::ready(())).await.unwrap();
    }
}
