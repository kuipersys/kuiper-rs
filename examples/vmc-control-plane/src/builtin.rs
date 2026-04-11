//! Seeds the built-in `VirtualMachineCluster` `ResourceDefinition` so that the
//! control plane can accept `vm.example.dev/v1alpha1/VirtualMachineCluster`
//! objects without requiring a user-supplied definition file.

use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use kuiper_runtime::command::CommandContext;
use resource_server_runtime::KuiperRuntime;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// The API group owned by this control plane.
pub const VMC_GROUP: &str = "vm.example.dev";

/// The API version exposed by this control plane.
pub const VMC_VERSION: &str = "v1alpha1";

/// Seeds the `VirtualMachineCluster` `ResourceDefinition` into the store via
/// an internal command.  The call is idempotent — if the definition already
/// exists the store will reject the write with a resource-version conflict,
/// which we intentionally ignore.
pub async fn seed(runtime: &Arc<KuiperRuntime>) -> anyhow::Result<()> {
    let rd_value = json!({
        "apiVersion": "ext.api.cloud-api.dev/v1alpha1",
        "kind": "ResourceDefinition",
        "metadata": {
            "name": "virtualmachineclusters",
            "namespace": "global"
        },
        "spec": {
            "group": VMC_GROUP,
            "scope": "Namespace",
            "names": {
                "kind": "VirtualMachineCluster",
                "singular": "virtualmachinecluster",
                "plural": "virtualmachineclusters",
                "shortNames": ["vmc"]
            },
            "versions": [
                {
                    "name": VMC_VERSION,
                    "enabled": true,
                    // The schema is validated against `spec` only — SchemaValidationCommand
                    // extracts `value.spec` before running the validator, so this schema
                    // must describe the spec object itself, not the full resource envelope.
                    "schema": {
                        "type": "object",
                        "properties": {
                            "replicas": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 100
                            },
                            "nodePool": {
                                "type": "string"
                            },
                            "machineType": {
                                "type": "string"
                            }
                        },
                        "additionalProperties": false
                    }
                }
            ]
        }
    });

    let mut ctx = CommandContext {
        command_name: "set".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert(
                "resource".to_string(),
                json!("ext.api.cloud-api.dev/v1alpha1/ResourceDefinition/virtualmachineclusters"),
            );
            p.insert("value".to_string(), rd_value);
            p
        },
        metadata: {
            let mut m = HashMap::new();
            m.insert("namespace".to_string(), "global".to_string());
            m
        },
        activity_id: Uuid::new_v4(),
        caller_id: None,
        is_internal: true,
        cancellation_token: CancellationToken::new(),
    };

    match runtime.execute(&mut ctx).await {
        Ok(_) => {
            tracing::info!("Seeded built-in ResourceDefinition: VirtualMachineCluster");
        }
        Err(e) => {
            // A resource-version conflict means the definition already exists — that is fine.
            let msg = e.to_string();
            if msg.contains("resourceVersion mismatch") || msg.contains("Conflict") {
                tracing::debug!("VirtualMachineCluster RD already present, skipping seed");
            } else {
                return Err(e).context("Failed to seed VirtualMachineCluster ResourceDefinition");
            }
        }
    }

    Ok(())
}
