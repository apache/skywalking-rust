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
use crate::skywalking::core::span::TracingSpan;

thread_local!( static CTX: RefCell<CurrentTracingContext> = RefCell::new(CurrentTracingContext::new()));
lazy_static! {
    static ref SKYWALKING_REPORTER : Reporter = {
        Reporter::new()
    };
}

pub struct ContextManager {}

impl ContextManager {
    pub fn tracing_entry<F>(operation_name: &str, parent: Option<&Box<dyn Span>>, extractor: Option<&dyn Extractable>, f: F)
        where F: FnOnce(Box<dyn Span>) -> Box<dyn Span> {
        CTX.with(|context| {
            let span = context.borrow_mut().create_entry_span(operation_name, parent, extractor);
            match span {
                None => {}
                Some(s) => {
                    let s = f(s);
                    let is_first_span = s.span_id() == 0;
                    context.borrow_mut().finish_span(s);

                    if is_first_span { context.borrow_mut().finish() }
                }
            }
        });
    }
}

struct CurrentTracingContext {
    option: Option<Box<WorkingContext>>,
}

struct WorkingContext {
    context: Box<TracingContext>,
}

impl CurrentTracingContext {
    fn new() -> Self {
        CurrentTracingContext {
            option: match TracingContext::new(SKYWALKING_REPORTER.service_instance_id()) {
                Some(tx) => {
                    Some(Box::new(WorkingContext {
                        context: Box::new(tx)
                    }))
                }
                None => { None }
            },
        }
    }

    fn create_entry_span(&mut self, operation_name: &str, parent: Option<&Box<dyn Span>>, extractor: Option<&dyn Extractable>) -> Option<Box<dyn Span>> {
        match self.option.borrow_mut() {
            None => { None }
            Some(wx) => {
                Some(wx.context.create_entry_span(operation_name, parent, extractor))
            }
        }
    }

    fn finish_span(&mut self, span: Box<dyn Span>) {
        match self.option.borrow_mut() {
            None => {}
            Some(wx) => {
                wx.context.finish_span(span);
            }
        };
    }

    fn finish(&mut self) {
    }
}


#[cfg(test)]
mod context_tests {
    use crate::skywalking::agent::context_manager::*;
    use crate::skywalking::core::{ContextListener, Tag, TracingContext};

    #[test]
    fn test_context_manager() {
        ContextManager::tracing_entry("op1", None, None, |mut span| {
            span.tag(Tag::new(String::from("tag1"), String::from("value1")));
            span
        });
    }

    struct MockReporter {}

    impl ContextListener for MockReporter {
        fn service_instance_id(&self) -> Option<i32> {
            Some(1)
        }

        fn report_trace(&self, finished_context: Box<TracingContext>) {}
    }
}
