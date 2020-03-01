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

use crate::TracingContext;

///Report bridge defines the traits for the skywalking-report

/// Register implementation communicate with the SkyWalking OAP backend.
/// It does metadata register, traces report, and runtime status report or interaction.
pub trait ContextListener {
    /// Return the registered service id
    /// If haven't registered successfully, return None.
    fn service_instance_id(&self) -> Option<i32>;

    /// Move the finished and inactive context to the reporter.
    /// The reporter should use async way to transport the data to the backend through HTTP, gRPC or SkyWalking forwarder.
    fn report_trace(&self, finished_context: TracingContext);
}
