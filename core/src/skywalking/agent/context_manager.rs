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
use crate::skywalking::core::{Context, ContextListener, Extractable, Injectable, Span, TracingContext};
use crate::skywalking::core::span::TracingSpan;

/// Thread Local tracing context. Host the context and propagate it in each thread if needed.
thread_local!( static CTX: RefCell<CurrentTracingContext> = RefCell::new(CurrentTracingContext::new()));
/// Global reporter
/// Status: WIP
lazy_static! {
    static ref SKYWALKING_REPORTER : Reporter = {
        Reporter::new()
    };
}

pub struct ContextManager {}

impl ContextManager {
    /// Run a closure under an entry span observing.
    /// Span is automatically created, started and ended around the closure f.
    pub fn tracing_entry<F>(operation_name: &str, extractor: Option<&dyn Extractable>, f: F)
        where F: FnOnce(&mut Box<dyn Span>) {
        CTX.with(|context| {
            let span;
            {
                // Borrow mut ref has to end in this specific scope, as the context is nested used in f<F>
                let mut mut_context = context.borrow_mut();
                let parent_span_id = mut_context.parent_span_id();
                span = mut_context.create_entry_span(operation_name, parent_span_id, extractor);
            }
            match span {
                None => {}
                Some(mut s) => {
                    s.start();
                    f(s.borrow_mut());
                    s.end();

                    let is_first_span = s.span_id() == 0;
                    let mut mut_context = context.borrow_mut();
                    mut_context.finish_span(s);

                    if is_first_span {
                        mut_context.finish();
                    }
                }
            };
        });
    }

    /// Run a closure under an exit span observing.
    /// Span is automatically created, started and ended around the closure f.
    pub fn tracing_exit<F>(operation_name: &str, peer: &str, injector: Option<&dyn Injectable>, f: F)
        where F: FnOnce(&mut Box<dyn Span>) {
        CTX.with(|context| {
            let span;
            {
                // Borrow mut ref has to end in this specific scope, as the context is nested used in f<F>
                let mut mut_context = context.borrow_mut();
                let parent_span_id = mut_context.parent_span_id();
                span = mut_context.create_exit_span(operation_name, parent_span_id, peer, injector);
            }
            match span {
                None => {}
                Some(mut s) => {
                    s.start();
                    f(s.borrow_mut());
                    s.end();

                    let is_first_span = s.span_id() == 0;

                    let mut mut_context = context.borrow_mut();
                    mut_context.finish_span(s);

                    if is_first_span {
                        mut_context.finish();
                    }
                }
            };
        });
    }

    /// Run a closure under a local span observing.
    /// Span is automatically created, started and ended around the closure f.
    pub fn tracing_local<F>(operation_name: &str, f: F)
        where F: FnOnce(&mut Box<dyn Span>) {
        CTX.with(|context| {
            let span;
            {
                // Borrow mut ref has to end in this specific scope, as the context is nested used in f<F>
                let mut mut_context = context.borrow_mut();
                let parent_span_id = mut_context.parent_span_id();
                span = mut_context.create_local(operation_name, parent_span_id);
            }
            match span {
                None => {}
                Some(mut s) => {
                    s.start();
                    f(s.borrow_mut());
                    s.end();

                    let is_first_span = s.span_id() == 0;

                    let mut mut_context = context.borrow_mut();
                    mut_context.finish_span(s);

                    if is_first_span {
                        mut_context.finish();
                    }
                }
            };
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
    /// Create the tracing context in the thread local at the first time.
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

    /// Delegate to the tracing core entry span creation method, if current context is valid.
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

    /// Delegate to the tracing core exit span creation method, if current context is valid.
    fn create_exit_span(&mut self, operation_name: &str, parent_span_id: Option<i32>, peer: &str, injector: Option<&dyn Injectable>) -> Option<Box<dyn Span>> {
        match self.option.borrow_mut() {
            None => { None }
            Some(wx) => {
                let span = wx.context.create_exit_span(operation_name, parent_span_id, peer, injector);
                wx.span_stack.push(span.span_id());
                Some(span)
            }
        }
    }

    /// Delegate to the tracing core local span creation method, if current context is valid.
    fn create_local(&mut self, operation_name: &str, parent_span_id: Option<i32>) -> Option<Box<dyn Span>> {
        match self.option.borrow_mut() {
            None => { None }
            Some(wx) => {
                let span = wx.context.create_local_span(operation_name, parent_span_id);
                wx.span_stack.push(span.span_id());
                Some(span)
            }
        }
    }

    /// Delegate to the tracing core span finish method, if current context is valid.
    fn finish_span(&mut self, span: Box<dyn Span>) {
        match self.option.borrow_mut() {
            None => {}
            Some(wx) => {
                wx.context.finish_span(span);
                wx.span_stack.pop();
            }
        };
    }

    /// Fetch the parent span id, to be used in next new span.
    /// The span id(s) are saved in the span_stack by following as same the stack-style as span creation sequence.
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

    /// Finish the current tracing context, including
    /// 1. Clear up the context
    /// 2. Transfer the context to profobuf format and pass to reporter.
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

            ContextManager::tracing_exit("op2", "127.0.0.1:8080", None, |mut span| {
                span.set_component_id(33);
            });

            ContextManager::tracing_local("op3", |mut span| {});
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
