use skywalking_rust::context::trace_context::TracingContext;
use skywalking_rust::reporter::grpc::Reporter;
use tokio;

#[tokio::main]
async fn main() {
    let tx = Reporter::start("http://0.0.0.0:11800").await;
    let mut context = TracingContext::default("service", "instance");
    {
        let span = context.create_entry_span("op1").unwrap();
        context.finalize_span(span);
    }
    let _ = tx.send(context).await;
}
