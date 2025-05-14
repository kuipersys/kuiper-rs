mod cmd;
mod formatter;

use std::sync::Arc;

use clap::Parser;
use cmd::Cli;
use kuiper_runtime::KuiperRuntimeBuilder;
use kuiper_runtime_sdk::data::file_system_store::FileSystemStore;
#[tokio::main]

async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = FileSystemStore::new("c:\\cloud-api\\kuiper\\store").unwrap();
    let builder = KuiperRuntimeBuilder::new(Arc::new(tokio::sync::RwLock::new(store)));
    let runtime = builder.build();
    
    let cli = Cli::parse();
    let mut context = cli.command.into_context();

    // Simulate cancellation after 2 seconds
    // tokio::spawn({
    //     let token = context.clone();
    //     async move {
    //         tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    //         token.cancel();
    //     }
    // });

    let command_result = runtime.execute(&mut context).await;

    match command_result {
        Ok(result) => {
            if let Some(result) = result {
                // Print the result in the specified format
                formatter::print_output(&result, cli.output, cli.pretty);
            }
        },
        Err(err) => {
            eprintln!("Error executing command: {}", err);
        }
    }

    Ok(())
}
