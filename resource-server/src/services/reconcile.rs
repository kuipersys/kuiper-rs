use crate::KuiperRuntime;
use async_trait::async_trait;
use core::panic;
use futures_util::FutureExt;
use kuiper_runtime_sdk::command::CommandContext;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::HostedService;

pub struct ReconciliationService {
    runtime: Arc<KuiperRuntime>,
    cancel_token: Arc<CancellationToken>,
    task_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    is_running: Arc<AtomicBool>,
}

impl ReconciliationService {
    pub fn new(runtime: Arc<KuiperRuntime>) -> Self {
        Self {
            runtime,
            cancel_token: Arc::new(CancellationToken::new()),
            task_handle: Arc::new(Mutex::new(None)),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn reconcile_once(&self) -> anyhow::Result<()> {
        let mut ctx = CommandContext {
            command_name: "reconcile".to_string(),
            parameters: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
            activity_id: uuid::Uuid::new_v4(),
            cancellation_token: self.cancel_token.child_token(),
        };

        ctx.metadata.insert("namespace".to_string(), "global".to_string());

        self.runtime.execute(&mut ctx).await?;

        Ok(())
    }
}

impl Drop for ReconciliationService {
    fn drop(&mut self) {
        // loop {
        //     if self.is_running.load(Ordering::SeqCst) {
        //         // let the background task gracefully shut itself down
        //         self.cancel_token.cancel();
        //         println!("Reconciliation service is still running, waiting for it to stop...");
        //         std::thread::sleep(std::time::Duration::from_millis(100));
        //     } else {
        //         println!("Reconciliation service has stopped.");
        //         break;
        //     }
        // }
    }
}

#[async_trait]
impl HostedService for ReconciliationService {
    async fn start(self: &Arc<Self>) -> anyhow::Result<()> {
        let token = self.cancel_token.clone();
        let service = Arc::new(self.clone());
 
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
            service.is_running.store(true, Ordering::SeqCst);

            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::warn!("Reconciliation service cancelled.");
                        break;
                    }
                    _ = ticker.tick() => {
                        let result = panic::AssertUnwindSafe(service.reconcile_once())
                            .catch_unwind()
                            .await;

                        match result {
                            Ok(Ok(())) => {
                                tracing::info!("Reconciliation completed successfully.");
                            }
                            Ok(Err(e)) => {
                                tracing::error!("Reconciliation failed: {:?}", e);
                            }
                            Err(e) => {
                                tracing::error!("Reconciliation panicked: {:?}", e);
                            }
                        }
                    }
                }
            }

            println!("Reconciliation service task exiting.");
            service.is_running.store(false, Ordering::SeqCst);
        });

        let mut guard = self.task_handle.lock().await;
        *guard = Some(handle);

        Ok(())
    }

    async fn stop(self: &Arc<Self>) -> anyhow::Result<()> {
        self.cancel_token.cancel();

        if let Some(handle) = self.task_handle.lock().await.take() {
            handle.await.ok(); // wait for the task to end
        }

        println!("Reconciliation service stopped.");

        Ok(())
    }
}
