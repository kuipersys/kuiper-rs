pub mod admission;
pub mod delete;
pub mod echo;
pub mod get;
pub mod list;
pub mod reconcile;
pub mod set;
pub mod validate;
pub mod version;

use async_trait::async_trait;
use kuiper_runtime::command::{
    CommandContext, CommandDispatcher, CommandHandler, CommandResult, CommandType,
};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

pub struct CommandExecutor {
    handlers: HashMap<String, Vec<Arc<dyn CommandHandler>>>,
}

impl CommandExecutor {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register_handler(&mut self, name: &str, handler: Arc<dyn CommandHandler>) {
        if let Some(existing_handlers) = self.handlers.get_mut(name) {
            existing_handlers.push(handler);
            return;
        }

        self.handlers.insert(name.to_string(), vec![handler]);
    }

    async fn execute_handler(
        &self,
        ctx: &mut CommandContext,
        handler: &Arc<dyn CommandHandler>,
    ) -> CommandResult {
        if let Some(validator) = handler.as_validator() {
            validator.validate(ctx).await?;

            return Ok(None);
        }

        if let Some(validator) = handler.as_mutator() {
            validator.mutate(ctx).await?;

            return Ok(None);
        }

        if let Some(executable) = handler.as_executable() {
            return Ok(executable.execute(ctx).await?);
        }

        Err(anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Handler does not implement any command type: {}",
                handler.get_type().as_str()
            ),
        )))
    }
}

#[async_trait]
impl CommandDispatcher for CommandExecutor {
    async fn dispatch(&self, ctx: &mut CommandContext) -> CommandResult {
        match self.handlers.get(&ctx.command_name) {
            Some(handlers) => {
                let mut final_result: Option<Value> = None;

                let mut sorted_handlers = handlers.clone();

                sorted_handlers
                    .sort_by(|a, b| a.get_type().priority().cmp(&b.get_type().priority()));

                for handler in sorted_handlers {
                    let result = self.execute_handler(ctx, &handler).await?;

                    if final_result.is_none() && handler.get_type() == CommandType::Internal {
                        final_result = result.clone();
                        if let Some(ref value) = result {
                            ctx.parameters.insert("value".to_string(), value.clone());
                        }
                    }
                }

                return Ok(final_result);
            }
            None => Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Command handler not found for: {}", ctx.command_name),
            ))),
        }
    }
}
