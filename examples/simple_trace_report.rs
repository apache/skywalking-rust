use skywalking_rust::context::system_time::UnixTimeStampFetcher;
use skywalking_rust::context::trace_context::TracingContext;
use skywalking_rust::reporter::grpc::Reporter;
use std::sync::Arc;
use tokio;

#[tokio::main]
async fn main() {
    let tx = Reporter::start("http://0.0.0.0:11800".to_string()).await;
    let mut context =
        TracingContext::default_internal("service", "instance");
    {
        let span = context.create_entry_span("op1").unwrap();
        context.finalize_span(span);
    }
    let _ = tx.send(context).await;
}
