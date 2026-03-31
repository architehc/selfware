# Tool Permission System

The integrated permission system provides granular safety metadata for tools to enable automatic permission classification. This is critical for safe autonomous operation and supports the `--auto` and `--plan` execution modes.

## Overview

The permission system follows the principle that **tools declare their own safety properties**, and a centralized permission checker uses those declarations along with the execution mode to make permission decisions.

## Components

### 1. Tool Metadata (`src/safety/tool_metadata.rs`)

#### `RiskLevel`
- `Low`: Safe read-only operations (file reads, searches)
- `Medium`: Potentially modifying but generally safe (file writes, git operations)
- `High`: Dangerous operations that can cause data loss (file deletions, shell commands)

#### `ExecutionMode`
- `Normal`: Prompt for medium and high risk operations (default)
- `Plan`: Only allow read-only operations
- `Auto`: Auto-approve low and medium risk, prompt for high
- `Yolo`: Auto-approve all operations (with audit logging)

#### `ToolMetadata`
```rust
pub struct ToolMetadata {
    pub read_only: bool,        // Tool only reads data
    pub destructive: bool,      // Tool can cause data loss
    pub risk_level: RiskLevel,  // Low/Medium/High
    pub network_access: bool,   // Tool accesses network
    pub shell_execution: bool,  // Tool executes shell commands
}
```

#### `PermissionChecker`
The central permission checker that determines if a tool operation should be allowed:

```rust
let checker = PermissionChecker::new(ExecutionMode::Auto);
match checker.check(tool_name, &metadata, &args) {
    PermissionResult::Allow => /* execute tool */,
    PermissionResult::Deny { reason } => /* block tool */,
    PermissionResult::Prompt { reason } => /* prompt user */,
}
```

### 2. Extended Tool Trait (`src/tools/mod.rs`)

The `Tool` trait has been extended with safety metadata methods:

```rust
pub trait Tool: Send + Sync {
    // ... existing methods ...
    
    /// Returns true if this tool only reads data
    fn is_readonly(&self) -> bool;
    
    /// Returns true if this tool can cause data loss
    fn is_destructive(&self) -> bool;
    
    /// Returns the risk level for this tool
    fn risk_level(&self) -> RiskLevel;
    
    /// Returns the full safety metadata
    fn metadata(&self) -> ToolMetadata;
}
```

Default implementations are provided that look up metadata by tool name. Individual tools can override `metadata()` to provide custom safety information.

### 3. Tool Implementations

Key tools have explicit metadata implementations:

| Tool | Read-Only | Destructive | Risk Level |
|------|-----------|-------------|------------|
| `file_read` | ✓ | ✗ | Low |
| `directory_tree` | ✓ | ✗ | Low |
| `file_write` | ✗ | ✗ | Medium |
| `file_edit` | ✗ | ✗ | Medium |
| `file_delete` | ✗ | ✓ | High |
| `shell_exec` | ✗ | ✓ | High |
| `grep_search` | ✓ | ✗ | Low |
| `git_status` | ✓ | ✗ | Low |
| `git_diff` | ✓ | ✗ | Low |
| `git_commit` | ✗ | ✗ | Medium |
| `git_push` | ✗ | ✗ | High |

### 4. Permission Check Integration

Permission checks are integrated into the tool execution flow in `src/agent/tool_execution.rs`:

```rust
async fn execute_single_tool(...) -> Result<()> {
    // ... hook check ...
    
    // Permission check based on execution mode and tool metadata
    let permission_result = self.check_tool_permission(name, tool, &args).await;
    match permission_result {
        Ok(true) => { /* continue with execution */ }
        Ok(false) => { /* permission denied, skip tool */ }
        Err(e) => { return Err(e); }
    }
    
    // ... tool execution ...
}
```

## Usage

### Execution Modes

#### Normal Mode (Default)
```bash
selfware "task description"
```
Prompts for medium and high risk operations.

#### Plan Mode
```bash
selfware --plan "task description"
```
Only allows read-only operations. Proposed modifications are shown but not executed.

#### Auto Mode
```bash
selfware --auto "task description"
```
Auto-approves low and medium risk operations, prompts for high risk.

#### YOLO Mode
```bash
selfware --yolo "task description"
```
Auto-approves all operations. Use with caution.

### Programmatic Usage

```rust
use selfware::safety::{PermissionChecker, ExecutionMode, ToolMetadata};

// Create a permission checker
let checker = PermissionChecker::new(ExecutionMode::Auto)
    .with_destructive_in_yolo(false)  // Require confirmation for destructive ops
    .with_network(true);              // Allow network operations

// Check permission for a tool
let metadata = ToolMetadata::file_write();
let args = serde_json::json!({"path": "/tmp/test.txt", "content": "hello"});

match checker.check("file_write", &metadata, &args) {
    PermissionResult::Allow => println!("Tool execution allowed"),
    PermissionResult::Deny { reason } => println!("Denied: {}", reason),
    PermissionResult::Prompt { reason } => println!("Please confirm: {}", reason),
}
```

## Adding Metadata to New Tools

To add safety metadata to a new tool, implement the `metadata()` method:

```rust
use selfware::safety::ToolMetadata;

#[async_trait]
impl Tool for MyNewTool {
    fn name(&self) -> &str { "my_new_tool" }
    fn description(&self) -> &str { "Does something useful" }
    fn schema(&self) -> Value { /* ... */ }
    
    async fn execute(&self, args: Value) -> Result<Value> { /* ... */ }
    
    fn metadata(&self) -> ToolMetadata {
        // Use a pre-defined metadata template
        ToolMetadata::read_only()  // For read-only tools
        // or
        ToolMetadata::file_write()  // For file write tools
        // or
        ToolMetadata::shell()  // For shell execution tools
        // or create custom metadata
        ToolMetadata::custom(
            /* read_only */ false,
            /* destructive */ false,
            /* risk_level */ RiskLevel::Medium,
            /* network_access */ true,
            /* shell_execution */ false,
        )
    }
}
```

## Future Enhancements

- **Interactive Prompting**: Implement user prompts for `PermissionResult::Prompt`
- **Fine-grained Permissions**: Add resource-level permissions (e.g., allow writes only to specific directories)
- **Permission Grants**: Extend the permission store for pre-authorized operations
- **Audit Logging**: Log all permission decisions for security review
