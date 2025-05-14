use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemObject {
    #[serde(rename = "apiVersion")]
    pub api_version: String,

    #[serde(rename = "kind")]
    pub kind: String,

    pub metadata: SystemObjectMetadata,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    #[serde(flatten)]
    pub extension_data: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SystemObjectMetadata {
    #[serde(rename = "name")]
    pub name: String,

    #[serde(rename = "namespace", skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    #[serde(rename = "uid", default = "Uuid::nil")]
    pub uid: Uuid,

    #[serde(rename = "creationTimestamp", skip_serializing_if = "Option::is_none")]
    pub creation_timestamp: Option<i64>,

    #[serde(rename = "deletionTimestamp", skip_serializing_if = "Option::is_none")]
    pub deletion_timestamp: Option<i64>,

    #[serde(rename = "resourceVersion", skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,

    #[serde(rename = "selfLink", skip_serializing_if = "Option::is_none")]
    pub self_link: Option<String>,

    #[serde(rename = "labels", skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,

    #[serde(rename = "annotations", skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,

    #[serde(rename = "finalizers", skip_serializing_if = "Option::is_none")]
    pub finalizers: Option<Vec<String>>,

    #[serde(flatten)]
    pub extension_data: HashMap<String, Value>,
}