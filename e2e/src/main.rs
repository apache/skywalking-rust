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
use hyper::client::HttpConnector;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use skywalking::context::propagation::context::SKYWALKING_HTTP_CONTEXT_HEADER_KEY;
use skywalking::context::propagation::decoder::decode_propagation;
use skywalking::context::propagation::encoder::encode_propagation;
use skywalking::context::tracer::Tracer;
use skywalking::reporter::grpc::GrpcReporter;
use std::convert::Infallible;
use std::error::Error;
use std::net::SocketAddr;
use structopt::StructOpt;
use tokio::sync::OnceCell;

static NOT_FOUND_MSG: &str = "not found";
static SUCCESS_MSG: &str = "Success";

static GLOBAL_TRACER: OnceCell<Tracer<GrpcReporter>> = OnceCell::const_new();

fn set_global_tracer(tracer: Tracer<GrpcReporter>) {
    if GLOBAL_TRACER.set(tracer).is_err() {
        panic!("TRACER has setted")
    }
}

fn get_global_tracer() -> &'static Tracer<GrpcReporter> {
    GLOBAL_TRACER.get().expect("TRACER haven't setted")
}

async fn handle_ping(
    _req: Request<Body>,
    client: Client<HttpConnector>,
) -> Result<Response<Body>, Infallible> {
    let mut context = get_global_tracer().create_trace_context();
    let span = context.create_entry_span("/ping").unwrap();
    {
        let span2 = context.create_exit_span("/pong", "consumer:8082").unwrap();
        let header = encode_propagation(&context, "/pong", "consumer:8082");
        let req = Request::builder()
            .method(Method::GET)
            .header(SKYWALKING_HTTP_CONTEXT_HEADER_KEY, header)
            .uri("http://consumer:8082/pong")
            .body(Body::from(""))
            .unwrap();

        client.request(req).await.unwrap();
        context.finalize_span(span2);
    }
    context.finalize_span(span);
    get_global_tracer().finalize_context(context);
    Ok(Response::new(Body::from("hoge")))
}

async fn producer_response(
    _req: Request<Body>,
    client: Client<HttpConnector>,
) -> Result<Response<Body>, Infallible> {
    match (_req.method(), _req.uri().path()) {
        (&Method::GET, "/ping") => handle_ping(_req, client).await,
        (&Method::GET, "/healthCheck") => Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(SUCCESS_MSG))
            .unwrap()),
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(NOT_FOUND_MSG))
            .unwrap()),
    }
}

async fn run_producer_service(host: [u8; 4]) {
    let client = Client::new();
    let make_svc = make_service_fn(|_| {
        let client = client.clone();

        async {
            Ok::<_, Infallible>(service_fn(move |req| {
                producer_response(req, client.to_owned())
            }))
        }
    });
    let addr = SocketAddr::from((host, 8081));
    let server = Server::bind(&addr).serve(make_svc);
    println!("starting producer on {:?}...", &addr);
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn handle_pong(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let ctx = decode_propagation(
        _req.headers()[SKYWALKING_HTTP_CONTEXT_HEADER_KEY]
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let mut context = get_global_tracer().create_trace_context_from_propagation(ctx);
    let span = context.create_entry_span("/pong").unwrap();
    context.finalize_span(span);
    get_global_tracer().finalize_context(context);
    Ok(Response::new(Body::from("hoge")))
}

async fn consumer_response(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (_req.method(), _req.uri().path()) {
        (&Method::GET, "/pong") => handle_pong(_req).await,
        (&Method::GET, "/healthCheck") => Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(SUCCESS_MSG))
            .unwrap()),
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(NOT_FOUND_MSG))
            .unwrap()),
    }
}

async fn run_consumer_service(host: [u8; 4]) {
    let make_svc =
        make_service_fn(|_| async { Ok::<_, Infallible>(service_fn(consumer_response)) });
    let addr = SocketAddr::from((host, 8082));
    let server = Server::bind(&addr).serve(make_svc);

    println!("starting consumer on {:?}...", &addr);
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

#[derive(StructOpt)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(short, long)]
    mode: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::from_args();
    let reporter = GrpcReporter::connect("http://collector:19876").await?;

    if opt.mode == "consumer" {
        set_global_tracer(Tracer::new("consumer", "node_0", reporter));
        run_consumer_service([0, 0, 0, 0]).await;
    } else if opt.mode == "producer" {
        set_global_tracer(Tracer::new("producer", "node_0", reporter));
        run_producer_service([0, 0, 0, 0]).await;
    }

    get_global_tracer().reporting(async move {}).await?;

    Ok(())
}
