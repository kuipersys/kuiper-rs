use std::sync::Arc;

use anyhow::Result;
use kuiper_runtime::{HostedService, KuiperConfig, KuiperRuntimeBuilder};
use kuiper_runtime_sdk::data::file_system_store::FileSystemStore;

mod protocol;
mod reconciler;

use reconciler::ReconcilerService;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
                .add_directive("kuiper_runtime=debug".parse().unwrap()),
        )
        .init();

    let config = KuiperConfig::from_env();
    let store = FileSystemStore::new(&config.store_path)?;
    let shared_store = Arc::new(tokio::sync::RwLock::new(store));

    let mut builder = KuiperRuntimeBuilder::new(shared_store);
    builder.with_reconciliation();
    let runtime = Arc::new(builder.build());

    let server_url = std::env::var("KUIPER_SERVER_WS_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:8080/ws".to_string());

    tracing::info!("Coordinator starting — will connect to {}", server_url);

    let service = Arc::new(ReconcilerService::new(runtime, server_url));
    service.start().await?;

    // Wait for Ctrl-C, then shut down cleanly.
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received.");
    service.stop().await?;

    Ok(())
}
