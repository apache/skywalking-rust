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

#![allow(clippy::bool_assert_comparison)]

use prost::Message;
use rdkafka::{
    consumer::{Consumer, StreamConsumer},
    ClientConfig, Message as _,
};
use skywalking::proto::v3::{
    log_data_body::Content, meter_data::Metric, JsonLog, KeyStringValuePair, LogData, LogTags,
    MeterData, RefType, SegmentObject, SpanLayer, SpanType,
};
use std::time::Duration;
use tokio::time::timeout;

async fn segment() {
    let consumer = create_consumer("skywalking-segments");

    for _ in 0..3 {
        let segment: SegmentObject = consumer_recv(&consumer).await;
        check_segment(&segment);
    }
}

fn check_segment(segment: &SegmentObject) {
    dbg!(segment);

    assert_eq!(segment.service_instance, "node_0");

    if segment.service == "consumer" {
        assert_eq!(segment.spans.len(), 1);

        assert_eq!(segment.spans[0].span_id, 0);
        assert_eq!(segment.spans[0].parent_span_id, -1);
        assert_eq!(segment.spans[0].operation_name, "/pong");
        assert_eq!(segment.spans[0].peer, "");
        assert_eq!(segment.spans[0].span_type, SpanType::Entry as i32);
        assert_eq!(segment.spans[0].span_layer, SpanLayer::Http as i32);
        assert_eq!(segment.spans[0].component_id, 11000);
        assert_eq!(segment.spans[0].is_error, false);
        assert_eq!(segment.spans[0].tags.len(), 0);
        assert_eq!(segment.spans[0].logs.len(), 0);
        assert_eq!(segment.spans[0].refs.len(), 1);

        assert_eq!(
            segment.spans[0].refs[0].ref_type,
            RefType::CrossProcess as i32
        );
        assert_eq!(segment.spans[0].refs[0].parent_span_id, 1);
        assert_eq!(segment.spans[0].refs[0].parent_service, "producer");
        assert_eq!(segment.spans[0].refs[0].parent_service_instance, "node_0");
        assert_eq!(segment.spans[0].refs[0].parent_endpoint, "/pong");
        assert_eq!(
            segment.spans[0].refs[0].network_address_used_at_peer,
            "consumer:8082"
        );
    } else if segment.service == "producer" {
        if segment.spans.last().unwrap().operation_name == "/ping" {
            assert_eq!(segment.spans.len(), 3);

            assert_eq!(segment.spans[0].span_id, 1);
            assert_eq!(segment.spans[0].parent_span_id, 0);
            assert_eq!(segment.spans[0].operation_name, "/pong");
            assert_eq!(segment.spans[0].peer, "consumer:8082");
            assert_eq!(segment.spans[0].span_type, SpanType::Exit as i32);
            assert_eq!(segment.spans[0].span_layer, SpanLayer::Unknown as i32);
            assert_eq!(segment.spans[0].component_id, 11000);
            assert_eq!(segment.spans[0].is_error, false);
            assert_eq!(segment.spans[0].tags.len(), 0);
            assert_eq!(segment.spans[0].logs.len(), 0);
            assert_eq!(segment.spans[0].refs.len(), 0);

            assert_eq!(segment.spans[1].span_id, 2);
            assert_eq!(segment.spans[1].parent_span_id, 0);
            assert_eq!(segment.spans[1].operation_name, "async-job");
            assert_eq!(segment.spans[1].peer, "");
            assert_eq!(segment.spans[1].span_type, SpanType::Local as i32);
            assert_eq!(segment.spans[1].span_layer, SpanLayer::Unknown as i32);
            assert_eq!(segment.spans[1].component_id, 11000);
            assert_eq!(segment.spans[1].is_error, false);
            assert_eq!(segment.spans[1].tags.len(), 0);
            assert_eq!(segment.spans[1].logs.len(), 0);
            assert_eq!(segment.spans[1].refs.len(), 0);

            assert_eq!(segment.spans[2].span_id, 0);
            assert_eq!(segment.spans[2].parent_span_id, -1);
            assert_eq!(segment.spans[2].operation_name, "/ping");
            assert_eq!(segment.spans[2].peer, "");
            assert_eq!(segment.spans[2].span_type, SpanType::Entry as i32);
            assert_eq!(segment.spans[2].span_layer, SpanLayer::Http as i32);
            assert_eq!(segment.spans[2].component_id, 11000);
            assert_eq!(segment.spans[2].is_error, false);
            assert_eq!(segment.spans[2].tags.len(), 0);
            assert_eq!(segment.spans[2].logs.len(), 0);
            assert_eq!(segment.spans[2].refs.len(), 0);
        } else if segment.spans.last().unwrap().operation_name == "async-callback" {
            assert_eq!(segment.spans.len(), 1);

            assert_eq!(segment.spans[0].span_id, 0);
            assert_eq!(segment.spans[0].parent_span_id, -1);
            assert_eq!(segment.spans[0].peer, "");
            assert_eq!(segment.spans[0].span_type, SpanType::Entry as i32);
            assert_eq!(segment.spans[0].span_layer, SpanLayer::Http as i32);
            assert_eq!(segment.spans[0].component_id, 11000);
            assert_eq!(segment.spans[0].is_error, false);
            assert_eq!(segment.spans[0].tags.len(), 0);
            assert_eq!(segment.spans[0].logs.len(), 0);
            assert_eq!(segment.spans[0].refs.len(), 1);

            assert_eq!(
                segment.spans[0].refs[0].ref_type,
                RefType::CrossThread as i32
            );
            assert_eq!(segment.spans[0].refs[0].parent_span_id, 2);
            assert_eq!(segment.spans[0].refs[0].parent_service, "producer");
            assert_eq!(segment.spans[0].refs[0].parent_service_instance, "node_0");
            assert_eq!(segment.spans[0].refs[0].parent_endpoint, "async-job");
            assert_eq!(segment.spans[0].refs[0].network_address_used_at_peer, "");
        } else {
            panic!(
                "unknown operation_name {}",
                segment.spans.last().unwrap().operation_name
            );
        }
    } else {
        panic!("unknown service {}", segment.service);
    }
}

async fn meter() {
    let consumer = create_consumer("skywalking-meters");

    for _ in 0..3 {
        let meter: MeterData = consumer_recv(&consumer).await;
        check_meter(&meter);
    }
}

fn check_meter(meter: &MeterData) {
    dbg!(meter);

    assert_eq!(meter.service, "consumer");
    assert_eq!(meter.service_instance, "node_0");

    match &meter.metric {
        Some(Metric::SingleValue(value)) => {
            assert_eq!(value.name, "instance_trace_count");

            if value.labels[0].name == "region" && value.labels[0].value == "us-west" {
                assert_eq!(value.labels[1].name, "az");
                assert_eq!(value.labels[1].value, "az-1");
                assert_eq!(value.value, 30.0);
            } else if value.labels[0].name == "region" && value.labels[0].value == "us-east" {
                assert_eq!(value.labels[1].name, "az");
                assert_eq!(value.labels[1].value, "az-3");
                assert_eq!(value.value, 20.0);
            } else {
                panic!("unknown label {:?}", &value.labels[0]);
            }
        }
        Some(Metric::Histogram(value)) => {
            assert_eq!(value.name, "instance_trace_count");
            assert_eq!(value.labels[0].name, "region");
            assert_eq!(value.labels[0].value, "us-north");
            assert_eq!(value.labels[1].name, "az");
            assert_eq!(value.labels[1].value, "az-1");
            assert_eq!(value.values[0].bucket, 10.0);
            assert_eq!(value.values[0].count, 1);
            assert_eq!(value.values[1].bucket, 20.0);
            assert_eq!(value.values[1].count, 2);
            assert_eq!(value.values[2].bucket, 30.0);
            assert_eq!(value.values[2].count, 0);
        }
        _ => {
            panic!("unknown metric");
        }
    }
}

async fn log() {
    let consumer = create_consumer("skywalking-logs");

    for _ in 0..3 {
        let log: LogData = consumer_recv(&consumer).await;
        check_log(&log);
    }
}

fn check_log(log: &LogData) {
    dbg!(log);

    if log.service == "producer" && log.service_instance == "node_0" {
        assert_eq!(log.endpoint, "/ping");

        match &log.body.as_ref().unwrap().content {
            Some(Content::Json(json)) => {
                assert_eq!(json.json, r#"{"message": "handle ping"}"#);
                assert_eq!(log.trace_context, None);
                assert_eq!(
                    log.tags,
                    Some(LogTags {
                        data: vec![KeyStringValuePair {
                            key: "level".to_string(),
                            value: "DEBUG".to_string()
                        }]
                    })
                );
            }
            Some(Content::Text(text)) => {
                assert_eq!(text.text, "do http request");
                assert_eq!(log.trace_context.as_ref().unwrap().span_id, 1);
                assert_eq!(
                    log.tags,
                    Some(LogTags {
                        data: vec![KeyStringValuePair {
                            key: "level".to_string(),
                            value: "INFO".to_string()
                        }]
                    })
                );
            }
            body => {
                panic!("unknown log body {:?}", body);
            }
        }
    } else if log.service == "consumer" && log.service_instance == "node_0" {
        assert_eq!(log.endpoint, "/pong");
        assert_eq!(
            log.body.as_ref().unwrap().content,
            Some(Content::Json(JsonLog {
                json: r#"{"message": "handle pong"}"#.to_string()
            }))
        );
        assert_eq!(log.trace_context, None);
        assert_eq!(
            log.tags,
            Some(LogTags {
                data: vec![KeyStringValuePair {
                    key: "level".to_string(),
                    value: "DEBUG".to_string()
                }]
            })
        );
    } else {
        panic!("unknown log {} {}", log.service, log.service_instance);
    }
}

fn create_consumer(topic: &str) -> StreamConsumer {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", "broker:9092")
        .set("broker.address.family", "v4")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.offset.store", "true")
        .set("group.id", topic)
        .create()
        .unwrap();
    consumer.subscribe(&[topic]).unwrap();
    consumer
}

async fn consumer_recv<T: Message + Default>(consumer: &StreamConsumer) -> T {
    let message = timeout(Duration::from_secs(6), consumer.recv())
        .await
        .unwrap()
        .unwrap();
    let value = message.payload_view::<[u8]>().unwrap().unwrap();
    Message::decode(value).unwrap()
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    segment().await;
    meter().await;
    log().await;
}
