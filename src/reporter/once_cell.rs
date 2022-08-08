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

use crate::reporter::{CollectItem, Report};
use tokio::sync::OnceCell;

#[derive(Clone, Default)]
pub struct OnceCellReporter<R: Report> {
    report: OnceCell<R>,
}

impl<R: Report> OnceCellReporter<R> {
    pub fn set(&self, report: R) -> Option<()> {
        self.report.set(report).ok()
    }
}

impl<R: Report> Report for OnceCellReporter<R> {
    fn report(&self, item: CollectItem) {
        self.report
            .get()
            .expect("Report hasn't setted")
            .report(item)
    }
}
