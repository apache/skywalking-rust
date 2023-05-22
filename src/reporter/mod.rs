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

//! Reporter contains common `Report` trait and the implementations.

pub mod grpc;
pub mod print;

#[cfg(feature = "management")]
use crate::proto::v3::{InstancePingPkg, InstanceProperties};
use crate::proto::v3::{LogData, MeterData, SegmentObject};
use serde::{Deserialize, Serialize};
use std::{ops::Deref, sync::Arc};
use tokio::sync::OnceCell;

/// Collect item of protobuf object.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CollectItem {
    /// Tracing object.
    Trace(Box<SegmentObject>),
    /// Log object.
    Log(Box<LogData>),
    /// Metric object.
    Meter(Box<MeterData>),
    /// Instance properties object.
    #[cfg(feature = "management")]
    #[cfg_attr(docsrs, doc(cfg(feature = "management")))]
    Instance(Box<InstanceProperties>),
    /// Keep alive object.
    #[cfg(feature = "management")]
    #[cfg_attr(docsrs, doc(cfg(feature = "management")))]
    Ping(Box<InstancePingPkg>),
}

pub(crate) type DynReport = dyn Report + Send + Sync + 'static;

/// Report provide non-blocking report method for trace, log and metric object.
pub trait Report {
    /// The non-blocking report method.
    fn report(&self, item: CollectItem);
}

/// Noop reporter.
impl Report for () {
    fn report(&self, _item: CollectItem) {}
}

impl<T: Report> Report for Box<T> {
    fn report(&self, item: CollectItem) {
        Report::report(self.deref(), item)
    }
}

impl<T: Report> Report for Arc<T> {
    fn report(&self, item: CollectItem) {
        Report::report(self.deref(), item)
    }
}

impl<T: Report> Report for OnceCell<T> {
    fn report(&self, item: CollectItem) {
        Report::report(self.get().expect("OnceCell is empty"), item)
    }
}
