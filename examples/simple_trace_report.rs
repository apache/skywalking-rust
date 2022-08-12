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

use skywalking::{reporter::grpc::GrpcReporter, trace::tracer::Tracer};
use std::error::Error;
use tokio::signal;

async fn handle_request(tracer: Tracer) {
    let mut ctx = tracer.create_trace_context();

    {
        // Generate an Entry Span when a request is received.
        // An Entry Span is generated only once per context.
        // Assign a variable name to guard the span not to be dropped immediately.
        let _span = ctx.create_entry_span("op1");

        // Something...

        {
            // Generates an Exit Span when executing an RPC.
            let _span2 = ctx.create_exit_span("op2", "remote_peer");

            // Something...

            // Auto close span2 when dropped.
        }

        // Auto close span when dropped.
    }

    // Auto report ctx when dropped.
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Connect to skywalking oap server.
    let reporter = GrpcReporter::connect("http://0.0.0.0:11800").await?;

    // Spawn the reporting in background, with listening the graceful shutdown
    // signal.
    let handle = reporter
        .reporting()
        .await
        .with_graceful_shutdown(async move {
            signal::ctrl_c().await.expect("failed to listen for event");
        })
        .spawn();

    // Do tracing.
    let tracer = Tracer::new("service", "instance", reporter);
    handle_request(tracer).await;

    // Wait the reporting to quit.
    handle.await?;

    Ok(())
}
