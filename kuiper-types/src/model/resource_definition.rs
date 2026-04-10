use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::resource::SystemObjectMetadata;

// ── ResourceScope ─────────────────────────────────────────────────────────────

/// Controls whether a resource type is scoped to a namespace or lives
/// globally in the system namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ResourceScope {
    Namespace,
    System,
}

// ── ResourceDefinitionNames ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDefinitionNames {
    pub kind: String,
    pub singular: String,
    pub plural: String,

    #[serde(rename = "shortNames", skip_serializing_if = "Option::is_none")]
    pub short_names: Option<Vec<String>>,
}

// ── ResourceDefinitionVersion ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDefinitionVersion {
    pub name: String,

    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Optional JSON Schema (OpenAPI v3 flavour) stored as a raw JSON value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

// ── ResourceDefinitionSpec ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDefinitionSpec {
    pub group: String,
    pub names: ResourceDefinitionNames,
    pub scope: ResourceScope,
    pub versions: Vec<ResourceDefinitionVersion>,
}

// ── ResourceDefinition ────────────────────────────────────────────────────────

/// A `ResourceDefinition` teaches the runtime about a new resource kind.
/// It is itself stored as a `SystemObject` in the `ext.api.cloud-api.dev/global`
/// namespace and re-loaded into the in-memory `ResourceRegistry` at startup
/// and whenever a new definition is saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDefinition {
    #[serde(rename = "apiVersion")]
    pub api_version: String,

    #[serde(rename = "kind")]
    pub kind: String,

    pub metadata: SystemObjectMetadata,

    pub spec: ResourceDefinitionSpec,
}

impl ResourceDefinition {
    /// Registry key: `{group}/{kind}` (lower-cased).
    pub fn registry_key(&self) -> String {
        format!("{}/{}", self.spec.group, self.spec.names.kind).to_lowercase()
    }

    /// Returns the enabled versions keyed by `{group}/{kind}/{version}` (lower-cased).
    pub fn enabled_versions(&self) -> HashMap<String, ResourceDefinitionVersion> {
        self.spec
            .versions
            .iter()
            .filter(|v| v.enabled)
            .map(|v| {
                let k = format!("{}/{}/{}", self.spec.group, self.spec.names.kind, v.name)
                    .to_lowercase();
                (k, v.clone())
            })
            .collect()
    }
}
