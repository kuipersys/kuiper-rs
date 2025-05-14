//--------------------------------------------------------------------------
// (C) Copyright Travis Sharp <travis@kuipersys.com>.  All rights reserved.
//--------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use actix_web::{delete, get, post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web::middleware::Logger;
use actors::models::ServerMessage;
use actors::ws_handler;
use commands::observer::SetObserverCommand;
use dashmap::DashMap;
use kuiper_runtime::{KuiperRuntime, KuiperRuntimeBuilder};
use kuiper_runtime_sdk::command::CommandContext;
use kuiper_runtime_sdk::data::file_system_store::FileSystemStore;
use routing::ResourceDescriptor;
use serde_json::Value;
use services::HostedService;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

mod commands;
mod routing;
mod middleware;
mod logging;
mod services;
mod actors;

// #[tokio::main]
// async fn main() -> Result<()> {
//     tracing_subscriber::fmt::init();
//     tracing::info!("Starting resource-server service...");

//     Ok(())
// }

type ClientId = String;
type SubscriberMap = Arc<DashMap<ClientId, UnboundedSender<ServerMessage>>>;

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

#[get("/version")]
async fn version_handler(
    runtime: web::Data<Arc<KuiperRuntime>>,
    req: HttpRequest,
) -> impl Responder {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    // this doesn't work
    // tokio::spawn(async move {
    //     req.connection_closed();
    //     token_clone.cancel();
    // });

    let mut ctx = CommandContext {
        command_name: "version".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: uuid::Uuid::new_v4(),
        cancellation_token: token,
    };

    match runtime.execute(&mut ctx).await {
        Ok(Some(result)) => HttpResponse::Ok().json(result),
        Ok(None) => HttpResponse::NoContent().finish(),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[put("/api/{tail:.*}")]
async fn api_put_handler(
    rt: web::Data<Arc<KuiperRuntime>>,
    req: HttpRequest,
    body: web::Json<Value>
) -> impl Responder {
    let full_path = req.path();

    let path = full_path
        .strip_prefix("/api/")
        .or_else(|| full_path.strip_prefix("/api"))
        .unwrap_or(full_path);

    let descriptor = ResourceDescriptor::parse(path);

    if descriptor.is_err() {
        return HttpResponse::BadRequest().body(format!("Invalid path: {}, {}", path, descriptor.err().unwrap()));
    }

    let descriptor = descriptor.unwrap();
    
    let mut ctx = CommandContext {
        command_name: "set".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: uuid::Uuid::new_v4(),
        cancellation_token: CancellationToken::new(),
    };

    // Read the body of the request
    ctx.parameters.insert("value".to_string(), body.clone());
    ctx.parameters.insert("resource".to_string(), serde_json::json!(format!("{}/{}/{}", descriptor.group, descriptor.kind, descriptor.name.unwrap())));
    ctx.metadata.insert("namespace".to_string(), descriptor.namespace.clone());

    let result = rt.execute(&mut ctx).await;

    if result.is_err() {
        return HttpResponse::InternalServerError().body(format!("Error executing command: {}", result.err().unwrap()));
    }

    HttpResponse::Ok().json(result.unwrap())
}

async fn api_handler(
    rt: web::Data<Arc<KuiperRuntime>>,
    req: HttpRequest,
) -> impl Responder {
    let full_path = req.path();

    let path = full_path
        .strip_prefix("/api/")
        .or_else(|| full_path.strip_prefix("/api"))
        .unwrap_or(full_path);

    let descriptor = ResourceDescriptor::parse(path);

    if descriptor.is_err() {
        return HttpResponse::BadRequest().body(format!("Invalid path: {}, {}", path, descriptor.err().unwrap()));
    }

    // if get, put, delete, it's ok - otherwise, return 405
    let method = req.method();
    if method != "GET" && method != "PUT" && method != "DELETE" {
        return HttpResponse::MethodNotAllowed().body(format!("Method {} not allowed", method));
    }

    let descriptor = descriptor.unwrap();
    
    let mut ctx = CommandContext {
        command_name: "get".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: uuid::Uuid::new_v4(),
        cancellation_token: CancellationToken::new(),
    };

    ctx.parameters.insert("resource".to_string(), serde_json::json!(format!("{}/{}/{}", descriptor.group, descriptor.kind, descriptor.name.unwrap())));
    ctx.metadata.insert("namespace".to_string(), descriptor.namespace.clone());

    let result = rt.execute(&mut ctx).await;

    if result.is_err() {
        return HttpResponse::InternalServerError().body(format!("Error executing command: {}", result.err().unwrap()));
    }

    HttpResponse::Ok().json(result.unwrap())
}

// https://github.com/rousan/rust-web-frameworks-benchmark
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    logging::init("warn");
    tracing::info!(">> Starting resource-server service...");

    let count = thread::available_parallelism()?.get();
    tracing::info!(">> Number of Threads: {}", count);

    let store = FileSystemStore::new("c:\\cloud-api\\kuiper\\store").unwrap();
    let shared_store = Arc::new(tokio::sync::RwLock::new(store));
    let subscribers: SubscriberMap = Arc::new(DashMap::new());

    let mut builder = KuiperRuntimeBuilder::new(shared_store.clone());
    builder.register_handler("set", Arc::new(SetObserverCommand::new(shared_store.clone(), subscribers.clone())));
    let runtime = Arc::new(builder.build());

    let service = Arc::new(services::reconcile::ReconciliationService::new(runtime.clone()));
    service.start().await.unwrap();

    let port = 8080;
    let ip = "0.0.0.0";

    let server = HttpServer::new(move || App::new()
        .app_data(web::Data::new(runtime.clone()))
        .app_data(web::Data::new(subscribers.clone()))
        .wrap(actix_web::middleware::DefaultHeaders::new()
            .add(("X-Content-Type-Options", "nosniff"))
            .add(("X-XSS-Protection", "1; mode=block"))
            .add(("X-Frame-Options", "DENY"))
            .add(("Referrer-Policy", "no-referrer"))
            .add(("X-Version", env!("CARGO_PKG_VERSION")))
        )
        .wrap(Logger::default())
        .wrap(middleware::catch_panic::CatchPanic)
        .service(version_handler)
        .service(api_put_handler)
        .route("/ws", web::get().to(ws_handler))
        .route(
            "/api/{tail:.*}",
            web::route().to(api_handler) // handles all methods
        )
    )
    .workers(count)
    .bind((ip, port))?;

    tracing::warn!(">> Number of Workers: {}", count);
    tracing::info!(">> {} v{}-{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), truncate(env!("VERGEN_GIT_SHA"), 8));
    tracing::info!(">> Build Time: {}", env!("VERGEN_BUILD_TIMESTAMP"));
    tracing::info!(">> Staring Server On {}:{}", ip, port);
    tracing::info!(">> Press Ctrl-C to stop the server.");
    server.run().await
}