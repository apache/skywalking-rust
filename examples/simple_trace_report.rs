use std::error::Error;

use tokio;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};

use skywalking_rust::context::trace_context::TracingContext;
use skywalking_rust::reporter::grpc::Reporter;
use skywalking_rust::skywalking_proto::v3::trace_segment_report_service_server::TraceSegmentReportService;
use skywalking_rust::skywalking_proto::v3::trace_segment_report_service_server::TraceSegmentReportServiceServer;
use skywalking_rust::skywalking_proto::v3::{Commands, SegmentCollection, SegmentObject};

#[derive(Default)]
pub struct TraceSegmentReportServer;

impl TraceSegmentReportServer {
    pub fn into_service(self) -> TraceSegmentReportServiceServer<TraceSegmentReportServer> {
        TraceSegmentReportServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl TraceSegmentReportService for TraceSegmentReportServer {
    async fn collect(
        &self,
        request: Request<Streaming<SegmentObject>>,
    ) -> Result<Response<Commands>, Status> {
        let mut streams = request.into_inner();
        while let Some(segment) = streams.next().await {
            println!("segment: {:?}", segment);
        }
        Ok(Response::new(Commands::default()))
    }

    async fn collect_in_sync(
        &self,
        request: Request<SegmentCollection>,
    ) -> Result<Response<Commands>, Status> {
        println!("request: {:?}", request.into_inner());
        Ok(Response::new(Commands::default()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(TraceSegmentReportServer::default().into_service())
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    let reporter = Reporter::start(format!("https://{}", addr)).await;
    let mut context = TracingContext::default("service", "instance");
    {
        let span = context.create_entry_span("op1").unwrap();
        context.finalize_span(span);
    }
    reporter.sender().send(context).await?;
    reporter.shutdown().await?;
    Ok(())
}
