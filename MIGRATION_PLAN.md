# anyhow::bail! to Typed Errors Migration Plan

## Summary
Found **146 instances** of `anyhow::bail!` across the codebase that should be converted to typed errors.

## Existing Error Hierarchy
The codebase already has a comprehensive error structure in `src/errors.rs`:
- `SelfwareError` - Central error enum
- `AgentError` - Agent-specific errors
- `ApiError` - API errors
- `ToolError` - Tool execution errors
- `SafetyError` - Safety policy violations
- `SessionError` - Session/checkpoint errors
- `ResourceError` - Resource exhaustion errors

## Migration Strategy

### Priority 1: Tool Errors (src/tools/*.rs)
Most `anyhow::bail!` calls in tools should use `ToolError::Execution` or `ToolError::InvalidArguments`.

**Files to migrate:**
- `src/tools/file.rs` - ~10 instances (file operations)
- `src/tools/git.rs` - ~5 instances (git operations)
- `src/tools/shell.rs` - ~10 instances (shell execution)
- `src/tools/container/tools.rs` - ~10 instances (container operations)
- `src/tools/browser.rs` - ~5 instances (browser automation)
- `src/tools/process.rs` - ~10 instances (process management)
- `src/tools/knowledge.rs` - ~3 instances (knowledge graph)
- `src/tools/vision.rs` - ~10 instances (vision API)
- `src/tools/http.rs` - ~1 instance (HTTP requests)
- `src/tools/screen_capture.rs` - ~3 instances (screen capture)
- `src/tools/hot_reload.rs` - ~5 instances (plugin loading)
- `src/tools/introspect/*.rs` - ~3 instances (code introspection)
- `src/tools/package.rs` - ~2 instances (package management)
- `src/tools/cargo.rs` - ~10 instances (cargo commands)
- `src/tools/net_policy.rs` - ~3 instances (network policy)
- `src/tools/computer.rs` - ~5 instances (computer control)
- `src/tools/search.rs` - ~1 instance (search)
- `src/tools/mod.rs` - ~2 instances (tool validation)

### Priority 2: Safety Errors (src/safety/*.rs)
Safety violations should use `SafetyError` variants.

**Files to migrate:**
- `src/safety/path_validator.rs` - ~12 instances (path validation)
- `src/safety/checker/validation.rs` - ~15 instances (command blocking)

### Priority 3: Process Manager Errors
The process manager needs its own error type or should use `ToolError`.

**Files to migrate:**
- `src/devops/process_manager.rs` - ~7 instances (process lifecycle)

### Priority 4: Other System Errors
- `src/computer/window.rs` - ~12 instances (window management)
- `src/cli.rs` - ~5 instances (CLI parsing)
- `src/agent/*.rs` - ~5 instances (agent logic)
- `src/session/*.rs` - ~3 instances (session management)
- `src/tools/*.rs` (misc) - remaining instances

## Conversion Examples

### Example 1: Tool Invalid Arguments
```rust
// Before
anyhow::bail!("Invalid arguments for tool '{}': {}", name, message);

// After
return Err(ToolError::InvalidArguments {
    name: name.to_string(),
    message: message.to_string(),
}.into());
```

### Example 2: Safety Blocked Path
```rust
// Before
anyhow::bail!("Path not in allowed list: {}", canonical_str);

// After
return Err(SafetyError::BlockedPath {
    path: canonical_str.to_string(),
}.into());
```

### Example 3: Tool Execution Failure
```rust
// Before
anyhow::bail!("Process '{}' started but health check failed", id);

// After
return Err(ToolError::Execution {
    name: "process_start".to_string(),
    message: format!("Process '{}' health check failed", id),
}.into());
```

## Implementation Steps

1. **Create typed error variants** if needed (check existing `ToolError`, `SafetyError`)
2. **Replace `anyhow::bail!`** with `return Err(TypedError::Variant.into())`
3. **Update imports** to include the error types
4. **Verify compilation** with `cargo check`
5. **Run tests** to ensure behavior is preserved

## Notes

- Some `anyhow::bail!` calls are in error paths that are already well-structured
- Focus on validation errors first (they're most common)
- Keep error messages informative for debugging
- Use `#[from]` attribute for automatic conversion where appropriate