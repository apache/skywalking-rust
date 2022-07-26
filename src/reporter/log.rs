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

use super::Reporter;
use crate::skywalking_proto::v3::SegmentObject;
use std::{collections::LinkedList, error::Error};
use tonic::async_trait;

enum Used {
    Println,
    Tracing,
}

pub struct LogReporter {
    tip: String,
    used: Used,
}

impl LogReporter {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    pub fn tip(mut self, tip: String) -> Self {
        self.tip = tip;
        self
    }

    pub fn use_tracing(mut self) -> Self {
        self.used = Used::Tracing;
        self
    }

    pub fn use_println(mut self) -> Self {
        self.used = Used::Println;
        self
    }
}

impl Default for LogReporter {
    fn default() -> Self {
        Self {
            tip: "Collect".to_string(),
            used: Used::Println,
        }
    }
}

#[async_trait]
impl Reporter for LogReporter {
    async fn collect(&mut self, segments: LinkedList<SegmentObject>) -> Result<(), Box<dyn Error>> {
        for segment in segments {
            match self.used {
                Used::Println => println!("{} segment={:?}", self.tip, segment),
                Used::Tracing => tracing::info!(?segment, "{}", self.tip),
            }
        }
        Ok(())
    }
}
