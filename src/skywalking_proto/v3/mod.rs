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
use crate::context::system_time::{fetch_time, TimePeriod};

tonic::include_proto!("skywalking.v3");

impl SpanObject {
    /// Add logs to the span.
    pub fn add_log<K, V, I>(&mut self, message: I)
    where
        K: ToString,
        V: ToString,
        I: IntoIterator<Item = (K, V)>,
    {
        let log = Log {
            time: fetch_time(TimePeriod::Log),
            data: message
                .into_iter()
                .map(|v| {
                    let (key, value) = v;
                    KeyStringValuePair {
                        key: key.to_string(),
                        value: value.to_string(),
                    }
                })
                .collect(),
        };
        self.logs.push(log);
    }

    /// Add tag to the span.
    pub fn add_tag(&mut self, key: impl ToString, value: impl ToString) {
        self.tags.push(KeyStringValuePair {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
}
