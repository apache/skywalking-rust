use std::error::Error;

use skywalking_rust::context::trace_context::TracingContext;
use skywalking_rust::reporter::grpc::Reporter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let reporter = Reporter::start("http://0.0.0.0:11800").await;
    let mut context = TracingContext::default("service", "instance");
    {
        let span = context.create_entry_span("op1").unwrap();
        context.finalize_span(span);
    }
    reporter.sender().send(context).await?;
    reporter.shutdown().await?;
    Ok(())
}
