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

//! Manager methods.

use super::instance::Properties;
use crate::reporter::{CollectItem, DynReport, Report};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    spawn,
    task::{JoinError, JoinHandle},
    time,
};

/// Manager handles skywalking management operations, integrate with reporter.
pub struct Manager {
    service_name: String,
    instance_name: String,
    reporter: Arc<DynReport>,
}

impl Manager {
    /// New with service info and reporter.
    pub fn new(
        service_name: impl Into<String>,
        instance_name: impl Into<String>,
        reporter: impl Report + Send + Sync + 'static,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            instance_name: instance_name.into(),
            reporter: Arc::new(reporter),
        }
    }

    /// Get service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Get instance name.
    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }

    /// Report instance properties.
    pub fn report_properties(&self, properties: Properties) {
        let props = properties
            .convert_to_instance_properties(self.service_name.clone(), self.instance_name.clone());
        self.reporter.report(CollectItem::Instance(Box::new(props)));
    }

    /// Do keep alive (heartbeat), with the interval, will be run in background.
    pub fn keep_alive(&self, interval: Duration) -> KeepAlive {
        let service_name = self.service_name.clone();
        let instance_name = self.instance_name.clone();
        let reporter = self.reporter.clone();
        let handle = spawn(async move {
            let mut ticker = time::interval(interval);
            loop {
                ticker.tick().await;

                reporter.report(CollectItem::Ping(Box::new(
                    crate::skywalking_proto::v3::InstancePingPkg {
                        service: service_name.clone(),
                        service_instance: instance_name.clone(),
                        layer: Default::default(),
                    },
                )));
            }
        });
        KeepAlive { handle }
    }
}

/// Handle of [Manager::keep_alive].
pub struct KeepAlive {
    handle: JoinHandle<()>,
}

impl Future for KeepAlive {
    type Output = Result<(), JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.handle).poll(cx)
    }
}
