Apache SkyWalking Rust Agent
==========

<img src="http://skywalking.apache.org/assets/logo.svg" alt="Sky Walking logo" height="90px" align="right" />

[![Twitter Follow](https://img.shields.io/twitter/follow/asfskywalking.svg?style=for-the-badge&label=Follow&logo=twitter)](https://twitter.com/AsfSkyWalking)

[![Crates](https://img.shields.io/badge/skywalking-crates.io-blue)](https://crates.io/crates/skywalking)
![CI](https://github.com/apache/skywalking-rust/workflows/CI/badge.svg?branch=master)

[**SkyWalking**](https://github.com/apache/skywalking) Rust Agent provides observability capability for Rust App and
Library, including tracing, metrics, topology map for distributed system and alert. It uses SkyWalking native formats
and core concepts to keep best compatibility and performance.

# Concepts

All concepts are from the official SkyWalking definitions.

## Span

Span is an important and common concept in distributed tracing system. Learn Span from Google Dapper Paper. For better
performance, we extend the span into 3 kinds.

1. EntrySpan EntrySpan represents a service provider, also the endpoint of server side. As an APM system, we are
   targeting the application servers. So almost all the services and MQ-consumer are EntrySpan(s).
2. LocalSpan LocalSpan represents a normal Java method, which does not relate to remote service, neither a MQ
   producer/consumer nor a service(e.g. HTTP service) provider/consumer.
3. ExitSpan ExitSpan represents a client of service or MQ-producer, as named as LeafSpan at early age of SkyWalking.
   e.g. accessing DB by JDBC, reading Redis/Memcached are cataloged an ExitSpan.

Tag and Log are similar attributes of the span.

- Tag is a key:value pair to indicate the attribute with a string value.
- Log is heavier than tag, with one timestamp and multiple key:value pairs. Log represents an event, typically an error
  happens.

## TracingContext

TracingContext is the context of the tracing process. Span should only be created through context, and be archived into
the context after the span finished.

# Example

```rust, no_run
use skywalking::context::tracer::Tracer;
use skywalking::reporter::grpc::GrpcReporter;
use std::error::Error;
use tokio::signal;

async fn handle_request(tracer: Tracer) {
    let mut ctx = tracer.create_trace_context();

    {
        // Generate an Entry Span when a request is received.
        // An Entry Span is generated only once per context.
        // Assign a variable name to guard the span not to be dropped immediately.
        let _span = ctx.create_entry_span("op1");

        // Something...

        {
            // Generates an Exit Span when executing an RPC.
            let _span2 = ctx.create_exit_span("op2", "remote_peer");

            // Something...

            // Auto close span2 when dropped.
        }

        // Auto close span when dropped.
    }

    // Auto report ctx when dropped.
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let reporter = GrpcReporter::connect("http://0.0.0.0:11800").await?;
    let tracer = Tracer::new("service", "instance", reporter);

    tokio::spawn(handle_request(tracer.clone()));

    // Start to report.
    let handle = tracer.reporting(async move {
        let _ = signal::ctrl_c().await;
    });

    handle.await?;

    Ok(())
}
```

# How to compile?

If you have `skywalking-(VERSION).crate`, you can unpack it with the way as follows:

```shell
tar -xvzf skywalking-(VERSION).crate
```

Using `cargo build` generates a library. If you'd like to verify the behavior, we recommend to
use `cargo run --example simple_trace_report`
which outputs executable, then run it.

# Release
The SkyWalking committer(PMC included) could follow [this doc](Release-guide.md) to release an official version.

# License

Apache 2.0
