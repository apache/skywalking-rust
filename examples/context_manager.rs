// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.
//
use skywalking::context::context_manager::ContextManager;
use skywalking::reporter::grpc::ReporterClient;
use skywalking::skywalking_proto::v3::SegmentObject;
use std::error::Error;
use tokio::{
    signal,
    sync::{
        mpsc::{self, UnboundedSender},
        OnceCell,
    },
};

static SENDER: OnceCell<UnboundedSender<SegmentObject>> = OnceCell::const_new();

// Global context manager.
static CONTEXT_MANAGER: ContextManager = ContextManager::new(|segment_object| {
    SENDER.get().unwrap().send(segment_object).unwrap();
});

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Init.
    let (sender, mut receiver) = mpsc::unbounded_channel();
    SENDER.set(sender)?;

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);

    CONTEXT_MANAGER.set_service("service", "instance");

    let mut client = ReporterClient::connect("http://0.0.0.0:11800").await?;

    // Collect.
    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                segment_object = receiver.recv() => {
                    let segment_object = match segment_object {
                        Some(segment_object) => segment_object,
                        None => break,
                    };
                    let stream = async_stream::stream! {
                        yield segment_object;
                    };
                    if let Err(e) = client.collect(stream).await {
                        eprint!("Collect failed: {:?}", e);
                    }
                }
                _ =  shutdown_rx.recv() => break,
            }
        }
    });

    // Tracing.
    do_tracing().await;

    // Graceful shutdown.
    signal::ctrl_c().await?;
    shutdown_tx.send(()).await?;
    handle.await?;

    Ok(())
}

async fn do_tracing() {
    let mut context = CONTEXT_MANAGER.create_trace_context();
    {
        let span = context.create_entry_span("op1").unwrap();
        context.finalize_span(span);
    }
    CONTEXT_MANAGER.finalize_context(context);
}
