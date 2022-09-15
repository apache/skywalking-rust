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

use super::record::LogRecord;
use crate::reporter::{CollectItem, DynReport, Report};
use std::sync::Arc;
use tokio::sync::OnceCell;

static GLOBAL_LOGGER: OnceCell<Logger> = OnceCell::const_new();

/// Set the global logger.
pub fn set_global_logger(logger: Logger) {
    if GLOBAL_LOGGER.set(logger).is_err() {
        panic!("global logger has setted")
    }
}

/// Get the global logger.
pub fn global_logger() -> &'static Logger {
    GLOBAL_LOGGER.get().expect("global logger haven't setted")
}

/// Log by global logger.
pub fn log(record: LogRecord) {
    global_logger().log(record);
}

pub struct Inner {
    service_name: String,
    instance_name: String,
    reporter: Box<DynReport>,
}

#[derive(Clone)]
pub struct Logger {
    inner: Arc<Inner>,
}

impl Logger {
    /// New with service info and reporter.
    pub fn new(
        service_name: impl Into<String>,
        instance_name: impl Into<String>,
        reporter: impl Report + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                service_name: service_name.into(),
                instance_name: instance_name.into(),
                reporter: Box::new(reporter),
            }),
        }
    }

    pub fn service_name(&self) -> &str {
        &self.inner.service_name
    }

    pub fn instance_name(&self) -> &str {
        &self.inner.instance_name
    }

    pub fn log(&self, record: LogRecord) {
        let data = record.convert_to_log_data(
            self.service_name().to_owned(),
            self.instance_name().to_owned(),
        );
        self.inner.reporter.report(CollectItem::Log(Box::new(data)));
    }
}
