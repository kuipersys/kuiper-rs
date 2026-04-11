//! Background cleanup loop for the VMC control plane.
//!
//! [`VmcCleanupService`] implements [`HostedService`] and runs a periodic
//! reconciliation pass that hard-deletes any `VirtualMachineCluster` (or any
//! other resource) whose `deletionTimestamp` is set and whose `finalizers`
//! list is empty.
//!
//! It delegates the actual deletion logic to the built-in `reconcile` command
//! so that the cleanup rules stay consistent with the coordinator.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use kuiper_runtime::{command::CommandContext, service::HostedService};
use resource_server_runtime::KuiperRuntime;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Interval between successive reconciliation passes.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

/// Embedded cleanup service that periodically reconciles soft-deleted
/// resources by delegating to the `reconcile` command.
pub struct VmcCleanupService {
    runtime: Arc<KuiperRuntime>,
    stop: CancellationToken,
    stopped: Arc<Notify>,
}

impl VmcCleanupService {
    pub fn new(runtime: Arc<KuiperRuntime>) -> Arc<Self> {
        Arc::new(Self {
            runtime,
            stop: CancellationToken::new(),
            stopped: Arc::new(Notify::new()),
        })
    }
}

#[async_trait]
impl HostedService for VmcCleanupService {
    async fn start(self: &Arc<Self>) -> anyhow::Result<()> {
        let service = self.clone();

        tokio::spawn(async move {
            tracing::info!(
                "VmcCleanupService started (interval={}s)",
                CLEANUP_INTERVAL.as_secs()
            );

            loop {
                tokio::select! {
                    _ = tokio::time::sleep(CLEANUP_INTERVAL) => {
                        if let Err(e) = service.run_pass().await {
                            tracing::warn!("Cleanup pass failed: {}", e);
                        }
                    }
                    _ = service.stop.cancelled() => {
                        tracing::info!("VmcCleanupService stopping");
                        service.stopped.notify_one();
                        return;
                    }
                }
            }
        });

        Ok(())
    }

    async fn stop(self: &Arc<Self>) -> anyhow::Result<()> {
        self.stop.cancel();
        self.stopped.notified().await;
        tracing::info!("VmcCleanupService stopped");
        Ok(())
    }
}

impl VmcCleanupService {
    /// Executes a single reconciliation pass via the built-in `reconcile` command.
    async fn run_pass(&self) -> anyhow::Result<()> {
        tracing::debug!("VmcCleanupService: starting reconciliation pass");

        let mut ctx = CommandContext {
            command_name: "reconcile".to_string(),
            parameters: Default::default(),
            metadata: Default::default(),
            activity_id: Uuid::new_v4(),
            caller_id: None,
            is_internal: true,
            cancellation_token: CancellationToken::new(),
        };

        self.runtime.execute(&mut ctx).await?;

        tracing::debug!("VmcCleanupService: reconciliation pass complete");
        Ok(())
    }
}
