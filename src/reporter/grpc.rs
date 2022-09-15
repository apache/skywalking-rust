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

#[cfg(feature = "management")]
use crate::skywalking_proto::v3::management_service_client::ManagementServiceClient;
use crate::{
    reporter::{CollectItem, Report},
    skywalking_proto::v3::{
        log_report_service_client::LogReportServiceClient,
        meter_report_service_client::MeterReportServiceClient,
        trace_segment_report_service_client::TraceSegmentReportServiceClient, LogData, MeterData,
        SegmentObject,
    },
};
use futures_util::stream;
use std::{
    collections::LinkedList,
    error::Error,
    future::{pending, Future},
    mem::take,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::{
    select,
    sync::{mpsc, Mutex},
    task::JoinHandle,
    try_join,
};
use tonic::{
    async_trait,
    transport::{self, Channel, Endpoint},
};

pub trait CollectItemProduce: Send + Sync + 'static {
    fn produce(&self, item: CollectItem) -> Result<(), Box<dyn Error>>;
}

impl CollectItemProduce for () {
    fn produce(&self, _item: CollectItem) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

impl CollectItemProduce for mpsc::UnboundedSender<CollectItem> {
    fn produce(&self, item: CollectItem) -> Result<(), Box<dyn Error>> {
        Ok(self.send(item)?)
    }
}

#[async_trait]
pub trait ColletcItemConsume: Send + Sync + 'static {
    async fn consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>>;

    async fn try_consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>>;
}

#[async_trait]
impl ColletcItemConsume for () {
    async fn consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>> {
        Ok(None)
    }

    async fn try_consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>> {
        Ok(None)
    }
}

#[async_trait]
impl ColletcItemConsume for mpsc::UnboundedReceiver<CollectItem> {
    async fn consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>> {
        Ok(self.recv().await)
    }

    async fn try_consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>> {
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

struct Inner<P, C> {
    trace_client: Mutex<TraceSegmentReportServiceClient<Channel>>,
    log_client: Mutex<LogReportServiceClient<Channel>>,
    meter_client: Mutex<MeterReportServiceClient<Channel>>,
    #[cfg(feature = "management")]
    #[cfg_attr(docsrs, doc(cfg(feature = "management")))]
    management_client: Mutex<ManagementServiceClient<Channel>>,
    producer: P,
    consumer: Mutex<Option<C>>,
    is_reporting: AtomicBool,
    is_closed: AtomicBool,
}

pub type DynErrHandle = dyn Fn(Box<dyn Error>) + Send + Sync + 'static;

pub struct GrpcReporter<P, C> {
    inner: Arc<Inner<P, C>>,
    err_handle: Arc<Option<Box<DynErrHandle>>>,
}

impl GrpcReporter<mpsc::UnboundedSender<CollectItem>, mpsc::UnboundedReceiver<CollectItem>> {
    pub fn new(channel: Channel) -> Self {
        let (p, c) = mpsc::unbounded_channel();
        Self::new_with_pc(channel, p, c)
    }

    pub async fn connect(
        address: impl TryInto<Endpoint, Error = transport::Error>,
    ) -> crate::Result<Self> {
        let endpoint = address.try_into()?;
        let channel = endpoint.connect().await?;
        Ok(Self::new(channel))
    }
}

impl<P: CollectItemProduce, C: ColletcItemConsume> GrpcReporter<P, C> {
    pub fn new_with_pc(channel: Channel, producer: P, consumer: C) -> Self {
        Self {
            inner: Arc::new(Inner {
                trace_client: Mutex::new(TraceSegmentReportServiceClient::new(channel.clone())),
                log_client: Mutex::new(LogReportServiceClient::new(channel.clone())),
                #[cfg(feature = "management")]
                management_client: Mutex::new(ManagementServiceClient::new(channel.clone())),
                meter_client: Mutex::new(MeterReportServiceClient::new(channel)),
                producer,
                consumer: Mutex::new(Some(consumer)),
                is_reporting: Default::default(),
                is_closed: Default::default(),
            }),
            err_handle: Default::default(),
        }
    }

    pub fn with_err_handle(
        mut self,
        handle: impl Fn(Box<dyn Error>) + Send + Sync + 'static,
    ) -> Self {
        self.err_handle = Arc::new(Some(Box::new(handle)));
        self
    }

    /// Start to reporting, quit when shutdown_signal received.
    ///
    /// Accept a `shutdown_signal` argument as a graceful shutdown signal.
    ///
    /// # Panics
    ///
    /// Panic if call more than once.
    pub async fn reporting(&self) -> Reporting<P, C> {
        if self.inner.is_reporting.swap(true, Ordering::Relaxed) {
            panic!("reporting already called");
        }

        Reporting {
            rb: ReporterAndBuffer {
                inner: Arc::clone(&self.inner),
                status_handle: None,
                trace_buffer: Default::default(),
                log_buffer: Default::default(),
                meter_buffer: Default::default(),
            },
            shutdown_signal: Box::pin(pending()),
            consumer: self.inner.consumer.lock().await.take().unwrap(),
        }
    }
}

impl<P, C> Clone for GrpcReporter<P, C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            err_handle: self.err_handle.clone(),
        }
    }
}

impl<P: CollectItemProduce, C: ColletcItemConsume> Report for GrpcReporter<P, C> {
    fn report(&self, item: CollectItem) {
        if !self.inner.is_closed.load(Ordering::Relaxed) {
            if let Err(e) = self.inner.producer.produce(item) {
                if let Some(handle) = self.err_handle.as_deref() {
                    handle(e);
                }
            }
        }
    }
}

struct ReporterAndBuffer<P, C> {
    inner: Arc<Inner<P, C>>,
    status_handle: Option<Box<dyn Fn(tonic::Status) + Send + 'static>>,
    trace_buffer: LinkedList<SegmentObject>,
    log_buffer: LinkedList<LogData>,
    meter_buffer: LinkedList<MeterData>,
}

impl<P: CollectItemProduce, C: ColletcItemConsume> ReporterAndBuffer<P, C> {
    async fn report(&mut self, item: CollectItem) {
        // TODO Implement batch collect in future.
        match item {
            CollectItem::Trace(item) => {
                self.trace_buffer.push_back(*item);
            }
            CollectItem::Log(item) => {
                self.log_buffer.push_back(*item);
            }
            CollectItem::Meter(item) => {
                self.meter_buffer.push_back(*item);
            }
            #[cfg(feature = "management")]
            CollectItem::Instance(item) => {
                if let Err(e) = self
                    .inner
                    .management_client
                    .lock()
                    .await
                    .report_instance_properties(*item)
                    .await
                {
                    if let Some(status_handle) = &self.status_handle {
                        status_handle(e);
                    }
                }
            }
            #[cfg(feature = "management")]
            CollectItem::Ping(item) => {
                if let Err(e) = self
                    .inner
                    .management_client
                    .lock()
                    .await
                    .keep_alive(*item)
                    .await
                {
                    if let Some(status_handle) = &self.status_handle {
                        status_handle(e);
                    }
                }
            }
        }

        if !self.trace_buffer.is_empty() {
            let buffer = take(&mut self.trace_buffer);
            if let Err(e) = self
                .inner
                .trace_client
                .lock()
                .await
                .collect(stream::iter(buffer))
                .await
            {
                if let Some(status_handle) = &self.status_handle {
                    status_handle(e);
                }
            }
        }
        if !self.log_buffer.is_empty() {
            let buffer = take(&mut self.log_buffer);
            if let Err(e) = self
                .inner
                .log_client
                .lock()
                .await
                .collect(stream::iter(buffer))
                .await
            {
                if let Some(status_handle) = &self.status_handle {
                    status_handle(e);
                }
            }
        }

        if !self.meter_buffer.is_empty() {
            let buffer = take(&mut self.meter_buffer);
            if let Err(e) = self
                .inner
                .meter_client
                .lock()
                .await
                .collect(stream::iter(buffer))
                .await
            {
                if let Some(status_handle) = &self.status_handle {
                    status_handle(e);
                }
            }
        }
    }
}

/// Created by [GrpcReporter::reporting].
pub struct Reporting<P, C> {
    rb: ReporterAndBuffer<P, C>,
    consumer: C,
    shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>,
}

impl<P: CollectItemProduce, C: ColletcItemConsume> Reporting<P, C> {
    pub fn with_graceful_shutdown(
        mut self,
        shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    ) -> Self {
        self.shutdown_signal = Box::pin(shutdown_signal);
        self
    }

    pub fn with_staus_handle(mut self, handle: impl Fn(tonic::Status) + Send + 'static) -> Self {
        self.rb.status_handle = Some(Box::new(handle));
        self
    }

    pub fn spawn(self) -> ReportingJoinHandle {
        ReportingJoinHandle {
            handle: tokio::spawn(self.start()),
        }
    }

    pub async fn start(self) -> crate::Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();
        let Reporting {
            mut rb,
            mut consumer,
            shutdown_signal,
        } = self;

        let work_fut = async move {
            loop {
                select! {
                    item = consumer.consume() => {
                        match item {
                            Ok(Some(item)) => {
                                rb.report(item).await;
                            }
                            Ok(None) => break,
                            Err(err) => return Err(crate::Error::Other(err)),
                        }
                    }
                    _ =  shutdown_rx.recv() => break,
                }
            }

            rb.inner.is_closed.store(true, Ordering::Relaxed);

            // Flush.
            loop {
                match consumer.try_consume().await {
                    Ok(Some(item)) => {
                        rb.report(item).await;
                    }
                    Ok(None) => break,
                    Err(err) => return Err(err.into()),
                }
            }

            Ok::<_, crate::Error>(())
        };

        let shutdown_fut = async move {
            shutdown_signal.await;
            shutdown_tx
                .send(())
                .map_err(|e| crate::Error::Other(Box::new(e)))?;
            Ok(())
        };

        try_join!(work_fut, shutdown_fut)?;

        Ok(())
    }
}

pub struct ReportingJoinHandle {
    handle: JoinHandle<crate::Result<()>>,
}

impl Future for ReportingJoinHandle {
    type Output = crate::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.handle).poll(cx).map(|r| r?)
    }
}
