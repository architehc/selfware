# Getting Started with Selfware

## Installation

Both methods below compile from source, which requires a few native packages.
On Debian/Ubuntu:

```bash
sudo apt-get update && sudo apt-get install -y \
  pkg-config libssl-dev cmake libdbus-1-dev \
  libxcb1-dev libxcb-randr0-dev libxcb-shm0-dev libxcb-composite0-dev
```

### From Git

```bash
cargo install --git https://github.com/architehc/selfware
```

### From source

```bash
git clone https://github.com/architehc/selfware
cd selfware
cargo install --path .
```

### Verify installation

```bash
selfware --version
```

### Run doctor first

Before doing anything else, run the system doctor to verify core dependencies are available:

```bash
selfware doctor
```

This checks core tools (rustc, cargo, git), optional language runtimes, browser automation, and computer control. Run `selfware llm-doctor` separately if you want backend/model diagnostics.

## First Run

Start an interactive chat session:

```bash
selfware chat
```

Or run a one-shot task:

```bash
selfware -p "Explain what this project does"
```

Or run a named task:

```bash
selfware run "Add error handling to src/main.rs"
```

Or answer a few structured interview questions (language, framework, testing strategy, etc.) while scaffolding a new project:

```bash
selfware init
```

## Configuration

Selfware looks for configuration in this order:

1. `--config <path>` CLI flag
2. `SELFWARE_CONFIG` environment variable
3. `selfware.toml` in the current directory
4. `~/.config/selfware/config.toml`

If no config file is found, built-in defaults are used: the hosted OpenRouter
endpoint (`https://openrouter.ai/api/v1`) with model `z-ai/glm-5.2`, which
requires an API key (see below). For a local backend, write a config file
pointing at it.

### Create your config

Run the setup wizard:

```bash
selfware init
```

Or create the file manually. Do not leave it empty — an empty file is valid
TOML, so selfware would silently start with the hosted OpenRouter defaults
above and no API key. Put real values in (see the backend examples below),
then restrict permissions:

```bash
mkdir -p ~/.config/selfware
$EDITOR ~/.config/selfware/config.toml   # see examples below
chmod 600 ~/.config/selfware/config.toml
```

### Local LLM with LM Studio

```toml
endpoint = "http://localhost:1234/v1"
model = "lmstudio-community/Qwen3-Coder-Next-FP8"
max_tokens = 65536
temperature = 1.0
```

### Local LLM with sglang

```toml
endpoint = "http://localhost:30000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 65536
temperature = 1.0

[extra_body]
chat_template_kwargs = { enable_thinking = false }
```

### Local LLM with vllm

```toml
endpoint = "http://localhost:8000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 65536
temperature = 1.0
```

### Local LLM with Ollama

```toml
endpoint = "http://localhost:11434/v1"
model = "qwen3:coder"
max_tokens = 65536
temperature = 1.0
```

### Cloud API (OpenAI-compatible)

```toml
endpoint = "https://api.openai.com/v1"
model = "gpt-4o"
max_tokens = 16384
temperature = 0.7
```

Set the API key securely (do not put it in the config file):

```bash
# Option 1: Environment variable (recommended)
export SELFWARE_API_KEY="sk-..."
```

Selfware also reads from the OS keyring if a key is already stored there, but this repo does not expose a dedicated command for writing to the keyring.

### Multiple model profiles

Define named profiles for different backends (e.g., a fast coder and a vision model):

```toml
endpoint = "http://localhost:8000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"

[models.coder]
endpoint = "http://localhost:8000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 65536
modalities = ["text"]

[models.vision]
endpoint = "http://localhost:8001/v1"
model = "Qwen/Qwen2.5-VL-72B"
max_tokens = 8192
modalities = ["text", "vision"]
context_length = 32768
```

## Running Doctor

Check that all dependencies are available:

```bash
selfware doctor
```

This checks:
- **Core** (required): `rustc`, `cargo`, `git`
- **Languages** (optional): `node`, `python`, `go`
- **Rust tools**: `clippy`, `rustfmt`, `tarpaulin`
- **Container tools**: `docker`, `docker-compose`
- **Browser automation**: `chromium`, `playwright`
- **Computer control**: `ImageMagick`, `ffmpeg`, screen capture

## Basic Commands

| Command | Alias | Description |
|---------|-------|-------------|
| `selfware chat` | `selfware c` | Interactive chat session |
| `selfware run "<task>"` | `selfware r "<task>"` | Run a specific task |
| `selfware analyze <path>` | `selfware a <path>` | Analyze a codebase |
| `selfware doctor` | | System diagnostics |
| `selfware llm-doctor` | | LLM backend diagnostics |
| `selfware init` | | First-time setup wizard |
| `selfware journal` | `selfware j` | Browse task history |
| `selfware status` | | Show agent statistics |
| `selfware lsp` | | Start LSP server mode |
| `selfware mcp-server` | | Run as MCP server (expose tools to other AI clients) |
| `selfware workflow validate <file>` | `selfware w validate <file>` | Validate a workflow file |
| `selfware workflow run <file>` | `selfware w run <file>` | Execute a workflow |
| `selfware workflow list` | `selfware w list` | List workflows in the current directory |

## CLI Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--prompt <text>` | `-p` | Headless mode: run prompt and exit |
| `--config <file>` | `-c` | Config file path |
| `--workdir <dir>` | `-C` | Change working directory |
| `--mode <mode>` | `-m` | Execution mode: `normal`, `auto-edit`, `yolo`, `daemon` |
| `--yolo` | `-y` | Shortcut for `--mode=yolo` |
| `--daemon` | | Shortcut for `--mode=daemon` |
| `--plan` | | Plan mode: propose tools without executing |
| `--compact` | | Compact output (less chrome) |
| `--verbose` | `-v` | Verbose tool output |
| `--show-tokens` | | Always display token usage |
| `--theme <name>` | | Color theme: `amber`, `ocean`, `minimal`, `high-contrast` |
| `--no-color` | | Disable colored output |
| `--ascii` | | ASCII-only output (no emoji) |
| `--resume-session <name>` | | Resume a saved chat session |

## Execution Modes

| Mode | Description |
|------|-------------|
| `normal` | Ask for confirmation before every tool execution (default) |
| `auto-edit` | Auto-approve file reads/writes, ask for shell commands |
| `yolo` | Auto-approve all tool executions |
| `daemon` | Permanent YOLO mode, runs autonomously |

## Next Steps

- [Configuration Reference](configuration.md) -- all config options
- [Tool Reference](tools.md) -- available tools
- [Interactive Commands](interactive-commands.md) -- slash commands
- [Hooks](hooks.md) -- event-driven automation
- [MCP Integration](mcp.md) -- external tool servers and MCP server mode
- [Doctor & Troubleshooting](doctor.md) -- system diagnostics and LLM doctor
