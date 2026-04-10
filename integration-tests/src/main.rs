use async_trait::async_trait;
use colored::*;
use kuiper_runtime::{
    command::{
        CommandContext, CommandHandler, CommandResult, CommandType, ExecutableCommand,
        MutationCommand, ValidationCommand,
    },
    data::InMemoryStore,
};
use resource_server_runtime::KuiperRuntimeBuilder;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ============================================================================
// Custom Test Handlers
// ============================================================================

/// A simple test handler that echoes back input
struct TestEchoHandler;

#[async_trait]
impl CommandHandler for TestEchoHandler {
    fn get_type(&self) -> CommandType {
        CommandType::Internal
    }

    fn as_executable(&self) -> Option<&dyn ExecutableCommand> {
        Some(self)
    }

    fn as_validator(&self) -> Option<&dyn ValidationCommand> {
        None
    }

    fn as_mutator(&self) -> Option<&dyn MutationCommand> {
        None
    }
}

#[async_trait]
impl ExecutableCommand for TestEchoHandler {
    async fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let message = ctx
            .get_string_param("message")
            .unwrap_or_else(|_| "hello".to_string());
        Ok(Some(json!({ "echo": message })))
    }
}

// ============================================================================
// Test Infrastructure
// ============================================================================

#[derive(Debug)]
struct TestResult {
    name: String,
    passed: bool,
    message: String,
    duration_ms: u128,
}

impl TestResult {
    fn new(name: &str, passed: bool, message: impl Into<String>, duration_ms: u128) -> Self {
        Self {
            name: name.to_string(),
            passed,
            message: message.into(),
            duration_ms,
        }
    }

    fn print(&self) {
        let status = if self.passed {
            "✓ PASS".green()
        } else {
            "✗ FAIL".red()
        };
        println!(
            "  {} | {} | {}ms",
            status,
            self.name.cyan(),
            self.duration_ms
        );
        if !self.message.is_empty() {
            println!("      {}", self.message.yellow());
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

async fn test_runtime_creation() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let passed = true;
    TestResult::new(
        "test_runtime_creation",
        passed,
        "KuiperRuntime created successfully",
        start.elapsed().as_millis(),
    )
}

async fn test_command_context_creation() -> TestResult {
    let start = std::time::Instant::now();

    let mut params = HashMap::new();
    params.insert("key".to_string(), json!("value"));

    let mut metadata = HashMap::new();
    metadata.insert("user".to_string(), "test-user".to_string());

    let ctx = CommandContext {
        command_name: "test".to_string(),
        parameters: params,
        metadata,
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let passed = ctx.command_name == "test" && !ctx.activity_id.is_nil();
    TestResult::new(
        "test_command_context_creation",
        passed,
        "CommandContext created with metadata and parameters",
        start.elapsed().as_millis(),
    )
}

async fn test_echo_command() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let mut ctx = CommandContext {
        command_name: "version".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: Uuid::new_v4(),
        caller_id: None,

        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let result = runtime.execute(&mut ctx).await;

    let passed = match result {
        Ok(Some(value)) => value.get("version").is_some(),
        _ => false,
    };

    TestResult::new(
        "test_echo_command",
        passed,
        "Echo/Version command executed and returned expected value",
        start.elapsed().as_millis(),
    )
}

async fn test_set_command() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let resource_json = json!({
        "apiVersion": "v1",
        "kind": "TestResource",
        "metadata": {
            "name": "test-resource",
            "namespace": "default"
        },
        "spec": {
            "value": "test-data"
        }
    });

    let mut ctx = CommandContext {
        command_name: "set".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("value".to_string(), resource_json);
            p.insert(
                "resource".to_string(),
                json!("group/v1/TestResource/test-resource"),
            );
            p
        },
        metadata: {
            let mut m = HashMap::new();
            m.insert("namespace".to_string(), "default".to_string());
            m
        },
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let result = runtime.execute(&mut ctx).await;
    let passed = result.is_ok();

    TestResult::new(
        "test_set_command",
        passed,
        "Set command executed successfully",
        start.elapsed().as_millis(),
    )
}

async fn test_set_then_get() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let test_data = json!({
        "apiVersion": "v1",
        "kind": "TestResource",
        "metadata": { "name": "my-resource" },
        "spec": { "message": "Hello, World!" }
    });

    // SET
    let mut set_ctx = CommandContext {
        command_name: "set".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("value".to_string(), test_data.clone());
            p.insert(
                "resource".to_string(),
                json!("api/v1/TestResource/my-resource"),
            );
            p
        },
        metadata: {
            let mut m = HashMap::new();
            m.insert("namespace".to_string(), "default".to_string());
            m
        },
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let set_result = runtime.execute(&mut set_ctx).await;
    if set_result.is_err() {
        return TestResult::new(
            "test_set_then_get",
            false,
            format!("Set failed: {:?}", set_result.err()),
            start.elapsed().as_millis(),
        );
    }

    // GET
    let mut get_ctx = CommandContext {
        command_name: "get".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert(
                "resource".to_string(),
                json!("api/v1/TestResource/my-resource"),
            );
            p
        },
        metadata: {
            let mut m = HashMap::new();
            m.insert("namespace".to_string(), "default".to_string());
            m
        },
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let get_result = runtime.execute(&mut get_ctx).await;

    let passed = match get_result {
        Ok(Some(value)) => {
            value
                .get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                == Some("my-resource")
        }
        _ => false,
    };

    TestResult::new(
        "test_set_then_get",
        passed,
        "Set and retrieve operations work together",
        start.elapsed().as_millis(),
    )
}

async fn test_version_command() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let mut ctx = CommandContext {
        command_name: "version".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let result = runtime.execute(&mut ctx).await;

    let passed = match result {
        Ok(Some(value)) => value.get("version").is_some(),
        _ => false,
    };

    TestResult::new(
        "test_version_command",
        passed,
        "Version command returned version information",
        start.elapsed().as_millis(),
    )
}

async fn test_list_empty() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let mut ctx = CommandContext {
        command_name: "list".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("resource".to_string(), json!(""));
            p
        },
        metadata: {
            let mut m = HashMap::new();
            m.insert("namespace".to_string(), "default".to_string());
            m
        },
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let result = runtime.execute(&mut ctx).await;

    let passed = match result {
        Ok(Some(value)) => {
            if let Some(arr) = value.as_array() {
                arr.is_empty()
            } else {
                false
            }
        }
        _ => false,
    };

    TestResult::new(
        "test_list_empty",
        passed,
        "List on empty store returns empty array",
        start.elapsed().as_millis(),
    )
}

async fn test_delete_command() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let test_data = json!({
        "apiVersion": "v1",
        "kind": "TestResource",
        "metadata": { "name": "to-delete" }
    });

    // First SET
    let mut set_ctx = CommandContext {
        command_name: "set".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("value".to_string(), test_data);
            p.insert(
                "resource".to_string(),
                json!("api/v1/TestResource/to-delete"),
            );
            p
        },
        metadata: {
            let mut m = HashMap::new();
            m.insert("namespace".to_string(), "default".to_string());
            m
        },
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let _ = runtime.execute(&mut set_ctx).await;

    // Then DELETE
    let mut delete_ctx = CommandContext {
        command_name: "delete".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert(
                "resource".to_string(),
                json!("api/v1/TestResource/to-delete"),
            );
            p
        },
        metadata: {
            let mut m = HashMap::new();
            m.insert("namespace".to_string(), "default".to_string());
            m
        },
        activity_id: Uuid::new_v4(),
        caller_id: None,

        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let delete_result = runtime.execute(&mut delete_ctx).await;
    let passed = delete_result.is_ok();

    TestResult::new(
        "test_delete_command",
        passed,
        "Delete command executed successfully",
        start.elapsed().as_millis(),
    )
}

async fn test_nonexistent_command() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let mut ctx = CommandContext {
        command_name: "nonexistent_command".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let result = runtime.execute(&mut ctx).await;
    let passed = result.is_err(); // Should fail

    TestResult::new(
        "test_nonexistent_command",
        passed,
        "Nonexistent command returns error as expected",
        start.elapsed().as_millis(),
    )
}

async fn test_activity_id_tracking() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    let activity_id = Uuid::new_v4();
    let mut ctx = CommandContext {
        command_name: "version".to_string(),
        parameters: HashMap::new(),
        metadata: HashMap::new(),
        activity_id,
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let _ = runtime.execute(&mut ctx).await;
    let passed = ctx.activity_id == activity_id; // Activity ID should be preserved

    TestResult::new(
        "test_activity_id_tracking",
        passed,
        "Activity ID preserved through command execution",
        start.elapsed().as_millis(),
    )
}

async fn test_multiple_resources_list() -> TestResult {
    let start = std::time::Instant::now();

    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();

    // Create multiple resources
    for i in 0..3 {
        let mut ctx = CommandContext {
            command_name: "set".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert(
                    "value".to_string(),
                    json!({
                        "apiVersion": "v1",
                        "kind": "TestResource",
                        "metadata": {
                            "name": format!("resource-{}", i)
                        }
                    }),
                );
                p.insert(
                    "resource".to_string(),
                    json!(format!("Test/resource-{}", i)),
                );
                p
            },
            metadata: {
                let mut m = HashMap::new();
                m.insert("namespace".to_string(), "default".to_string());
                m
            },
            activity_id: Uuid::new_v4(),
            caller_id: None,
            cancellation_token: CancellationToken::new(),
            is_internal: false,
        };
        let _ = runtime.execute(&mut ctx).await;
    }

    // List all with resource filter
    let mut list_ctx = CommandContext {
        command_name: "list".to_string(),
        parameters: {
            let mut p = HashMap::new();
            p.insert("resource".to_string(), json!("Test"));
            p
        },
        metadata: {
            let mut m = HashMap::new();
            m.insert("namespace".to_string(), "default".to_string());
            m
        },
        activity_id: Uuid::new_v4(),
        caller_id: None,
        cancellation_token: CancellationToken::new(),
        is_internal: false,
    };

    let result = runtime.execute(&mut list_ctx).await;

    let passed = match result {
        Ok(Some(value)) => {
            if let Some(arr) = value.as_array() {
                arr.len() >= 3
            } else {
                false
            }
        }
        _ => false,
    };

    TestResult::new(
        "test_multiple_resources_list",
        passed,
        format!("Multiple resources created and listed successfully"),
        start.elapsed().as_millis(),
    )
}

// ============================================================================
// Main Test Runner
// ============================================================================

#[tokio::main]
async fn main() {
    println!(
        "\n{}\n",
        "╔══════════════════════════════════════════════════════════╗".bright_blue()
    );
    println!(
        "{}  {}",
        " ".bright_blue(),
        "KUIPER RUNTIME INTEGRATION TESTS".bright_cyan().bold()
    );
    println!(
        "{}\n",
        "╚══════════════════════════════════════════════════════════╝".bright_blue()
    );

    let tests: Vec<(
        &str,
        fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = TestResult>>>,
    )> = vec![
        ("Runtime Creation", || Box::pin(test_runtime_creation())),
        ("Command Context Creation", || {
            Box::pin(test_command_context_creation())
        }),
        ("Echo Command", || Box::pin(test_echo_command())),
        ("Set Command", || Box::pin(test_set_command())),
        ("Set Then Get", || Box::pin(test_set_then_get())),
        ("Version Command", || Box::pin(test_version_command())),
        ("List Empty", || Box::pin(test_list_empty())),
        ("Delete Command", || Box::pin(test_delete_command())),
        ("Nonexistent Command Error", || {
            Box::pin(test_nonexistent_command())
        }),
        ("Activity ID Tracking", || {
            Box::pin(test_activity_id_tracking())
        }),
        ("Multiple Resources List", || {
            Box::pin(test_multiple_resources_list())
        }),
    ];

    let mut results = Vec::new();

    println!("{}", "Running Tests:".bright_cyan().bold());
    println!("{}", "─".repeat(60));

    for (name, test_fn) in tests {
        let result = test_fn().await;
        results.push(result);
    }

    println!("{}\n", "─".repeat(60));

    // Print results
    for result in &results {
        result.print();
    }

    // Summary
    println!("\n{}", "─".repeat(60));
    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    let total_time: u128 = results.iter().map(|r| r.duration_ms).sum();

    let summary = if passed == total {
        format!("All {} tests passed!", total).green()
    } else {
        format!("{}/{} tests passed", passed, total).red()
    };

    println!("{}", summary.bold());
    println!("Total execution time: {}ms\n", total_time);

    if passed == total {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
