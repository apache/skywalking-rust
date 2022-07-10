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
use skywalking::context::tracer::Tracer;
use skywalking::reporter::grpc::GrpcReporter;
use std::error::Error;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::oneshot;

async fn handle_request(tracer: Arc<Tracer<GrpcReporter>>) {
    let mut ctx = tracer.create_trace_context();

    {
        // Generate an Entry Span when a request
        // is received. An Entry Span is generated only once per context.
        let span = ctx.create_entry_span("op1").unwrap();

        // Something...

        {
            // Generates an Exit Span when executing an RPC.
            let span2 = ctx.create_exit_span("op2", "remote_peer").unwrap();

            // Something...

            ctx.finalize_span(span2);
        }

        ctx.finalize_span(span);
    }

    tracer.finalize_context(ctx);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let reporter = GrpcReporter::connect("http://0.0.0.0:11800").await?;
    let tracer = Arc::new(Tracer::new("service", "instance", reporter));
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(handle_request(tracer.clone()));

    // Block to report.
    let handle = tokio::spawn(async move { tracer.reporting(shutdown_rx).await });

    // Graceful shutdown.
    signal::ctrl_c().await?;
    let _ = shutdown_tx.send(());
    handle.await??;

    Ok(())
}
