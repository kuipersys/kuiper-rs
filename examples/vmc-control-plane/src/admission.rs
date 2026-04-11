//! In-process admission controller for `VirtualMachineCluster` resources.
//!
//! Two handlers collaborate in the command pipeline:
//!
//! * [`VmcMutatingAdmission`] — runs as a `Mutator` (priority 0, before the
//!   store write).  It injects default field values so that downstream
//!   validation and storage always see a well-formed object.
//!
//! * [`VmcValidatingAdmission`] — runs as a `Validator` (priority 1, after
//!   mutation but before the store write).  It enforces invariants that cannot
//!   be expressed in the JSON Schema alone (e.g. cross-field constraints).
//!
//! Both handlers are no-ops for any resource whose `kind` is not
//! `VirtualMachineCluster`, so they are safe to register globally on the `set`
//! command without filtering at the call site.

use async_trait::async_trait;
use kuiper_runtime::command::{
    CommandContext, CommandHandler, CommandResult, CommandType, MutationCommand, ValidationCommand,
};
use kuiper_types::error::KuiperError;

use crate::builtin::{VMC_GROUP, VMC_VERSION};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` when `ctx` carries a `VirtualMachineCluster` value that
/// this control plane owns.
fn is_vmc(ctx: &CommandContext) -> bool {
    let Some(value) = ctx.parameters.get("value") else {
        return false;
    };

    let api_version = value
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");

    let expected_api_version = format!("{}/{}", VMC_GROUP, VMC_VERSION);
    api_version.eq_ignore_ascii_case(&expected_api_version)
        && kind.eq_ignore_ascii_case("VirtualMachineCluster")
}

// ── VmcMutatingAdmission ──────────────────────────────────────────────────────

/// Mutating admission step: injects default values into every incoming
/// `VirtualMachineCluster` before it reaches the store.
///
/// Mutations applied:
/// | Target | Value |
/// |---|---|
/// | `spec.replicas` | `1` (if absent) |
/// | `spec.nodePool` | `"default"` (if absent) |
/// | `spec.machineType` | `"standard"` (if absent) |
/// | `metadata.finalizers` | `["vm.example.dev/protection"]` ensured on create |
pub struct VmcMutatingAdmission;

impl CommandHandler for VmcMutatingAdmission {
    fn get_type(&self) -> CommandType {
        CommandType::Mutator
    }

    fn as_mutator(&self) -> Option<&dyn MutationCommand> {
        Some(self)
    }
}

#[async_trait]
impl MutationCommand for VmcMutatingAdmission {
    async fn mutate(&self, ctx: &mut CommandContext) -> CommandResult {
        if !is_vmc(ctx) {
            return Ok(None);
        }

        let Some(value) = ctx.parameters.get_mut("value") else {
            return Ok(None);
        };

        // ── spec defaults ─────────────────────────────────────────────────
        {
            if value.get("spec").is_none() {
                value["spec"] = serde_json::json!({});
            }

            let spec = &mut value["spec"];

            if spec.get("replicas").is_none() {
                spec["replicas"] = serde_json::json!(1);
            }
            if spec.get("nodePool").is_none() {
                spec["nodePool"] = serde_json::json!("default");
            }
            if spec.get("machineType").is_none() {
                spec["machineType"] = serde_json::json!("standard");
            }
        } // spec borrow ends here

        // ── finalizer ─────────────────────────────────────────────────────
        // Ensure the protection finalizer is present on every VirtualMachineCluster
        // so that DELETE always triggers a soft-delete rather than an immediate
        // hard-delete.  We only add it; we never remove it — removal is the
        // responsibility of the controller that handles the deletion workflow.
        let metadata = &mut value["metadata"];
        let finalizers = metadata
            .get_mut("finalizers")
            .and_then(|f| f.as_array_mut());

        const PROTECTION_FINALIZER: &str = "vm.example.dev/protection";

        match finalizers {
            Some(list) => {
                let already_present = list
                    .iter()
                    .any(|f| f.as_str() == Some(PROTECTION_FINALIZER));
                if !already_present {
                    list.push(serde_json::json!(PROTECTION_FINALIZER));
                }
            }
            None => {
                metadata["finalizers"] = serde_json::json!([PROTECTION_FINALIZER]);
            }
        }

        tracing::debug!(
            name = %value.get("metadata").and_then(|m| m.get("name")).and_then(|n| n.as_str()).unwrap_or("<unknown>"),
            "VmcMutatingAdmission: defaults and finalizer injected"
        );

        Ok(None)
    }
}

// ── VmcValidatingAdmission ────────────────────────────────────────────────────

/// Validating admission step: runs after mutation to enforce invariants that
/// the JSON Schema alone cannot express.
///
/// Rules enforced:
/// * `spec.replicas` must be between 1 and 100 (inclusive).
/// * `spec.nodePool` must not be an empty string.
/// * `spec.machineType` must not be an empty string.
pub struct VmcValidatingAdmission;

impl CommandHandler for VmcValidatingAdmission {
    fn get_type(&self) -> CommandType {
        CommandType::Validator
    }

    fn as_validator(&self) -> Option<&dyn ValidationCommand> {
        Some(self)
    }
}

#[async_trait]
impl ValidationCommand for VmcValidatingAdmission {
    async fn validate(&self, ctx: &CommandContext) -> CommandResult {
        if !is_vmc(ctx) {
            return Ok(None);
        }

        let value = ctx
            .parameters
            .get("value")
            .expect("value present — is_vmc already checked");

        let spec = value.get("spec").unwrap_or(&serde_json::Value::Null);

        // ── replicas ─────────────────────────────────────────────────────────
        if let Some(replicas) = spec.get("replicas") {
            let n = replicas.as_i64().ok_or_else(|| {
                KuiperError::Invalid("spec.replicas must be an integer".to_string())
            })?;
            if !(1..=100).contains(&n) {
                return Err(KuiperError::Invalid(format!(
                    "spec.replicas must be between 1 and 100, got {}",
                    n
                ))
                .into());
            }
        }

        // ── nodePool ──────────────────────────────────────────────────────────
        if let Some(node_pool) = spec.get("nodePool").and_then(|v| v.as_str()) {
            if node_pool.trim().is_empty() {
                return Err(
                    KuiperError::Invalid("spec.nodePool must not be empty".to_string()).into(),
                );
            }
        }

        // ── machineType ───────────────────────────────────────────────────────
        if let Some(machine_type) = spec.get("machineType").and_then(|v| v.as_str()) {
            if machine_type.trim().is_empty() {
                return Err(
                    KuiperError::Invalid("spec.machineType must not be empty".to_string()).into(),
                );
            }
        }

        Ok(None)
    }
}
