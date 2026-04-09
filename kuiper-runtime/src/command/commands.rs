use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use kuiper_runtime_sdk::{
    command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand},
    data::TransactionalKeyValueStore,
    error::KuiperError,
    model::resource::SystemObject,
};
use tokio::sync::RwLock;

const RESOURCE_CONTAINER: &str = "resource";

fn resource_key(namespace: &str, resource: Option<&str>) -> String {
    format!("{}/{}", namespace, resource.unwrap_or("")).to_lowercase()
}

// ── EchoCommand ──────────────────────────────────────────────────────────────

pub struct EchoCommand;

#[async_trait]
impl CommandHandler for EchoCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for EchoCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let message = ctx
            .get_string_param("message")
            .unwrap_or_else(|_| "hello".to_string());
        Ok(Some(serde_json::json!({ "echo": message })))
    }
}

// ── VersionCommand ────────────────────────────────────────────────────────────

pub struct VersionCommand;

impl CommandHandler for VersionCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for VersionCommand {
    async fn execute(&self, _: &CommandContext) -> CommandResult {
        Ok(Some(serde_json::json!({
            "version": crate::command::version::get_version_string(),
        })))
    }
}

// ── GetCommand ────────────────────────────────────────────────────────────────

pub struct GetCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
}

impl GetCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl CommandHandler for GetCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for GetCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let namespace = ctx
            .metadata
            .get("namespace")
            .cloned()
            .context("Missing required parameter: namespace")?
            .to_lowercase();

        let resource = ctx
            .get_string_param("resource")
            .context("Missing required parameter: resource")?
            .to_lowercase();

        let key = resource_key(&namespace, Some(&resource));

        let bytes = self
            .store
            .read()
            .await
            .get(RESOURCE_CONTAINER, &key)
            .await
            .map_err(|_| KuiperError::NotFound(format!("Resource '{}' not found", resource)))?;

        let obj: SystemObject = serde_json::from_slice(&bytes)
            .context("Failed to parse stored value as SystemObject")?;

        // Treat soft-deleted resources as not found — they are pending reconciliation.
        if obj.metadata.deletion_timestamp.is_some() {
            return Err(KuiperError::NotFound(format!(
                "Resource '{}' is pending deletion",
                resource
            ))
            .into());
        }

        let result = serde_json::to_value(&obj).context("Failed to serialize SystemObject")?;
        Ok(Some(result))
    }
}

// ── SetCommand ────────────────────────────────────────────────────────────────

pub struct SetCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
}

impl SetCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

impl CommandHandler for SetCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for SetCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let namespace = ctx
            .metadata
            .get("namespace")
            .cloned()
            .context("Missing required parameter: namespace")?
            .to_lowercase();

        let resource = ctx
            .get_string_param("resource")
            .context("Missing required parameter: resource")?
            .to_lowercase();

        let raw_value = ctx
            .get_param("value")
            .context("Missing required parameter: value")?;

        let mut obj: SystemObject =
            serde_json::from_str(&raw_value).context("Failed to parse 'value' as SystemObject")?;

        let key = resource_key(&namespace, Some(&resource));

        let store = self.store.write().await;

        // Ensure the resource container exists.
        if !store
            .container_exists(RESOURCE_CONTAINER)
            .await
            .context("Failed to check resource container")?
        {
            store
                .new_container(RESOURCE_CONTAINER)
                .await
                .context("Failed to create resource container")?;
        }

        // Detect create vs update by attempting to read the existing record.
        match store.get(RESOURCE_CONTAINER, &key).await {
            Ok(existing_bytes) => {
                // ── Update path ──
                let stored_obj: SystemObject = serde_json::from_slice(&existing_bytes)
                    .context("Failed to parse stored value as SystemObject")?;

                // Optimistic concurrency: if the caller supplied a resourceVersion
                // it must match what is currently stored.
                if let Some(provided_rv) = &obj.metadata.resource_version {
                    let stored_rv = stored_obj
                        .metadata
                        .resource_version
                        .as_deref()
                        .unwrap_or("");
                    if provided_rv.as_str() != stored_rv {
                        return Err(KuiperError::Conflict(format!(
                            "resourceVersion mismatch: provided '{}', stored '{}'",
                            provided_rv, stored_rv
                        ))
                        .into());
                    }
                }

                // Preserve immutable identity fields from the stored object.
                obj.metadata.uid = stored_obj.metadata.uid;
                obj.metadata.creation_timestamp = stored_obj.metadata.creation_timestamp;
            }
            Err(_) => {
                // ── Create path ──
                if obj.metadata.uid.is_nil() {
                    obj.metadata.uid = uuid::Uuid::new_v4();
                }
                if obj.metadata.creation_timestamp.is_none() {
                    obj.metadata.creation_timestamp = Some(chrono::Utc::now().timestamp_micros());
                }
            }
        }

        // Always: force namespace to match the request context and bump resourceVersion.
        obj.metadata.namespace = Some(namespace);
        obj.metadata.resource_version = Some(uuid::Uuid::new_v4().to_string());

        let value_bytes =
            serde_json::to_vec_pretty(&obj).context("Failed to serialize SystemObject")?;

        store
            .put(RESOURCE_CONTAINER, &key, value_bytes)
            .await
            .context("Failed to write resource to store")?;

        let result =
            serde_json::to_value(&obj).context("Failed to convert SystemObject to JSON")?;
        Ok(Some(result))
    }
}

// ── DeleteCommand ─────────────────────────────────────────────────────────────

pub struct DeleteCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
}

impl DeleteCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

impl CommandHandler for DeleteCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for DeleteCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let namespace = ctx
            .metadata
            .get("namespace")
            .cloned()
            .context("Missing required parameter: namespace")?
            .to_lowercase();

        let resource = ctx
            .get_string_param("resource")
            .context("Missing required parameter: resource")?
            .to_lowercase();

        let key = resource_key(&namespace, Some(&resource));

        let store = self.store.write().await;

        let bytes = store
            .get(RESOURCE_CONTAINER, &key)
            .await
            .map_err(|_| KuiperError::NotFound(format!("Resource '{}' not found", resource)))?;

        let mut obj: SystemObject = serde_json::from_slice(&bytes)
            .context("Failed to parse stored value as SystemObject")?;

        // If the resource is already marked for deletion, no-op.
        if obj.metadata.deletion_timestamp.is_some() {
            return Ok(None);
        }

        obj.metadata.deletion_timestamp = Some(chrono::Utc::now().timestamp_micros());

        let value_bytes =
            serde_json::to_vec_pretty(&obj).context("Failed to serialize SystemObject")?;

        store.put(RESOURCE_CONTAINER, &key, value_bytes).await?;

        Ok(None)
    }
}

// ── ListCommand ───────────────────────────────────────────────────────────────

pub struct ListCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
}

impl ListCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

impl CommandHandler for ListCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for ListCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let namespace = ctx
            .metadata
            .get("namespace")
            .cloned()
            .context("Missing required parameter: namespace")?
            .to_lowercase();

        let resource = ctx
            .get_string_param("resource")
            .context("Missing required parameter: resource")?
            .to_lowercase();

        let key_prefix = resource_key(&namespace, Some(&resource));

        let store = self.store.read().await;

        let keys = store
            .list_keys(RESOURCE_CONTAINER, Some(&key_prefix))
            .await
            .context("Failed to list keys")?;

        let mut items: Vec<serde_json::Value> = Vec::with_capacity(keys.len());

        for key in &keys {
            let bytes = match store.get(RESOURCE_CONTAINER, key).await {
                Ok(b) => b,
                Err(_) => continue, // key disappeared between list and get — skip
            };

            let obj: SystemObject = match serde_json::from_slice(&bytes) {
                Ok(o) => o,
                Err(_) => continue, // corrupt entry — skip rather than fail the whole list
            };

            // Exclude resources that are pending deletion.
            if obj.metadata.deletion_timestamp.is_some() {
                continue;
            }

            if let Ok(v) = serde_json::to_value(&obj) {
                items.push(v);
            }
        }

        Ok(Some(serde_json::json!(items)))
    }
}
