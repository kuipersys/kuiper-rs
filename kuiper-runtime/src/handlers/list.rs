use std::sync::Arc;

use crate::{
    command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand},
    data::TransactionalKeyValueStore,
};
use anyhow::Context;
use async_trait::async_trait;
use kuiper_types::model::resource::SystemObject;
use tokio::sync::RwLock;

use crate::constants::{resource_key, RESOURCE_CONTAINER};

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

            if let Ok(v) = serde_json::to_value(&obj) {
                items.push(v);
            }
        }

        Ok(Some(serde_json::json!(items)))
    }
}
