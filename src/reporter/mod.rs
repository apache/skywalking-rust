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

pub mod grpc;
pub mod print;

use crate::skywalking_proto::v3::{LogData, MeterData, SegmentObject};
use serde::{Deserialize, Serialize};
use std::{ops::Deref, sync::Arc};
use tokio::sync::OnceCell;

#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CollectItem {
    Trace(SegmentObject),
    Log(LogData),
    Meter(MeterData),
}

pub(crate) type DynReport = dyn Report + Send + Sync + 'static;

/// Report provide non-blocking report method for trace, log and metric object.
pub trait Report {
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
