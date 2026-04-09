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

use super::{resource_key, RESOURCE_CONTAINER};

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
