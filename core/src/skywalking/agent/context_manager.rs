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

use std::borrow::{Borrow, BorrowMut};
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
    pub fn tracing_entry<F>(operation_name: &str, extractor: Option<&dyn Extractable>, f: F)
        where F: FnOnce(Box<dyn Span>) -> Box<dyn Span> {
        CTX.with(|context| {
            let span = context.borrow_mut().create_entry_span(operation_name, context.borrow().parent_span_id(), extractor);
            match span {
                None => {}
                Some(s) => {
                    let s = f(s);

                    let is_first_span = s.span_id() == 0;
                    context.borrow_mut().finish_span(s);

                    if is_first_span {
                        context.borrow_mut().finish();
                    }
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
    span_stack: Vec<i32>,
}

impl CurrentTracingContext {
    fn new() -> Self {
        CurrentTracingContext {
            option: match TracingContext::new(SKYWALKING_REPORTER.service_instance_id()) {
                Some(tx) => {
                    Some(Box::new(WorkingContext {
                        context: Box::new(tx),
                        span_stack: Vec::new(),
                    }))
                }
                None => { None }
            },
        }
    }

    fn create_entry_span(&mut self, operation_name: &str, parent_span_id: Option<i32>, extractor: Option<&dyn Extractable>) -> Option<Box<dyn Span>> {
        match self.option.borrow_mut() {
            None => { None }
            Some(wx) => {
                let span = wx.context.create_entry_span(operation_name, parent_span_id, extractor);
                wx.span_stack.push(span.span_id());
                Some(span)
            }
        }
    }

    fn finish_span(&mut self, span: Box<dyn Span>) {
        match self.option.borrow_mut() {
            None => {}
            Some(wx) => {
                wx.context.finish_span(span);
                wx.span_stack.pop();
            }
        };
    }

    fn parent_span_id(&self) -> Option<i32> {
        match self.option.borrow() {
            None => { None }
            Some(wx) => {
                match wx.span_stack.last() {
                    None => { None }
                    Some(span_id) => { Some(span_id.clone()) }
                }
            }
        }
    }

    fn finish(&mut self) {
        match self.option.borrow_mut() {
            None => {}
            Some(wx) => {
                let tracingContext = &wx.context;
                wx.span_stack.clear();

                // TODO: Transfer tracingContext to protobuf
            }
        }
        self.option = None;
    }
}


#[cfg(test)]
mod context_tests {
    use crate::skywalking::agent::context_manager::*;
    use crate::skywalking::core::{ContextListener, Tag, TracingContext};

    #[test]
    fn test_context_manager() {
        ContextManager::tracing_entry("op1", None, |mut span| {
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
