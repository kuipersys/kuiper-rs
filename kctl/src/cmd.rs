use std::collections::HashMap;

use clap::{Args, Parser, Subcommand, ValueEnum};
use kuiper_runtime_sdk::command::CommandContext;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "kctl", about = "Kuiper Control CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Print output in pretty (indented) format
    #[arg(long, default_value_t = false)]
    pub pretty: bool,

    /// Set the output format (standard, json), default is standard
    #[arg(long, short = 'o', value_enum, default_value_t = OutputFormat::Standard)]
    pub output: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq)]
pub enum OutputFormat {
    Standard,
    Json,
    Yaml
}

#[derive(Subcommand, Debug)]
pub enum Command {
    // Data Interaction Commands
    Get(CommonArgs),
    List(CommonArgs),
    Set(CommonArgs),
    Delete(CommonArgs),

    // Config / Application Management Commands
    // Apply(CommonArgs),
    // Patch(CommonArgs),
    // Sync(CommonArgs),

    // Runtime / Administrative Commands
    // Status(CommonArgs),
    // Reload(CommonArgs),
    // Restart(CommonArgs),
    // Start(CommonArgs),
    // Stop(CommonArgs),
    Version(CommonArgs),

    // Debugging / Monitoring Commands
    Echo(CommonArgs),
    // Log(CommonArgs),
    // Diff(CommonArgs),
    // Validate(CommonArgs),
    // Used to export the current state of the system to a file or other format
    // Export(CommonArgs),
    // Bulk Import of resources from a file - this the reverse of export
    // Import(CommonArgs),

    // Miscellaneous Commands
    // Exec(CommonArgs),
}

#[derive(Args, Debug)]
pub struct CommonArgs {
    /// Target resource (e.g., sensor, node, etc.)
    #[arg()]
    pub resource: Option<String>,

    /// Name of the resource, if applicable
    #[arg()]
    #[arg(long = "name", value_name = "NAME")]
    pub name: Option<String>,

    /// Parameters in key=value format
    #[arg(long = "param", value_parser = parse_key_val, num_args = 0..)]
    pub parameters: Vec<(String, String)>,

    /// Metadata in key=value format
    #[arg(long = "meta", value_parser = parse_key_val, num_args = 0..)]
    pub metadata: Vec<(String, String)>,

    /// Namespace for the resource, if applicable. The default namespace is "default"
    #[arg(long = "namespace", short = 'n', value_name = "NAMESPACE", default_value_t = String::from("default"))]
    pub namespace: String,

    /// Input file for the command, if applicable
    #[arg(long = "file", short = 'f', value_name = "FILE")]
    pub file: Option<String>,

    /// Force the command to execute, ignoring any warnings or errors if applicable
    #[arg(long, default_value_t = false)]
    pub force: bool,

    /// Print verbose output
    #[arg(long, short = 'v', default_value_t = false)]
    pub verbose: bool,
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid key=value pair: {}", s));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

impl Command {
    pub fn into_context(self) -> CommandContext {
        let (verb, args) = match self {
            Command::Echo(args) => ("echo", args),
            Command::Get(args) => ("get", args),
            Command::List(args) => ("list", args),
            Command::Delete(args) => ("delete", args),
            Command::Set(args) => ("set", args),
            Command::Version(args) => ("version", args),
            _ => panic!("Unsupported command"),
        };

        let mut parameters: HashMap<String, serde_json::Value> = args.parameters
            .into_iter()
            .map(|(k, v)| (k, serde_json::json!(v)))
            .collect();

        // Optionally include the resource as a parameter
        if let Some(res) = args.resource {
            parameters.insert("resource".to_string(), serde_json::json!(res));
        }

        if let Some(name) = args.name {
            parameters.insert("name".to_string(), serde_json::json!(name));
        }


        let mut metadata: HashMap<String, String> = args.metadata.into_iter().collect();

        metadata.insert("namespace".to_string(), args.namespace);
        metadata.insert("force".to_string(), args.force.to_string());
        metadata.insert("verbose".to_string(), args.verbose.to_string());

        if let Some(file) = args.file {
            metadata.insert("file".to_string(), file);
        }
        
        CommandContext {
            command_name: verb.to_string(),
            parameters,
            metadata,
            activity_id: Uuid::new_v4(),
            cancellation_token: CancellationToken::new(),
        }
    }
}

