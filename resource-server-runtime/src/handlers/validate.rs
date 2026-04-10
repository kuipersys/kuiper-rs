use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use kuiper_runtime::command::{
    CommandContext, CommandHandler, CommandResult, CommandType, ValidationCommand,
};
use kuiper_types::error::KuiperError;
use tokio::sync::RwLock;

use crate::registry::ResourceRegistry;

/// Validates the `spec` of an incoming resource against the JSON Schema stored
/// in the matching `ResourceDefinitionVersion`.
pub struct SchemaValidationCommand {
    registry: Arc<RwLock<ResourceRegistry>>,
}

impl SchemaValidationCommand {
    pub fn new(registry: Arc<RwLock<ResourceRegistry>>) -> Self {
        Self { registry }
    }
}

impl CommandHandler for SchemaValidationCommand {
    fn get_type(&self) -> CommandType {
        CommandType::Validator
    }

    fn as_validator(&self) -> Option<&dyn ValidationCommand> {
        Some(self)
    }
}

#[async_trait]
impl ValidationCommand for SchemaValidationCommand {
    async fn validate(&self, ctx: &CommandContext) -> CommandResult {
        if ctx.command_name != "set" || ctx.is_internal {
            return Ok(None);
        }

        let raw_value = match ctx.parameters.get("value") {
            Some(v) => v.clone(),
            None => return Ok(None),
        };

        let api_version = match raw_value.get("apiVersion").and_then(|v| v.as_str()) {
            Some(v) => v.to_string(),
            None => return Ok(None),
        };
        let kind = match raw_value.get("kind").and_then(|v| v.as_str()) {
            Some(v) => v.to_string(),
            None => return Ok(None),
        };

        let (group, version) = match api_version.split_once('/') {
            Some((g, v)) => (g.to_string(), v.to_string()),
            None => return Ok(None),
        };

        let schema = {
            let reg = self.registry.read().await;
            reg.get_version(&group, &kind, &version)
                .and_then(|v| v.schema.clone())
        };

        let schema = match schema {
            Some(s) => s,
            None => return Ok(None),
        };

        let subject = raw_value.get("spec").unwrap_or(&raw_value);

        let validator = jsonschema::validator_for(&schema)
            .context("Failed to compile JSON Schema from ResourceDefinition")?;

        let errors: Vec<String> = validator
            .iter_errors(subject)
            .map(|e| format!("{} (path: {})", e, e.instance_path()))
            .collect();

        if !errors.is_empty() {
            return Err(KuiperError::Invalid(format!(
                "Resource '{}' failed schema validation:\n{}",
                kind,
                errors.join("\n")
            ))
            .into());
        }

        Ok(None)
    }
}
