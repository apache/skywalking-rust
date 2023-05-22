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

use prost::Message;
use skywalking::{
    proto::v3::{
        KeyStringValuePair, Log, RefType, SegmentObject, SegmentReference, SpanLayer, SpanObject,
        SpanType,
    },
    reporter::{print::PrintReporter, CollectItem, Report},
    trace::{
        propagation::{decoder::decode_propagation, encoder::encode_propagation},
        tracer::Tracer,
    },
};
use std::{
    collections::LinkedList,
    sync::{Arc, Mutex},
    thread,
};
use tokio::{runtime::Handle, task};

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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_span() {
    MockReporter::with(
        |reporter| {
            let tracer = Tracer::new("service", "instance", reporter);
            let mut context = tracer.create_trace_context();
            assert_eq!(context.service(), "service");
            assert_eq!(context.service_instance(), "instance");

            {
                let mut span1 = context.create_entry_span("op1");
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
                    time: 10,
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
                span1.add_tag(tags[0].0, tags[0].1);

                {
                    let _span2 = context.create_local_span("op2");
                }

                {
                    let span3 = context.create_exit_span("op3", "example.com/test");
                    drop(span3);

                    let span3_expected = SpanObject {
                        span_id: 2,
                        parent_span_id: 0,
                        start_time: 1,
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
                    assert_eq!(context.last_span(), Some(span3_expected));
                }

                {
                    let _span4 = context.create_local_span("op4");

                    {
                        let span5 = context.create_exit_span("op5", "example.com/test");
                        drop(span5);

                        let span5_expected = SpanObject {
                            span_id: 4,
                            parent_span_id: 3,
                            start_time: 1,
                            end_time: 100,
                            refs: Vec::<SegmentReference>::new(),
                            operation_name: "op5".to_string(),
                            peer: "example.com/test".to_string(),
                            span_type: SpanType::Exit as i32,
                            span_layer: SpanLayer::Http as i32,
                            component_id: 11000,
                            is_error: false,
                            tags: Vec::<KeyStringValuePair>::new(),
                            logs: Vec::<Log>::new(),
                            skip_analysis: false,
                        };
                        assert_eq!(context.last_span(), Some(span5_expected));
                    }
                }

                drop(span1);

                let span1_expected = SpanObject {
                    span_id: 0,
                    parent_span_id: -1,
                    start_time: 1,
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
                assert_eq!(context.last_span(), Some(span1_expected));
            }

            tracer
        },
        |segment| {
            assert_ne!(segment.trace_id.len(), 0);
            assert_ne!(segment.trace_segment_id.len(), 0);
            assert_eq!(segment.service, "service");
            assert_eq!(segment.service_instance, "instance");
            assert!(!segment.is_size_limited);
        },
    )
    .await;
}

#[test]
#[should_panic]
fn create_local_span_failed() {
    let tracer = Tracer::new("service", "instance", PrintReporter::new());
    let mut context = tracer.create_trace_context();
    let _span1 = context.create_local_span("op1");
}

#[test]
#[should_panic]
fn create_exit_span_failed() {
    let tracer = Tracer::new("service", "instance", PrintReporter::new());
    let mut context = tracer.create_trace_context();
    let _span1 = context.create_exit_span("op1", "example.com/test");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_span_from_context() {
    let data = "1-MQ==-NQ==-3-bWVzaA==-aW5zdGFuY2U=-L2FwaS92MS9oZWFsdGg=-ZXhhbXBsZS5jb206ODA4MA==";
    let prop = decode_propagation(data).unwrap();

    MockReporter::with(
        |reporter| {
            let tracer = Tracer::new("service2", "instance2", reporter);
            let mut context = tracer.create_trace_context();
            let _span = context.create_entry_span_with_propagation("operation_name", &prop);
            tracer
        },
        |segment| {
            assert_ne!(segment.trace_id.len(), 0);
            assert_ne!(segment.trace_segment_id.len(), 0);
            assert_eq!(segment.service, "service2");
            assert_eq!(segment.service_instance, "instance2");
            assert!(!segment.is_size_limited);
        },
    )
    .await;
}

#[test]
fn cross_process_test() {
    let tracer = Tracer::new("service", "instance", PrintReporter::new());
    let mut context1 = tracer.create_trace_context();
    assert_eq!(context1.service(), "service");
    assert_eq!(context1.service_instance(), "instance");

    let _span1 = context1.create_entry_span("op1");
    {
        let _span2 = context1.create_exit_span("op2", "remote_peer");

        {
            let enc_prop = encode_propagation(&context1, "endpoint", "address");
            let dec_prop = decode_propagation(&enc_prop).unwrap();

            let tracer = Tracer::new("service2", "instance2", PrintReporter::new());
            let mut context2 = tracer.create_trace_context();

            let span3 = context2.create_entry_span_with_propagation("op2", &dec_prop);
            drop(span3);

            let span3 = context2.last_span().unwrap();

            assert_eq!(context1.trace_id(), context2.trace_id());
            assert_eq!(span3.span_id, 0);
            assert_eq!(span3.parent_span_id, -1);
            assert_eq!(span3.refs.len(), 1);

            let expected_ref = SegmentReference {
                ref_type: RefType::CrossProcess as i32,
                trace_id: context2.trace_id().to_owned(),
                parent_trace_segment_id: context1.trace_segment_id().to_owned(),
                parent_span_id: 1,
                parent_service: context1.service().to_owned(),
                parent_service_instance: context1.service_instance().to_owned(),
                parent_endpoint: "endpoint".to_string(),
                network_address_used_at_peer: "address".to_string(),
            };

            check_serialize_equivalent(&expected_ref, &span3.refs[0]);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn cross_process_test_1() {
    let propagation = Mutex::new(String::new());

    MockReporter::with(
        |reporter| {
            let tracer = Tracer::new("service1", "instance1", reporter);
            let mut context = tracer.create_trace_context();
            let _span1 = context.create_entry_span("entry_1");
            let _span2 = context.create_exit_span("exit_1", "peer_1");
            *propagation.try_lock().unwrap() = encode_propagation(&context, "exit_1", "peer_1");
            tracer
        },
        |segment1| {
            let propagation = propagation.lock().unwrap().clone();
            task::block_in_place(move || {
                Handle::current().block_on(async move {
                    MockReporter::with(
                        |reporter| {
                            let tracer = Tracer::new("service2", "instance2", reporter);
                            let mut context = tracer.create_trace_context();
                            let _span1 = context.create_entry_span_with_propagation(
                                "entry_1",
                                &decode_propagation(&propagation).unwrap(),
                            );
                            tracer
                        },
                        |segment2| {
                            assert_eq!(segment1.trace_id, segment2.trace_id);
                            assert_eq!(
                                segment2.spans[0].refs[0],
                                SegmentReference {
                                    ref_type: RefType::CrossProcess as i32,
                                    trace_id: segment1.trace_id.clone(),
                                    parent_trace_segment_id: segment1.trace_segment_id.clone(),
                                    parent_span_id: 1,
                                    parent_service: segment1.service.clone(),
                                    parent_service_instance: segment1.service_instance.clone(),
                                    parent_endpoint: segment1.spans[0].operation_name.clone(),
                                    network_address_used_at_peer: segment1.spans[0].peer.clone(),
                                }
                            );
                        },
                    )
                    .await;
                });
            });
        },
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn cross_threads_test() {
    MockReporter::with_many(
        |reporter| {
            let tracer = Tracer::new("service", "instance", reporter);
            let mut ctx1 = tracer.create_trace_context();
            let _span1 = ctx1.create_entry_span("op1");
            let _span2 = ctx1.create_local_span("op2");
            let snapshot = ctx1.capture();

            let tracer_ = tracer.clone();
            thread::spawn(move || {
                let mut ctx2 = tracer_.create_trace_context();
                let _span3 = ctx2.create_entry_span("op3");
                ctx2.continued(snapshot);
            })
            .join()
            .unwrap();

            tracer
        },
        |segments| {
            let mut iter = segments.iter();
            let first = iter.next().unwrap();
            let second = iter.next().unwrap();

            assert_eq!(first.trace_id, second.trace_id);
            assert_eq!(first.spans.last().unwrap().refs.len(), 1);
            assert_eq!(
                first.spans.last().unwrap().refs[0],
                SegmentReference {
                    ref_type: RefType::CrossThread as i32,
                    trace_id: second.trace_id.clone(),
                    parent_trace_segment_id: second.trace_segment_id.clone(),
                    parent_span_id: 1,
                    parent_service: "service".to_owned(),
                    parent_service_instance: "instance".to_owned(),
                    parent_endpoint: "op2".to_owned(),
                    ..Default::default()
                }
            );
            assert_eq!(second.spans.len(), 2);
        },
    )
    .await;
}

#[derive(Default, Clone)]
struct MockReporter {
    segments: Arc<Mutex<LinkedList<SegmentObject>>>,
}

impl MockReporter {
    async fn with(f1: impl FnOnce(MockReporter) -> Tracer, f2: impl FnOnce(&SegmentObject)) {
        Self::with_many(f1, |segments| f2(&segments.front().unwrap())).await;
    }

    async fn with_many(
        f1: impl FnOnce(MockReporter) -> Tracer,
        f2: impl FnOnce(&LinkedList<SegmentObject>),
    ) {
        let reporter = MockReporter::default();
        f1(reporter.clone());
        let segments = reporter.segments.try_lock().unwrap();
        f2(&*segments);
    }
}

impl Report for MockReporter {
    fn report(&self, item: CollectItem) {
        let segment = match item {
            CollectItem::Trace(segment) => segment,
            _ => unreachable!(),
        };
        self.segments.try_lock().unwrap().push_back(*segment);
    }
}
