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

pub mod grpc;
pub mod log;

use crate::skywalking_proto::v3::SegmentObject;
use std::collections::LinkedList;
use std::{error::Error, result::Result};
use tonic::async_trait;

pub(crate) type DynReporter = dyn Reporter + Send + Sync + 'static;

#[async_trait]
pub trait Reporter {
    async fn collect(&mut self, segments: LinkedList<SegmentObject>) -> Result<(), Box<dyn Error>>;

    fn sync_collect(&mut self, segments: LinkedList<SegmentObject>) -> Result<(), Box<dyn Error>>;
}
