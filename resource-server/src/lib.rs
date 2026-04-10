//! Resource server library — exposes HTTP handlers and app configuration
//! for both the production binary and integration tests.

pub mod actors;
pub mod commands;
pub mod logging;
pub mod middleware;
pub mod routing;
pub mod services;

use actix_web::{get, put, web, HttpRequest, HttpResponse, Responder};
use actors::models::ServerMessage;
use actors::ws_handler;
use dashmap::DashMap;
use kuiper_runtime::KuiperRuntime;
use kuiper_runtime_sdk::command::CommandContext;
use kuiper_runtime_sdk::error::KuiperError;
use routing::ResourceDescriptor;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

pub type ClientId = String;
pub type SubscriberMap = Arc<DashMap<ClientId, UnboundedSender<ServerMessage>>>;
/// Maps each connected client to the resource types it has subscribed to (e.g. `"apiVersion/Kind"`).
pub type SubscriptionMap = Arc<DashMap<ClientId, Vec<String>>>;

pub fn kuiper_error_response(e: anyhow::Error) -> HttpResponse {
    if let Some(kuiper_err) = e.downcast_ref::<KuiperError>() {
        return match kuiper_err {
            KuiperError::NotFound(msg) => HttpResponse::NotFound().body(msg.clone()),
            KuiperError::Conflict(msg) => HttpResponse::Conflict().body(msg.clone()),
            KuiperError::Invalid(msg) => HttpResponse::BadRequest().body(msg.clone()),
            KuiperError::Forbidden(msg) => HttpResponse::Forbidden().body(msg.clone()),
            KuiperError::ServiceUnavailable(msg) => {
                HttpResponse::ServiceUnavailable().body(msg.clone())
            }
        };
    }
    HttpResponse::InternalServerError().body(e.to_string())
}

pub fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

#[get("/version")]
pub async fn version_handler(
    runtime: web::Data<Arc<KuiperRuntime>>,
    _req: HttpRequest,
) -> impl Responder {
    let mut ctx = CommandContext {
        command_name: "version".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: uuid::Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    match runtime.execute(&mut ctx).await {
        Ok(Some(result)) => HttpResponse::Ok().json(result),
        Ok(None) => HttpResponse::NoContent().finish(),
        Err(e) => kuiper_error_response(e),
    }
}

#[put("/api/{tail:.*}")]
pub async fn api_put_handler(
    rt: web::Data<Arc<KuiperRuntime>>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> impl Responder {
    let full_path = req.path();

    let path = full_path
        .strip_prefix("/api/")
        .or_else(|| full_path.strip_prefix("/api"))
        .unwrap_or(full_path);

    let descriptor = match ResourceDescriptor::parse(path) {
        Ok(d) => d,
        Err(e) => return HttpResponse::BadRequest().body(format!("Invalid path: {}, {}", path, e)),
    };

    let name = match &descriptor.name {
        Some(n) => n.clone(),
        None => return HttpResponse::BadRequest().body("PUT requires a resource name"),
    };

    let mut ctx = CommandContext {
        command_name: "set".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: uuid::Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    ctx.parameters.insert("value".to_string(), body.clone());
    ctx.parameters.insert(
        "resource".to_string(),
        serde_json::json!(format!("{}/{}/{}", descriptor.group, descriptor.kind, name)),
    );
    ctx.metadata
        .insert("namespace".to_string(), descriptor.namespace.clone());

    match rt.execute(&mut ctx).await {
        Ok(Some(value)) => HttpResponse::Ok().json(value),
        Ok(None) => HttpResponse::Ok().finish(),
        Err(e) => kuiper_error_response(e),
    }
}

pub async fn api_handler(rt: web::Data<Arc<KuiperRuntime>>, req: HttpRequest) -> impl Responder {
    let full_path = req.path();

    let path = full_path
        .strip_prefix("/api/")
        .or_else(|| full_path.strip_prefix("/api"))
        .unwrap_or(full_path);

    let descriptor = match ResourceDescriptor::parse(path) {
        Ok(d) => d,
        Err(e) => return HttpResponse::BadRequest().body(format!("Invalid path: {}, {}", path, e)),
    };

    let method = req.method().as_str();

    let (command_name, resource_path) = match method {
        "GET" => match &descriptor.name {
            Some(name) => (
                "get",
                format!("{}/{}/{}", descriptor.group, descriptor.kind, name),
            ),
            None => ("list", format!("{}/{}", descriptor.group, descriptor.kind)),
        },
        "DELETE" => match &descriptor.name {
            Some(name) => (
                "delete",
                format!("{}/{}/{}", descriptor.group, descriptor.kind, name),
            ),
            None => return HttpResponse::BadRequest().body("DELETE requires a resource name"),
        },
        _ => {
            return HttpResponse::MethodNotAllowed().body(format!("Method {} not allowed", method))
        }
    };

    let mut ctx = CommandContext {
        command_name: command_name.to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: uuid::Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    ctx.parameters
        .insert("resource".to_string(), serde_json::json!(resource_path));
    ctx.metadata
        .insert("namespace".to_string(), descriptor.namespace.clone());

    match rt.execute(&mut ctx).await {
        Ok(result) => match command_name {
            "delete" => HttpResponse::NoContent().finish(),
            _ => match result {
                Some(value) => HttpResponse::Ok().json(value),
                None => HttpResponse::NoContent().finish(),
            },
        },
        Err(e) => kuiper_error_response(e),
    }
}

/// Registers all route handlers and shared app data onto the given `ServiceConfig`.
///
/// Used by both the production `HttpServer` and `actix_web::test::init_service` in tests.
pub fn configure_app(
    cfg: &mut web::ServiceConfig,
    runtime: Arc<KuiperRuntime>,
    subscribers: SubscriberMap,
    subscription_map: SubscriptionMap,
) {
    cfg.app_data(web::Data::new(runtime))
        .app_data(web::Data::new(subscribers))
        .app_data(web::Data::new(subscription_map))
        .service(version_handler)
        .service(api_put_handler)
        .route("/ws", web::get().to(ws_handler))
        .route("/api/{tail:.*}", web::route().to(api_handler));
}
