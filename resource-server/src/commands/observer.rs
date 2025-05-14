use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use kuiper_runtime_sdk::{command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand}, data::TransactionalKeyValueStore, model::resource::SystemObject};
use tokio::sync::RwLock;

use crate::SubscriberMap;

pub struct SetObserverCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
    subscribers: SubscriberMap,
}

impl SetObserverCommand {
    pub fn new(store: Arc<RwLock<dyn TransactionalKeyValueStore>>, subscribers: SubscriberMap) -> Self {
        Self { 
            store,
            subscribers
         }
    }

    pub fn as_handler(&self) -> &dyn CommandHandler {
        self
    }
}

#[async_trait]
impl CommandHandler for SetObserverCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Observer
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for SetObserverCommand {
    // This command is executed in the context of the Kuiper runtime
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let value = ctx.get_param("value")
            .context("Missing 'value' parameter")?;

        let system_object = serde_json::from_str::<SystemObject>(&value)
            .context("Failed to deserialize 'value' parameter")?;

        let resource = format!("{}/{}", system_object.api_version, system_object.kind);

        let ctx_str_value = serde_json::to_string(&system_object)
            .context("Failed to serialize system object")?;

        let ctx_value = serde_json::from_str::<serde_json::Value>(&ctx_str_value)
            .context("Failed to deserialize system object to JSON")?;

        for subscriber in self.subscribers.iter() {
            if let Err(e) = subscriber.send(crate::actors::models::ServerMessage::Event { 
                resource: resource.clone(), 
                namespace: system_object.metadata.namespace.clone(), 
                action: ctx.command_name.clone(), 
                object: ctx_value.clone()
            }) {
                return Err(anyhow::anyhow!("Failed to notify subscriber: {}", e));
            }
        }

        return Ok(None);
    }
}