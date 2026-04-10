use std::sync::Arc;

use crate::{
    command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand},
    data::TransactionalKeyValueStore,
};
use anyhow::Context;
use async_trait::async_trait;
use kuiper_types::{error::KuiperError, model::resource::SystemObject};
use tokio::sync::RwLock;

use crate::constants::{resource_key, RESOURCE_CONTAINER};

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

        let result = serde_json::to_value(&obj).context("Failed to serialize SystemObject")?;
        Ok(Some(result))
    }
}
