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

pub static SKYWALKING_HTTP_CONTEXT_HEADER_KEY: &str = "sw8";

#[derive(Debug)]
pub struct PropagationContext {
    /// It defines whether next span should be trace or not.
    /// In SkyWalking, If `do_sample == true`, the span should be reported to
    /// OAP server and can be analyzed.
    pub do_sample: bool,

    /// It defines trace ID that previous span has. It expresses unique value of entire trace.
    pub parent_trace_id: String,

    /// It defines segment ID that previos span has. It expresses unique value of entire trace.
    pub parent_trace_segment_id: String,

    /// It defines parent span's span ID.
    pub parent_span_id: i32,

    /// Service name of service parent belongs.
    pub parent_service: String,

    /// Instance name of service parent belongs.
    pub parent_service_instance: String,

    /// An endpoint name that parent requested to.
    pub destination_endpoint: String,

    /// An address that parent requested to. It can be authority or network address.
    pub destination_address: String,
}

/// PropagationContext carries the context which include trace infomation.
/// In general, this context will be used if you create new TraceContext after received
/// decoded context that should be packed in `sw8` header.
impl PropagationContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        do_sample: bool,
        parent_trace_id: String,
        parent_trace_segment_id: String,
        parent_span_id: i32,
        parent_service: String,
        parent_service_instance: String,
        destination_endpoint: String,
        destination_address: String,
    ) -> PropagationContext {
        PropagationContext {
            do_sample,
            parent_trace_id,
            parent_trace_segment_id,
            parent_span_id,
            parent_service,
            parent_service_instance,
            destination_endpoint,
            destination_address,
        }
    }
}
