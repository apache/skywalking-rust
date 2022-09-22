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

//! Meter with multiple types.

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
    sync::atomic::{AtomicI64, Ordering},
};

/// Transform to [MeterData].
pub trait Transform: Send + Sync {
    /// Get the meter id.
    fn meter_id(&self) -> MeterId;

    /// Transform to [MeterData].
    fn transform(&self, metricer: &Metricer) -> MeterData;
}

/// Predefine meter type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum MeterType {
    Counter,
    Gauge,
    Histogram,
}

/// Unique meter id for metrics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeterId {
    name: String,
    typ: MeterType,
    labels: Vec<(String, String)>,
}

impl MeterId {
    fn add_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.push((key.into(), value.into()));
        self
    }

    fn add_labels<K, V, I>(mut self, tags: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        self.labels
            .extend(tags.into_iter().map(|(k, v)| (k.into(), v.into())));
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

/// Meter with type `Counter`.
pub struct Counter {
    id: MeterId,
    mode: CounterMode,
    count: AtomicF64,
    previous_count: AtomicF64,
}

impl Counter {
    /// New meter with type `Counter`.
    #[inline]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: MeterId {
                name: name.into(),
                typ: MeterType::Counter,
                labels: vec![],
            },
            mode: CounterMode::INCREMENT,
            count: AtomicF64::new(0.),
            previous_count: AtomicF64::new(0.),
        }
    }

    /// Add label.
    #[inline]
    pub fn add_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.id = self.id.add_label(key, value);
        self
    }

    /// Add labels.
    #[inline]
    pub fn add_labels<K, V, I>(mut self, tags: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        self.id = self.id.add_labels(tags);
        self
    }

    /// Set counter mode.
    #[inline]
    pub fn mode(mut self, mode: CounterMode) -> Self {
        self.mode = mode;
        self
    }

    /// Increment meter value by count.
    pub fn increment(&self, count: f64) {
        self.count.fetch_add(count, Ordering::Acquire);
    }

    /// Get meter value.
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

/// Meter with type `Gauge`.
pub struct Gauge<G> {
    id: MeterId,
    getter: G,
}

impl<G: Fn() -> f64> Gauge<G> {
    /// New meter with type `Gauge` and getter.
    #[inline]
    pub fn new(name: impl Into<String>, getter: G) -> Self {
        Self {
            id: MeterId {
                name: name.into(),
                typ: MeterType::Gauge,
                labels: vec![],
            },
            getter,
        }
    }

    /// Add label.
    #[inline]
    pub fn add_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.id = self.id.add_label(key, value);
        self
    }

    /// Add labels.
    #[inline]
    pub fn add_labels<K, V, I>(mut self, tags: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        self.id = self.id.add_labels(tags);
        self
    }

    /// Get the meter value by calling getter.
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

/// Meter with type `Histogram`.
pub struct Histogram {
    id: MeterId,
    buckets: Vec<Bucket>,
}

impl Histogram {
    /// New meter with type `Histogram`.
    pub fn new(name: impl Into<String>, mut steps: Vec<f64>) -> Self {
        Self {
            id: MeterId {
                name: name.into(),
                typ: MeterType::Histogram,
                labels: vec![],
            },
            buckets: {
                steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Equal));
                steps.dedup();
                steps.into_iter().map(Bucket::new).collect()
            },
        }
    }

    /// Add label.
    #[inline]
    pub fn add_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.id = self.id.add_label(key, value);
        self
    }

    /// Add labels.
    #[inline]
    pub fn add_labels<K, V, I>(mut self, tags: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        self.id = self.id.add_labels(tags);
        self
    }

    /// Increment meter value by the bucket owned the value.
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
