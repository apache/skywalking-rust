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

#![allow(unused_imports)]

pub mod skywalking_proto {
    pub mod v3 {
        tonic::include_proto!("skywalking.v3");
    }
}

use prost::Message;
use skywalking_proto::v3::{
    KeyStringValuePair, Log, RefType, SegmentObject, SegmentReference, SpanLayer, SpanObject,
    SpanType,
};
use skywalking_rust::common::time::TimeFetcher;
use skywalking_rust::context::propagation::context::PropagationContext;
use skywalking_rust::context::propagation::decoder::decode_propagation;
use skywalking_rust::context::propagation::encoder::encode_propagation;
use skywalking_rust::context::trace_context::TracingContext;
use std::{cell::Ref, sync::Arc};

/// Serialize from A should equal Serialize from B
#[allow(dead_code)]
pub fn check_serialize_equivalent<M, N>(msg_a: &M, msg_b: &N)
where
    M: Message + Default + PartialEq,
    N: Message + Default + PartialEq,
{
    let mut buf_a = Vec::new();
    msg_a.encode(&mut buf_a).unwrap();
    let mut buf_b = Vec::new();
    msg_b.encode(&mut buf_b).unwrap();
    assert_eq!(buf_a, buf_b);
}

struct MockTimeFetcher {}

impl TimeFetcher for MockTimeFetcher {
    fn get(&self) -> i64 {
        100
    }
}

#[test]
fn create_span() {
    let time_fetcher = MockTimeFetcher {};
    let mut context =
        TracingContext::default_internal(Arc::new(time_fetcher), "service", "instance");
    assert_eq!(context.service, "service");
    assert_eq!(context.service_instance, "instance");

    {
        let mut span1 = context.create_entry_span("op1").unwrap();
        let mut logs = Vec::<(&str, &str)>::new();
        logs.push(("hoge", "fuga"));
        logs.push(("hoge2", "fuga2"));
        let expected_log_message = logs
            .to_owned()
            .into_iter()
            .map(|v| {
                let (key, value) = v;
                KeyStringValuePair {
                    key: key.to_string(),
                    value: value.to_string(),
                }
            })
            .collect();
        let mut expected_log = Vec::<Log>::new();
        expected_log.push(Log {
            time: 100,
            data: expected_log_message,
        });
        span1.add_log(logs);

        let mut tags = Vec::<(&str, &str)>::new();
        tags.push(("hoge", "fuga"));
        let expected_tags = tags
            .to_owned()
            .into_iter()
            .map(|v| {
                let (key, value) = v;
                KeyStringValuePair {
                    key: key.to_string(),
                    value: value.to_string(),
                }
            })
            .collect();
        span1.add_tag(tags[0].clone());

        let span1_expected = SpanObject {
            span_id: 1,
            parent_span_id: 0,
            start_time: 100,
            end_time: 100,
            refs: Vec::<SegmentReference>::new(),
            operation_name: "op1".to_string(),
            peer: String::default(),
            span_type: SpanType::Entry as i32,
            span_layer: SpanLayer::Http as i32,
            component_id: 11000,
            is_error: false,
            tags: expected_tags,
            logs: expected_log,
            skip_analysis: false,
        };
        context.finalize_span_for_test(&mut span1);
        check_serialize_equivalent(span1.span_object(), &span1_expected);
    }

    {
        let span2 = context.create_entry_span("op2");
        assert_eq!(span2.is_err(), true);
    }

    {
        let mut span3 = context.create_exit_span("op3", "example.com/test").unwrap();
        let span3_expected = SpanObject {
            span_id: 2,
            parent_span_id: 1,
            start_time: 100,
            end_time: 100,
            refs: Vec::<SegmentReference>::new(),
            operation_name: "op3".to_string(),
            peer: "example.com/test".to_string(),
            span_type: SpanType::Exit as i32,
            span_layer: SpanLayer::Http as i32,
            component_id: 11000,
            is_error: false,
            tags: Vec::<KeyStringValuePair>::new(),
            logs: Vec::<Log>::new(),
            skip_analysis: false,
        };
        context.finalize_span_for_test(&mut span3);
        check_serialize_equivalent(span3.span_object(), &span3_expected);
    }

    let segment = context.convert_segment_object();
    assert_eq!(segment.trace_id.len() != 0, true);
    assert_eq!(segment.trace_segment_id.len() != 0, true);
    assert_eq!(segment.service, "service");
    assert_eq!(segment.service_instance, "instance");
    assert_eq!(segment.is_size_limited, false);
}

#[test]
fn create_span_from_context() {
    let data = "1-MQ==-NQ==-3-bWVzaA==-aW5zdGFuY2U=-L2FwaS92MS9oZWFsdGg=-ZXhhbXBsZS5jb206ODA4MA==";
    let prop = decode_propagation(data).unwrap();
    let time_fetcher = MockTimeFetcher {};
    let context = TracingContext::from_propagation_context_internal(
        Arc::new(time_fetcher),
        "service2",
        "instance2",
        prop,
    );

    let segment = context.convert_segment_object();
    assert_eq!(segment.trace_id.len() != 0, true);
    assert_eq!(segment.trace_segment_id.len() != 0, true);
    assert_eq!(segment.service, "service2");
    assert_eq!(segment.service_instance, "instance2");
    assert_eq!(segment.is_size_limited, false);
}

#[test]
fn crossprocess_test() {
    let time_fetcher1 = MockTimeFetcher {};
    let mut context1 =
        TracingContext::default_internal(Arc::new(time_fetcher1), "service", "instance");
    assert_eq!(context1.service, "service");
    assert_eq!(context1.service_instance, "instance");

    let mut span1 = context1.create_entry_span("op1").unwrap();
    context1.finalize_span_for_test(&mut span1);

    let mut span2 = context1.create_exit_span("op2", "remote_peer").unwrap();
    context1.finalize_span_for_test(&mut span2);

    let enc_prop = encode_propagation(&context1, "endpoint", "address");
    let dec_prop = decode_propagation(&enc_prop).unwrap();

    let time_fetcher2 = MockTimeFetcher {};
    let mut context2 = TracingContext::from_propagation_context_internal(
        Arc::new(time_fetcher2),
        "service2",
        "instance2",
        dec_prop,
    );

    let mut span3 = context2.create_entry_span("op2").unwrap();
    context2.finalize_span_for_test(&mut span3);

    assert_eq!(span3.span_object().span_id, 1);
    assert_eq!(span3.span_object().parent_span_id, 0);
    assert_eq!(span3.span_object().refs.len(), 1);

    let expected_ref = SegmentReference {
        ref_type: RefType::CrossProcess as i32,
        trace_id: context2.trace_id,
        parent_trace_segment_id: context1.trace_segment_id,
        parent_span_id: context1.next_span_id,
        parent_service: context1.service,
        parent_service_instance: context1.service_instance,
        parent_endpoint: "endpoint".to_string(),
        network_address_used_at_peer: "address".to_string(),
    };

    check_serialize_equivalent(&expected_ref, &span3.span_object().refs[0]);
}
