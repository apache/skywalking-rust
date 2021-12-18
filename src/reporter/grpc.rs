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

pub struct Reporter {}

static CHANNEL_BUF_SIZE: usize = 1024;

impl Reporter {
    /// Open gRPC client stream to send collected trace context.
    /// This function generates a new async task which watch to arrive new trace context.
    /// We can send collected context to push into sender.
    ///
    /// # Example
    ///
    /// ```
    /// use tokio;
    ///
    /// #[tokio::main]
    /// async fn main {
    ///     let tx = Reporter::start("localhost:12800");
    ///     tx.send(context).await;
    /// }
    /// ```
    pub async fn start(address: &str) -> mpsc::Sender<TracingContext> {
        let (tx, mut rx): (mpsc::Sender<TracingContext>, mpsc::Receiver<TracingContext>) =
            mpsc::channel(CHANNEL_BUF_SIZE);
        let mut reporter = ReporterClient::connect(address.to_string()).await.unwrap();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                flush(&mut reporter, message.convert_segment_object())
                    .await
                    .unwrap();
            }
        });
        tx
    }
}
