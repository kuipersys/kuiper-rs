use async_trait::async_trait;
use kuiper_runtime::command::{
    CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand,
};

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

pub fn get_version_string() -> String {
    format!(
        "{} v{}-{}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        truncate(env!("VERGEN_GIT_SHA"), 8)
    )
}

// ── VersionCommand ────────────────────────────────────────────────────────────

pub struct VersionCommand;

impl CommandHandler for VersionCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }
}

#[async_trait]
impl ExecutableCommand for VersionCommand {
    async fn execute(&self, _: &CommandContext) -> CommandResult {
        Ok(Some(serde_json::json!({
            "version": get_version_string(),
        })))
    }
}
