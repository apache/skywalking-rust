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
use async_stream::stream;
use futures_core::Stream;
use futures_util::future::{try_join_all, TryJoinAll};
use std::{
    error::Error,
    future::{pending, Future},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
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

type DynInterceptHandler = dyn Fn(Request<()>) -> Result<Request<()>, Status> + Send + Sync;

#[derive(Default, Clone)]
struct CustomInterceptor {
    authentication: Option<Arc<String>>,
    custom_intercept: Option<Arc<DynInterceptHandler>>,
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

struct State {
    is_reporting: AtomicBool,
    is_closed: AtomicBool,
}

/// Alias of dyn [Error] callback.
pub type DynErrHandle = dyn Fn(Box<dyn Error>) + Send + Sync + 'static;

/// Reporter which will report to Skywalking OAP server via grpc protocol.
pub struct GrpcReporter<P, C> {
    state: Arc<State>,
    producer: Arc<P>,
    consumer: Arc<Mutex<Option<C>>>,
    err_handle: Option<Arc<DynErrHandle>>,
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
            state: Arc::new(State {
                is_reporting: Default::default(),
                is_closed: Default::default(),
            }),
            producer: Arc::new(producer),
            consumer: Arc::new(Mutex::new(Some(consumer))),
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
        self.err_handle = Some(Arc::new(handle));
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
    pub async fn reporting(&self) -> Reporting<C> {
        if self.state.is_reporting.swap(true, Ordering::Relaxed) {
            panic!("reporting already called");
        }

        let (trace_sender, trace_receiver) = mpsc::channel(255);
        let (log_sender, log_receiver) = mpsc::channel(255);
        let (meter_sender, meter_receiver) = mpsc::channel(255);

        Reporting {
            report_sender: ReportSender {
                state: Arc::clone(&self.state),
                inner_report_sender: InnerReportSender {
                    status_handle: None,
                    err_handle: self.err_handle.clone(),
                    trace_sender,
                    log_sender,
                    meter_sender,

                    #[cfg(feature = "management")]
                    management_client: ManagementServiceClient::with_interceptor(
                        self.channel.clone(),
                        self.interceptor.clone(),
                    ),
                },
                shutdown_signal: Box::pin(pending()),
                consumer: self.consumer.lock().await.take().unwrap(),
            },

            trace_receive_reporter: TraceReceiveReporter {
                trace_client: TraceSegmentReportServiceClient::with_interceptor(
                    self.channel.clone(),
                    self.interceptor.clone(),
                ),
                trace_receiver,
                status_handle: None,
            },

            log_receive_reporter: LogReceiveReporter {
                log_client: LogReportServiceClient::with_interceptor(
                    self.channel.clone(),
                    self.interceptor.clone(),
                ),
                log_receiver,
                status_handle: None,
            },

            meter_receive_reporter: MeterReceiveReporter {
                meter_client: MeterReportServiceClient::with_interceptor(
                    self.channel.clone(),
                    self.interceptor.clone(),
                ),
                meter_receiver,
                status_handle: None,
            },
        }
    }
}

impl<P, C> Clone for GrpcReporter<P, C> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            producer: self.producer.clone(),
            consumer: self.consumer.clone(),
            err_handle: self.err_handle.clone(),
            channel: self.channel.clone(),
            interceptor: self.interceptor.clone(),
        }
    }
}

impl<P: CollectItemProduce, C: CollectItemConsume> Report for GrpcReporter<P, C> {
    fn report(&self, item: CollectItem) {
        if !self.state.is_closed.load(Ordering::Relaxed) {
            if let Err(e) = self.producer.produce(item) {
                if let Some(handle) = self.err_handle.as_deref() {
                    handle(e);
                }
            }
        }
    }
}

struct InnerReportSender {
    status_handle: Option<Arc<dyn Fn(tonic::Status) + Send + Sync + 'static>>,
    err_handle: Option<Arc<DynErrHandle>>,

    trace_sender: Sender<SegmentObject>,
    log_sender: Sender<LogData>,
    meter_sender: Sender<MeterData>,

    #[cfg(feature = "management")]
    management_client: ManagementServiceClient<InterceptedService<Channel, CustomInterceptor>>,
}

impl InnerReportSender {
    async fn report(&mut self, item: CollectItem) {
        match item {
            CollectItem::Trace(item) => {
                if let Err(e) = self.trace_sender.try_send(*item) {
                    if let Some(err_handle) = self.err_handle.as_deref() {
                        err_handle(Box::new(e));
                    }
                }
            }
            CollectItem::Log(item) => {
                if let Err(e) = self.log_sender.try_send(*item) {
                    if let Some(err_handle) = self.err_handle.as_deref() {
                        err_handle(Box::new(e));
                    }
                }
            }
            CollectItem::Meter(item) => {
                if let Err(e) = self.meter_sender.try_send(*item) {
                    if let Some(err_handle) = self.err_handle.as_deref() {
                        err_handle(Box::new(e));
                    }
                }
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
    }
}

struct ReportSender<C> {
    state: Arc<State>,
    inner_report_sender: InnerReportSender,
    consumer: C,
    shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>,
}

impl<C: CollectItemConsume> ReportSender<C> {
    async fn start(self) -> crate::Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();
        let ReportSender {
            state,
            mut inner_report_sender,
            consumer: mut collect_item_consumer,
            shutdown_signal,
            ..
        } = self;

        let work_fut = async move {
            loop {
                select! {
                    item = collect_item_consumer.consume() => {
                        match item {
                            Ok(Some(item)) => {
                                inner_report_sender.report(item).await;
                            }
                            Ok(None) => break,
                            Err(err) => return Err(crate::Error::Other(err)),
                        }
                    }
                    _ =  shutdown_rx.recv() => break,
                }
            }

            state.is_closed.store(true, Ordering::Relaxed);

            // Flush.
            loop {
                match collect_item_consumer.try_consume().await {
                    Ok(Some(item)) => {
                        inner_report_sender.report(item).await;
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

/// Handle of [GrpcReporter::reporting].
pub struct Reporting<C> {
    report_sender: ReportSender<C>,
    trace_receive_reporter: TraceReceiveReporter,
    log_receive_reporter: LogReceiveReporter,
    meter_receive_reporter: MeterReceiveReporter,
}

impl<C: CollectItemConsume> Reporting<C> {
    /// Quit when shutdown_signal received.
    ///
    /// Accept a `shutdown_signal` argument as a graceful shutdown signal.
    pub fn with_graceful_shutdown(
        mut self,
        shutdown_signal: impl Future<Output = ()> + Send + Sync + 'static,
    ) -> Self {
        self.report_sender.shutdown_signal = Box::pin(shutdown_signal);
        self
    }

    /// Set the failed status handle. By default, the status will not be handle.
    pub fn with_status_handle(
        mut self,
        handle: impl Fn(tonic::Status) + Send + Sync + 'static,
    ) -> Self {
        let handle = Arc::new(handle);
        self.report_sender.inner_report_sender.status_handle = Some(handle.clone());
        self.trace_receive_reporter.status_handle = Some(handle.clone());
        self.log_receive_reporter.status_handle = Some(handle.clone());
        self.meter_receive_reporter.status_handle = Some(handle);
        self
    }

    /// Spawn the reporting in background.
    pub fn spawn(self) -> ReportingJoinHandle {
        ReportingJoinHandle {
            handles: try_join_all(vec![
                tokio::spawn(self.report_sender.start()),
                tokio::spawn(self.trace_receive_reporter.start()),
                tokio::spawn(self.log_receive_reporter.start()),
                tokio::spawn(self.meter_receive_reporter.start()),
            ]),
        }
    }
}

/// Handle of [Reporting::spawn].
pub struct ReportingJoinHandle {
    handles: TryJoinAll<JoinHandle<crate::Result<()>>>,
}

impl Future for ReportingJoinHandle {
    type Output = crate::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.handles).poll(cx).map(|rs| {
            let rs = rs?;
            for r in rs {
                r?;
            }
            Ok(())
        })
    }
}

struct TraceReceiveReporter {
    trace_client: TraceSegmentReportServiceClient<InterceptedService<Channel, CustomInterceptor>>,
    trace_receiver: Receiver<SegmentObject>,
    status_handle: Option<Arc<dyn Fn(tonic::Status) + Send + Sync + 'static>>,
}

impl TraceReceiveReporter {
    async fn start(self) -> crate::Result<()> {
        let TraceReceiveReporter {
            mut trace_client,
            trace_receiver,
            status_handle,
        } = self;

        let stream = receive_report(trace_receiver);
        if let Err(e) = trace_client.collect(stream).await {
            if let Some(status_handle) = status_handle {
                status_handle(e);
            }
        }

        Ok(())
    }
}

struct LogReceiveReporter {
    log_client: LogReportServiceClient<InterceptedService<Channel, CustomInterceptor>>,
    log_receiver: Receiver<LogData>,
    status_handle: Option<Arc<dyn Fn(tonic::Status) + Send + Sync + 'static>>,
}

impl LogReceiveReporter {
    async fn start(self) -> crate::Result<()> {
        let LogReceiveReporter {
            mut log_client,
            log_receiver,
            status_handle,
        } = self;

        let stream = receive_report(log_receiver);
        if let Err(e) = log_client.collect(stream).await {
            if let Some(status_handle) = status_handle {
                status_handle(e);
            }
        }

        Ok(())
    }
}

struct MeterReceiveReporter {
    meter_client: MeterReportServiceClient<InterceptedService<Channel, CustomInterceptor>>,
    meter_receiver: Receiver<MeterData>,
    status_handle: Option<Arc<dyn Fn(tonic::Status) + Send + Sync + 'static>>,
}

impl MeterReceiveReporter {
    async fn start(self) -> crate::Result<()> {
        let MeterReceiveReporter {
            mut meter_client,
            meter_receiver,
            status_handle,
        } = self;

        let stream = receive_report(meter_receiver);
        if let Err(e) = meter_client.collect(stream).await {
            if let Some(status_handle) = status_handle {
                status_handle(e);
            }
        }

        Ok(())
    }
}

fn receive_report<I>(mut receiver: Receiver<I>) -> impl Stream<Item = I> {
    stream! {
        loop {
            match receiver.recv().await {
                Some(item) => yield item,
                None => break,
            }
        }
    }
}
