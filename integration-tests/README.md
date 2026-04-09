# Kuiper Runtime Integration Tests

A comprehensive, **standalone integration test suite** for the Kuiper runtime and command execution pipeline. This test application is completely independent from the rest of the codebase and serves as both a test harness and a practical guide to understanding how the runtime works.

## Overview

The Kuiper runtime is built on a **command-driven architecture** with a priority-based handler pipeline. This integration test suite validates the core execution pipeline by testing:

- ✅ Runtime initialization and configuration
- ✅ Command context creation and metadata handling
- ✅ Command dispatch and handler execution
- ✅ CRUD operations (create, read, update, delete)
- ✅ List operations with resource filtering
- ✅ Error handling and edge cases
- ✅ Activity ID tracking for request correlation

## Running the Tests

### Quick Start

```bash
# Build and run in debug mode
cargo run -p kuiper-integration-tests

# Build and run in release mode (faster)
cargo run -p kuiper-integration-tests --release
```

### What to Expect

When you run the tests, you'll see a summary like this:

```
╔══════════════════════════════════════════════════════════╗

   KUIPER RUNTIME INTEGRATION TESTS
╚══════════════════════════════════════════════════════════╝

Running Tests:
────────────────────────────────────────────────────────────

  ✓ PASS | test_runtime_creation | 0ms
  ✓ PASS | test_command_context_creation | 0ms
  ✓ PASS | test_echo_command | 0ms
  ✓ PASS | test_set_command | 0ms
  ✓ PASS | test_set_then_get | 0ms
  ✓ PASS | test_version_command | 0ms
  ✓ PASS | test_list_empty | 0ms
  ✓ PASS | test_delete_command | 0ms
  ✓ PASS | test_nonexistent_command | 0ms
  ✓ PASS | test_activity_id_tracking | 0ms
  ✓ PASS | test_multiple_resources_list | 0ms

────────────────────────────────────────────────────────────
All 11 tests passed!
Total execution time: 0ms
```

## Test Coverage

### 1. Runtime Creation (`test_runtime_creation`)
- ✅ Creates a KuiperRuntime with InMemoryStore
- **What it tests**: Builder pattern and runtime initialization

### 2. Command Context Creation (`test_command_context_creation`)
- ✅ Creates CommandContext with parameters and metadata
- **What it tests**: Context instantiation with activity ID and correlation data

### 3. Echo/Version Command (`test_echo_command`)
- ✅ Executes the built-in version command
- **What it tests**: Command dispatch for simple read-only operations

### 4. Set Command (`test_set_command`)
- ✅ Stores a resource in the in-memory store
- **What it tests**: Write operations and state mutation

### 5. Set Then Get (`test_set_then_get`)
- ✅ Creates a resource, then retrieves it
- **What it tests**: Full CRUD round-trip and data integrity

### 6. Version Command (`test_version_command`)
- ✅ Retrieves version information
- **What it tests**: Built-in query commands

### 7. List Empty (`test_list_empty`)
- ✅ Lists resources from an empty store
- **What it tests**: List operations with no data

### 8. Delete Command (`test_delete_command`)
- ✅ Deletes a previously stored resource
- **What it tests**: Cleanup and deletion semantics

### 9. Nonexistent Command Error (`test_nonexistent_command`)
- ✅ Validates error handling for unknown commands
- **What it tests**: Error propagation and command not found scenarios

### 10. Activity ID Tracking (`test_activity_id_tracking`)
- ✅ Verifies activity ID preservation through execution
- **What it tests**: Request correlation and tracing

### 11. Multiple Resources List (`test_multiple_resources_list`)
- ✅ Creates 3 resources and lists them with filtering
- **What it tests**: Batch operations and list filtering

## Understanding the Runtime Pipeline

The tests demonstrate this execution pipeline:

```
1. Create CommandContext
   ├─ command_name: "set" (or "get", "list", "delete", etc.)
   ├─ parameters: {"resource": "...", "value": ...}
   ├─ metadata: {"namespace": "default"}
   ├─ activity_id: UUID (for request correlation)
   └─ cancellation_token: (for graceful shutdown)

2. Call runtime.execute(ctx)

3. CommandExecutor.dispatch(ctx)
   ├─ Look up handlers for command_name
   └─ Sort by priority:
      ├─ Mutator (priority 0)     - Change state
      ├─ Validator (priority 1)   - Validate state
      ├─ Internal (priority 2)    - Execute business logic
      └─ Observer (priority 4)    - Side effects (notify, etc.)

4. Execute each handler in priority order
   ├─ Mutators run first
   ├─ Validators run next
   ├─ One Internal command executes and returns result
   └─ Observers run last

5. Return result to caller
```

## Key Concepts

### CommandContext
Represents a single command invocation with:
- **command_name**: Which command to execute (e.g., "set", "get")
- **parameters**: Input data for the command
- **metadata**: Context information (namespace, user, etc.)
- **activity_id**: UUID for distributed tracing
- **cancellation_token**: For graceful shutdown

### CommandHandler Trait
All handlers implement `CommandHandler` with a `CommandType`:

```rust
pub trait CommandHandler: Send + Sync {
    fn get_type(&self) -> CommandType;
    fn as_validator(&self) -> Option<&dyn ValidationCommand> { ... }
    fn as_mutator(&self) -> Option<&dyn MutationCommand> { ... }
    fn as_executable(&self) -> Option<&dyn ExecutableCommand> { ... }
}
```

### Built-in Commands

| Command | Type | Purpose |
|---------|------|---------|
| `version` | Internal | Returns version info |
| `get` | Internal | Retrieve resource from store |
| `set` | Internal | Store/update resource |
| `delete` | Internal | Mark resource for deletion |
| `list` | Internal | List resources by prefix |
| `reconcile` | Internal | Background state reconciliation |

### Store Interface

Tests use `InMemoryStore`, but the runtime also supports `FileSystemStore`. Both implement `TransactionalKeyValueStore`:

```rust
pub trait TransactionalKeyValueStore: Send + Sync {
    async fn get(&self, container: &str, key: &str) -> StoreResult<StoreValue>;
    async fn put(&self, container: &str, key: &str, value: StoreValue) -> StoreResult<StoreValue>;
    async fn delete(&self, container: &str, key: &str) -> StoreResult<()>;
    async fn list_keys(&self, container: &str, key_prefix: Option<&str>) -> StoreResult<Vec<StoreKey>>;
    // ... transaction support, container operations, etc.
}
```

## Adding New Tests

To add a new test, follow this pattern:

```rust
async fn test_my_feature() -> TestResult {
    let start = std::time::Instant::now();
    
    // Setup
    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let runtime = KuiperRuntimeBuilder::new(store).build();
    
    // Execute
    let mut ctx = CommandContext {
        command_name: "my_command".to_string(),
        parameters: { /* ... */ },
        metadata: { /* ... */ },
        activity_id: Uuid::new_v4(),
        cancellation_token: CancellationToken::new(),
    };
    
    let result = runtime.execute(&mut ctx).await;
    
    // Verify
    let passed = result.is_ok(); // Your assertion
    
    TestResult::new(
        "test_my_feature",
        passed,
        "Description of what passed/failed",
        start.elapsed().as_millis(),
    )
}
```

Then add it to the `tests` vector in `main()`:

```rust
let tests: Vec<...> = vec![
    // ... existing tests ...
    ("My Feature", || Box::pin(test_my_feature())),
];
```

## Performance Notes

- Tests run in **memory** (not file system), so they're very fast (~0ms)
- InMemoryStore uses `std::sync::Mutex`, not async-aware locking
- No network I/O or external dependencies
- Safe for concurrent execution

## Architecture

```
integration-tests/
├── Cargo.toml          # Dependencies (kuiper-runtime, kuiper-runtime-sdk)
└── src/
    └── main.rs         # Complete test harness (standalone executable)
```

The crate is **intentionally standalone**:
- No dependencies on HTTP/web layers
- No dependencies on resource-server or kctl
- Can be run independently to validate core runtime behavior
- Serves as a reference implementation for using the runtime

## Troubleshooting

### Test fails with "Command handler not found"
- Make sure the command is registered in `KuiperRuntimeBuilder::new()`
- Check that the command name matches (case-sensitive)

### Test fails with "Missing required parameter"
- Verify all required parameters are in `CommandContext.parameters`
- Check the command implementation for parameter names

### Test fails with store errors
- Ensure the store container exists (some operations require initialization)
- Check that namespace metadata is set correctly

## Next Steps

- Add tests for custom handlers
- Add tests for handler priority ordering
- Add tests for transaction/commit semantics
- Add performance benchmarks
- Add tests for error recovery scenarios

## References

- **Core Runtime**: `kuiper-runtime/src/lib.rs`
- **Command Executor**: `kuiper-runtime/src/command/mod.rs`
- **Built-in Commands**: `kuiper-runtime/src/command/commands.rs`
- **SDK Interfaces**: `kuiper-runtime-sdk/src/command/mod.rs`
- **Store Interface**: `kuiper-runtime-sdk/src/data/mod.rs`
