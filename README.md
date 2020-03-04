Apache SkyWalking Rust Agent
==========

<img src="http://skywalking.apache.org/assets/logo.svg" alt="Sky Walking logo" height="90px" align="right" />

[![Twitter Follow](https://img.shields.io/twitter/follow/asfskywalking.svg?style=for-the-badge&label=Follow&logo=twitter)](https://twitter.com/AsfSkyWalking)

![CI](https://github.com/apache/skywalking-nginx-lua/workflows/CI/badge.svg?branch=master)


[**SkyWalking**](https://github.com/apache/skywalking) Rust Agent provides observability capability for Rust App and Library, 
including tracing, metrics, topology map for distributed system and alert.
It uses SkyWalking native formats and core concepts to keep best compatibility and performance.

# Concepts
All concepts are from the official SkyWalking definitions.
## Span
Span is an important and common concept in distributed tracing system. Learn Span from Google Dapper Paper.
For better performance, we extend the span into 3 kinds.
   
1. EntrySpan EntrySpan represents a service provider, also the endpoint of server side. As an APM system, we are targeting the application servers. So almost all the services and MQ-consumer are EntrySpan(s).
2. LocalSpan LocalSpan represents a normal Java method, which does not relate to remote service, neither a MQ producer/consumer nor a service(e.g. HTTP service) provider/consumer.
3. ExitSpan ExitSpan represents a client of service or MQ-producer, as named as LeafSpan at early age of SkyWalking. e.g. accessing DB by JDBC, reading Redis/Memcached are cataloged an ExitSpan.

Tag and Log are similar attributes of the span. 
- Tag is a key:value pair to indicate the attribute with a string value.
- Log is heavier than tag, with one timestamp and multiple key:value pairs. Log represents an event, typically an error happens.

## TracingContext
TracingContext is the context of the tracing process. Span should only be created through context, and be archived into the
context after the span finished.

## Injectable
Injectable is used(optional) when the exit span creates. This Injectable received the notification from tracing context,
including the key and value for tracing context across process propagation. Typically, Injectable implementation would 
manipulate the RPC header/metadata to make the key/value sent to the server side.

## Extractable
Extractable is used(optional) when the entry span creates. The Extractable fetches the value of the given key from the propagated
context. Typically, Extractable implementation would read the RPC header/metadata, which sent from the client side. 

# APIs
## High-Level APIs
High level APIs are targeting convenient usages. These APIs use the ThreadLocal to propagate the context, so users could
create span at any moment, and the context will finish automatically once the first created span of this thread stopped.

```rust
ContextManager::tracing_entry("op1", Some(&injector), |mut span| {
    // Use span freely in this closure
    // Span's start/end time is set automatically with this closure start/end(s).
    span.tag(Tag::new(String::from("tag1"), String::from("value1")));

    ContextManager::tracing_exit("op2", "127.0.0.1:8080", Some(&extractor), |mut span| {
        span.set_component_id(33);
    });

    ContextManager::tracing_local("op3", |mut span| {});
});
```

## Low-Level Core APIs
Tracing core APIs are 100% manual control tracing APIs. Users could use them to trace any process by following SkyWalking
core concepts.

Low Level APIs request users to create and hold the context and span by the codes manually.

```rust
let mut context = TracingContext::new(reporter.service_instance_id()).unwrap();
let span1 = context.create_entry_span("op1", None, Some(&dyn injector));
{
    assert_eq!(span1.span_id(), 0);
    let mut span2 = context.create_local_span("op2", Some(&span1));
    span2.tag(Tag::new(String::from("tag1"), String::from("value1")));
    {
        assert_eq!(span2.span_id(), 1);
        let mut span3 = context.create_exit_span("op3", Some(&span2), "127.0.0.1:8080", Some(&dyn extractor));
        assert_eq!(span3.span_id(), 2);

        context.finish_span(span3);
    }
    context.finish_span(span2);
}
context.finish_span(span1);

reporter.report_trace(context);
```

# License
Apache 2.0 