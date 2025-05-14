use std::collections::HashMap;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::model::resource::{SystemObject, SystemObjectMetadata};

#[test]
fn test_system_object_metadata_serialization() {
    let mut system_object: SystemObject = SystemObject {
        api_version: "v1".to_string(),
        kind: "MyObject".to_string(),
        metadata: SystemObjectMetadata {
            name: "my-object".to_string(),
            namespace: Some("default".to_string()),
            uid: Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
            creation_timestamp: Some(1682839600),
            deletion_timestamp: None,
            resource_version: Some("v1".to_string()),
            self_link: Some("/api/v1/namespaces/default/my-object".to_string()),
            labels: Some(HashMap::from([("env".to_string(), "production".to_string())])),
            annotations: Some(HashMap::from([("owner".to_string(), "dev-team".to_string())])),
            finalizers: Some(vec!["cleanup".to_string()]),
            extension_data: HashMap::new(),
        },
        status: None,
        extension_data: HashMap::new(),
    };

    system_object.extension_data.insert("spec".to_string(), json!("value"));

    let serialized = serde_json::to_string_pretty(&system_object).unwrap();

    let expected_json = json!({
        "apiVersion": "v1",
        "kind": "MyObject",
        "metadata": {
            "name": "my-object",
            "namespace": "default",
            "uid": "123e4567-e89b-12d3-a456-426614174000",
            "creationTimestamp": 1682839600,
            "resourceVersion": "v1",
            "selfLink": "/api/v1/namespaces/default/my-object",
            "labels": {
                "env": "production"
            },
            "annotations": {
                "owner": "dev-team"
            },
            "finalizers": [
                "cleanup"
            ]
        },
        "spec": "value",
    });

    println!("Serialized JSON: {}", serialized);

    let actual_json: Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(expected_json, actual_json);
}