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

//! Grpc implementation of [Report].

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
    metadata::{Ascii, MetadataValue},
    service::{interceptor::InterceptedService, Interceptor},
    transport::{self, Channel, Endpoint},
    Request, Status,
};

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

impl CollectItemProduce for mpsc::UnboundedSender<CollectItem> {
    fn produce(&self, item: CollectItem) -> Result<(), Box<dyn Error>> {
        Ok(self.send(item)?)
    }
}

/// Special purpose, used for user-defined consume operations. Generally, it
/// does not need to be handled.
#[async_trait]
pub trait CollectItemConsume: Send + Sync + 'static {
    /// Consume the collect item blocking.
    async fn consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>>;

    /// Try to consume the collect item non-blocking.
    async fn try_consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>>;
}

#[async_trait]
impl CollectItemConsume for () {
    async fn consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>> {
        Ok(None)
    }

    async fn try_consume(&mut self) -> Result<Option<CollectItem>, Box<dyn Error + Send>> {
        Ok(None)
    }
}

#[async_trait]
impl CollectItemConsume for mpsc::UnboundedReceiver<CollectItem> {
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

#[derive(Default, Clone)]
struct CustomInterceptor {
    authentication: Option<Arc<String>>,
    custom_intercept: Option<Arc<dyn Fn(Request<()>) -> Result<Request<()>, Status> + Send + Sync>>,
}

impl Interceptor for CustomInterceptor {
    fn call(&mut self, mut request: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
        if let Some(authentication) = &self.authentication {
            if let Ok(authentication) = authentication.parse::<MetadataValue<Ascii>>() {
                request
                    .metadata_mut()
                    .insert("authentication", authentication);
            }
        }
        if let Some(custom_intercept) = &self.custom_intercept {
            request = custom_intercept(request)?;
        }
        Ok(request)
    }
}

struct Inner<P, C> {
    producer: P,
    consumer: Mutex<Option<C>>,
    is_reporting: AtomicBool,
    is_closed: AtomicBool,
}

/// Alias of dyn [Error] callback.
pub type DynErrHandle = dyn Fn(Box<dyn Error>) + Send + Sync + 'static;

/// Reporter which will report to Skywalking OAP server via grpc protocol.
pub struct GrpcReporter<P, C> {
    inner: Arc<Inner<P, C>>,
    err_handle: Arc<Option<Box<DynErrHandle>>>,
    channel: Channel,
    interceptor: CustomInterceptor,
}

impl GrpcReporter<mpsc::UnboundedSender<CollectItem>, mpsc::UnboundedReceiver<CollectItem>> {
    /// New with exists [Channel], so you can clone the [Channel] for multiplex.
    pub fn new(channel: Channel) -> Self {
        let (p, c) = mpsc::unbounded_channel();
        Self::new_with_pc(channel, p, c)
    }

    /// Connect to the Skywalking OAP server.
    pub async fn connect(
        address: impl TryInto<Endpoint, Error = transport::Error>,
    ) -> crate::Result<Self> {
        let endpoint = address.try_into()?;
        let channel = endpoint.connect().await?;
        Ok(Self::new(channel))
    }
}

impl<P: CollectItemProduce, C: CollectItemConsume> GrpcReporter<P, C> {
    /// Special purpose, used for user-defined produce and consume operations,
    /// usually you can use [GrpcReporter::connect] and [GrpcReporter::new].
    pub fn new_with_pc(channel: Channel, producer: P, consumer: C) -> Self {
        Self {
            inner: Arc::new(Inner {
                producer,
                consumer: Mutex::new(Some(consumer)),
                is_reporting: Default::default(),
                is_closed: Default::default(),
            }),
            err_handle: Default::default(),
            channel,
            interceptor: Default::default(),
        }
    }

    /// Set error handle. By default, the error will not be handle.
    pub fn with_err_handle(
        mut self,
        handle: impl Fn(Box<dyn Error>) + Send + Sync + 'static,
    ) -> Self {
        self.err_handle = Arc::new(Some(Box::new(handle)));
        self
    }

    /// Set the authentication header value. By default, the authentication is
    /// not set.
    pub fn with_authentication(mut self, authentication: impl Into<String>) -> Self {
        self.interceptor.authentication = Some(Arc::new(authentication.into()));
        self
    }

    /// Set the custom intercept. By default, the custom intercept is not set.
    pub fn with_custom_intercept(
        mut self,
        custom_intercept: impl Fn(Request<()>) -> Result<Request<()>, Status> + Send + Sync + 'static,
    ) -> Self {
        self.interceptor.custom_intercept = Some(Arc::new(custom_intercept));
        self
    }

    /// Start to reporting.
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

                trace_client: TraceSegmentReportServiceClient::with_interceptor(
                    self.channel.clone(),
                    self.interceptor.clone(),
                ),
                log_client: LogReportServiceClient::with_interceptor(
                    self.channel.clone(),
                    self.interceptor.clone(),
                ),
                meter_client: MeterReportServiceClient::with_interceptor(
                    self.channel.clone(),
                    self.interceptor.clone(),
                ),
                #[cfg(feature = "management")]
                management_client: ManagementServiceClient::with_interceptor(
                    self.channel.clone(),
                    self.interceptor.clone(),
                ),
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
            channel: self.channel.clone(),
            interceptor: self.interceptor.clone(),
        }
    }
}

impl<P: CollectItemProduce, C: CollectItemConsume> Report for GrpcReporter<P, C> {
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

    trace_client: TraceSegmentReportServiceClient<InterceptedService<Channel, CustomInterceptor>>,
    log_client: LogReportServiceClient<InterceptedService<Channel, CustomInterceptor>>,
    meter_client: MeterReportServiceClient<InterceptedService<Channel, CustomInterceptor>>,
    #[cfg(feature = "management")]
    #[cfg_attr(docsrs, doc(cfg(feature = "management")))]
    management_client: ManagementServiceClient<InterceptedService<Channel, CustomInterceptor>>,
}

impl<P: CollectItemProduce, C: CollectItemConsume> ReporterAndBuffer<P, C> {
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
                    .management_client
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
                if let Err(e) = self.management_client.keep_alive(*item).await {
                    if let Some(status_handle) = &self.status_handle {
                        status_handle(e);
                    }
                }
            }
        }

        if !self.trace_buffer.is_empty() {
            let buffer = take(&mut self.trace_buffer);
            if let Err(e) = self.trace_client.collect(stream::iter(buffer)).await {
                if let Some(status_handle) = &self.status_handle {
                    status_handle(e);
                }
            }
        }
        if !self.log_buffer.is_empty() {
            let buffer = take(&mut self.log_buffer);
            if let Err(e) = self.log_client.collect(stream::iter(buffer)).await {
                if let Some(status_handle) = &self.status_handle {
                    status_handle(e);
                }
            }
        }

        if !self.meter_buffer.is_empty() {
            let buffer = take(&mut self.meter_buffer);
            if let Err(e) = self.meter_client.collect(stream::iter(buffer)).await {
                if let Some(status_handle) = &self.status_handle {
                    status_handle(e);
                }
            }
        }
    }
}

/// Handle of [GrpcReporter::reporting].
pub struct Reporting<P, C> {
    rb: ReporterAndBuffer<P, C>,
    consumer: C,
    shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>,
}

impl<P: CollectItemProduce, C: CollectItemConsume> Reporting<P, C> {
    /// Quit when shutdown_signal received.
    ///
    /// Accept a `shutdown_signal` argument as a graceful shutdown signal.
    pub fn with_graceful_shutdown(
        mut self,
        shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    ) -> Self {
        self.shutdown_signal = Box::pin(shutdown_signal);
        self
    }

    /// Set the failed status handle. By default, the status will not be handle.
    pub fn with_status_handle(mut self, handle: impl Fn(tonic::Status) + Send + 'static) -> Self {
        self.rb.status_handle = Some(Box::new(handle));
        self
    }

    /// Spawn the reporting in background.
    pub fn spawn(self) -> ReportingJoinHandle {
        ReportingJoinHandle {
            handle: tokio::spawn(self.start()),
        }
    }

    /// Start the consume and report task.
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

/// Handle of [Reporting::spawn].
pub struct ReportingJoinHandle {
    handle: JoinHandle<crate::Result<()>>,
}

impl Future for ReportingJoinHandle {
    type Output = crate::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.handle).poll(cx).map(|r| r?)
    }
}
