use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use kuiper_runtime_sdk::{command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand}, data::TransactionalKeyValueStore, model::resource::SystemObject};
use tokio::sync::RwLock;

const RESOURCE_CONTAINER: &str = "resource";
fn resource_key(namespace: &str, resource: Option<&str>) -> String {
    format!("{}/{}", namespace, resource.unwrap_or("")).to_lowercase()
}

pub struct EchoCommand;
pub struct GetCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>
}

impl GetCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

pub struct SetCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>
}

impl SetCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

pub struct DeleteCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>
}

impl DeleteCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

pub struct ListCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>
}

impl ListCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

pub struct VersionCommand;

#[async_trait]
impl CommandHandler for EchoCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }
}

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
        // do some extra validation here to ensure that resource is a valid key
        // meaning it'll be a string in the form of "{kind}/{name}"
        // if a simple kind is provided, we'll look up what 

        let namespace = ctx.metadata.get("namespace").cloned()
            .context("Missing required parameter: namespace")?.to_lowercase();

        let resource = ctx.get_string_param("resource")
            .context("Missing required parameter: key")?.to_lowercase();

        let key = resource_key(&namespace, Some(&resource));

        let result = self.store.read().await
            .get(RESOURCE_CONTAINER, &key).await?;

        let result = serde_json::from_slice::<serde_json::Value>(&result)
            .context("Failed to parse value as JSON")?;

        Ok(Some(result))
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
        let namespace = ctx.metadata.get("namespace").cloned()
            .context("Missing required parameter: namespace")?.to_lowercase();

        let resource = ctx.get_string_param("resource")
            .context("Missing required parameter: key")?.to_lowercase();

        let value = ctx.get_param("value")
            .context("Missing required parameter: value")?;

        let value = serde_json::from_str::<serde_json::Value>(&value)
            .context("Failed to parse value as JSON")?;

        let value_bytes = serde_json::to_vec_pretty(&value)
            .context("Failed to serialize value to JSON")?;

        let key = resource_key(&namespace, Some(&resource));

        let _ = self.store.write().await
            .put(RESOURCE_CONTAINER, &key, value_bytes).await?;

        Ok(Some(value))
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
        let namespace = ctx.metadata.get("namespace").cloned()
            .context("Missing required parameter: namespace")?.to_lowercase();

        let resource = ctx.get_string_param("resource")
            .context("Missing required parameter: key")?.to_lowercase();

        let key = resource_key(&namespace, Some(&resource));

        let store = self.store.write().await;

        // get value
        let value = store.get(RESOURCE_CONTAINER, &key).await?;

        if value.is_empty() {
            return Ok(None);
        }

        // update deletion timestamp
        let mut resourceObject: SystemObject = serde_json::from_slice(&value)
            .context("Failed to parse value as JSON")?;

        // if the resource is already marked for deletion, we can just skip it and return
        if resourceObject.metadata.deletion_timestamp.is_some() {
            return Ok(None);
        }

        resourceObject.metadata.deletion_timestamp = Some(chrono::Utc::now().timestamp_micros());

        let value = serde_json::to_vec_pretty(&resourceObject)
            .context("Failed to serialize value to JSON")?;

        // update the resource with the new deletion timestamp
        store.put(RESOURCE_CONTAINER, &key, value).await?;

        Ok(None)
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
        let namespace = ctx.metadata.get("namespace").cloned()
            .context("Missing required parameter: namespace")?.to_lowercase();

        let resource = ctx.get_string_param("resource")
            .context("Missing required parameter: key")?.to_lowercase();

        let namespace_prefix = format!("{}/{}/", RESOURCE_CONTAINER, namespace);
        let key_prefix = format!("{}/{}", namespace, resource);

        let keys = self.store.read().await
            .list_keys(RESOURCE_CONTAINER, Some(&key_prefix)).await?;

        // remove namespaces from the keys
        let keys: Vec<String> = keys.into_iter()
            .map(|key| key.replace(&namespace_prefix, ""))
            .collect();

        Ok(Some(serde_json::json!(keys)))
    }
}