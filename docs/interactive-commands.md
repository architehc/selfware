# Interactive Commands Reference

When running `selfware chat`, you enter interactive mode. Type slash commands to control the agent, or natural language to chat.

## Quick Reference

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/status` | Agent status (messages, memory, step count) |
| `/stats` | Detailed session statistics |
| `/mode` | Cycle execution mode (normal -> auto-edit -> yolo -> daemon) |
| `/tools` | List all registered tools |
| `/clear` | Clear conversation (keeps system prompt) |
| `/config` | Show current configuration |
| `/model` | Show model configuration |

## Context Management

| Command | Description |
|---------|-------------|
| `/ctx` | Show context window statistics |
| `/ctx clear` | Clear all loaded context |
| `/ctx load <glob>` | Load files into context (e.g., `/ctx load .rs`) |
| `/ctx reload` | Reload all currently loaded files |
| `/ctx copy` | Copy loaded source files to clipboard |
| `/compress` | Compress context to save tokens |

**Example:**
```
/ctx load .rs,.toml
/ctx
/compress
```

## Git Integration

| Command | Description |
|---------|-------------|
| `/git` | Git status (short format with branch) |
| `/diff` | Git diff --stat |
| `/undo` | Undo last file edit (restores from checkpoint) |

## Edit History

| Command | Description |
|---------|-------------|
| `/undo` | Undo the last file edit |
| `/restore` | List edit checkpoints with timestamps |
| `/restore <n>` | Restore checkpoint by index |

**Example:**
```
/restore
  [0] 14:32:01 - file_edit: src/main.rs
  [1] 14:31:45 - file_write: src/utils.rs
/restore 1
```

## Chat Sessions

| Command | Description |
|---------|-------------|
| `/chat save <name>` | Save current session to disk |
| `/chat resume <name>` | Resume a saved session |
| `/chat list` | List all saved sessions |
| `/chat delete <name>` | Delete a saved session |

Sessions persist across restarts. You can also resume from the CLI:
```bash
selfware --resume-session my-session
```

## Work Queue

Queue messages to be processed after the current task completes. The queue supports delayed execution and priority levels.

| Command | Alias | Description |
|---------|-------|-------------|
| `/queue` | `/q` | Show all queued items |
| `/queue clear` | `/q clear` | Clear all queued items |
| `/queue next` | `/q next` | Peek at the next item |

### Delayed Execution

Prefix a message with `@<duration>` to delay its execution:

```
@5m run tests
@30s check the build
@2h deploy to staging
```

Supported units: `s` (seconds), `m` (minutes), `h` (hours). Delayed items become eligible for execution after the specified duration elapses.

### Priority Queue

Prefix a message with `!` to mark it as high-priority. High-priority items are processed before normal items:

```
!fix this compilation error
!run the failing test
```

## Code Analysis

| Command | Description |
|---------|-------------|
| `/analyze <path>` | Analyze a codebase or directory |
| `/review <file>` | Review a specific code file |
| `/plan <task>` | Create a task plan (reasoning only) |
| `/swarm <task>` | Run task with multi-agent swarm |
| `/interview` | Start interview mode -- answer structured questions (language, framework, testing strategy, project type) before the agent begins a task |

## Mode & Display Controls

| Command | Description |
|---------|-------------|
| `/mode` | Cycle execution mode |
| `/compact` | Toggle compact output mode |
| `/verbose` | Toggle verbose output mode |
| `/plan` | Toggle plan mode (propose tools without executing) |
| `/think` | Toggle thinking mode (enable/disable model reasoning) |
| `/think on` | Enable thinking |
| `/think off` | Disable thinking (faster responses) |
| `/theme` | List available color themes |
| `/theme <name>` | Switch to a theme (`amber`, `ocean`, `minimal`, `high-contrast`) |
| `/vim` | Toggle vi/emacs input mode |

## Token & Cost Tracking

| Command | Description |
|---------|-------------|
| `/cost` | Show token usage and estimated cost |
| `/memory` | Show memory system statistics |
| `/last` | Show last tool output (name, duration, status, output) |

## Hooks

| Command | Description |
|---------|-------------|
| `/hooks` | Show registered hooks and their count |

## Clipboard & Output

| Command | Description |
|---------|-------------|
| `/copy` | Copy last assistant response to clipboard |

## File References

Use `@path/to/file` in your message to include file content:

```
@src/main.rs refactor the error handling in this file
```

Multiple files:
```
@Cargo.toml @src/lib.rs update the version and re-export the new module
```

## Shell Escape

Run shell commands directly with `!`:

```
!ls -la
!cargo test
!git log --oneline -5
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `ESC` | Interrupt running task |
| `Ctrl+C` | Interrupt running task |
| `Ctrl+C` x2 | Exit (double-tap at prompt) |
| `Ctrl+J` | Insert newline (multi-line input) |
| `Ctrl+Y` | Toggle YOLO mode |
| `Shift+Tab` | Toggle Auto-Edit mode |
| `Ctrl+X` | Open external editor (`$EDITOR`) |
| `Ctrl+L` | Clear screen |
| `Ctrl+R` | Reverse history search |
| `Tab` | Autocomplete / cycle suggestions |

## Exiting

Type any of:
- `exit`
- `quit`
- `/exit`
- `/quit`
- `Ctrl+C` twice (double-tap)
