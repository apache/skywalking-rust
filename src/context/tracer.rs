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
    context::trace_context::TracingContext, reporter::Reporter, skywalking_proto::v3::SegmentObject,
};
use futures_util::stream;
use std::future::Future;
use std::{collections::LinkedList, sync::Arc};
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver},
        Mutex,
    },
    task::JoinHandle,
};

/// Skywalking tracer.
pub struct Tracer<R: Reporter + Send + Sync + 'static> {
    service_name: String,
    instance_name: String,
    reporter: Arc<Mutex<R>>,
    segment_sender: mpsc::UnboundedSender<SegmentObject>,
    segment_receiver: Arc<Mutex<mpsc::UnboundedReceiver<SegmentObject>>>,
}

impl<R: Reporter + Send + Sync + 'static> Tracer<R> {
    /// New with service info and reporter.
    pub fn new(service_name: impl ToString, instance_name: impl ToString, reporter: R) -> Self {
        let (segment_sender, segment_receiver) = mpsc::unbounded_channel();

        Self {
            service_name: service_name.to_string(),
            instance_name: instance_name.to_string(),
            reporter: Arc::new(Mutex::new(reporter)),
            segment_sender,
            segment_receiver: Arc::new(Mutex::new(segment_receiver)),
        }
    }

    /// Create trace conetxt.
    pub fn create_trace_context(&self) -> TracingContext {
        TracingContext::default(&self.service_name, &self.instance_name)
    }

    /// Create trace conetxt from propagation.
    pub fn create_trace_context_from_propagation(
        &self,
        context: PropagationContext,
    ) -> TracingContext {
        TracingContext::from_propagation_context(&self.service_name, &self.instance_name, context)
    }

    /// Finalize the trace context.
    pub fn finalize_context(&self, context: TracingContext) {
        let segment_object = context.convert_segment_object();
        if self.segment_sender.send(segment_object).is_err() {
            tracing::warn!("segment object channel has closed");
        }
    }

    /// Start to reporting, quit when shutdown_signal received.
    ///
    /// Accept a `shutdown_signal` argument as a graceful shutdown signal.
    pub fn reporting(
        &self,
        shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    ) -> JoinHandle<()> {
        let reporter = self.reporter.clone();
        let segment_receiver = self.segment_receiver.clone();
        tokio::spawn(Self::do_reporting(
            reporter,
            segment_receiver,
            shutdown_signal,
        ))
    }

    async fn do_reporting(
        reporter: Arc<Mutex<R>>,
        segment_receiver: Arc<Mutex<UnboundedReceiver<SegmentObject>>>,
        shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    ) {
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            loop {
                let mut segment_receiver = segment_receiver.lock().await;
                let mut segments = LinkedList::new();

                tokio::select! {
                    segment = segment_receiver.recv() => {
                        drop(segment_receiver);

                        if let Some(segment) = segment {
                            // TODO Implement batch collect in future.
                            segments.push_back(segment);
                            let mut reporter = reporter.lock().await;
                            Self::report_segment_object(&mut reporter, segments).await;
                        } else {
                            break;
                        }
                    }
                    _ =  shutdown_rx.recv() => break,
                }
            }
        });

        shutdown_signal.await;

        if shutdown_tx.send(()).is_err() {
            tracing::error!("Shutdown signal send failed");
        }
        if let Err(e) = handle.await {
            tracing::error!("Tokio handle join failed: {:?}", e);
        }
    }

    async fn report_segment_object(reporter: &mut R, segments: LinkedList<SegmentObject>) {
        let stream = stream::iter(segments);
        if let Err(e) = reporter.collect(stream).await {
            tracing::error!("Collect failed: {:?}", e);
        }
    }
}
