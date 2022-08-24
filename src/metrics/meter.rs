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
    common::system_time::{fetch_time, TimePeriod},
    metrics::metricer::Metricer,
    skywalking_proto::v3::{
        meter_data::Metric, Label, MeterBucketValue, MeterData, MeterHistogram, MeterSingleValue,
    },
};
use portable_atomic::AtomicF64;
use std::{
    cmp::Ordering::Equal,
    collections::HashSet,
    sync::atomic::{self, AtomicI64, AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

pub trait Transform: Send + Sync {
    fn meter_id(&self) -> MeterId;

    fn transform(&self, metricer: &Metricer) -> MeterData;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum MeterType {
    COUNTER,
    GAUGE,
    HISTOGRAM,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeterId {
    name: String,
    typ: MeterType,
    labels: Vec<(String, String)>,
}

impl MeterId {
    fn add_label(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.labels.push((key.to_string(), value.to_string()));
        self
    }

    fn add_labels<K, V, I>(mut self, tags: I) -> Self
    where
        K: ToString,
        V: ToString,
        I: IntoIterator<Item = (K, V)>,
    {
        self.labels.extend(
            tags.into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        self
    }
}

/// Counter mode.
pub enum CounterMode {
    /// INCREMENT mode represents reporting the latest value.
    INCREMENT,

    /// RATE mode represents reporting the increment rate. Value = latest value
    /// - last reported value.
    RATE,
}

pub struct Counter {
    id: MeterId,
    mode: CounterMode,
    count: AtomicF64,
    previous_count: AtomicF64,
}

impl Counter {
    #[inline]
    pub fn new(name: impl ToString) -> Self {
        Self {
            id: MeterId {
                name: name.to_string(),
                typ: MeterType::COUNTER,
                labels: vec![],
            },
            mode: CounterMode::INCREMENT,
            count: AtomicF64::new(0.),
            previous_count: AtomicF64::new(0.),
        }
    }

    #[inline]
    pub fn add_label(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.id = self.id.add_label(key, value);
        self
    }

    #[inline]
    pub fn add_labels<K, V, I>(mut self, tags: I) -> Self
    where
        K: ToString,
        V: ToString,
        I: IntoIterator<Item = (K, V)>,
    {
        self.id = self.id.add_labels(tags);
        self
    }

    #[inline]
    pub fn mode(mut self, mode: CounterMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn increment(&self, count: f64) {
        self.count.fetch_add(count, Ordering::Acquire);
    }

    pub fn get(&self) -> f64 {
        self.count.load(Ordering::Acquire)
    }
}

impl Transform for Counter {
    fn meter_id(&self) -> MeterId {
        self.id.clone()
    }

    fn transform(&self, metricer: &Metricer) -> MeterData {
        MeterData {
            service: metricer.service_name().to_owned(),
            service_instance: metricer.instance_name().to_owned(),
            timestamp: fetch_time(TimePeriod::Metric),
            metric: Some(Metric::SingleValue(MeterSingleValue {
                name: self.id.name.to_owned(),
                labels: self
                    .id
                    .labels
                    .iter()
                    .map(|(name, value)| Label {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
                value: match self.mode {
                    CounterMode::INCREMENT => self.get(),
                    CounterMode::RATE => {
                        let current_count = self.get();
                        let previous_count =
                            self.previous_count.swap(current_count, Ordering::Acquire);
                        current_count - previous_count
                    }
                },
            })),
        }
    }
}

pub struct Gauge<G> {
    id: MeterId,
    getter: G,
}

impl<G: Fn() -> f64> Gauge<G> {
    #[inline]
    pub fn new(name: impl ToString, getter: G) -> Self {
        Self {
            id: MeterId {
                name: name.to_string(),
                typ: MeterType::GAUGE,
                labels: vec![],
            },
            getter,
        }
    }

    #[inline]
    pub fn add_label(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.id = self.id.add_label(key, value);
        self
    }

    #[inline]
    pub fn add_labels<K, V, I>(mut self, tags: I) -> Self
    where
        K: ToString,
        V: ToString,
        I: IntoIterator<Item = (K, V)>,
    {
        self.id = self.id.add_labels(tags);
        self
    }

    pub fn get(&self) -> f64 {
        (self.getter)()
    }
}

impl<G: Fn() -> f64 + Send + Sync> Transform for Gauge<G> {
    fn meter_id(&self) -> MeterId {
        self.id.clone()
    }

    fn transform(&self, metricer: &Metricer) -> MeterData {
        MeterData {
            service: metricer.service_name().to_owned(),
            service_instance: metricer.instance_name().to_owned(),
            timestamp: fetch_time(TimePeriod::Metric),
            metric: Some(Metric::SingleValue(MeterSingleValue {
                name: self.id.name.to_owned(),
                labels: self
                    .id
                    .labels
                    .iter()
                    .map(|(name, value)| Label {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
                value: self.get(),
            })),
        }
    }
}

struct Bucket {
    bucket: f64,
    count: AtomicI64,
}

impl Bucket {
    fn new(bucker: f64) -> Self {
        Self {
            bucket: bucker,
            count: Default::default(),
        }
    }
}

pub struct Histogram {
    id: MeterId,
    buckets: Vec<Bucket>,
}

impl Histogram {
    pub fn new(name: impl ToString, mut steps: Vec<f64>) -> Self {
        Self {
            id: MeterId {
                name: name.to_string(),
                typ: MeterType::HISTOGRAM,
                labels: vec![],
            },
            buckets: {
                steps.sort_by(|a, b| a.partial_cmp(&b).unwrap_or(Equal));
                steps.dedup();
                steps.into_iter().map(Bucket::new).collect()
            },
        }
    }

    #[inline]
    pub fn add_label(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.id = self.id.add_label(key, value);
        self
    }

    #[inline]
    pub fn add_labels<K, V, I>(mut self, tags: I) -> Self
    where
        K: ToString,
        V: ToString,
        I: IntoIterator<Item = (K, V)>,
    {
        self.id = self.id.add_labels(tags);
        self
    }

    pub fn add_value(&self, value: f64) {
        if let Some(index) = self.find_bucket(value) {
            self.buckets[index].count.fetch_add(1, Ordering::Acquire);
        }
    }

    fn find_bucket(&self, value: f64) -> Option<usize> {
        match self
            .buckets
            .binary_search_by(|bucket| bucket.bucket.partial_cmp(&value).unwrap_or(Equal))
        {
            Ok(i) => Some(i),
            Err(i) => {
                if i >= 1 {
                    Some(i - 1)
                } else {
                    None
                }
            }
        }
    }
}

impl Transform for Histogram {
    fn meter_id(&self) -> MeterId {
        self.id.clone()
    }

    fn transform(&self, metricer: &Metricer) -> MeterData {
        MeterData {
            service: metricer.service_name().to_owned(),
            service_instance: metricer.instance_name().to_owned(),
            timestamp: fetch_time(TimePeriod::Metric),
            metric: Some(Metric::Histogram(MeterHistogram {
                name: self.id.name.to_owned(),
                labels: self
                    .id
                    .labels
                    .iter()
                    .map(|(name, value)| Label {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect(),
                values: self
                    .buckets
                    .iter()
                    .map(|bucket| MeterBucketValue {
                        bucket: bucket.bucket,
                        count: bucket.count.load(Ordering::Acquire),
                        is_negative_infinity: false,
                    })
                    .collect(),
            })),
        }
    }
}
