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
    metrics::meter::{IntoMeterDataWithMeter, Meter},
    skywalking_proto::v3::{
        meter_data::Metric, Label, MeterBucketValue, MeterData, MeterHistogram, MeterSingleValue,
    },
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub struct MeterRecord {
    time: Option<SystemTime>,
}

impl MeterRecord {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    #[inline]
    pub fn custome_time(mut self, time: SystemTime) -> Self {
        self.time = Some(time);
        self
    }

    #[inline]
    pub fn single_value(self) -> SingleValue {
        SingleValue {
            record: self,
            ..Default::default()
        }
    }

    #[inline]
    pub fn histogram(self) -> Histogram {
        Histogram {
            record: self,
            ..Default::default()
        }
    }
}

impl IntoMeterDataWithMeter for MeterRecord {
    fn into_meter_data_with_meter(self, meter: &Meter) -> MeterData {
        MeterData {
            service: meter.service_name().to_owned(),
            service_instance: meter.instance_name().to_owned(),
            timestamp: match self.time {
                Some(time) => time
                    .duration_since(UNIX_EPOCH)
                    .map(|dur| dur.as_millis() as i64)
                    .unwrap_or_default(),
                None => fetch_time(TimePeriod::Metric),
            },
            metric: None,
        }
    }
}

#[derive(Default)]
pub struct SingleValue {
    record: MeterRecord,
    name: String,
    labels: Vec<(String, String)>,
    value: f64,
}

impl SingleValue {
    #[inline]
    pub fn name(mut self, name: impl ToString) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn add_label(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.labels.push((key.to_string(), value.to_string()));
        self
    }

    pub fn add_labels<K, V, I>(mut self, tags: I) -> Self
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

    #[inline]
    pub fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }
}

impl IntoMeterDataWithMeter for SingleValue {
    fn into_meter_data_with_meter(self, meter: &Meter) -> MeterData {
        MeterData {
            service: meter.service_name().to_owned(),
            service_instance: meter.instance_name().to_owned(),
            timestamp: match self.record.time {
                Some(time) => time
                    .duration_since(UNIX_EPOCH)
                    .map(|dur| dur.as_millis() as i64)
                    .unwrap_or_default(),
                None => fetch_time(TimePeriod::Metric),
            },
            metric: Some(Metric::SingleValue(MeterSingleValue {
                name: self.name,
                labels: self
                    .labels
                    .into_iter()
                    .map(|(name, value)| Label { name, value })
                    .collect(),
                value: self.value,
            })),
        }
    }
}

#[derive(Default)]
pub struct Histogram {
    record: MeterRecord,
    name: String,
    labels: Vec<(String, String)>,
    values: Vec<(f64, i64, bool)>,
}

impl Histogram {
    #[inline]
    pub fn name(mut self, name: impl ToString) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn add_label(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.labels.push((key.to_string(), value.to_string()));
        self
    }

    pub fn add_labels<K, V, I>(mut self, tags: I) -> Self
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

    #[inline]
    pub fn add_value(mut self, bucket: f64, count: i64, is_negative_infinity: bool) -> Self {
        self.values.push((bucket, count, is_negative_infinity));
        self
    }
}

impl IntoMeterDataWithMeter for Histogram {
    fn into_meter_data_with_meter(self, meter: &Meter) -> MeterData {
        MeterData {
            service: meter.service_name().to_owned(),
            service_instance: meter.instance_name().to_owned(),
            timestamp: match self.record.time {
                Some(time) => time
                    .duration_since(UNIX_EPOCH)
                    .map(|dur| dur.as_millis() as i64)
                    .unwrap_or_default(),
                None => fetch_time(TimePeriod::Metric),
            },
            metric: Some(Metric::Histogram(MeterHistogram {
                name: self.name,
                labels: self
                    .labels
                    .into_iter()
                    .map(|(name, value)| Label { name, value })
                    .collect(),
                values: self
                    .values
                    .into_iter()
                    .map(|(bucket, count, is_negative_infinity)| MeterBucketValue {
                        bucket,
                        count,
                        is_negative_infinity,
                    })
                    .collect(),
            })),
        }
    }
}
