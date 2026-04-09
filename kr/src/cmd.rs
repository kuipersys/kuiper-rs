use std::collections::HashMap;

use clap::{Args, Parser, Subcommand, ValueEnum};
use kuiper_runtime_sdk::command::CommandContext;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Reads a file (JSON or YAML) and returns the content as a `serde_json::Value`.
/// YAML files are detected by `.yaml` / `.yml` extension; everything else is
/// assumed to be JSON.
fn read_resource_file(path: &str) -> anyhow::Result<serde_json::Value> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", path, e))?;

    let value: serde_json::Value = if path.ends_with(".yaml") || path.ends_with(".yml") {
        yaml_serde::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("Failed to parse YAML file '{}': {}", path, e))?
    } else {
        serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON file '{}': {}", path, e))?
    };

    Ok(value)
}

/// Derives the resource path string (`{group}/{version}/{kind}/{name}`) from a
/// parsed `SystemObject`-shaped JSON value so `SetCommand` can store it under
/// the right key.
fn resource_path_from_value(value: &serde_json::Value) -> anyhow::Result<String> {
    let api_version = value
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'apiVersion' field in resource file"))?;

    let kind = value
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'kind' field in resource file"))?;

    let name = value
        .pointer("/metadata/name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'metadata.name' field in resource file"))?;

    // apiVersion can be either "{group}/{version}" or just "{version}"
    let resource_path = if api_version.contains('/') {
        format!("{}/{}/{}", api_version, kind, name)
    } else {
        format!("{}/{}/{}", api_version, kind, name)
    };

    Ok(resource_path)
}

#[derive(Parser, Debug)]
#[command(name = "kr", about = "Kuiper Resources CLI")]
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
    Yaml,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    // Data Interaction Commands
    Get(CommonArgs),
    List(CommonArgs),
    Set(CommonArgs),
    Delete(CommonArgs),

    /// Register a ResourceDefinition (privileged — sets is_internal on the context).
    Define(CommonArgs),

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
        let (verb, args, is_internal) = match self {
            Command::Echo(args) => ("echo", args, false),
            Command::Get(args) => ("get", args, false),
            Command::List(args) => ("list", args, false),
            Command::Delete(args) => ("delete", args, false),
            Command::Set(args) => ("set", args, false),
            Command::Version(args) => ("version", args, false),
            Command::Define(args) => ("set", args, true),
            _ => panic!("Unsupported command"),
        };

        let mut parameters: HashMap<String, serde_json::Value> = args
            .parameters
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

        metadata.insert("namespace".to_string(), args.namespace.clone());
        metadata.insert("force".to_string(), args.force.to_string());
        metadata.insert("verbose".to_string(), args.verbose.to_string());

        // ── File loading ──────────────────────────────────────────────────────
        // If a --file is provided and this is a set/apply-style command, read
        // the file and inject its content as the `value` parameter.  The
        // resource path is derived automatically from the object's apiVersion /
        // kind / metadata.name fields so the caller does not need to supply
        // --param resource=... separately.
        if let Some(ref file_path) = args.file {
            metadata.insert("file".to_string(), file_path.clone());

            if (verb == "set") && !parameters.contains_key("value") {
                match read_resource_file(file_path) {
                    Ok(value) => {
                        // Derive the resource path if not already supplied.
                        if !parameters.contains_key("resource") {
                            match resource_path_from_value(&value) {
                                Ok(path) => {
                                    parameters
                                        .insert("resource".to_string(), serde_json::json!(path));
                                }
                                Err(e) => {
                                    eprintln!("Warning: could not derive resource path: {}", e);
                                }
                            }
                        }

                        // Derive namespace from metadata.namespace if not
                        // explicitly overridden on the command line.
                        if args.namespace == "default" {
                            if let Some(ns) = value
                                .pointer("/metadata/namespace")
                                .and_then(|v| v.as_str())
                            {
                                metadata.insert("namespace".to_string(), ns.to_string());
                            }
                        }

                        parameters.insert("value".to_string(), value);
                    }
                    Err(e) => {
                        eprintln!("Error reading resource file: {}", e);
                    }
                }
            }
        }

        CommandContext {
            command_name: verb.to_string(),
            parameters,
            metadata,
            activity_id: Uuid::new_v4(),
            is_internal,
            cancellation_token: CancellationToken::new(),
        }
    }
}
