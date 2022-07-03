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
use std::error::Error;

use skywalking::context::trace_context::TracingContext;
use skywalking::reporter::grpc::Reporter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let reporter = Reporter::start("http://0.0.0.0:11800").await;
    let mut context = TracingContext::default("service", "instance");
    {
        let span = context.create_entry_span("op1").unwrap();
        context.finalize_span(span);
    }
    reporter.sender().send(context).await?;
    reporter.shutdown().await?;
    Ok(())
}
