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

use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::rc::Rc;

use lazy_static::lazy_static;

use crate::skywalking::agent::reporter::Reporter;
use crate::skywalking::core::{Context, ContextListener, Extractable, Span, TracingContext};

thread_local!( static CTX: RefCell<CurrentTracingContext> = RefCell::new(CurrentTracingContext::new()));
lazy_static! {
    static ref SKYWALKING_REPORTER : Reporter = {
        Reporter::new()
    };
}

pub struct ContextManager {}

impl ContextManager {
    pub fn createEntrySpan(operation_name: &str, parent: Option<&Box<dyn Span>>, extractor: Option<&dyn Extractable>) {
        CTX.with(|context| {
            context.borrow_mut().createEntrySpan(operation_name, parent, extractor);
        });
    }
}

struct CurrentTracingContext {
    context: Option<Box<TracingContext>>,
    active_spans: Box<Vec<Rc<Box<dyn Span>>>>,
}

impl CurrentTracingContext {
    fn new() -> Self {
        CurrentTracingContext {
            context: match TracingContext::new(SKYWALKING_REPORTER.service_instance_id()) {
                Some(tx) => { Some(Box::new(tx)) }
                None => { None }
            },
            active_spans: Box::new(Vec::new()),
        }
    }

    pub fn createEntrySpan(&mut self, operation_name: &str, parent: Option<&Box<dyn Span>>, extractor: Option<&dyn Extractable>) -> Option<Rc<Box<dyn Span>>> {
        match self.context.borrow_mut() {
            None => { None }
            Some(txt) => {
                let mut span = Rc::new(txt.create_entry_span(operation_name, parent, extractor));
                let response = Rc::clone(&span);
                self.active_spans.push(span);
                Some(response)
            }
        }
    }
}


#[cfg(test)]
mod context_tests {
    use crate::skywalking::agent::context_manager::*;
    use crate::skywalking::core::{ContextListener, TracingContext};

    #[test]
    fn test_static_reporter() {
//REPORTER.set_reporter(Box::new(MockReporter {}));
    }

    struct MockReporter {}

    impl ContextListener for MockReporter {
        fn service_instance_id(&self) -> Option<i32> {
            Some(1)
        }

        fn report_trace(&self, finished_context: TracingContext) {}
    }
}
