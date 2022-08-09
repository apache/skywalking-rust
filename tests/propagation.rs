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
    reporter::print::PrintReporter,
    trace::{
        propagation::{
            context::PropagationContext, decoder::decode_propagation, encoder::encode_propagation,
        },
        trace_context::TracingContext,
        tracer::Tracer,
    },
};
use std::sync::Arc;

#[test]
fn basic() {
    let data = "1-MQ==-NQ==-3-bWVzaA==-aW5zdGFuY2U=-L2FwaS92MS9oZWFsdGg=-ZXhhbXBsZS5jb206ODA4MA==";
    let res = decode_propagation(data).unwrap();

    assert!(res.do_sample);
    assert_eq!(res.parent_trace_id, "1");
    assert_eq!(res.parent_trace_segment_id, "5");
    assert_eq!(res.parent_span_id, 3);
    assert_eq!(res.parent_service, "mesh");
    assert_eq!(res.parent_service_instance, "instance");
    assert_eq!(res.destination_endpoint, "/api/v1/health");
    assert_eq!(res.destination_address, "example.com:8080");
}

#[test]
fn less_field() {
    let data = "1-MQ==-NQ==-3-bWVzaA==-aW5zdGFuY2U=-L2FwaS92MS9oZWFsdGg=";
    let res = decode_propagation(data);

    assert!(res.is_err());
}

#[test]
fn more_field() {
    let data = "1-MQ==-NQ==-3-bWVzaA==-aW5zdGFuY2U=-L2FwaS92MS9oZWFsdGg=-ZXhhbXBsZS5jb206ODA4MA==-hogehoge";
    let res = decode_propagation(data);

    assert!(res.is_err());
}

#[test]
fn invalid_sample() {
    let data = "3-MQ==-NQ==-3-bWVzaA==-aW5zdGFuY2U=-L2FwaS92MS9oZWFsdGg=-ZXhhbXBsZS5jb206ODA4MA==";
    let res = decode_propagation(data);

    assert!(res.is_err());
}

#[test]
fn basic_encode() {
    let tracer = Tracer::new("mesh", "instance", PrintReporter::new());
    let tc = tracer.create_trace_context();
    let res = encode_propagation(&tc, "/api/v1/health", "example.com:8080");
    let res2 = decode_propagation(&res).unwrap();
    assert!(res2.do_sample);
    assert_eq!("/api/v1/health", res2.destination_endpoint);
    assert_eq!("example.com:8080", res2.destination_address)
}
