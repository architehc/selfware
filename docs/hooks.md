# Hooks System Guide

Hooks let you run custom shell commands at key points in the agent lifecycle. Use them for auto-formatting, linting, testing, auto-committing, and other automation.

## Events

| Event | When it fires | Use case |
|-------|--------------|----------|
| `PreToolUse` | Before a tool is executed | Block dangerous operations, validate inputs |
| `PostToolUse` | After a tool completes successfully | Auto-format, lint, auto-commit |
| `Stop` | When the agent finishes a task | Run tests, final validation |

## Configuration

Hooks are defined in `selfware.toml` as `[[hooks]]` array entries:

```toml
[[hooks]]
event = "PostToolUse"           # Required: PreToolUse, PostToolUse, or Stop
command = "cargo fmt -- {path}" # Required: shell command to run
match_tools = ["file_write", "file_edit"]  # Optional: only trigger for these tools
timeout_secs = 30               # Optional: timeout (default: 30)
```

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `event` | string | yes | -- | `"PreToolUse"`, `"PostToolUse"`, or `"Stop"` |
| `command` | string | yes | -- | Shell command to execute |
| `match_tools` | array | no | `[]` (all) | Only trigger for these tool names |
| `timeout_secs` | number | no | `30` | Max seconds before the hook is killed |

### Placeholders

The `command` string supports these placeholders:

| Placeholder | Replaced with |
|-------------|---------------|
| `{path}` | Affected file path (if the tool has a `path` argument) |
| `{tool}` | Tool name (e.g., `"file_write"`) |

## Hook Behavior

- **PostToolUse hooks**: Run after the tool succeeds. Errors are logged but do not block the agent.
- **PreToolUse hooks**: If the command exits non-zero, the tool execution is **skipped** (the hook can block operations).
- **Stop hooks**: Run once when the agent completes its task.
- Multiple hooks can fire for the same event. They run in order.
- Hook stdout/stderr is captured (max 64KB). Output is logged at debug level.

## Built-in Hook Presets

Selfware includes built-in hook factories for common workflows. These are available programmatically but can be replicated in config:

### Format on Save (Rust)

```toml
[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "cargo fmt -- {path}"
timeout_secs = 30
```

### Lint After Edit (Rust)

```toml
[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "cargo clippy --quiet -- -D warnings 2>&1 | head -20"
timeout_secs = 60
```

### Test on Stop (Rust)

```toml
[[hooks]]
event = "Stop"
command = "cargo test --quiet 2>&1 | tail -5"
timeout_secs = 120
```

### Auto-Commit

```toml
[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "git add {path} && git commit -m \"[selfware] auto-commit: {tool} {path}\" --no-verify 2>/dev/null || true"
timeout_secs = 15
```

### Format on Save (Python)

```toml
[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "ruff format {path} 2>/dev/null || black {path} 2>/dev/null || true"
timeout_secs = 15
```

### Format on Save (Node.js)

```toml
[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "npx prettier --write {path} 2>/dev/null || true"
timeout_secs = 15
```

## Examples

### Full Rust Development Setup

```toml
# Auto-format every file write
[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "cargo fmt -- {path}"
timeout_secs = 30

# Run clippy after edits
[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "cargo clippy --quiet -- -D warnings 2>&1 | head -20"
timeout_secs = 60

# Run tests when task completes
[[hooks]]
event = "Stop"
command = "cargo test --quiet 2>&1 | tail -10"
timeout_secs = 120
```

### Block Dangerous Operations

Use a PreToolUse hook to prevent writes to production configs:

```toml
[[hooks]]
event = "PreToolUse"
match_tools = ["file_write", "file_edit"]
command = "echo {path} | grep -q 'production\\|prod\\.toml' && exit 1 || exit 0"
timeout_secs = 5
```

If the command exits non-zero, the tool execution is skipped.

### Notification on Task Completion

```toml
[[hooks]]
event = "Stop"
command = "osascript -e 'display notification \"Task complete\" with title \"Selfware\"' 2>/dev/null || true"
timeout_secs = 5
```

## Viewing Hooks at Runtime

In interactive mode:

```
/hooks
```

This shows the number of registered hooks. Hook events are logged at debug level -- use `SELFWARE_LOG_LEVEL=debug` to see detailed hook execution output.
