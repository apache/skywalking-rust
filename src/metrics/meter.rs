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
    reporter::{CollectItem, DynReport, Report},
    skywalking_proto::v3::MeterData,
};
use std::sync::Arc;
use tokio::sync::OnceCell;

static GLOBAL_METER: OnceCell<Meter> = OnceCell::const_new();

/// Set the global meter.
pub fn set_global_meter(tracer: Meter) {
    if GLOBAL_METER.set(tracer).is_err() {
        panic!("global meter has setted")
    }
}

/// Get the global meter.
pub fn global_meter() -> &'static Meter {
    GLOBAL_METER.get().expect("global meter haven't setted")
}

/// Log by global meter.
pub fn metric(record: impl IntoMeterDataWithMeter) {
    global_meter().metric(record);
}

pub trait IntoMeterDataWithMeter {
    fn into_meter_data_with_meter(self, meter: &Meter) -> MeterData;
}

pub struct Inner {
    service_name: String,
    instance_name: String,
    reporter: Box<DynReport>,
}

#[derive(Clone)]
pub struct Meter {
    inner: Arc<Inner>,
}

impl Meter {
    /// New with service info and reporter.
    pub fn new(
        service_name: impl ToString,
        instance_name: impl ToString,
        reporter: impl Report + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                service_name: service_name.to_string(),
                instance_name: instance_name.to_string(),
                reporter: Box::new(reporter),
            }),
        }
    }

    pub fn service_name(&self) -> &str {
        &self.inner.service_name
    }

    pub fn instance_name(&self) -> &str {
        &self.inner.instance_name
    }

    pub fn metric(&self, record: impl IntoMeterDataWithMeter) {
        self.inner
            .reporter
            .report(CollectItem::Meter(record.into_meter_data_with_meter(self)));
    }
}
