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

//! Crate errors.

pub(crate) const LOCK_MSG: &str = "should not cross threads/coroutines (locked)";

/// Skywalking Result.
pub type Result<T> = std::result::Result<T, Error>;

/// Skywalking Error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Decode propagation failed.
    #[error("decode propagation failed: {0}")]
    DecodePropagation(&'static str),

    /// Reporter shutdown failed.
    #[error("reporter shutdown failed: {0}")]
    ReporterShutdown(String),

    /// Tonic transport failed.
    #[error("tonic transport failed: {0}")]
    TonicTransport(#[from] tonic::transport::Error),

    /// Tonic status error.
    #[error("tonic status: {0}")]
    TonicStatus(#[from] tonic::Status),

    /// Tokio join failed.
    #[error("tokio join failed: {0}")]
    TokioJoin(#[from] tokio::task::JoinError),

    /// Other uncovered errors.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + 'static>),
}
