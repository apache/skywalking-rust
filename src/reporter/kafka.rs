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

//! Kafka implementation of [Report].

use super::{CollectItemConsume, CollectItemProduce};
use crate::reporter::{CollectItem, Report};
pub use rdkafka::config::{ClientConfig as RDKafkaClientConfig, RDKafkaLogLevel};
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::{
    error,
    future::{pending, Future},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        Arc,
    },
    time::Duration,
};
use tokio::{select, spawn, sync::mpsc, task::JoinHandle, try_join};
use tracing::error;

/// Kafka reporter error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// ksKafka error.
    #[error(transparent)]
    RdKafka(#[from] rdkafka::error::KafkaError),

    /// kafka topic not found
    #[error("topic not found: {topic}")]
    TopicNotFound {
        /// Name of kafka topic.
        topic: String,
    },
}

type DynErrHandler = dyn Fn(&str, &dyn error::Error) + Send + Sync + 'static;

fn default_err_handle(message: &str, err: &dyn error::Error) {
    error!(?err, "{}", message);
}

#[derive(Default)]
struct State {
    is_closing: AtomicBool,
}

impl State {
    fn is_closing(&self) -> bool {
        self.is_closing.load(Relaxed)
    }
}

/// Reporter which just print the collect items, not actual report to server,
/// for debug usage.
pub struct KafkaReportBuilder<P, C> {
    state: Arc<State>,
    producer: Arc<P>,
    consumer: C,
    client_config: RDKafkaClientConfig,
    namespace: Option<String>,
    err_handle: Arc<DynErrHandler>,
}

impl KafkaReportBuilder<mpsc::UnboundedSender<CollectItem>, mpsc::UnboundedReceiver<CollectItem>> {
    /// Create builder, with rdkafka future.
    pub fn new(client_config: RDKafkaClientConfig) -> Self {
        let (producer, consumer) = mpsc::unbounded_channel();
        Self::new_with_pc(client_config, producer, consumer)
    }
}

impl<P: CollectItemProduce, C: CollectItemConsume> KafkaReportBuilder<P, C> {
    /// Special purpose, used for user-defined produce and consume operations,
    /// usually you can use [KafkaReportBuilder::new].
    pub fn new_with_pc(client_config: RDKafkaClientConfig, producer: P, consumer: C) -> Self {
        Self {
            state: Default::default(),
            producer: Arc::new(producer),
            consumer,
            client_config,
            namespace: None,
            err_handle: Arc::new(default_err_handle),
        }
    }

    /// Set error handle. By default, the error will be logged.
    pub fn with_err_handle(
        mut self,
        handle: impl Fn(&str, &dyn error::Error) + Send + Sync + 'static,
    ) -> Self {
        self.err_handle = Arc::new(handle);
        self
    }

    /// Use to isolate multi OAP server when using same Kafka cluster (final
    /// topic name will append namespace before Kafka topics with - ).
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Build the Reporter implemented [Report] in the foreground, and the
    /// handle to push data to kafka in the background.
    pub async fn build(self) -> Result<(KafkaReporter<P>, KafkaReporting<C>), Error> {
        let kafka_producer = KafkaProducer::new(
            self.client_config.create()?,
            self.err_handle.clone(),
            self.namespace,
        )
        .await?;
        Ok((
            KafkaReporter {
                state: self.state.clone(),
                producer: self.producer,
                err_handle: self.err_handle,
            },
            KafkaReporting {
                state: self.state,
                consumer: self.consumer,
                kafka_producer,
                shutdown_signal: Box::pin(pending()),
            },
        ))
    }
}

/// The kafka reporter implemented [Report].
pub struct KafkaReporter<P> {
    state: Arc<State>,
    producer: Arc<P>,
    err_handle: Arc<DynErrHandler>,
}

impl<P> Clone for KafkaReporter<P> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            producer: self.producer.clone(),
            err_handle: self.err_handle.clone(),
        }
    }
}

impl<P: CollectItemProduce> Report for KafkaReporter<P> {
    fn report(&self, item: CollectItem) {
        if !self.state.is_closing() {
            if let Err(e) = self.producer.produce(item) {
                (self.err_handle)("report collect item failed", &*e);
            }
        }
    }
}

/// The handle to push data to kafka.
pub struct KafkaReporting<C> {
    state: Arc<State>,
    consumer: C,
    kafka_producer: KafkaProducer,
    shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>,
}

impl<C: CollectItemConsume> KafkaReporting<C> {
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

    /// Spawn the reporting in background.
    pub fn spawn(self) -> ReportingJoinHandle {
        let handle = spawn(async move {
            let KafkaReporting {
                state,
                mut consumer,
                mut kafka_producer,
                shutdown_signal,
            } = self;

            let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();

            let work_fut = async move {
                loop {
                    select! {
                        item = consumer.consume() => {
                            match item {
                                Ok(Some(item)) => {
                                    kafka_producer.produce(item).await;
                                }
                                Ok(None) => break,
                                Err(err) => return Err(crate::Error::Other(err)),
                            }
                        }
                        _ =  shutdown_rx.recv() => break,
                    }
                }

                state.is_closing.store(true, Relaxed);

                // Flush.
                loop {
                    match consumer.try_consume().await {
                        Ok(Some(item)) => {
                            kafka_producer.produce(item).await;
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
        });
        ReportingJoinHandle { handle }
    }
}

/// Handle of [KafkaReporting::spawn].
pub struct ReportingJoinHandle {
    handle: JoinHandle<crate::Result<()>>,
}

impl Future for ReportingJoinHandle {
    type Output = crate::Result<()>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        Pin::new(&mut self.handle).poll(cx).map(|rs| rs?)
    }
}

struct TopicNames {
    segment: String,
    meter: String,
    log: String,
    #[cfg(feature = "management")]
    management: String,
}

impl TopicNames {
    const TOPIC_LOG: &str = "skywalking-logs";
    #[cfg(feature = "management")]
    const TOPIC_MANAGEMENT: &str = "skywalking-managements";
    const TOPIC_METER: &str = "skywalking-meters";
    const TOPIC_SEGMENT: &str = "skywalking-segments";

    fn new(namespace: Option<&str>) -> Self {
        Self {
            segment: Self::real_topic_name(namespace, Self::TOPIC_SEGMENT),
            meter: Self::real_topic_name(namespace, Self::TOPIC_METER),
            log: Self::real_topic_name(namespace, Self::TOPIC_LOG),
            #[cfg(feature = "management")]
            management: Self::real_topic_name(namespace, Self::TOPIC_MANAGEMENT),
        }
    }

    fn real_topic_name(namespace: Option<&str>, topic_name: &str) -> String {
        namespace
            .map(|namespace| format!("{}-{}", namespace, topic_name))
            .unwrap_or_else(|| topic_name.to_string())
    }
}

struct KafkaProducer {
    topic_names: TopicNames,
    client: FutureProducer,
    err_handle: Arc<DynErrHandler>,
}

impl KafkaProducer {
    async fn new(
        client: FutureProducer,
        err_handle: Arc<DynErrHandler>,
        namespace: Option<String>,
    ) -> Result<Self, Error> {
        let topic_names = TopicNames::new(namespace.as_deref());
        Ok(Self {
            client,
            err_handle,
            topic_names,
        })
    }

    async fn produce(&mut self, item: CollectItem) {
        let (topic_name, key) = match &item {
            CollectItem::Trace(item) => (
                &self.topic_names.segment,
                item.trace_segment_id.as_bytes().to_vec(),
            ),
            CollectItem::Log(item) => (&self.topic_names.log, item.service.as_bytes().to_vec()),
            CollectItem::Meter(item) => (
                &self.topic_names.meter,
                item.service_instance.as_bytes().to_vec(),
            ),
            #[cfg(feature = "management")]
            CollectItem::Instance(item) => (
                &self.topic_names.management,
                format!("register-{}", &item.service_instance).into_bytes(),
            ),
            #[cfg(feature = "management")]
            CollectItem::Ping(item) => (
                &self.topic_names.log,
                item.service_instance.as_bytes().to_vec(),
            ),
        };

        let payload = item.encode_to_vec();
        let record = FutureRecord::to(topic_name).payload(&payload).key(&key);

        if let Err((err, _)) = self.client.send(record, Duration::from_secs(0)).await {
            (self.err_handle)("Collect data to kafka failed", &err);
        }
    }
}
