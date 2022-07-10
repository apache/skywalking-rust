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

use crate::{
    context::trace_context::TracingContext, reporter::Reporter, skywalking_proto::v3::SegmentObject,
};
use futures_util::stream;
use std::{collections::LinkedList, sync::Arc};
use tokio::sync::{mpsc, oneshot, Mutex};

use super::propagation::context::PropagationContext;

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
            tracing::debug!("segment object channel has closed");
        }
    }

    /// Block to report, quit when shutdown_signal received.
    ///
    /// Accept a `shutdown_signal` argument as a graceful shutdown signal.
    pub async fn reporting(&self, shutdown_signal: oneshot::Receiver<()>) -> crate::Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();
        let segment_receiver = self.segment_receiver.clone();
        let reporter = self.reporter.clone();

        let handle = tokio::spawn(async move {
            loop {
                let mut segment_receiver = segment_receiver.lock().await;
                let mut segments = LinkedList::new();

                tokio::select! {
                    segment_object = segment_receiver.recv() => {
                        if let Some(segment_object) = segment_object {
                            // TODO Implement batch collect in future.
                            segments.push_back(segment_object);
                            let mut reporter = reporter.lock().await;
                            Self::report_segment_object(&mut reporter, segments).await;
                        }
                    }
                    _ =  shutdown_rx.recv() => break,
                }
            }
        });

        shutdown_signal.await?;
        shutdown_tx.send(()).unwrap();
        handle.await?;

        Ok(())
    }

    async fn report_segment_object(reporter: &mut R, segments: LinkedList<SegmentObject>) {
        let stream = stream::iter(segments);
        if let Err(e) = reporter.collect(stream).await {
            tracing::error!("Collect failed: {:?}", e);
        }
    }
}
