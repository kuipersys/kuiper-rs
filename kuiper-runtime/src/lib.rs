mod command;
mod config;
mod constants;
pub mod registry;
pub mod service;

#[cfg(test)]
mod tests;

pub use config::KuiperConfig;
pub use registry::ResourceRegistry;
pub use service::HostedService;

use std::sync::Arc;

use command::{
    delete::DeleteCommand, echo::EchoCommand, get::GetCommand, list::ListCommand,
    reconcile::ReconcileCommand, set::SetCommand, validate::SchemaValidationCommand,
    version::VersionCommand, CommandExecutor,
};
use kuiper_runtime_sdk::{
    command::{CommandContext, CommandDispatcher, CommandHandler, CommandResult},
    data::TransactionalKeyValueStore,
};
use tokio::sync::RwLock;

pub struct KuiperRuntimeBuilder {
    config: KuiperConfig,
    executor: CommandExecutor,
    registry: Arc<RwLock<ResourceRegistry>>,
    store: Arc<RwLock<dyn TransactionalKeyValueStore>>,
}

impl KuiperRuntimeBuilder {
    pub fn new(shared_store: Arc<RwLock<dyn TransactionalKeyValueStore>>) -> Self {
        let registry = Arc::new(RwLock::new(ResourceRegistry::new(shared_store.clone())));

        let mut executor = CommandExecutor::new();
        executor.register_handler("echo", Arc::new(EchoCommand));
        executor.register_handler("version", Arc::new(VersionCommand));
        executor.register_handler("get", Arc::new(GetCommand::new(shared_store.clone())));
        executor.register_handler(
            "set",
            Arc::new(SetCommand::new(
                shared_store.clone(),
                Some(registry.clone()),
            )),
        );
        executor.register_handler(
            "set",
            Arc::new(SchemaValidationCommand::new(registry.clone())),
        );
        executor.register_handler("delete", Arc::new(DeleteCommand::new(shared_store.clone())));
        executor.register_handler("list", Arc::new(ListCommand::new(shared_store.clone())));

        Self {
            config: KuiperConfig::default(),
            executor,
            registry,
            store: shared_store,
        }
    }

    pub fn register_handler(&mut self, name: &str, handler: Arc<dyn CommandHandler>) -> &mut Self {
        self.executor.register_handler(name, handler);
        self
    }

    /// Registers the `reconcile` command. Call this for consumers that are
    /// permitted to run reconciliation (coordinator, kr CLI). Do **not** call
    /// this for the resource-server so that reconcile is not reachable via the
    /// HTTP/WebSocket API.
    pub fn with_reconciliation(&mut self) -> &mut Self {
        self.executor.register_handler(
            "reconcile",
            Arc::new(ReconcileCommand::new(self.store.clone())),
        );
        self
    }

    pub fn build(self) -> KuiperRuntime {
        KuiperRuntime {
            config: self.config,
            executor: Arc::new(self.executor),
            registry: self.registry,
        }
    }
}

pub struct KuiperRuntime {
    config: KuiperConfig,
    executor: Arc<CommandExecutor>,
    registry: Arc<RwLock<ResourceRegistry>>,
}

impl KuiperRuntime {
    /// Seeds the built-in core `ResourceDefinition` objects and loads all
    /// persisted definitions into the in-memory registry.
    ///
    /// Call this once after `KuiperRuntimeBuilder::build()`, analogous to
    /// `InitializeResourceServerAsync()` in the C# implementation.
    pub async fn initialize(&self) -> anyhow::Result<()> {
        self.registry.write().await.initialize().await
    }

    /// Returns a clone of the registry handle for external inspection.
    pub fn registry(&self) -> Arc<RwLock<ResourceRegistry>> {
        self.registry.clone()
    }

    pub async fn execute(&self, context: &mut CommandContext) -> CommandResult {
        self.executor.clone().dispatch(context).await
    }
}
