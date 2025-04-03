// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.
//
use http_body_util::{Empty, Full};
use hyper::{
    Method, Request, Response, StatusCode,
    body::{Bytes, Incoming},
    client, server,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use skywalking::{
    logging::{
        logger::{self, Logger},
        record::{LogRecord, RecordType},
    },
    metrics::{
        meter::{Counter, Gauge, Histogram},
        metricer::Metricer,
    },
    reporter::{
        CollectItem, Report,
        grpc::GrpcReporter,
        kafka::{KafkaReportBuilder, KafkaReporter, RDKafkaClientConfig},
    },
    trace::{
        propagation::{
            context::SKYWALKING_HTTP_CONTEXT_HEADER_KEY, decoder::decode_propagation,
            encoder::encode_propagation,
        },
        tracer::{self, Tracer},
    },
};
use std::{convert::Infallible, error::Error, net::SocketAddr};
use structopt::StructOpt;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

static NOT_FOUND_MSG: &str = "not found";
static SUCCESS_MSG: &str = "Success";

async fn handle_ping(_req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    logger::log(
        LogRecord::new()
            .add_tag("level", "DEBUG")
            .endpoint("/ping")
            .record_type(RecordType::Json)
            .content(r#"{"message": "handle ping"}"#),
    );

    let mut context = tracer::create_trace_context();
    let _span = context.create_entry_span("/ping");
    {
        let span2 = context.create_exit_span("/pong", "127.0.0.1:8082");
        let header = encode_propagation(&context, "/pong", "127.0.0.1:8082");
        let req = Request::builder()
            .method(Method::GET)
            .header(SKYWALKING_HTTP_CONTEXT_HEADER_KEY, header)
            .uri("http://127.0.0.1:8082/pong")
            .body(Empty::<Bytes>::new())
            .unwrap();

        logger::log(
            LogRecord::new()
                .add_tag("level", "INFO")
                .endpoint("/ping")
                .with_tracing_context(&context)
                .with_span(&span2)
                .record_type(RecordType::Text)
                .content("do http request"),
        );

        let stream = TcpStream::connect("127.0.0.1:8082").await.unwrap();
        let io = TokioIo::new(stream);
        let (mut sender, conn) = client::conn::http1::handshake(io).await.unwrap();
        tokio::task::spawn(async move {
            conn.await.unwrap();
        });
        sender.send_request(req).await.unwrap();
    }
    {
        let _span3 = context.create_local_span("async-job");
        let snapshot = context.capture();

        tokio::spawn(async move {
            let mut context2 = tracer::create_trace_context();
            let _span3 = context2.create_entry_span("async-callback");
            context2.continued(snapshot);
        })
        .await
        .unwrap();
    }
    Ok(Response::new(Full::new(Bytes::from("ok"))))
}

async fn producer_response(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/ping") => handle_ping(req).await,
        (&Method::GET, "/healthCheck") => Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from(SUCCESS_MSG)))
            .unwrap()),
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from(NOT_FOUND_MSG)))
            .unwrap()),
    }
}

async fn run_producer_service(host: [u8; 4]) {
    let addr = SocketAddr::from((host, 8081));
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("starting producer on {:?}...", &addr);

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            server::conn::http1::Builder::new()
                .serve_connection(io, service_fn(producer_response))
                .await
                .unwrap()
        });
    }
}

async fn handle_pong(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    logger::log(
        LogRecord::new()
            .add_tag("level", "DEBUG")
            .endpoint("/pong")
            .record_type(RecordType::Json)
            .content(r#"{"message": "handle pong"}"#),
    );

    let ctx = decode_propagation(
        req.headers()[SKYWALKING_HTTP_CONTEXT_HEADER_KEY]
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let mut context = tracer::create_trace_context();
    let _span = context.create_entry_span_with_propagation("/pong", &ctx);
    Ok(Response::new(Full::new(Bytes::from("ok"))))
}

async fn consumer_response(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/pong") => handle_pong(req).await,
        (&Method::GET, "/healthCheck") => Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from(SUCCESS_MSG)))
            .unwrap()),
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from(NOT_FOUND_MSG)))
            .unwrap()),
    }
}

async fn run_consumer_service(host: [u8; 4]) {
    let addr = SocketAddr::from((host, 8082));
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("starting consumer on {:?}...", &addr);

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            server::conn::http1::Builder::new()
                .serve_connection(io, service_fn(consumer_response))
                .await
                .unwrap()
        });
    }
}

fn run_consumer_metric(mut metricer: Metricer) {
    let counter = metricer.register(
        Counter::new("instance_trace_count")
            .add_label("region", "us-west")
            .add_label("az", "az-1"),
    );
    metricer.register(
        Gauge::new("instance_trace_count", || 20.)
            .add_label("region", "us-east")
            .add_label("az", "az-3"),
    );
    let histogram = metricer.register(
        Histogram::new("instance_trace_count", vec![10., 20., 30.])
            .add_label("region", "us-north")
            .add_label("az", "az-1"),
    );

    counter.increment(10.);
    counter.increment(20.);

    histogram.add_value(10.);
    histogram.add_value(29.);
    histogram.add_value(20.);

    metricer.boot();
}

#[derive(StructOpt)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(short, long)]
    mode: String,
}

#[derive(Clone)]
struct CombineReporter {
    grpc_reporter: GrpcReporter<UnboundedSender<CollectItem>, UnboundedReceiver<CollectItem>>,
    kafka_reporter: KafkaReporter<UnboundedSender<CollectItem>>,
}

impl Report for CombineReporter {
    fn report(&self, item: CollectItem) {
        let typ = match &item {
            CollectItem::Trace(_) => "trace",
            CollectItem::Log(_) => "log",
            CollectItem::Meter(_) => "meter",
            _ => "unknown",
        };
        println!("report item type: {:?}", typ);
        self.grpc_reporter.report(item.clone());
        self.kafka_reporter.report(item);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::from_args();

    let reporter1 = GrpcReporter::connect("http://127.0.0.1:19876").await?;
    let handle1 = reporter1.reporting().await.spawn();

    let mut client_config = RDKafkaClientConfig::new();
    client_config
        .set("bootstrap.servers", "127.0.0.1:9092")
        .set("message.timeout.ms", "6000")
        .set("allow.auto.create.topics", "true");
    let (reporter2, reporting) = KafkaReportBuilder::new(client_config)
        .with_err_handle(|message, err| {
            eprintln!(
                "kafka reporter failed, message: {}, err: {:?}",
                message, err
            );
        })
        .build()
        .await?;
    let handle2 = reporting.spawn();

    let reporter = CombineReporter {
        grpc_reporter: reporter1,
        kafka_reporter: reporter2,
    };

    if opt.mode == "consumer" {
        tracer::set_global_tracer(Tracer::new("consumer", "node_0", reporter.clone()));
        logger::set_global_logger(Logger::new("consumer", "node_0", reporter.clone()));
        run_consumer_metric(Metricer::new("consumer", "node_0", reporter));
        run_consumer_service([0, 0, 0, 0]).await;
    } else if opt.mode == "producer" {
        tracer::set_global_tracer(Tracer::new("producer", "node_0", reporter.clone()));
        logger::set_global_logger(Logger::new("producer", "node_0", reporter));
        run_producer_service([0, 0, 0, 0]).await;
    } else {
        unreachable!()
    }

    handle1.await?;
    handle2.await?;

    Ok(())
}
