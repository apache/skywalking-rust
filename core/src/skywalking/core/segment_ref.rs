// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements.  See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License.  You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::os::raw::c_long;

use crate::skywalking::core::{ID, Span, TracingContext};
use crate::skywalking::core::segment_ref::SegmentRefType::CROSS_PROCESS;

#[derive(Clone, Hash)]
pub struct SegmentRef {
    ref_type: SegmentRefType,
    trace_id: ID,
    segment_id: ID,
    span_id: i32,
    parent_service_instance_id: i32,
    entry_service_instance_id: i32,
    network_address: Option<String>,
    network_address_id: i32,
    entry_endpoint: Option<String>,
    entry_endpoint_id: i32,
    parent_endpoint: Option<String>,
    parent_endpoint_id: i32,
}

#[derive(Clone, Hash)]
enum SegmentRefType {
    CROSS_PROCESS,
    CROSS_THREAD,
}

impl SegmentRef {
    pub fn from_text(value: &str) -> Option<Self> {
        let strings: Vec<&str> = value.split("-").collect();
        if strings.len() == 9 {
            // Ignore string[0].
            let trace_id = match SegmentRef::string_to_id(strings[1]) {
                Some(id) => { id }
                _ => { return None; }
            };
            let segment_id = match SegmentRef::string_to_id(strings[2]) {
                Some(id) => { id }
                _ => { return None; }
            };
            let span_id = match strings[3].parse::<i32>() {
                Ok(id) => { id }
                _ => { return None; }
            };
            let parent_service_instance_id = match strings[4].parse::<i32>() {
                Ok(id) => { id }
                _ => { return None; }
            };
            let entry_service_instance_id = match strings[5].parse::<i32>() {
                Ok(id) => { id }
                _ => { return None; }
            };

            let (network_address, network_address_id) = match SegmentRef::decode_base64_to_string_or_id(strings[6]) {
                Some(decoded) => { decoded }
                _ => { return None; }
            };
            let (entry_endpoint, entry_endpoint_id) = match SegmentRef::decode_base64_to_string_or_id(strings[7]) {
                Some(decoded) => { decoded }
                _ => { return None; }
            };
            let (parent_endpoint, parent_endpoint_id) = match SegmentRef::decode_base64_to_string_or_id(strings[8]) {
                Some(decoded) => { decoded }
                _ => { return None; }
            };

            Some(SegmentRef {
                ref_type: CROSS_PROCESS,
                trace_id,
                segment_id,
                span_id,
                parent_service_instance_id,
                entry_service_instance_id,
                network_address,
                network_address_id,
                entry_endpoint,
                entry_endpoint_id,
                parent_endpoint,
                parent_endpoint_id,
            })
        } else {
            None
        }
    }

    pub fn for_across_process(context: &TracingContext, exit_span: &dyn Span, peer: &str) -> Self {
        // -1 represent the object doesn't exist.
        let inexistence = -1;
        let (entry_endpoint, entry_endpoint_id) = match context.first_ref() {
            None => {
                match context.entry_endpoint_name() {
                    None => { (None, inexistence) }
                    Some(endpoint) => { (Some(endpoint.clone()), 0) }
                }
            }
            Some(reference) => {
                match &reference.entry_endpoint {
                    None => { (None, reference.entry_endpoint_id) }
                    Some(endpoint) => { (Some(endpoint.clone()), 0) }
                }
            }
        };
        let (parent_endpoint, parent_endpoint_id) = match context.entry_endpoint_name() {
            None => { (None, inexistence) }
            Some(endpoint) => { (Some(endpoint.clone()), 0) }
        };

        SegmentRef {
            ref_type: CROSS_PROCESS,
            trace_id: context.trace_id(),
            segment_id: context.segment_id(),
            span_id: exit_span.span_id(),
            network_address: Some(String::from(peer.clone())),
            // No network address register, the id always be 0
            network_address_id: 0,
            entry_service_instance_id: {
                match context.first_ref() {
                    None => { context.service_instance_id() }
                    Some(reference) => { reference.entry_service_instance_id }
                }
            },
            parent_service_instance_id: context.service_instance_id(),
            entry_endpoint,
            entry_endpoint_id,
            parent_endpoint,
            parent_endpoint_id,
        }
    }

    pub fn serialize(&self) -> String {
        let parts: Vec<String> = vec![
            "1".to_string(),
            base64::encode(self.trace_id.to_string().as_bytes()),
            base64::encode(self.segment_id.to_string().as_bytes()),
            self.span_id.to_string(),
            self.parent_service_instance_id.to_string(),
            self.entry_service_instance_id.to_string(),
            SegmentRef::string_or_id_to_encode_base64(&self.network_address, self.network_address_id),
            SegmentRef::string_or_id_to_encode_base64(&self.entry_endpoint, self.entry_endpoint_id),
            SegmentRef::string_or_id_to_encode_base64(&self.parent_endpoint, self.parent_endpoint_id),
        ];
        parts.join("-")
    }

    pub fn get_trace_id(&self) -> ID {
        self.trace_id.clone()
    }

    fn string_to_id(text: &str) -> Option<ID> {
        match base64::decode(text) {
            Ok(value) => {
                match String::from_utf8(value) {
                    Ok(str) => {
                        match ID::from(str) {
                            Ok(id) => { Some(id) }
                            _ => None
                        }
                    }
                    _ => { None }
                }
            }
            _ => { None }
        }
    }

    fn decode_base64_to_string_or_id(text: &str) -> Option<(Option<String>, i32)> {
        match base64::decode(text) {
            Ok(value) => {
                match String::from_utf8(value) {
                    Ok(str) => {
                        if str.starts_with("#") {
                            let network: Vec<&str> = str.split("#").collect();
                            (Some((Some(network[1].to_string()), 0)))
                        } else {
                            match str.parse::<i32>() {
                                Ok(id) => { Some((None, id)) }
                                _ => { None }
                            }
                        }
                    }
                    _ => { None }
                }
            }
            _ => { None }
        }
    }

    fn string_or_id_to_encode_base64(text: &Option<String>, id: i32) -> String {
        base64::encode(match text {
            None => { id.to_string() }
            Some(t) => {
                let mut network = "#".to_string();
                network.push_str(&t);
                network
            }
        }.as_bytes())
    }
}

#[cfg(test)]
mod segment_ref_tests {
    use crate::skywalking::core::ID;
    use crate::skywalking::core::segment_ref::SegmentRef;

    #[test]
    fn test_deserialize_context_carrier() {
        let carrier = SegmentRef::from_text("1-My40LjU=-MS4yLjM=-4-1-1-IzEyNy4wLjAuMTo4MDgw-Iy9wb3J0YWw=-MTIz").unwrap();
        assert_eq!(carrier.trace_id == ID::new(3, 4, 5), true);
        assert_eq!(carrier.segment_id == ID::new(1, 2, 3), true);
        assert_eq!(carrier.span_id, 4);
        assert_eq!(carrier.entry_service_instance_id, 1);
        assert_eq!(carrier.parent_service_instance_id, 1);
        assert_eq!(carrier.network_address, Some("127.0.0.1:8080".to_string()));
        assert_eq!(carrier.entry_endpoint, Some("/portal".to_string()));
        assert_eq!(carrier.parent_endpoint_id, 123);
    }

    #[test]
    fn test_serialize_ref() {
        let carrier = SegmentRef::from_text("1-My40LjU=-MS4yLjM=-4-1-1-IzEyNy4wLjAuMTo4MDgw-Iy9wb3J0YWw=-MTIz").unwrap();
        assert_eq!(carrier.trace_id == ID::new(3, 4, 5), true);
        assert_eq!(carrier.segment_id == ID::new(1, 2, 3), true);
        assert_eq!(carrier.span_id, 4);
        assert_eq!(carrier.entry_service_instance_id, 1);
        assert_eq!(carrier.parent_service_instance_id, 1);
        assert_eq!(carrier.network_address, Some("127.0.0.1:8080".to_string()));
        assert_eq!(carrier.entry_endpoint, Some("/portal".to_string()));
        assert_eq!(carrier.parent_endpoint_id, 123);

        let carrier_text = carrier.serialize();
        assert_eq!(carrier_text, "1-My40LjU=-MS4yLjM=-4-1-1-IzEyNy4wLjAuMTo4MDgw-Iy9wb3J0YWw=-MTIz");
    }
}
