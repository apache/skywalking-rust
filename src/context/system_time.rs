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

use cfg_if::cfg_if;

pub(crate) enum TimePeriod {
    Start,
    Log,
    End,
}

cfg_if! {
    if #[cfg(test)] {
        pub(crate) fn fetch_time(period: TimePeriod) -> i64 {
            match period {
                TimePeriod::Start => 1,
                TimePeriod::Log => 10,
                TimePeriod::End => 100,
            }
        }
    } else {
        pub(crate) fn fetch_time(_period: TimePeriod) -> i64 {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|dur| dur.as_millis() as i64)
                .unwrap_or_default()
        }
    }
}
