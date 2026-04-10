use crate::command::{
    CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand,
};
use async_trait::async_trait;

pub struct EchoCommand;

#[async_trait]
impl CommandHandler for EchoCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for EchoCommand {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let message = ctx
            .get_string_param("message")
            .unwrap_or_else(|_| "hello".to_string());
        Ok(Some(serde_json::json!({ "echo": message })))
    }
}
