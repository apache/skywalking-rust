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

use super::meter::{MeterId, Transform};
use crate::{
    reporter::{CollectItem, DynReport, Report},
    skywalking_proto::v3::MeterData,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    runtime::Handle,
    spawn,
    sync::{Mutex, OnceCell},
    task::{block_in_place, spawn_blocking, spawn_local, JoinHandle},
    time::interval,
};

pub struct Metricer {
    service_name: String,
    instance_name: String,
    reporter: Box<DynReport>,
    meter_map: HashMap<MeterId, Arc<dyn Transform>>,
    report_interval: Duration,
}

impl Metricer {
    /// New with service info and reporter.
    pub fn new(
        service_name: impl ToString,
        instance_name: impl ToString,
        reporter: impl Report + Send + Sync + 'static,
    ) -> Self {
        Self {
            service_name: service_name.to_string(),
            instance_name: instance_name.to_string(),
            reporter: Box::new(reporter),
            meter_map: Default::default(),
            report_interval: Duration::from_secs(20),
        }
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }

    pub fn set_report_interval(&mut self, report_interval: Duration) {
        self.report_interval = report_interval;
    }

    pub fn register<T: Transform + 'static>(&mut self, transform: T) -> Arc<T> {
        let transform = Arc::new(transform);
        self.meter_map
            .insert(transform.meter_id(), transform.clone());
        transform
    }

    // TODO Shutdownable.
    pub fn boot(self) -> JoinHandle<()> {
        spawn(async move {
            let mut ticker = interval(self.report_interval);
            let metricer = Arc::new(self);
            loop {
                let metricer_ = metricer.clone();
                let _ = spawn_blocking(move || {
                    for (_, trans) in &metricer_.meter_map {
                        metricer_
                            .reporter
                            .report(CollectItem::Meter(trans.transform(&metricer_)));
                    }
                })
                .await;
                ticker.tick().await;
            }
        })
    }
}

pub struct Booting {}
