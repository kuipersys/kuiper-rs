//--------------------------------------------------------------------------
// (C) Copyright Travis Sharp <travis@kuipersys.com>.  All rights reserved.
//--------------------------------------------------------------------------

use actix_web::middleware::Logger;
use actix_web::{App, HttpServer};
use dashmap::DashMap;
use kuiper_runtime::data::file_system_store::FileSystemStore;
use kuiper_runtime::data::TransactionalKeyValueStore;
use kuiper_runtime::KuiperConfig;
use resource_server::{
    commands::observer::{DeleteObserverCommand, SetObserverCommand},
    configure_app, SubscriberMap, SubscriptionMap,
};
use resource_server_runtime::KuiperRuntimeBuilder;
use std::sync::Arc;
use std::thread;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    resource_server::logging::init("warn");
    tracing::info!(">> Starting resource-server service...");

    let count = thread::available_parallelism()?.get();
    tracing::info!(">> Number of Threads: {}", count);

    let config = KuiperConfig::from_env();

    let shared_store: Arc<tokio::sync::RwLock<dyn TransactionalKeyValueStore>> =
        build_store(&config).await;

    let subscribers: SubscriberMap = Arc::new(DashMap::new());
    let subscription_map: SubscriptionMap = Arc::new(DashMap::new());

    let mut builder = KuiperRuntimeBuilder::new(shared_store.clone());
    builder.with_admission_webhooks();
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
    let runtime = Arc::new(builder.build());

    runtime
        .initialize()
        .await
        .expect("Failed to initialize runtime — could not seed/load ResourceDefinitions");

    let port = 8080;
    let ip = "0.0.0.0";

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

    tracing::warn!(">> Number of Workers: {}", count);
    tracing::info!(
        ">> {} v{}-{}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        resource_server::truncate(env!("VERGEN_GIT_SHA"), 8)
    );
    tracing::info!(">> Build Time: {}", env!("VERGEN_BUILD_TIMESTAMP"));
    tracing::info!(">> Starting Server On {}:{}", ip, port);
    tracing::info!(">> Press Ctrl-C to stop the server.");
    server.run().await
}

// ── Store factory ─────────────────────────────────────────────────────────────

async fn build_store(
    config: &KuiperConfig,
) -> Arc<tokio::sync::RwLock<dyn TransactionalKeyValueStore>> {
    if let Some(conn_str) = &config.documentdb_connection_string {
        use kuiper_runtime::data::DocumentDbStore;
        tracing::warn!(
            ">> Using DocumentDB persistent store (database: {})",
            config.documentdb_database
        );
        let store = DocumentDbStore::new(conn_str, &config.documentdb_database)
            .await
            .expect("Failed to connect to DocumentDB — check KUIPER_DOCUMENTDB_CONNECTION_STRING");
        return Arc::new(tokio::sync::RwLock::new(store));
    }

    tracing::warn!(">> Using FileSystem store (path: {})", config.store_path);
    let store =
        FileSystemStore::new(&config.store_path).expect("Failed to initialise FileSystem store");
    Arc::new(tokio::sync::RwLock::new(store))
}
