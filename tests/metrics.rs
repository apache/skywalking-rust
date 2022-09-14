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
    metrics::{
        meter::{Counter, Gauge, Histogram},
        metricer::Metricer,
    },
    reporter::{CollectItem, Report},
    skywalking_proto::v3::{
        meter_data::Metric, Label, MeterBucketValue, MeterData, MeterHistogram, MeterSingleValue,
    },
};
use std::{
    collections::LinkedList,
    sync::{Arc, Mutex},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn metrics() {
    let reporter = Arc::new(MockReporter::default());

    {
        let mut metricer = Metricer::new("service_name", "instance_name", reporter.clone());
        let counter = metricer.register(
            Counter::new("instance_trace_count")
                .add_label("region", "us-west")
                .add_labels([("az", "az-1")]),
        );
        counter.increment(100.);

        metricer.boot().shutdown().await.unwrap();

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
        let mut metricer = Metricer::new("service_name", "instance_name", reporter.clone());
        metricer.register(
            Gauge::new("instance_trace_count", || 100.)
                .add_label("region", "us-west")
                .add_labels([("az", "az-1")]),
        );

        metricer.boot().shutdown().await.unwrap();

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
        let mut metricer = Metricer::new("service_name", "instance_name", reporter.clone());
        let histogram = metricer.register(
            Histogram::new("instance_trace_count", vec![1., 2.])
                .add_label("region", "us-west")
                .add_labels([("az", "az-1")]),
        );
        histogram.add_value(1.);
        histogram.add_value(1.5);
        histogram.add_value(2.);

        metricer.boot().shutdown().await.unwrap();

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
                            count: 2,
                            is_negative_infinity: false
                        },
                        MeterBucketValue {
                            bucket: 2.,
                            count: 1,
                            is_negative_infinity: false
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
                self.items.try_lock().unwrap().push_back(*data);
            }
            _ => {}
        }
    }
}
