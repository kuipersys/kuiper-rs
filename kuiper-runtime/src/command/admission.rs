use std::{sync::Arc, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use kuiper_runtime_sdk::{
    command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand},
    error::KuiperError,
    model::admission_policy::{AdmissionOperation, FailurePolicy},
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::registry::ResourceRegistry;

/// Observer command that intercepts `set` and `delete` operations and calls any
/// matching `AdmissionPolicy` webhooks before the store mutation is committed.
///
/// A non-2xx HTTP response (or a network/timeout error) from a webhook whose
/// `failurePolicy` is `Fail` causes the command to return a `Forbidden` error,
/// rejecting the operation. When `failurePolicy` is `Ignore`, failures are
/// logged and the operation proceeds.
///
/// Internal writes (`ctx.is_internal == true`) bypass admission entirely so
/// bootstrapped system resources are never blocked.
pub struct AdmissionWebhookCommand {
    registry: Arc<RwLock<ResourceRegistry>>,
    http_client: reqwest::Client,
}

impl AdmissionWebhookCommand {
    pub fn new(registry: Arc<RwLock<ResourceRegistry>>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client for AdmissionWebhookCommand");

        Self {
            registry,
            http_client,
        }
    }
}

impl CommandHandler for AdmissionWebhookCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Validator
    }

    fn as_validator(&self) -> Option<&dyn kuiper_runtime_sdk::command::ValidationCommand> {
        Some(self)
    }
}

#[async_trait]
impl kuiper_runtime_sdk::command::ValidationCommand for AdmissionWebhookCommand {
    async fn validate(&self, ctx: &CommandContext) -> CommandResult {
        // Skip internal writes (bootstrap, reconcile, etc.).
        if ctx.is_internal {
            return Ok(None);
        }

        // Only intercept `set` and `delete`.
        let operation = match ctx.command_name.as_str() {
            "set" => {
                // Distinguish Create vs Update based on whether the object already
                // has a UID assigned by the caller.  The runtime assigns a fresh UID
                // on Create, so a nil/absent UID in the payload means Create.
                let uid = ctx
                    .parameters
                    .get("value")
                    .and_then(|v| v.get("metadata"))
                    .and_then(|m| m.get("uid"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");

                if uid.is_empty() || uid == "00000000-0000-0000-0000-000000000000" {
                    AdmissionOperation::Create
                } else {
                    AdmissionOperation::Update
                }
            }
            "delete" => AdmissionOperation::Delete,
            _ => return Ok(None),
        };

        // Extract group and kind from the submitted resource value.
        let value = match ctx.parameters.get("value") {
            Some(v) => v.clone(),
            None => return Ok(None),
        };

        let api_version = value
            .get("apiVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let (group, _version) = match api_version.split_once('/') {
            Some((g, v)) => (g, v),
            None => return Ok(None),
        };

        let kind = match value.get("kind").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return Ok(None),
        };

        // Load matching admission policies.
        let policies = {
            let reg = self.registry.read().await;
            reg.get_admission_policies(group, kind)
                .await
                .unwrap_or_default()
        };

        for policy in &policies {
            // Check that this operation is covered by the policy.
            if !policy.spec.operations.contains(&operation) {
                continue;
            }

            let webhook = &policy.spec.webhook;

            // Resolve the target URL.
            let base_url = if let Some(service_ref) = &webhook.service_ref {
                let reg = self.registry.read().await;
                match reg.get_service_endpoint(service_ref).await {
                    Ok(ep) => ep.spec.url.clone(),
                    Err(e) => match webhook.failure_policy {
                        FailurePolicy::Ignore => {
                            warn!(
                                policy = %policy.metadata.name,
                                service_ref = %service_ref,
                                error = %e,
                                "ServiceEndpoint lookup failed; ignoring (FailurePolicy=Ignore)"
                            );
                            continue;
                        }
                        FailurePolicy::Fail => {
                            return Err(KuiperError::ServiceUnavailable(format!(
                                "AdmissionPolicy '{}': ServiceEndpoint '{}' not found: {}",
                                policy.metadata.name, service_ref, e
                            ))
                            .into());
                        }
                    },
                }
            } else if let Some(url) = &webhook.url {
                url.clone()
            } else {
                warn!(
                    policy = %policy.metadata.name,
                    "AdmissionPolicy has neither serviceRef nor url — skipping"
                );
                continue;
            };

            let endpoint_url = format!("{}{}", base_url.trim_end_matches('/'), webhook.path);

            // Resolve auth if the policy uses a serviceRef.
            let (mut headers, timeout_secs) = if let Some(service_ref) = &webhook.service_ref {
                let reg = self.registry.read().await;
                if let Ok(ep) = reg.get_service_endpoint(service_ref).await {
                    let h = build_auth_headers(&ep.spec.auth)?;
                    let t = webhook.timeout_seconds.unwrap_or(ep.spec.timeout_seconds);
                    (h, t)
                } else {
                    (HeaderMap::new(), webhook.timeout_seconds.unwrap_or(10))
                }
            } else {
                (HeaderMap::new(), webhook.timeout_seconds.unwrap_or(10))
            };

            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

            let body = json!({
                "operation": operation,
                "object": value,
            });

            debug!(
                policy = %policy.metadata.name,
                url = %endpoint_url,
                ?operation,
                "Calling admission webhook"
            );

            let result = self
                .http_client
                .post(&endpoint_url)
                .headers(headers)
                .timeout(Duration::from_secs(timeout_secs as u64))
                .json(&body)
                .send()
                .await;

            match result {
                Ok(resp) if resp.status().is_success() => {
                    // Optionally parse an `allowed: false` response body.
                    if let Ok(resp_body) = resp.json::<Value>().await {
                        if resp_body.get("allowed").and_then(|v| v.as_bool()) == Some(false) {
                            let reason = resp_body
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("rejected by admission webhook");
                            return Err(KuiperError::Forbidden(format!(
                                "AdmissionPolicy '{}': {}",
                                policy.metadata.name, reason
                            ))
                            .into());
                        }
                    }
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body_text = resp
                        .text()
                        .await
                        .unwrap_or_else(|_| "<unreadable>".to_string());
                    match webhook.failure_policy {
                        FailurePolicy::Ignore => {
                            warn!(
                                policy = %policy.metadata.name,
                                %status,
                                body = %body_text,
                                "Admission webhook returned non-2xx; ignoring (FailurePolicy=Ignore)"
                            );
                        }
                        FailurePolicy::Fail => {
                            return Err(KuiperError::Forbidden(format!(
                                "AdmissionPolicy '{}': webhook returned {}: {}",
                                policy.metadata.name, status, body_text
                            ))
                            .into());
                        }
                    }
                }
                Err(e) => match webhook.failure_policy {
                    FailurePolicy::Ignore => {
                        warn!(
                            policy = %policy.metadata.name,
                            error = %e,
                            "Admission webhook call failed; ignoring (FailurePolicy=Ignore)"
                        );
                    }
                    FailurePolicy::Fail => {
                        return Err(KuiperError::ServiceUnavailable(format!(
                            "AdmissionPolicy '{}': webhook call failed: {}",
                            policy.metadata.name, e
                        ))
                        .into());
                    }
                },
            }
        }

        Ok(None)
    }
}

/// Builds HTTP auth headers from a `ServiceAuth` configuration.
fn build_auth_headers(
    auth: &kuiper_runtime_sdk::model::service_endpoint::ServiceAuth,
) -> anyhow::Result<HeaderMap> {
    use kuiper_runtime_sdk::model::service_endpoint::ServiceAuth;

    let mut headers = HeaderMap::new();

    match auth {
        ServiceAuth::None | ServiceAuth::ClusterCert => {}
        ServiceAuth::Bearer { token_env } => {
            let token = std::env::var(token_env).context(format!(
                "Bearer auth: environment variable '{}' is not set",
                token_env
            ))?;
            let value = HeaderValue::from_str(&format!("Bearer {}", token))
                .context("Bearer token contains invalid header characters")?;
            headers.insert(AUTHORIZATION, value);
        }
        ServiceAuth::Basic {
            username_env,
            password_env,
        } => {
            let username = std::env::var(username_env).context(format!(
                "Basic auth: environment variable '{}' is not set",
                username_env
            ))?;
            let password = std::env::var(password_env).context(format!(
                "Basic auth: environment variable '{}' is not set",
                password_env
            ))?;
            let encoded = STANDARD.encode(format!("{}:{}", username, password));
            let value = HeaderValue::from_str(&format!("Basic {}", encoded))
                .context("Basic auth credentials contain invalid header characters")?;
            headers.insert(AUTHORIZATION, value);
        }
        ServiceAuth::Hmac { .. } => {
            // HMAC signing is applied at the request body layer; header
            // construction is a no-op here — the signer middleware handles it.
        }
    }

    Ok(headers)
}
