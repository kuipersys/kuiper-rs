//! VMC Control Plane — example embedded control plane for `VirtualMachineCluster`.
//!
//! This binary wires together the kuiper runtime with:
//!
//! * **Built-in** `VirtualMachineCluster` `ResourceDefinition` (seeded on startup).
//! * **In-process mutating admission** — injects spec defaults before persisting.
//! * **In-process validating admission** — enforces invariants after mutation.
//! * **Background cleanup loop** — reconciles soft-deleted resources every 30 s.
//! * **HTTP API** — identical surface to `resource-server` (REST + WebSocket).
//!
//! # Running
//!
//! ```text
//! KUIPER_STORE_PATH=./vmc-store cargo run -p vmc-control-plane
//! ```

mod admission;
mod builtin;
mod cleanup;

use std::{sync::Arc, thread};

use actix_web::{middleware::Logger, App, HttpServer};
use dashmap::DashMap;
use kuiper_runtime::{data::file_system_store::FileSystemStore, KuiperConfig};
use resource_server::{
    commands::observer::{DeleteObserverCommand, SetObserverCommand},
    configure_app, SubscriberMap, SubscriptionMap,
};
use resource_server_runtime::KuiperRuntimeBuilder;

use admission::{VmcMutatingAdmission, VmcValidatingAdmission};
use cleanup::VmcCleanupService;
use kuiper_runtime::service::HostedService;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // ── Logging ───────────────────────────────────────────────────────────────
    resource_server::logging::init("info");
    tracing::info!(">> Starting vmc-control-plane...");

    let count = thread::available_parallelism()?.get();
    tracing::info!(">> Available threads: {}", count);

    // ── Store + registry ──────────────────────────────────────────────────────
    let config = KuiperConfig::from_env();
    let store = FileSystemStore::new(&config.store_path).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to open store at '{}': {}", config.store_path, e),
        )
    })?;
    let shared_store = Arc::new(tokio::sync::RwLock::new(store));

    let subscribers: SubscriberMap = Arc::new(DashMap::new());
    let subscription_map: SubscriptionMap = Arc::new(DashMap::new());

    // ── Runtime builder ──────────────────────────────────────────────────────
    //
    // The pipeline for the `set` command (in execution order):
    //
    //   1. VmcMutatingAdmission   (Mutator,   priority 0) — inject defaults
    //   2. VmcValidatingAdmission (Validator, priority 1) — enforce invariants
    //   3. AdmissionWebhookCommand(Validator, priority 1) — call webhook policies
    //   4. SchemaValidationCommand(Validator, priority 1) — JSON Schema check
    //   5. SetCommand             (Internal,  priority 2) — write to store
    //   6. SetObserverCommand     (Observer,  priority 4) — notify WS clients
    //
    let mut builder = KuiperRuntimeBuilder::new(shared_store.clone());

    // Webhook-based admission (AdmissionPolicy resources) — same as resource-server.
    builder.with_admission_webhooks();

    // In-process mutating admission: inject VirtualMachineCluster defaults.
    builder.register_handler("set", Arc::new(VmcMutatingAdmission));

    // In-process validating admission: enforce VirtualMachineCluster invariants.
    builder.register_handler("set", Arc::new(VmcValidatingAdmission));

    // WebSocket observer commands — fan-out change events to connected clients.
    builder.register_handler(
        "set",
        Arc::new(SetObserverCommand::new(
            shared_store.clone(),
            subscribers.clone(),
            subscription_map.clone(),
        )),
    );
    
    builder.register_handler(
        "delete",
        Arc::new(DeleteObserverCommand::new(
            shared_store.clone(),
            subscribers.clone(),
            subscription_map.clone(),
        )),
    );

    // Reconcile command — needed by the embedded cleanup loop.
    builder.with_reconciliation();

    let runtime = Arc::new(builder.build());

    // ── Initialise: seed core + persisted ResourceDefinitions ────────────────
    runtime.initialize().await.map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Runtime initialisation failed: {}", e),
        )
    })?;

    // Seed the built-in VirtualMachineCluster ResourceDefinition.
    builtin::seed(&runtime).await.map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Built-in seed failed: {}", e),
        )
    })?;

    // ── Background cleanup service ────────────────────────────────────────────
    let cleanup = VmcCleanupService::new(runtime.clone());
    cleanup.start().await.map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to start cleanup service: {}", e),
        )
    })?;

    // ── HTTP server ───────────────────────────────────────────────────────────
    let port: u16 = std::env::var("VMC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8090);
    let ip = "0.0.0.0";

    tracing::warn!(">> Listening on {}:{}", ip, port);

    let server = HttpServer::new(move || {
        let rt = runtime.clone();
        let subs = subscribers.clone();
        let sub_map = subscription_map.clone();

        App::new()
            .configure(move |cfg| configure_app(cfg, rt.clone(), subs.clone(), sub_map.clone()))
            .wrap(
                actix_web::middleware::DefaultHeaders::new()
                    .add(("X-Content-Type-Options", "nosniff"))
                    .add(("X-XSS-Protection", "1; mode=block"))
                    .add(("X-Frame-Options", "DENY"))
                    .add(("Referrer-Policy", "no-referrer"))
                    .add(("X-Version", env!("CARGO_PKG_VERSION"))),
            )
            .wrap(Logger::default())
            .wrap(resource_server::middleware::catch_panic::CatchPanic::default())
    })
    .workers(count)
    .bind((ip, port))?;

    tracing::warn!(">> vmc-control-plane v{} ready", env!("CARGO_PKG_VERSION"),);

    server.run().await?;

    // ── Graceful shutdown ─────────────────────────────────────────────────────
    cleanup.stop().await.map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Cleanup service shutdown error: {}", e),
        )
    })?;

    Ok(())
}
