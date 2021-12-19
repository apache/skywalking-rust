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

use crate::context::propagation::context::PropagationContext;
use base64::decode;

/// Decode context value packed in `sw8` header.
pub fn decode_propagation(header_value: &str) -> Result<PropagationContext, &str> {
    let pieces: Vec<&str> = header_value.split('-').collect();

    if pieces.len() != 8 {
        return Err("failed to parse propagation context: it must have 8 properties.");
    }

    let do_sample = try_parse_sample_status(pieces[0])?;
    let parent_trace_id = b64_encoded_into_string(pieces[1])?;
    let parent_trace_segment_id = b64_encoded_into_string(pieces[2])?;
    let parent_span_id: i32 = try_parse_parent_span_id(pieces[3])?;
    let parent_service = b64_encoded_into_string(pieces[4])?;
    let parent_service_instance = b64_encoded_into_string(pieces[5])?;
    let destination_endpoint = b64_encoded_into_string(pieces[6])?;
    let destination_address = b64_encoded_into_string(pieces[7])?;

    let context = PropagationContext::new(
        do_sample,
        parent_trace_id,
        parent_trace_segment_id,
        parent_span_id,
        parent_service,
        parent_service_instance,
        destination_endpoint,
        destination_address,
    );

    Ok(context)
}

fn try_parse_parent_span_id(id: &str) -> Result<i32, &str> {
    if let Ok(result) = id.parse::<i32>() {
        Ok(result)
    } else {
        Err("failed to parse span id from parent.")
    }
}

fn try_parse_sample_status(status: &str) -> Result<bool, &str> {
    if status == "0" {
        Ok(false)
    } else if status == "1" {
        Ok(true)
    } else {
        Err("failed to parse sample status.")
    }
}

fn b64_encoded_into_string(enc: &str) -> Result<String, &str> {
    if let Ok(result) = decode(enc) {
        if let Ok(decoded_str) = String::from_utf8(result) {
            return Ok(decoded_str);
        }
    }

    Err("failed to decode value.")
}
