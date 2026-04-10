use std::sync::Arc;

use async_trait::async_trait;

/// Trait for long-running background services that can be started and stopped
/// gracefully. Implementations are typically held behind an `Arc` so that the
/// handle can be shared between the lifecycle owner and the spawned task.
#[async_trait]
pub trait HostedService: Send + Sync {
    /// Starts the background service, e.g. by spawning a Tokio task.
    async fn start(self: &Arc<Self>) -> anyhow::Result<()>;

    /// Signals the service to stop and waits for it to shut down cleanly.
    async fn stop(self: &Arc<Self>) -> anyhow::Result<()>;
}
