use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use kuiper_runtime::{HostedService, KuiperRuntime};
use kuiper_runtime_sdk::command::CommandContext;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::protocol::{ClientMessage, ServerMessage};

/// Background service that:
///   1. Maintains a WebSocket connection to the resource-server.
///   2. Subscribes to all resource events.
///   3. On events that carry a `deletionTimestamp`, immediately executes a
///      local reconciliation pass via the runtime (debounced to 1 s).
///   4. Always runs a backup reconciliation pass every 30 s.
///
/// All reconciliation is executed **locally** through `KuiperRuntime` — no RPC
/// is sent back over the wire.
pub struct ReconcilerService {
    runtime: Arc<KuiperRuntime>,
    server_url: String,
    cancel_token: CancellationToken,
    task_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ReconcilerService {
    pub fn new(runtime: Arc<KuiperRuntime>, server_url: String) -> Self {
        Self {
            runtime,
            server_url,
            cancel_token: CancellationToken::new(),
            task_handle: Mutex::new(None),
        }
    }

    async fn reconcile_once(&self) -> anyhow::Result<()> {
        let mut ctx = CommandContext {
            command_name: "reconcile".to_string(),
            parameters: HashMap::new(),
            metadata: HashMap::new(),
            activity_id: Uuid::new_v4(),
            caller_id: None,
            cancellation_token: self.cancel_token.child_token(),
            is_internal: true,
        };
        self.runtime.execute(&mut ctx).await?;
        Ok(())
    }
}

#[async_trait]
impl HostedService for ReconcilerService {
    async fn start(self: &Arc<Self>) -> anyhow::Result<()> {
        let service = self.clone();
        let token = self.cancel_token.clone();

        let handle = tokio::spawn(async move {
            let initial_backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(60);
            let mut backoff = initial_backoff;

            loop {
                if token.is_cancelled() {
                    break;
                }

                tracing::info!("Connecting to {}…", service.server_url);

                let ws_stream = tokio::select! {
                    _ = token.cancelled() => break,
                    result = connect_async(&service.server_url) => match result {
                        Ok((stream, _)) => {
                            tracing::info!("WebSocket connected. Resetting backoff.");
                            backoff = initial_backoff;
                            stream
                        }
                        Err(e) => {
                            tracing::error!("Connection failed: {:#}. Retrying in {:?}…", e, backoff);
                            tokio::select! {
                                _ = token.cancelled() => break,
                                _ = tokio::time::sleep(backoff) => {}
                            }
                            backoff = (backoff * 2).min(max_backoff);
                            continue;
                        }
                    },
                };

                if let Err(e) = service.run_session(ws_stream, &token).await {
                    tracing::error!("Session error: {:#}. Reconnecting…", e);
                } else {
                    tracing::info!("Session ended cleanly. Reconnecting…");
                }
            }

            tracing::info!("Reconciler service task exiting.");
        });

        *self.task_handle.lock().await = Some(handle);
        Ok(())
    }

    async fn stop(self: &Arc<Self>) -> anyhow::Result<()> {
        self.cancel_token.cancel();
        if let Some(handle) = self.task_handle.lock().await.take() {
            handle.await.ok();
        }
        tracing::info!("Reconciler service stopped.");
        Ok(())
    }
}

impl ReconcilerService {
    async fn run_session(
        &self,
        ws_stream: tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        token: &CancellationToken,
    ) -> anyhow::Result<()> {
        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));

        // Subscribe to all resource types.
        {
            let mut w = write.lock().await;
            let text = serde_json::to_string(&ClientMessage::Subscribe {
                resource: "*".to_string(),
            })?;
            w.send(Message::Text(text.into())).await?;
        }

        let mut debounce_timer = tokio::time::interval(Duration::from_secs(5));
        debounce_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut backup_timer = tokio::time::interval(Duration::from_secs(30));
        backup_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut pending_reconcile = false;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    tracing::warn!("Reconciler session cancelled.");
                    break;
                }

                raw = read.next() => {
                    match raw {
                        None => break,
                        Some(Err(e)) => return Err(e.into()),
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<ServerMessage>(&text) {
                                Err(e) => tracing::warn!("Unrecognised server message ({}): {}", e, text),
                                Ok(msg) => {
                                    if self.handle_message(msg) {
                                        if !pending_reconcile {
                                            // First deletion event — start the 5 s debounce window.
                                            pending_reconcile = true;
                                            debounce_timer.reset();
                                        } else {
                                            tracing::info!("⚡ Reconcile already pending — event received during debounce window.");
                                        }
                                        // Already pending: leave the timer alone so the window
                                        // doesn't keep getting pushed out by subsequent events.
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.lock().await.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(Message::Close(frame))) => {
                            tracing::info!("Server closed connection: {:?}", frame);
                            break;
                        }
                        _ => {}
                    }
                }

                _ = debounce_timer.tick() => {
                    if pending_reconcile {
                        pending_reconcile = false;
                        tracing::info!("🚧 Reconcile triggered by Event");
                        if let Err(e) = self.reconcile_once().await {
                            tracing::error!("Reconciliation failed: {:#}", e);
                        }
                    }
                }

                _ = backup_timer.tick() => {
                    tracing::debug!("⏰ Backup 30s reconcile triggered");
                    if let Err(e) = self.reconcile_once().await {
                        tracing::error!("Reconciliation failed: {:#}", e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns `true` if the message indicates that a reconciliation pass is needed.
    fn handle_message(&self, msg: ServerMessage) -> bool {
        match msg {
            ServerMessage::Hello { client_id, message } => {
                tracing::info!(client_id = %client_id, "Connected: {}", message);
                false
            }
            ServerMessage::Subscribed { resource } => {
                tracing::info!(resource = %resource, "Subscription confirmed.");
                false
            }
            ServerMessage::Event {
                resource,
                namespace,
                action,
                object,
            } => {
                let ns = namespace.as_deref().unwrap_or("global");
                tracing::info!(resource = %resource, namespace = %ns, action = %action, "Event received.");
                needs_reconciliation(&object)
            }
            ServerMessage::RpcResult { value } => {
                tracing::debug!("RPC result: {}", value);
                false
            }
            ServerMessage::Error { message } => {
                tracing::error!("Server error: {}", message);
                false
            }
            ServerMessage::Pong => false,
        }
    }
}

fn needs_reconciliation(object: &Value) -> bool {
    object
        .pointer("/metadata/deletionTimestamp")
        .map(|v| !v.is_null())
        .unwrap_or(false)
}
