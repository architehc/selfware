# Selfware

```
    🦊 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
       Your Personal AI Workshop
       Software you own. Software that knows you. Software that lasts.
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

An artisanal agentic harness for local LLMs (Kimi K2.5, Qwen, etc.) that enables autonomous coding with safety guardrails, task persistence, and a warm terminal aesthetic.

## Philosophy

This is **selfware** — software crafted for your local workshop, not rented from the cloud. Like a well-worn tool that fits your hand perfectly:

- **Runs locally** on your hardware, your data stays yours
- **Remembers your patterns** across sessions
- **Grows with your garden** — your codebase is a living thing

## Installation

### Option 1: Download Prebuilt Binary (Recommended)

Download the latest release for your platform:

| Platform | Architecture | Download |
|----------|--------------|----------|
| **Linux** | x86_64 (Intel/AMD) | [selfware-linux-x86_64.tar.gz](https://github.com/architehc/selfware/releases/latest) |
| **Linux** | aarch64 (ARM64) | [selfware-linux-aarch64.tar.gz](https://github.com/architehc/selfware/releases/latest) |
| **macOS** | Apple Silicon (M1/M2/M3) | [selfware-macos-aarch64.tar.gz](https://github.com/architehc/selfware/releases/latest) |
| **macOS** | Intel | [selfware-macos-x86_64.tar.gz](https://github.com/architehc/selfware/releases/latest) |
| **Windows** | x86_64 | [selfware-windows-x86_64.zip](https://github.com/architehc/selfware/releases/latest) |

```bash
# Linux/macOS quick install
# Translates platform names: Darwin->macos, arm64->aarch64
OS=$(uname -s | tr '[:upper:]' '[:lower:]' | sed 's/darwin/macos/')
ARCH=$(uname -m | sed 's/arm64/aarch64/')
curl -fsSL "https://github.com/architehc/selfware/releases/latest/download/selfware-${OS}-${ARCH}.tar.gz" | tar -xz
sudo mv selfware /usr/local/bin/

# Verify installation
selfware --help
```

### Option 2: Install via Cargo

```bash
cargo install selfware
```

### Option 3: Build from Source

```bash
git clone https://github.com/architehc/selfware.git
cd selfware
cargo build --release
./target/release/selfware --help
```

### Option 4: Docker

```bash
# Build the image
docker build -t selfware .

# Run interactively
docker run --rm -it -v $(pwd):/workspace selfware chat

# Run a specific task
docker run --rm -it -v $(pwd):/workspace selfware run "Add unit tests"
```

## Quick Start

### 1. Set Up Your LLM Backend

Selfware works with any OpenAI-compatible API. Popular options:

| Backend | Best For | Setup |
|---------|----------|-------|
| **[vLLM](https://docs.vllm.ai/)** | Fast inference, production | `vllm serve Qwen/Qwen3-Coder-Next-FP8` |
| **[Ollama](https://ollama.ai/)** | Easy setup, consumer hardware | `ollama run qwen2.5-coder` |
| **[llama.cpp](https://github.com/ggerganov/llama.cpp)** | Minimal dependencies | `./server -m model.gguf` |
| **[LM Studio](https://lmstudio.ai/)** | GUI, Windows/Mac | Download and run |

### 2. Create Configuration

Create `selfware.toml` in your project directory:

```toml
# Your local workshop
endpoint = "http://localhost:8000/v1"  # Your LLM backend
model = "Qwen/Qwen3-Coder-Next-FP8"    # Model name
max_tokens = 65536
temperature = 0.7

[safety]
allowed_paths = ["./**", "/home/*/projects/**"]
denied_paths = ["**/.env", "**/secrets/**"]
protected_branches = ["main"]

[agent]
max_iterations = 100
step_timeout_secs = 600     # 10 min for fast models
token_budget = 500000
```

### 3. Start Coding

```bash
# Interactive chat mode
selfware chat

# Run a specific task
selfware run "Add unit tests for the authentication module"

# Multi-agent collaboration (16 concurrent agents)
selfware multi-chat

# Analyze your codebase
selfware analyze ./src
```

## The Digital Garden

Your codebase is visualized as a **digital garden**:

```
╭─ 🌱 Your Digital Garden ─────────────────────────────────────────╮
│                                                                   │
│  src/          ████████████████░░░░  82% healthy                 │
│    🌳 mod.rs        [THRIVING]  last tended 2h ago               │
│    🌿 agent.rs      [GROWING]   needs water                      │
│    🌱 tools.rs      [SEEDLING]  freshly planted                  │
│                                                                   │
│  Season: WINTER  ❄️   Growth rate: steady                        │
╰───────────────────────────────────────────────────────────────────╯
```

Files are **plants**, directories are **beds**, and your tools are **craftsman implements**.

## Features

- **53 Built-in Tools**: File tending, git cultivation, cargo crafting, code foraging
- **Multi-Agent Swarm**: Up to 16 concurrent agents with role specialization
- **Multi-layer Safety**: Path guardians, command sentinels, protected groves
- **Task Persistence**: Checkpoint seeds survive frost (crashes)
- **Cognitive Architecture**: PDVR cycle with working memory
- **Selfware UI**: Warm amber tones, animated spinners, ASCII art banners
- **Multi-Model Support**: Works with Qwen3-Coder, Kimi K2.5, DeepSeek, and other local LLMs
- **Robust Tool Parser**: Handles multiple XML formats from different models
- **4-Hour Patience**: Tolerant of slow local models (0.1 tok/s supported)

## Environment Variables

Configure Selfware via environment variables (override config file):

| Variable | Description | Default |
|----------|-------------|---------|
| `SELFWARE_ENDPOINT` | LLM API endpoint | `http://localhost:8000/v1` |
| `SELFWARE_MODEL` | Model name | `Qwen/Qwen3-Coder-Next-FP8` |
| `SELFWARE_API_KEY` | API key (if required) | None |
| `SELFWARE_MAX_TOKENS` | Max tokens per response | `65536` |
| `SELFWARE_TEMPERATURE` | Sampling temperature | `0.7` |
| `SELFWARE_TIMEOUT` | Request timeout (seconds) | `600` |
| `SELFWARE_DEBUG` | Enable debug logging | Disabled |

## The Selfware Palette

The UI uses warm, organic colors inspired by aged paper, wood grain, and amber resin:

| Color | Hex | Use |
|-------|-----|-----|
| 🟠 Amber | `#D4A373` | Primary actions, warmth |
| 🟢 Garden Green | `#606C38` | Growth, success, health |
| 🟤 Soil Brown | `#BC6C25` | Warnings, needs attention |
| ⬛ Ink | `#283618` | Deep text, emphasis |
| 🟡 Parchment | `#FEFAE0` | Light backgrounds |

### Status Messages

Instead of cold red/green/yellow:

- **BLOOM** 🌸 — Success, fresh growth
- **WILT** 🥀 — Warning, needs attention
- **FROST** ❄️ — Error, needs warmth

## Tools Reference

### Garden Tending (Files)

| Tool | Metaphor | Description |
|------|----------|-------------|
| `file_read` | 🔍 Examine | Read file contents |
| `file_write` | ✍️ Inscribe | Create or overwrite |
| `file_edit` | 🔧 Mend | Search and replace |
| `directory_tree` | 🗺️ Survey | List structure |

### Cultivation (Git)

| Tool | Metaphor | Description |
|------|----------|-------------|
| `git_status` | 📋 Assess | Working tree status |
| `git_diff` | 🔬 Compare | Show changes |
| `git_commit` | 📦 Preserve | Create a commit |
| `git_checkpoint` | 🏷️ Mark | Create checkpoint |

### Workshop (Cargo)

| Tool | Metaphor | Description |
|------|----------|-------------|
| `cargo_test` | 🧪 Verify | Run tests |
| `cargo_check` | ✓ Validate | Type check |
| `cargo_clippy` | 🧹 Polish | Run lints |
| `cargo_fmt` | 📐 Align | Format code |

### Foraging (Search)

| Tool | Metaphor | Description |
|------|----------|-------------|
| `grep_search` | 🔎 Hunt | Regex search |
| `glob_find` | 🧭 Locate | Find by pattern |
| `symbol_search` | 📍 Pinpoint | Find definitions |

## Slow Model Support

Designed for local LLMs running on consumer hardware:

```
Model Speed          Timeout Setting
─────────────────────────────────────
> 10 tok/s           300s (5 min)
1-10 tok/s           3600s (1 hour)
< 1 tok/s            14400s (4 hours)
0.08 tok/s           Works! Be patient.
```

The agent will wait. Good things take time.

## Task Persistence

Tasks are automatically checkpointed — your work survives crashes:

```bash
# Start a long task
selfware run "Refactor authentication system"

# Power outage? System crash? No problem.
selfware journal

# Resume exactly where you left off
selfware resume <task-id>
```

## Cognitive Architecture

The agent thinks in cycles:

```
    ╭─────────╮         ╭─────────╮
    │  PLAN   │────────▶│   DO    │
    ╰─────────╯         ╰─────────╯
         ▲                    │
         │                    ▼
    ╭─────────╮         ╭─────────╮
    │ REFLECT │◀────────│ VERIFY  │
    ╰─────────╯         ╰─────────╯
```

**Working Memory** tracks:
- Current plan and progress
- Active hypothesis
- Open questions
- Discovered facts

**Episodic Memory** learns:
- What approaches worked
- Your preferences
- Project patterns

## Development

### Run Tests

```bash
# Unit tests (6,700+ tests, ~2 min)
cargo test

# Integration tests with real LLM
cargo test --features integration

# Extended E2E tests (multi-hour sessions)
SELFWARE_TIMEOUT=7200 cargo test --features integration extended_

# Deep tests for slow models (4 hour timeout)
cargo test --features integration deep_
```

### Test Coverage

```bash
cargo tarpaulin --out Html
```

| Metric | Value |
|--------|-------|
| **Total Tests** | 6,771 |
| **Line Coverage** | ~77% |
| **New Module Coverage** | 92-95% |

Key coverage areas:
- `ui/animations.rs` — 92.8% (47 tests)
- `ui/banners.rs` — 95.3% (38 tests)
- `tool_parser.rs` — 94% (43 tests)
- `multiagent.rs` — 85% (27 tests)

### E2E Testing

The agent can create projects of varying complexity:

| Complexity | Example | Duration |
|------------|---------|----------|
| Simple | Hello World program | 3-5s |
| Medium | Library with tests | 30-60s |
| Complex | Multi-module CLI app | 2-5min |

```bash
# Run E2E test in isolated directory
./target/release/selfware -C /tmp/test-project run "Create a Rust library"
```

### Extended Test Configuration

For multi-hour test sessions, use `selfware-extended-test.toml`:

```toml
[agent]
max_iterations = 500
step_timeout_secs = 1800    # 30 min per step
token_budget = 500000

[extended_test]
max_duration_hours = 4
checkpoint_interval_mins = 15
max_concurrent_agents = 16
```

### Project Structure

```
src/
├── agent/          # Core agent logic
├── tools/          # 53 tool implementations
├── api/            # LLM client (4hr timeout)
├── ui/             # Selfware aesthetic
│   ├── style.rs    # Warm organic palette
│   ├── animations.rs # Animated spinners, progress bars
│   ├── banners.rs  # ASCII art banners
│   └── components.rs # Workshop UI elements
├── multiagent.rs   # Multi-agent swarm (16 concurrent)
├── tool_parser.rs  # Robust multi-format parser
├── checkpoint.rs   # Task persistence
├── cognitive.rs    # PDVR cycle, memory
└── safety.rs       # Path guardians
```

### Multi-Agent System

The agent supports up to 16 concurrent specialists:

```bash
# Launch multi-agent chat
./target/release/selfware multi-chat

# Roles: Architect, Coder, Tester, Reviewer, DevOps, Security
```

## Troubleshooting

### "Connection refused"
```bash
# Is your LLM backend running?
curl http://localhost:8000/v1/models
```

### "Request timeout"
```bash
# Increase timeout for slow models
# In selfware.toml:
[agent]
step_timeout_secs = 14400  # 4 hours
```

### "Safety check failed"
```bash
# Check allowed_paths in config
# The agent only accesses paths you permit
```

## License

MIT License

## Acknowledgments

- Built for [Kimi K2.5](https://kimi.moonshot.cn/), [Qwen](https://qwenlm.github.io/), and other local LLMs
- Inspired by the [AiSocratic](https://aisocratic.org/) movement
- UI philosophy: software should feel like a warm workshop, not a cold datacenter

---

```
    "Tend your garden. The code will grow."
                                    — selfware proverb
```
