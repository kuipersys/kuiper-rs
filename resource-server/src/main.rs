//--------------------------------------------------------------------------
// (C) Copyright Travis Sharp <travis@kuipersys.com>.  All rights reserved.
//--------------------------------------------------------------------------

use actix_web::middleware::Logger;
use actix_web::{get, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actors::models::ServerMessage;
use actors::ws_handler;
use commands::observer::SetObserverCommand;
use dashmap::DashMap;
use kuiper_runtime::{KuiperConfig, KuiperRuntime, KuiperRuntimeBuilder};
use kuiper_runtime_sdk::command::CommandContext;
use kuiper_runtime_sdk::data::file_system_store::FileSystemStore;
use kuiper_runtime_sdk::error::KuiperError;
use routing::ResourceDescriptor;
use serde_json::Value;
use services::HostedService;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

mod actors;
mod commands;
mod logging;
mod middleware;
mod routing;
mod services;

// #[tokio::main]
// async fn main() -> Result<()> {
//     tracing_subscriber::fmt::init();
//     tracing::info!("Starting resource-server service...");

//     Ok(())
// }

type ClientId = String;
type SubscriberMap = Arc<DashMap<ClientId, UnboundedSender<ServerMessage>>>;

fn kuiper_error_response(e: anyhow::Error) -> HttpResponse {
    if let Some(kuiper_err) = e.downcast_ref::<KuiperError>() {
        return match kuiper_err {
            KuiperError::NotFound(msg) => HttpResponse::NotFound().body(msg.clone()),
            KuiperError::Conflict(msg) => HttpResponse::Conflict().body(msg.clone()),
            KuiperError::Invalid(msg) => HttpResponse::BadRequest().body(msg.clone()),
            KuiperError::Forbidden(msg) => HttpResponse::Forbidden().body(msg.clone()),
        };
    }
    HttpResponse::InternalServerError().body(e.to_string())
}

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
        is_internal: false,
    };

    match runtime.execute(&mut ctx).await {
        Ok(Some(result)) => HttpResponse::Ok().json(result),
        Ok(None) => HttpResponse::NoContent().finish(),
        Err(e) => kuiper_error_response(e),
    }
}

#[put("/api/{tail:.*}")]
async fn api_put_handler(
    rt: web::Data<Arc<KuiperRuntime>>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> impl Responder {
    let full_path = req.path();

    let path = full_path
        .strip_prefix("/api/")
        .or_else(|| full_path.strip_prefix("/api"))
        .unwrap_or(full_path);

    let descriptor = ResourceDescriptor::parse(path);

    if descriptor.is_err() {
        return HttpResponse::BadRequest().body(format!(
            "Invalid path: {}, {}",
            path,
            descriptor.err().unwrap()
        ));
    }

    let descriptor = descriptor.unwrap();

    let mut ctx = CommandContext {
        command_name: "set".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: uuid::Uuid::new_v4(),
        cancellation_token: CancellationToken::new(),
    is_internal: false,
    };

    // Read the body of the request
    ctx.parameters.insert("value".to_string(), body.clone());
    ctx.parameters.insert(
        "resource".to_string(),
        serde_json::json!(format!(
            "{}/{}/{}",
            descriptor.group,
            descriptor.kind,
            descriptor.name.unwrap()
        )),
    );
    ctx.metadata
        .insert("namespace".to_string(), descriptor.namespace.clone());

    let result = rt.execute(&mut ctx).await;

    match result {
        Ok(Some(value)) => HttpResponse::Ok().json(value),
        Ok(None) => HttpResponse::Ok().finish(),
        Err(e) => kuiper_error_response(e),
    }
}

async fn api_handler(rt: web::Data<Arc<KuiperRuntime>>, req: HttpRequest) -> impl Responder {
    let full_path = req.path();

    let path = full_path
        .strip_prefix("/api/")
        .or_else(|| full_path.strip_prefix("/api"))
        .unwrap_or(full_path);

    let descriptor = ResourceDescriptor::parse(path);

    if descriptor.is_err() {
        return HttpResponse::BadRequest().body(format!(
            "Invalid path: {}, {}",
            path,
            descriptor.err().unwrap()
        ));
    }

    let method = req.method();
    let command_name = match method.as_str() {
        "GET" => "get",
        "DELETE" => "delete",
        _ => {
            return HttpResponse::MethodNotAllowed().body(format!("Method {} not allowed", method))
        }
    };

    let descriptor = descriptor.unwrap();

    let mut ctx = CommandContext {
        command_name: command_name.to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: uuid::Uuid::new_v4(),
        cancellation_token: CancellationToken::new(),
    is_internal: false,
    };

    ctx.parameters.insert(
        "resource".to_string(),
        serde_json::json!(format!(
            "{}/{}/{}",
            descriptor.group,
            descriptor.kind,
            descriptor.name.unwrap()
        )),
    );
    ctx.metadata
        .insert("namespace".to_string(), descriptor.namespace.clone());

    let result = rt.execute(&mut ctx).await;

    if result.is_err() {
        return kuiper_error_response(result.err().unwrap());
    }

    match command_name {
        "delete" => HttpResponse::NoContent().finish(),
        _ => HttpResponse::Ok().json(result.unwrap()),
    }
}

// https://github.com/rousan/rust-web-frameworks-benchmark
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    logging::init("warn");
    tracing::info!(">> Starting resource-server service...");

    let count = thread::available_parallelism()?.get();
    tracing::info!(">> Number of Threads: {}", count);

    let config = KuiperConfig::from_env();
    let store = FileSystemStore::new(&config.store_path).unwrap();
    let shared_store = Arc::new(tokio::sync::RwLock::new(store));
    let subscribers: SubscriberMap = Arc::new(DashMap::new());

    let mut builder = KuiperRuntimeBuilder::new(shared_store.clone());
    builder.register_handler(
        "set",
        Arc::new(SetObserverCommand::new(
            shared_store.clone(),
            subscribers.clone(),
        )),
    );
    let runtime = Arc::new(builder.build());

    let service = Arc::new(services::reconcile::ReconciliationService::new(
        runtime.clone(),
    ));
    service.start().await.unwrap();

    let port = 8080;
    let ip = "0.0.0.0";

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(runtime.clone()))
            .app_data(web::Data::new(subscribers.clone()))
            .wrap(
                actix_web::middleware::DefaultHeaders::new()
                    .add(("X-Content-Type-Options", "nosniff"))
                    .add(("X-XSS-Protection", "1; mode=block"))
                    .add(("X-Frame-Options", "DENY"))
                    .add(("Referrer-Policy", "no-referrer"))
                    .add(("X-Version", env!("CARGO_PKG_VERSION"))),
            )
            .wrap(Logger::default())
            .wrap(middleware::catch_panic::CatchPanic)
            .service(version_handler)
            .service(api_put_handler)
            .route("/ws", web::get().to(ws_handler))
            .route(
                "/api/{tail:.*}",
                web::route().to(api_handler), // handles all methods
            )
    })
    .workers(count)
    .bind((ip, port))?;

    tracing::warn!(">> Number of Workers: {}", count);
    tracing::info!(
        ">> {} v{}-{}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        truncate(env!("VERGEN_GIT_SHA"), 8)
    );
    tracing::info!(">> Build Time: {}", env!("VERGEN_BUILD_TIMESTAMP"));
    tracing::info!(">> Staring Server On {}:{}", ip, port);
    tracing::info!(">> Press Ctrl-C to stop the server.");
    server.run().await
}
