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

use crate::context::trace_context::TracingContext;
use crate::skywalking_proto::v3::trace_segment_report_service_client::TraceSegmentReportServiceClient;
use crate::skywalking_proto::v3::SegmentObject;
use tokio::sync::mpsc;
use tonic::transport::Channel;

pub type ReporterClient = TraceSegmentReportServiceClient<Channel>;

async fn flush(client: &mut ReporterClient, context: SegmentObject) -> Result<(), tonic::Status> {
    let stream = async_stream::stream! {
        yield context;
    };
    match client.collect(stream).await {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

pub struct Reporter {
    tx: mpsc::Sender<TracingContext>,
    shutdown_tx: mpsc::Sender<()>,
}

static CHANNEL_BUF_SIZE: usize = 1024;

impl Reporter {
    /// Open gRPC client stream to send collected trace context.
    /// This function generates a new async task which watch to arrive new trace context.
    /// We can send collected context to push into sender.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::error::Error;
    ///
    /// use tokio;
    ///
    /// use skywalking::context::trace_context::TracingContext;
    /// use skywalking::reporter::grpc::Reporter;
    ///
    /// #[tokio::main]
    /// async fn main () -> Result<(), Box<dyn Error>> {
    ///     let reporter = Reporter::start("localhost:12800").await?;
    ///     let mut context = TracingContext::default("service", "instance");
    ///     reporter.sender().send(context).await?;
    ///     reporter.shutdown().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn start(address: impl Into<String>) -> crate::Result<Self> {
        let (tx, mut rx): (mpsc::Sender<TracingContext>, mpsc::Receiver<TracingContext>) =
            mpsc::channel(CHANNEL_BUF_SIZE);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

        let mut reporter = ReporterClient::connect(address.into()).await?;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    message = rx.recv() => {
                        if let Some(message) = message {
                            flush(&mut reporter, message.convert_segment_object()).await.unwrap();
                        } else {
                            break;
                        }
                    },
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
            rx.close();
            while let Some(message) = rx.recv().await {
                flush(&mut reporter, message.convert_segment_object())
                    .await
                    .unwrap();
            }
        });
        Ok(Self { tx, shutdown_tx })
    }

    pub async fn shutdown(self) -> crate::Result<()> {
        self.shutdown_tx
            .send(())
            .await
            .map_err(|e| crate::Error::ReporterShutdown(e.to_string()))?;
        self.shutdown_tx.closed().await;
        Ok(())
    }

    pub fn sender(&self) -> mpsc::Sender<TracingContext> {
        self.tx.clone()
    }
}
