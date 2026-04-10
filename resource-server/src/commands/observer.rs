use std::{str::FromStr, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use kuiper_runtime_sdk::{
    command::{CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand},
    data::TransactionalKeyValueStore,
    model::resource::SystemObject,
};
use tokio::sync::RwLock;

use crate::{SubscriberMap, SubscriptionMap};

const WILDCARD_RESOURCE: &str = "*";

pub struct SetObserverCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
    subscribers: SubscriberMap,
    subscription_map: SubscriptionMap,
}

impl SetObserverCommand {
    pub fn new(
        store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
        subscribers: SubscriberMap,
        subscription_map: SubscriptionMap,
    ) -> Self {
        Self {
            store,
            subscribers,
            subscription_map,
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
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let value = match ctx.get_param("value") {
            Ok(v) => v,
            Err(_) => return Ok(None), // no result to observe
        };

        let system_object = serde_json::from_str::<SystemObject>(&value)
            .context("Failed to deserialize 'value' parameter")?;

        let resource = format!("{}/{}", system_object.api_version, system_object.kind);

        let ctx_value =
            serde_json::to_value(&system_object).context("Failed to serialize system object")?;

        let wildcard = WILDCARD_RESOURCE.to_string();

        for entry in self.subscribers.iter() {
            let client_id = entry.key();

            // Notify clients subscribed to this specific resource type or to the wildcard "*".
            let is_subscribed = self
                .subscription_map
                .get(client_id)
                .map(|subs| subs.contains(&resource) || subs.contains(&wildcard))
                .unwrap_or(false);

            if !is_subscribed {
                continue;
            }

            if let Err(e) = entry
                .value()
                .send(crate::actors::models::ServerMessage::Event {
                    resource: resource.clone(),
                    namespace: system_object.metadata.namespace.clone(),
                    action: ctx.command_name.clone(),
                    object: ctx_value.clone(),
                })
            {
                tracing::warn!("Failed to notify subscriber {}: {}", client_id, e);
            }
        }

        Ok(None)
    }
}

pub struct DeleteObserverCommand {
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
    subscribers: SubscriberMap,
    subscription_map: SubscriptionMap,
}

impl DeleteObserverCommand {
    pub fn new(
        store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
        subscribers: SubscriberMap,
        subscription_map: SubscriptionMap,
    ) -> Self {
        Self {
            store,
            subscribers,
            subscription_map,
        }
    }

    pub fn as_handler(&self) -> &dyn CommandHandler {
        self
    }
}

#[async_trait]
impl CommandHandler for DeleteObserverCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Observer
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for DeleteObserverCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let value = match ctx.get_param("value") {
            Ok(v) => v,
            Err(_) => return Ok(None), // no result to observe
        };

        let system_object = serde_json::from_str::<SystemObject>(&value)
            .context("Failed to deserialize 'value' parameter")?;

        let resource = format!("{}/{}", system_object.api_version, system_object.kind);

        let ctx_value =
            serde_json::to_value(&system_object).context("Failed to serialize system object")?;

        let wildcard = WILDCARD_RESOURCE.to_string();

        for entry in self.subscribers.iter() {
            let client_id = entry.key();

            // Notify clients subscribed to this specific resource type or to the wildcard "*".
            let is_subscribed = self
                .subscription_map
                .get(client_id)
                .map(|subs| subs.contains(&resource) || subs.contains(&wildcard))
                .unwrap_or(false);

            if !is_subscribed {
                continue;
            }

            if let Err(e) = entry
                .value()
                .send(crate::actors::models::ServerMessage::Event {
                    resource: resource.clone(),
                    namespace: system_object.metadata.namespace.clone(),
                    action: ctx.command_name.clone(),
                    object: ctx_value.clone(),
                })
            {
                tracing::warn!("Failed to notify subscriber {}: {}", client_id, e);
            }
        }

        Ok(None)
    }
}
