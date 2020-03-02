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

/// Tag is a key value pair to represent an supplementary instruction for the span.
/// Common and most widely used tags could be found here,
/// https://github.com/apache/skywalking/blob/master/apm-sniffer/apm-agent-core/src/main/java/org/apache/skywalking/apm/agent/core/context/tag/Tags.java.
#[derive(Clone, Hash)]
pub struct Tag {
    key: String,
    value: String,
}

impl Tag {
    pub fn new(key: String, value: String) -> Self {
        Tag {
            key,
            value,
        }
    }

    pub fn key(&self) -> String {
        self.key.clone()
    }

    pub fn value(&self) -> String {
        self.value.clone()
    }
}

#[cfg(test)]
mod tag_tests {
    use crate::skywalking::core::Tag;

    #[test]
    fn test_tag_new() {
        let tag = Tag::new(String::from("tag_key"), String::from("tag_value"));
        assert_eq!(tag.key, "tag_key");
        assert_eq!(tag.value, "tag_value");
        let tag_clone = tag.clone();
        assert_eq!(tag_clone.key, "tag_key");
        assert_eq!(tag_clone.value, "tag_value");
    }
}