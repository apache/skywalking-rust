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
#[cfg(feature = "kafka-reporter")]
#[cfg_attr(docsrs, doc(cfg(feature = "kafka-reporter")))]
pub mod kafka;
pub mod print;

#[cfg(feature = "management")]
use crate::proto::v3::{InstancePingPkg, InstanceProperties};
use crate::proto::v3::{LogData, MeterData, SegmentObject};
use serde::{Deserialize, Serialize};
use std::{error::Error, ops::Deref, sync::Arc};
use tokio::sync::{mpsc, OnceCell};
use tonic::async_trait;

/// Collect item of protobuf object.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl CollectItem {
    #[cfg(feature = "kafka-reporter")]
    pub(crate) fn encode_to_vec(self) -> Vec<u8> {
        use prost::Message;

        match self {
            CollectItem::Trace(item) => item.encode_to_vec(),
            CollectItem::Log(item) => item.encode_to_vec(),
            CollectItem::Meter(item) => item.encode_to_vec(),
            #[cfg(feature = "management")]
            CollectItem::Instance(item) => item.encode_to_vec(),
            #[cfg(feature = "management")]
            CollectItem::Ping(item) => item.encode_to_vec(),
        }
    }
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

/// Special purpose, used for user-defined production operations. Generally, it
/// does not need to be handled.
pub trait CollectItemProduce: Send + Sync + 'static {
    /// Produce the collect item non-blocking.
    fn produce(&self, item: CollectItem) -> Result<(), Box<dyn Error>>;
}

impl CollectItemProduce for () {
    fn produce(&self, _item: CollectItem) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

impl CollectItemProduce for mpsc::Sender<CollectItem> {
    fn produce(&self, item: CollectItem) -> Result<(), Box<dyn Error>> {
        Ok(self.blocking_send(item)?)
    }
}

impl CollectItemProduce for mpsc::UnboundedSender<CollectItem> {
    fn produce(&self, item: CollectItem) -> Result<(), Box<dyn Error>> {
        Ok(self.send(item)?)
    }
}

/// Alias of method result of [CollectItemConsume].
pub type ConsumeResult = Result<Option<CollectItem>, Box<dyn Error + Send>>;

/// Special purpose, used for user-defined consume operations. Generally, it
/// does not need to be handled.
#[async_trait]
pub trait CollectItemConsume: Send + Sync + 'static {
    /// Consume the collect item blocking.
    async fn consume(&mut self) -> ConsumeResult;

    /// Try to consume the collect item non-blocking.
    async fn try_consume(&mut self) -> ConsumeResult;
}

#[async_trait]
impl CollectItemConsume for () {
    async fn consume(&mut self) -> ConsumeResult {
        Ok(None)
    }

    async fn try_consume(&mut self) -> ConsumeResult {
        Ok(None)
    }
}

#[async_trait]
impl CollectItemConsume for mpsc::Receiver<CollectItem> {
    async fn consume(&mut self) -> ConsumeResult {
        Ok(self.recv().await)
    }

    async fn try_consume(&mut self) -> ConsumeResult {
        use mpsc::error::TryRecvError;

        match self.try_recv() {
            Ok(item) => Ok(Some(item)),
            Err(e) => match e {
                TryRecvError::Empty => Ok(None),
                TryRecvError::Disconnected => Err(Box::new(e)),
            },
        }
    }
}

#[async_trait]
impl CollectItemConsume for mpsc::UnboundedReceiver<CollectItem> {
    async fn consume(&mut self) -> ConsumeResult {
        Ok(self.recv().await)
    }

    async fn try_consume(&mut self) -> ConsumeResult {
        use mpsc::error::TryRecvError;

        match self.try_recv() {
            Ok(item) => Ok(Some(item)),
            Err(e) => match e {
                TryRecvError::Empty => Ok(None),
                TryRecvError::Disconnected => Err(Box::new(e)),
            },
        }
    }
}
