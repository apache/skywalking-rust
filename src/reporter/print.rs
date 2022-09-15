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

#[derive(Default, Clone)]
pub struct PrintReporter {
    use_stderr: bool,
}

impl PrintReporter {
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    pub fn use_stderr(mut self, use_stderr: bool) -> Self {
        self.use_stderr = use_stderr;
        self
    }
}

impl Report for PrintReporter {
    fn report(&self, items: CollectItem) {
        match items {
            CollectItem::Trace(data) => {
                if self.use_stderr {
                    eprintln!("trace segment={:?}", data);
                } else {
                    println!("trace segment={:?}", data);
                }
            }
            CollectItem::Log(data) => {
                if self.use_stderr {
                    eprintln!("log data={:?}", data);
                } else {
                    println!("log data={:?}", data);
                }
            }
            CollectItem::Meter(data) => {
                if self.use_stderr {
                    eprintln!("meter data={:?}", data);
                } else {
                    println!("meter data={:?}", data);
                }
            }
            #[cfg(feature = "management")]
            CollectItem::Instance(data) => {
                if self.use_stderr {
                    eprintln!("instance properties data={:?}", data);
                } else {
                    println!("instance properties data={:?}", data);
                }
            }
            #[cfg(feature = "management")]
            CollectItem::Ping(data) => {
                if self.use_stderr {
                    eprintln!("ping data={:?}", data);
                } else {
                    println!("ping data={:?}", data);
                }
            }
        }
    }
}
