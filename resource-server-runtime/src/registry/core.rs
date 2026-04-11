use uuid::Uuid;

use crate::model::resource_definition::{
    ResourceDefinition, ResourceDefinitionNames, ResourceDefinitionSpec, ResourceDefinitionVersion,
    ResourceScope,
};
use kuiper_types::model::resource::SystemObjectMetadata;

use crate::constants::{GLOBAL_NAMESPACE, SYSTEM_API_VERSION, SYSTEM_EXTENSION_GROUP};

/// Returns the built-in `ResourceDefinition` objects that must exist before
/// the system can accept any user-supplied definitions.
pub(super) fn core_resource_definitions() -> Vec<ResourceDefinition> {
    vec![
        resource_definition_definition(),
        namespace_definition(),
        service_endpoint_definition(),
        admission_policy_definition(),
    ]
}

/// Reserved UID prefix — only core system resources may carry a UID beginning
/// with this prefix.
pub const RESERVED_UID_PREFIX: &str = "00000000-0000-0000-0000-";

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

fn service_endpoint_definition() -> ResourceDefinition {
    ResourceDefinition {
        api_version: format!("{}/{}", SYSTEM_EXTENSION_GROUP, SYSTEM_API_VERSION),
        kind: "ResourceDefinition".to_string(),
        metadata: SystemObjectMetadata {
            name: "serviceendpoints".to_string(),
            namespace: Some(GLOBAL_NAMESPACE.to_string()),
            uid: Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
            ..Default::default()
        },
        spec: ResourceDefinitionSpec {
            group: SYSTEM_EXTENSION_GROUP.to_string(),
            scope: ResourceScope::System,
            names: ResourceDefinitionNames {
                kind: "ServiceEndpoint".to_string(),
                singular: "serviceendpoint".to_string(),
                plural: "serviceendpoints".to_string(),
                short_names: Some(vec!["sep".to_string()]),
            },
            versions: vec![ResourceDefinitionVersion {
                name: SYSTEM_API_VERSION.to_string(),
                enabled: true,
                schema: None,
            }],
        },
    }
}

fn admission_policy_definition() -> ResourceDefinition {
    ResourceDefinition {
        api_version: format!("{}/{}", SYSTEM_EXTENSION_GROUP, SYSTEM_API_VERSION),
        kind: "ResourceDefinition".to_string(),
        metadata: SystemObjectMetadata {
            name: "admissionpolicies".to_string(),
            namespace: Some(GLOBAL_NAMESPACE.to_string()),
            uid: Uuid::parse_str("00000000-0000-0000-0000-000000000004").unwrap(),
            ..Default::default()
        },
        spec: ResourceDefinitionSpec {
            group: SYSTEM_EXTENSION_GROUP.to_string(),
            scope: ResourceScope::System,
            names: ResourceDefinitionNames {
                kind: "AdmissionPolicy".to_string(),
                singular: "admissionpolicy".to_string(),
                plural: "admissionpolicies".to_string(),
                short_names: Some(vec!["ap".to_string()]),
            },
            versions: vec![ResourceDefinitionVersion {
                name: SYSTEM_API_VERSION.to_string(),
                enabled: true,
                schema: None,
            }],
        },
    }
}
