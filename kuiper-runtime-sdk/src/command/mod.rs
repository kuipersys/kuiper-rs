use std::collections::HashMap;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContext {
    pub command_name: String,
    pub parameters: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, String>,
    pub activity_id: Uuid,

    #[serde(skip)]
    pub cancellation_token: CancellationToken,
}

impl CommandContext {
    pub fn get_string_param(&self, name: &str) -> anyhow::Result<String> {
        self.parameters.get(name)
            .ok_or_else( || anyhow::anyhow!("Missing required parameter: {}", name))?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("Invalid type for parameter '{}'; expected string", name))
    }

    pub fn get_param(&self, name: &str) -> anyhow::Result<String> {
        let parameter = self.parameters.get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: {}", name));

        if let Ok(param) = parameter {
            return Ok(serde_json::to_string(&param).unwrap());
        }

        return Err(anyhow::anyhow!("Missing required parameter: {}", name));
    }
}

// Optional: Standardized Result Type
pub type CommandResult = anyhow::Result<Option<serde_json::Value>>;

#[async_trait]
pub trait CommandDispatcher: Send + Sync {
    async fn dispatch(&self, ctx: &mut CommandContext) -> CommandResult;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandType {
    /// Mutator commands are responsible for changing the state of the system.
    /// They are executed first in the command pipeline.
    Mutator,
    
    /// Validator commands are responsible for validating the state of the system.
    /// They are executed after mutator commands and before finalizer commands.
    Validator,

    /// Internal commands are used for internal operations and are not exposed to users.
    /// They are executed after validator commands and before observer commands.
    Internal,

    /// Observer commands are used for observing the state of the system.
    /// They are executed last in the command pipeline.
    Observer,
}

impl CommandType {
    pub fn priority(&self) -> u8 {
        match self {
            CommandType::Mutator => 0,
            CommandType::Validator => 1,
            CommandType::Internal => 2,
            CommandType::Observer => 4,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CommandType::Mutator => "mutator",
            CommandType::Validator => "validator",
            CommandType::Internal => "internal",
            CommandType::Observer => "observer",
        }
    }
}

#[async_trait]
pub trait ValidationCommand: Send + Sync {
    async fn validate(&self, ctx: &CommandContext) -> CommandResult;
}

#[async_trait]
pub trait MutationCommand: Send + Sync {
    async fn mutate(&self, ctx: &mut CommandContext) -> CommandResult;
}

#[async_trait]
pub trait ExecutableCommand: Send + Sync {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult;
}

#[async_trait]
pub trait CommandHandler: Send + Sync {
    fn get_type(&self) -> CommandType;

    fn as_validator(&self) -> Option<&dyn ValidationCommand> {
        None
    }

    fn as_mutator(&self) -> Option<&dyn MutationCommand> {
        None
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        None
    }
}