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

## Tracing

### Span

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

### TracingContext

TracingContext is the context of the tracing process. Span should only be created through context, and be archived into
the context after the span finished.

## Logging

### LogRecord

LogRecord is the simple builder for the LogData, which is the Log format of Skywalking.

## Metrics

### Meter

- **Counter** API represents a single monotonically increasing counter which automatically collects data and reports to the backend.
- **Gauge** API represents a single numerical value.
- **Histogram** API represents a summary sample observations with customized buckets.

# Example

```rust, no_run
use skywalking::{
    logging::{logger::Logger, record::{LogRecord, RecordType}},
    reporter::grpc::GrpcReporter,
    trace::tracer::Tracer,
    metrics::{meter::Counter, metricer::Metricer},
};
use std::error::Error;
use tokio::signal;

async fn handle_request(tracer: Tracer, logger: Logger) {
    let mut ctx = tracer.create_trace_context();

    {
        // Generate an Entry Span when a request is received.
        // An Entry Span is generated only once per context.
        // Assign a variable name to guard the span not to be dropped immediately.
        let _span = ctx.create_entry_span("op1");

        // Something...

        {
            // Generates an Exit Span when executing an RPC.
            let span2 = ctx.create_exit_span("op2", "remote_peer");

            // Something...

            // Do logging.
            logger.log(
                LogRecord::new()
                    .add_tag("level", "INFO")
                    .with_tracing_context(&ctx)
                    .with_span(&span2)
                    .record_type(RecordType::Text)
                    .content("Something...")
            );

            // Auto close span2 when dropped.
        }

        // Auto close span when dropped.
    }

    // Auto report ctx when dropped.
}

async fn handle_metric(mut metricer: Metricer) {
    let counter = metricer.register(
        Counter::new("instance_trace_count")
            .add_label("region", "us-west")
            .add_label("az", "az-1"),
    );

    metricer.boot().await;

    counter.increment(10.);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Connect to skywalking oap server.
    let reporter = GrpcReporter::connect("http://0.0.0.0:11800").await?;

    // Spawn the reporting in background, with listening the graceful shutdown signal.
    let handle = reporter
        .reporting()
        .await
        .with_graceful_shutdown(async move {
            signal::ctrl_c().await.expect("failed to listen for event");
        })
        .spawn();

    let tracer = Tracer::new("service", "instance", reporter.clone());
    let logger = Logger::new("service", "instance", reporter.clone());
    let metricer = Metricer::new("service", "instance", reporter);

    handle_metric(metricer).await;

    handle_request(tracer, logger).await;

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

## NOTICE

This crate automatically generates protobuf related code, which requires `protoc` before compile.

Please choose one of the ways to install `protoc`.

1. Using your OS package manager.

   For Debian-base system:

   ```shell
   sudo apt install protobuf-compiler
   ```

   For MacOS:

   ```shell
   brew install protobuf
   ```

2. Auto compile `protoc` in the crate build script, just by adding the feature `vendored` in the `Cargo.toml`:

   ```shell
   cargo add skywalking --features vendored
   ```

3. Build from [source](https://github.com/protocolbuffers/protobuf). If `protc` isn't install inside $PATH, the env value `PROTOC` should be set.

   ```shell
   PROTOC=/the/path/of/protoc
   ```

For details, please refer to [prost-build:sourcing-protoc](https://docs.rs/prost-build/latest/prost_build/index.html#sourcing-protoc).

# Release

The SkyWalking committer(PMC included) could follow [this doc](https://github.com/apache/skywalking-rust/blob/master/Release-guide.md) to release an official version.

# License

Apache 2.0
