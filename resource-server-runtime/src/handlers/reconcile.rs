use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use kuiper_runtime::{
    command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand},
    data::TransactionalKeyValueStore,
};
use kuiper_types::model::resource::SystemObject;
use tokio::sync::RwLock;
use tracing::{self, instrument};

use crate::constants::RESOURCE_CONTAINER;

pub struct ReconcileCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
}

impl ReconcileCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl CommandHandler for ReconcileCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for ReconcileCommand {
    async fn execute(&self, _: &CommandContext) -> CommandResult {
        let store = self.store.write().await;

        if !store
            .container_exists("resource")
            .await
            .context("Failed to check if resource container exists")?
        {
            store
                .new_container("resource")
                .await
                .context("Failed to create new resource container")?;
        }

        let resources: Vec<String> = store
            .list_keys("resource", None)
            .await
            .context("Failed to list resource")?;

        for resource in resources {
            let resource_data = store
                .get("resource", &resource)
                .await
                .context(format!("Failed to get resource {}", resource))?;

            let resource_value: SystemObject = serde_json::from_slice(&resource_data)?;

            if resource_value.metadata.deletion_timestamp.is_none() {
                continue;
            }

            if resource_value
                .metadata
                .finalizers
                .unwrap_or(vec![])
                .is_empty()
            {
                // No finalizers, safe to delete immediately
                store
                    .delete("resource", &resource)
                    .await
                    .context(format!("Failed to delete resource {}", resource))?;
                tracing::info!("Deleted resource {}", resource);
                continue;
            } else {
                tracing::debug!(
                    "Resource {} has finalizers, marked for pending deletion",
                    resource
                );
            }
        }

        Ok(None)
    }
}
