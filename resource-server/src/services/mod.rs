pub mod reconcile;

use std::sync::Arc;

use async_trait::async_trait;

#[async_trait]
pub trait HostedService: Send + Sync {
    /// Starts the background service (e.g., spawn a tokio task).
    async fn start(self: &Arc<Self>) -> anyhow::Result<()>;

    /// Stops the background service (e.g., cancel task, clean up).
    async fn stop(self: &Arc<Self>) -> anyhow::Result<()>;
}