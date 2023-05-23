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

use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
pub(crate) struct WaitGroup {
    inner: Arc<Inner>,
}

struct Inner {
    var: Condvar,
    count: Mutex<usize>,
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                var: Condvar::new(),
                count: Mutex::new(0),
            }),
        }
    }
}

impl WaitGroup {
    pub(crate) fn add(&self, n: usize) {
        *self.inner.count.lock().unwrap() += n;
    }

    pub(crate) fn done(&self) {
        let mut count = self.inner.count.lock().unwrap();
        *count -= 1;
        if *count == 0 {
            self.inner.var.notify_all();
        }
    }

    pub(crate) fn wait(self) {
        let mut count = self.inner.count.lock().unwrap();
        while *count > 0 {
            count = self.inner.var.wait(count).unwrap();
        }
    }
}
