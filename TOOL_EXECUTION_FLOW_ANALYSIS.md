# Tool Execution Flow Analysis

This document traces the complete execution flow of tools in the Selfware system, from dispatch to safety validation to execution.

## Architecture Overview

The tool system has multiple entry points:
1. **Native Tools** - Direct Rust implementations via the `Tool` trait
2. **MCP Tools** - Remote tools via the MCP bridge
3. **Context Tools** - Special tools that operate on agent state

## Execution Flow

### 1. Tool Dispatch (`src/agent/tool_dispatch.rs`)

The primary dispatch mechanism is in `Agent::execute_tool_batch()`:

```
execute_tool_batch()
├── Phase 1: Partition tools
│   ├── Parallel-safe tools (read-only, no path conflicts)
│   └── Sequential tools (mutating, or with path conflicts)
│
├── Phase 2: Execute parallel-safe tools concurrently
│   └── execute_parallel_tools()
│       ├── Pre-validate all tools
│       ├── Build validated tool list
│       └── Execute concurrently using FuturesUnordered
│
└── Phase 3: Execute sequential tools one-by-one
    └── execute_single_tool_in_batch()
```

### 2. Safety Validation Point

**Location**: `src/agent/tool_dispatch.rs` → `execute_single_tool_in_batch()` and `execute_parallel_tools()`

**Validation Sequence** (for each tool):
```
1. parse_tool_args()         - Parse JSON arguments
2. safety.check_tool_call()  - ⚠️ SAFETY CHECK POINT
3. validate_tool_args()      - Validate against schema
4. maybe_block_redundant_reread() - Block repeated unchanged file reads
5. confirm_tool_execution()  - Interactive confirmation (if enabled)
6. PreToolUse hooks          - May skip execution
7. tool.execute(args)        - Actual execution
8. PostToolUse hooks         - Auto-format, lint, etc.
```

### 3. Safety Checker (`src/safety/checker/validation.rs`)

The `check_tool_call()` function dispatches on tool name and validates:

#### Tool-Specific Validations:

| Tool Category | Validation |
|--------------|------------|
| File ops (`file_write`, `file_edit`, `file_read`, `file_delete`) | Path validation + secret scanning |
| Shell ops (`shell_exec`, `container_exec`, `process_start`) | Shell command validation |
| Git ops (`git_push`) | Block force push |
| Container ops (`container_run`) | Command validation + volume mount checks |
| HTTP ops (`http_request`, `browser_*`, `vision_*`) | URL/SSRF validation |
| Package ops (`npm_install`, `pip_install`) | Shell command validation |
| Read-only ops (`git_status`, `grep_search`, etc.) | No validation (safe) |
| Context ops (`context_*`, `code_introspect`) | No validation (internal state only) |
| Unknown tools | **BLOCKED** - Must be registered |

#### Key Safety Checks:

1. **Path Validation** (`check_path()`):
   - Validates paths are within allowed directories
   - Uses `PathValidator` from `src/safety/path_validator.rs`

2. **Shell Command Validation** (`check_shell_command()`):
   - Blocks dangerous commands (rm -rf, dd, etc.)
   - Detects encoded command execution (base64, hex)
   - Validates command chaining
   - Blocks environment variable injection

3. **URL/SSRF Validation** (`check_url_ssrf_with_options()`):
   - Blocks file:// and other dangerous schemes
   - Blocks cloud metadata endpoints
   - Validates against private network access
   - DNS pinning with `PinnedDnsResolver`

4. **Secret Scanning** (`check_content_for_secrets()`):
   - Scans file content for hardcoded secrets
   - Uses `SecurityScanner` from `src/safety/scanner.rs`

### 4. Tool Execution (`src/tools/mod.rs`)

The `Tool` trait defines the execution interface:

```rust
#[async_trait]
pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    async fn execute(&self, args: Value) -> Result<Value>;
}
```

#### Executor Implementations:

1. **Native Executors** (`src/self_healing/executor.rs`, etc.):
   - Implement `Tool` trait directly
   - Execute via `tool.execute(args)`

2. **MCP Bridge** (`src/mcp/tool_bridge.rs`):
   - `McpTool` adapts MCP tools to `Tool` trait
   - Forwards to `McpClient::call_tool()`

3. **Context Tools** (`src/tools/context.rs`):
   - Intercepted in `execute_single_tool()`
   - Bypass normal tool registry
   - Operate on agent state only

### 5. Result Processing

After execution, results are processed:

```rust
// In execute_single_tool():
match execution {
    Ok(Ok(result)) => {
        // Success path
        - Cache result (if cacheable)
        - Display diff (for file mutations)
        - Visual verification (if enabled)
        - Record for learning
    }
    Ok(Err(e)) => {
        // Execution error
        - Record for learning
        - Build recovery hint
    }
    Err(_) => {
        // Timeout
        - Record timeout error
    }
}
```

## Safety Validation Integration Points

### Critical Integration: Safety Check Before Execution

The safety check happens **early** in the execution flow, before any actual work is done:

```
execute_single_tool_in_batch()
├── parse_tool_args()          - Parse JSON
├── safety.check_tool_call()   ⚠️ BLOCKS HERE if unsafe
├── validate_tool_args()       - Schema validation
├── ... additional checks ...
└── tool.execute(args)         - Only if all checks pass
```

### Error Handling

When safety check fails:
```rust
if let Err(e) = self.safety.check_tool_call(&fake_call) {
    let error_msg = format!("Safety check failed: {}", e);
    crate::output::safety_blocked(&error_msg);
    self.push_tool_result_message(..., &error_msg);
    self.record_failed_tool_attempt(&name, &args_str, "safety", &error_msg);
    // Returns early - execution blocked
}
```

## Parallel Execution Safety

The system supports concurrent execution of **parallel-safe** tools:

```rust
const PARALLEL_SAFE_TOOLS: &[&str] = &[
    "file_read", "directory_tree", "glob_find", "grep_search",
    "symbol_search", "git_status", "git_diff", "git_log",
    "lsp_document_symbols", "lsp_find_references",
    "lsp_goto_definition", "lsp_hover",
];
```

**Safety guarantees**:
- All parallel tools are pre-validated before concurrent execution
- Path conflicts detected and moved to sequential batch
- Each tool still goes through full safety check

## Key Files for Tool Execution

| File | Purpose |
|------|---------|
| `src/agent/tool_dispatch.rs` | Main dispatch logic, batch execution |
| `src/safety/checker/validation.rs` | Safety validation rules |
| `src/safety/checker/types.rs` | Safety checker types |
| `src/tools/mod.rs` | Tool trait definition |
| `src/tools/context.rs` | Context management tools |
| `src/mcp/tool_bridge.rs` | MCP tool adaptation |
| `src/self_healing/executor.rs` | Self-healing executor |
| `src/bench_harness/computer_control/executor.rs` | Computer control executor |
| `src/orchestration/parallel/executor.rs` | Parallel executor |

## Error Recovery Flow

When a tool fails:

```
Tool execution fails
├── classify_error_type()       - Categorize error
├── record_failed_tool_attempt() - Track for retry suppression
├── build_error_recovery_hint() - Generate guidance
└── Add recovery hint to messages
```

## Security Considerations

1. **Defense in Depth**: Multiple validation layers (safety, schema, path)
2. **Fail-Safe**: Unknown tools are blocked by default
3. **Audit Logging**: All tool calls logged for audit trail
4. **Secret Scanning**: Content scanned before file writes
5. **SSRF Protection**: URL validation with DNS pinning
6. **Container Safety**: Volume mount restrictions

## Testing

Key test locations:
- `src/agent/tool_dispatch.rs` - Tool error classification tests
- `src/safety/checker/validation.rs` - Safety validation tests
- `src/mcp/tool_bridge.rs` - MCP tool tests