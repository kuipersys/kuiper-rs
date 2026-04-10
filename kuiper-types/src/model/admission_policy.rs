use serde::{Deserialize, Serialize};

use super::resource::SystemObjectMetadata;

// ── AdmissionOperation ────────────────────────────────────────────────────────

/// The resource lifecycle operations that an admission webhook can intercept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AdmissionOperation {
    Create,
    Update,
    Delete,
}

// ── FailurePolicy ─────────────────────────────────────────────────────────────

/// What the runtime does when the webhook call fails or times out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailurePolicy {
    /// Reject the operation — treat failure as a hard error.
    Fail,
    /// Allow the operation to proceed regardless.
    Ignore,
}

impl Default for FailurePolicy {
    fn default() -> Self {
        FailurePolicy::Fail
    }
}

// ── AdmissionWebhookSpec ──────────────────────────────────────────────────────

/// Inline webhook target. Use either `service_ref` (lookup via `ServiceEndpoint`
/// registry) or `url` (direct call). `service_ref` takes priority when both are
/// set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionWebhookSpec {
    /// Name of a `ServiceEndpoint` resource (same namespace or `global`) whose
    /// URL and auth config will be used to make the webhook call.
    #[serde(rename = "serviceRef", skip_serializing_if = "Option::is_none")]
    pub service_ref: Option<String>,

    /// Inline URL — used when no `ServiceEndpoint` is registered.
    /// Ignored when `service_ref` is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// HTTP path appended to the resolved base URL. Defaults to `/admit`.
    #[serde(default = "default_path")]
    pub path: String,

    /// What to do when the call fails or times out.
    #[serde(rename = "failurePolicy", default)]
    pub failure_policy: FailurePolicy,

    /// Per-call timeout in seconds. Overrides the `ServiceEndpoint` default.
    #[serde(rename = "timeoutSeconds", skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
}

fn default_path() -> String {
    "/admit".to_string()
}

// ── AdmissionPolicyTarget ─────────────────────────────────────────────────────

/// Identifies which resource kind this policy applies to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionPolicyTarget {
    /// API group (e.g. `compute.cloud-api.dev`).
    pub group: String,

    /// Kind name (e.g. `VirtualMachine`).
    pub kind: String,
}

// ── AdmissionPolicySpec ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionPolicySpec {
    /// The resource kind this policy governs.
    pub target: AdmissionPolicyTarget,

    /// Which operations trigger this webhook. Defaults to all three.
    #[serde(default = "default_operations")]
    pub operations: Vec<AdmissionOperation>,

    /// The webhook to call.
    pub webhook: AdmissionWebhookSpec,
}

fn default_operations() -> Vec<AdmissionOperation> {
    vec![
        AdmissionOperation::Create,
        AdmissionOperation::Update,
        AdmissionOperation::Delete,
    ]
}

// ── AdmissionPolicy ───────────────────────────────────────────────────────────

/// Binds an admission webhook to a resource kind and set of operations.
///
/// When a `set` or `delete` command is dispatched for a matching resource, the
/// runtime calls the configured webhook before committing the change. A non-2xx
/// response (or network failure with `failurePolicy: Fail`) rejects the operation.
///
/// Internal writes (`ctx.is_internal == true`) bypass admission entirely so
/// bootstrapped system resources are never blocked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionPolicy {
    #[serde(rename = "apiVersion")]
    pub api_version: String,

    #[serde(rename = "kind")]
    pub kind: String,

    pub metadata: SystemObjectMetadata,

    pub spec: AdmissionPolicySpec,
}
