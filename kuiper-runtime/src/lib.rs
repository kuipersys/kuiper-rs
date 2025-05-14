mod command;
mod config;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use command::{commands::{DeleteCommand, EchoCommand, GetCommand, ListCommand, SetCommand, VersionCommand}, reconcile::ReconcileCommand, CommandExecutor};
use config::KuiperConfig;
use kuiper_runtime_sdk::{command::{CommandContext, CommandDispatcher, CommandHandler, CommandResult, CommandType}, data::TransactionalKeyValueStore};
use tokio::sync::RwLock;

pub struct KuiperRuntimeBuilder {
    config: KuiperConfig,
    executor: CommandExecutor,
}

impl KuiperRuntimeBuilder {
    pub fn new(shared_store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        let mut executor = CommandExecutor::new();
        executor.register_handler("echo", Arc::new(EchoCommand));
        executor.register_handler("version", Arc::new(VersionCommand));
        executor.register_handler("get", Arc::new(GetCommand::new(shared_store.clone())));
        executor.register_handler("set", Arc::new(SetCommand::new(shared_store.clone())));
        executor.register_handler("delete", Arc::new(DeleteCommand::new(shared_store.clone())));
        executor.register_handler("list", Arc::new(ListCommand::new(shared_store.clone())));
        executor.register_handler("reconcile", Arc::new(ReconcileCommand::new(shared_store.clone())));

        Self {
            config: KuiperConfig {},
            executor
        }
    }

    pub fn register_handler(
        &mut self,
        name: &str,
        handler: Arc<dyn CommandHandler>
    ) -> &mut Self
    {
        if handler.get_type() == CommandType::Internal {
            panic!("Cannot register internal command handler: {}", name);
        }

        self.executor.register_handler(name, handler);

        return self;
    }

    pub fn build(self) -> KuiperRuntime {
        KuiperRuntime {
            config: self.config,
            executor: Arc::new(self.executor),
        }
    }
}

pub struct KuiperRuntime {
    config: KuiperConfig,
    executor: Arc<CommandExecutor>,
}

impl KuiperRuntime {
    // pub fn hotswap_executor(&mut self, executor: CommandExecutor) {
    //     self.executor = Arc::new(executor);
    // }

    pub async fn execute(&self, context: &mut CommandContext) -> CommandResult {
        self.executor.clone().dispatch(context).await
    }
}