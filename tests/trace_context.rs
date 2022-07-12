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

use prost::Message;
use skywalking::common::time::TimeFetcher;
use skywalking::context::propagation::context::PropagationContext;
use skywalking::context::propagation::decoder::decode_propagation;
use skywalking::context::propagation::encoder::encode_propagation;
use skywalking::context::trace_context::TracingContext;
use skywalking::skywalking_proto::v3::{
    KeyStringValuePair, Log, RefType, SegmentObject, SegmentReference, SpanLayer, SpanObject,
    SpanType,
};
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
    let mut context = TracingContext::new(Arc::new(time_fetcher), "service", "instance");
    assert_eq!(context.service, "service");
    assert_eq!(context.service_instance, "instance");

    {
        let mut span1 = context.create_entry_span("op1").unwrap();
        let logs = vec![("hoge", "fuga"), ("hoge2", "fuga2")];
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
        let expected_log = vec![Log {
            time: 100,
            data: expected_log_message,
        }];
        span1.add_log(logs);

        let tags = vec![("hoge", "fuga")];
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
        span1.add_tag(tags[0]);

        {
            let span2 = context.create_entry_span("op2");
            assert!(span2.is_err());
        }

        {
            let span3 = context.create_exit_span("op3", "example.com/test").unwrap();
            context.finalize_span(span3);

            let span3_expected = SpanObject {
                span_id: 1,
                parent_span_id: 0,
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
            assert_eq!(*context.spans.last().unwrap().span_object(), span3_expected);
        }

        {
            let span4 = context.create_exit_span("op3", "example.com/test").unwrap();

            {
                let span5 = context.create_exit_span("op4", "example.com/test").unwrap();
                context.finalize_span(span5);

                let span5_expected = SpanObject {
                    span_id: 3,
                    parent_span_id: 2,
                    start_time: 100,
                    end_time: 100,
                    refs: Vec::<SegmentReference>::new(),
                    operation_name: "op4".to_string(),
                    peer: "example.com/test".to_string(),
                    span_type: SpanType::Exit as i32,
                    span_layer: SpanLayer::Http as i32,
                    component_id: 11000,
                    is_error: false,
                    tags: Vec::<KeyStringValuePair>::new(),
                    logs: Vec::<Log>::new(),
                    skip_analysis: false,
                };
                assert_eq!(*context.spans.last().unwrap().span_object(), span5_expected);
            }

            context.finalize_span(span4);
        }

        context.finalize_span(span1);

        let span1_expected = SpanObject {
            span_id: 0,
            parent_span_id: -1,
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
        assert_eq!(*context.spans.last().unwrap().span_object(), span1_expected);
    }

    let segment = context.convert_segment_object();
    assert_ne!(segment.trace_id.len(), 0);
    assert_ne!(segment.trace_segment_id.len(), 0);
    assert_eq!(segment.service, "service");
    assert_eq!(segment.service_instance, "instance");
    assert!(!segment.is_size_limited);
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
    assert_ne!(segment.trace_id.len(), 0);
    assert_ne!(segment.trace_segment_id.len(), 0);
    assert_eq!(segment.service, "service2");
    assert_eq!(segment.service_instance, "instance2");
    assert!(!segment.is_size_limited);
}

#[test]
fn crossprocess_test() {
    let time_fetcher1 = MockTimeFetcher {};
    let mut context1 = TracingContext::new(Arc::new(time_fetcher1), "service", "instance");
    assert_eq!(context1.service, "service");
    assert_eq!(context1.service_instance, "instance");

    let span1 = context1.create_entry_span("op1").unwrap();
    {
        let span2 = context1.create_exit_span("op2", "remote_peer").unwrap();

        {
            let enc_prop = encode_propagation(&context1, "endpoint", "address");
            let dec_prop = decode_propagation(&enc_prop).unwrap();

            let time_fetcher2 = MockTimeFetcher {};
            let mut context2 = TracingContext::from_propagation_context_internal(
                Arc::new(time_fetcher2),
                "service2",
                "instance2",
                dec_prop,
            );

            let span3 = context2.create_entry_span("op2").unwrap();
            context2.finalize_span(span3);

            let span3 = context2.spans.last().unwrap();
            assert_eq!(span3.span_object().span_id, 0);
            assert_eq!(span3.span_object().parent_span_id, -1);
            assert_eq!(span3.span_object().refs.len(), 1);

            let expected_ref = SegmentReference {
                ref_type: RefType::CrossProcess as i32,
                trace_id: context2.trace_id,
                parent_trace_segment_id: context1.trace_segment_id.clone(),
                parent_span_id: 1,
                parent_service: context1.service.clone(),
                parent_service_instance: context1.service_instance.clone(),
                parent_endpoint: "endpoint".to_string(),
                network_address_used_at_peer: "address".to_string(),
            };

            check_serialize_equivalent(&expected_ref, &span3.span_object().refs[0]);
        }

        context1.finalize_span(span2);
    }

    context1.finalize_span(span1);
}
