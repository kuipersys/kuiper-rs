use uuid::Uuid;

use kuiper_runtime_sdk::model::{
    resource::SystemObjectMetadata,
    resource_definition::{
        ResourceDefinition, ResourceDefinitionNames, ResourceDefinitionSpec,
        ResourceDefinitionVersion, ResourceScope,
    },
};

use crate::constants::{GLOBAL_NAMESPACE, SYSTEM_API_VERSION, SYSTEM_EXTENSION_GROUP};

/// Returns the built-in `ResourceDefinition` objects that must exist before
/// the system can accept any user-supplied definitions.  These are written to
/// the store on first startup (no-overwrite) and indexed into the registry.
///
/// This solves the chicken-and-egg bootstrap: the `ResourceDefinition` kind
/// must be registered in the registry *before* the validation layer can permit
/// new `ResourceDefinition` resources to be written.
pub(super) fn core_resource_definitions() -> Vec<ResourceDefinition> {
    vec![
        resource_definition_definition(),
        namespace_definition(),
    ]
}

/// Reserved UID prefix — only core system resources may carry a UID beginning
/// with this prefix.
pub const RESERVED_UID_PREFIX: &str = "00000000-0000-0000-0000-";

/// The self-referential bootstrap definition: defines the `ResourceDefinition`
/// kind itself.
fn resource_definition_definition() -> ResourceDefinition {
    ResourceDefinition {
        api_version: format!("{}/{}", SYSTEM_EXTENSION_GROUP, SYSTEM_API_VERSION),
        kind: "ResourceDefinition".to_string(),
        metadata: SystemObjectMetadata {
            name: "resourcedefinitions".to_string(),
            namespace: Some(GLOBAL_NAMESPACE.to_string()),
            uid: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            ..Default::default()
        },
        spec: ResourceDefinitionSpec {
            group: SYSTEM_EXTENSION_GROUP.to_string(),
            scope: ResourceScope::System,
            names: ResourceDefinitionNames {
                kind: "ResourceDefinition".to_string(),
                singular: "resourcedefinition".to_string(),
                plural: "resourcedefinitions".to_string(),
                short_names: Some(vec!["rd".to_string()]),
            },
            versions: vec![ResourceDefinitionVersion {
                name: SYSTEM_API_VERSION.to_string(),
                enabled: true,
                schema: None,
            }],
        },
    }
}

/// Built-in definition for the `Namespace` resource kind.
fn namespace_definition() -> ResourceDefinition {
    ResourceDefinition {
        api_version: format!("{}/{}", SYSTEM_EXTENSION_GROUP, SYSTEM_API_VERSION),
        kind: "ResourceDefinition".to_string(),
        metadata: SystemObjectMetadata {
            name: "namespaces".to_string(),
            namespace: Some(GLOBAL_NAMESPACE.to_string()),
            uid: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            ..Default::default()
        },
        spec: ResourceDefinitionSpec {
            group: SYSTEM_EXTENSION_GROUP.to_string(),
            scope: ResourceScope::System,
            names: ResourceDefinitionNames {
                kind: "Namespace".to_string(),
                singular: "namespace".to_string(),
                plural: "namespaces".to_string(),
                short_names: Some(vec!["ns".to_string()]),
            },
            versions: vec![ResourceDefinitionVersion {
                name: SYSTEM_API_VERSION.to_string(),
                enabled: true,
                schema: None,
            }],
        },
    }
}
