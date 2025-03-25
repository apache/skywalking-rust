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

//! Generated code of `skywalking.v3`, by `tonic`.

#![allow(missing_docs)]
#![allow(rustdoc::invalid_html_tags)]
#![allow(clippy::derive_partial_eq_without_eq)]

use crate::common::system_time::{TimePeriod, fetch_time};

tonic::include_proto!("skywalking.v3");

impl SpanObject {
    /// Add logs to the span.
    pub fn add_log<K, V, I>(&mut self, message: I)
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        let log = Log {
            time: fetch_time(TimePeriod::Log),
            data: message
                .into_iter()
                .map(|v| {
                    let (key, value) = v;
                    KeyStringValuePair {
                        key: key.into(),
                        value: value.into(),
                    }
                })
                .collect(),
        };
        self.logs.push(log);
    }

    /// Add tag to the span.
    pub fn add_tag(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.tags.push(KeyStringValuePair {
            key: key.into(),
            value: value.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    trait AssertSerialize: serde::Serialize {}

    impl AssertSerialize for SegmentObject {}

    impl AssertSerialize for SpanObject {}

    #[allow(dead_code)]
    trait AssertDeserialize<'de>: serde::Deserialize<'de> {}

    impl AssertDeserialize<'_> for SegmentObject {}

    impl AssertDeserialize<'_> for SpanObject {}
}
