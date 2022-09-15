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

use skywalking::{
    management::{instance::Properties, manager::Manager},
    reporter::grpc::GrpcReporter,
};
use std::{error::Error, time::Duration};
use tokio::signal;

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

    let manager = Manager::new("service", "instance", reporter);

    // Report instance properties.
    let mut props = Properties::default();
    props.insert_os_info();
    manager.report_properties(props);

    // Keep alive
    manager.keep_alive(Duration::from_secs(10));

    handle.await?;

    Ok(())
}
