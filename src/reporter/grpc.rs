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
use crate::skywalking_proto::v3::{
    trace_segment_report_service_client::TraceSegmentReportServiceClient, SegmentObject,
};
use futures_util::stream;
use std::collections::LinkedList;
use tonic::{
    async_trait,
    transport::{self, Channel, Endpoint},
};

type ReporterClient = TraceSegmentReportServiceClient<Channel>;

pub struct GrpcReporter {
    client: ReporterClient,
}

impl GrpcReporter {
    pub fn new(channel: Channel) -> Self {
        let client = ReporterClient::new(channel);
        Self { client }
    }

    pub async fn connect(
        address: impl TryInto<Endpoint, Error = transport::Error>,
    ) -> crate::Result<Self> {
        let client = ReporterClient::connect(address.try_into()?).await?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Reporter for GrpcReporter {
    async fn collect(&mut self, segments: LinkedList<SegmentObject>) -> crate::Result<()> {
        let stream = stream::iter(segments);
        self.client.collect(stream).await?;
        Ok(())
    }
}
