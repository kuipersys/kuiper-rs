use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use kuiper_runtime::{
    command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand},
    data::TransactionalKeyValueStore,
};
use kuiper_types::{error::KuiperError, model::resource::SystemObject};
use tokio::sync::RwLock;

use crate::constants::{resource_key, RESOURCE_CONTAINER};

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

        // If there are no finalizers, we can delete immediately. Otherwise, we need to set the deletion timestamp.
        if obj
            .metadata
            .finalizers
            .as_ref()
            .map_or(true, |f| f.is_empty())
        {
            // No finalizers, safe to delete immediately
            store
                .delete(RESOURCE_CONTAINER, &key)
                .await
                .context(format!("Failed to delete resource {}", resource))?;
            tracing::info!("Deleted resource {}", resource);
            return Ok(None);
        }

        // If the resource is already marked for deletion, return the existing object
        // so observers still receive the event.
        if obj.metadata.deletion_timestamp.is_some() {
            let result =
                serde_json::to_value(&obj).context("Failed to convert SystemObject to JSON")?;
            return Ok(Some(result));
        }

        obj.metadata.deletion_timestamp = Some(chrono::Utc::now().timestamp_micros());

        let value_bytes =
            serde_json::to_vec_pretty(&obj).context("Failed to serialize SystemObject")?;

        store.put(RESOURCE_CONTAINER, &key, value_bytes).await?;

        let result =
            serde_json::to_value(&obj).context("Failed to convert SystemObject to JSON")?;
        Ok(Some(result))
    }
}
