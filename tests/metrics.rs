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

use skywalking::{
    metrics::{meter::Meter, record::MeterRecord},
    reporter::{CollectItem, Report},
    skywalking_proto::v3::{
        meter_data::Metric, Label, MeterBucketValue, MeterData, MeterHistogram, MeterSingleValue,
    },
};
use std::{
    collections::LinkedList,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

#[test]
fn metrics() {
    let reporter = Arc::new(MockReporter::default());
    let meter = Meter::new("service_name", "instance_name", reporter.clone());

    {
        meter.metric(MeterRecord::new());
        assert_eq!(
            reporter.pop(),
            MeterData {
                timestamp: 10,
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                metric: None,
            }
        );
    }

    {
        meter.metric(
            MeterRecord::new()
                .custome_time(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)),
        );
        assert_eq!(
            reporter.pop(),
            MeterData {
                timestamp: 1_000_000_000,
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                metric: None,
            }
        );
    }

    {
        meter.metric(
            MeterRecord::new()
                .single_value()
                .name("instance_trace_count")
                .add_label("region", "us-west")
                .add_labels([("az", "az-1")])
                .value(100.),
        );
        assert_eq!(
            reporter.pop(),
            MeterData {
                timestamp: 10,
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                metric: Some(Metric::SingleValue(MeterSingleValue {
                    name: "instance_trace_count".to_owned(),
                    labels: vec![
                        Label {
                            name: "region".to_owned(),
                            value: "us-west".to_owned()
                        },
                        Label {
                            name: "az".to_owned(),
                            value: "az-1".to_owned()
                        },
                    ],
                    value: 100.
                })),
            }
        );
    }

    {
        meter.metric(
            MeterRecord::new()
                .histogram()
                .name("instance_trace_count")
                .add_label("region", "us-west")
                .add_labels([("az", "az-1")])
                .add_value(1., 100, false)
                .add_value(2., 200, true),
        );
        assert_eq!(
            reporter.pop(),
            MeterData {
                timestamp: 10,
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                metric: Some(Metric::Histogram(MeterHistogram {
                    name: "instance_trace_count".to_owned(),
                    labels: vec![
                        Label {
                            name: "region".to_owned(),
                            value: "us-west".to_owned()
                        },
                        Label {
                            name: "az".to_owned(),
                            value: "az-1".to_owned()
                        },
                    ],
                    values: vec![
                        MeterBucketValue {
                            bucket: 1.,
                            count: 100,
                            is_negative_infinity: false
                        },
                        MeterBucketValue {
                            bucket: 2.,
                            count: 200,
                            is_negative_infinity: true
                        },
                    ]
                })),
            }
        );
    }
}

#[derive(Default, Clone)]
struct MockReporter {
    items: Arc<Mutex<LinkedList<MeterData>>>,
}

impl MockReporter {
    fn pop(&self) -> MeterData {
        self.items.try_lock().unwrap().pop_back().unwrap()
    }
}

impl Report for MockReporter {
    fn report(&self, item: CollectItem) {
        match item {
            CollectItem::Meter(data) => {
                self.items.try_lock().unwrap().push_back(data);
            }
            _ => {}
        }
    }
}
