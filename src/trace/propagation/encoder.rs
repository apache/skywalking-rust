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

//! Propagation encoder.

use crate::trace::trace_context::TracingContext;
use base64::prelude::*;

/// Encode TracingContext to carry current trace info to the destination of RPC
/// call. In general, the output of this function will be packed in `sw8` header
/// in HTTP call.
pub fn encode_propagation(context: &TracingContext, endpoint: &str, address: &str) -> String {
    format!(
        "1-{}-{}-{}-{}-{}-{}-{}",
        BASE64_STANDARD.encode(context.trace_id()),
        BASE64_STANDARD.encode(context.trace_segment_id()),
        context.peek_active_span_id().unwrap_or(0),
        BASE64_STANDARD.encode(context.service()),
        BASE64_STANDARD.encode(context.service_instance()),
        BASE64_STANDARD.encode(endpoint),
        BASE64_STANDARD.encode(address)
    )
}
