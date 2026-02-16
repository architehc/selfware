# Selfware

[![CI](https://github.com/architehc/selfware/actions/workflows/ci.yml/badge.svg)](https://github.com/architehc/selfware/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/selfware)](https://crates.io/crates/selfware)
[![Docs.rs](https://docs.rs/selfware/badge.svg)](https://docs.rs/selfware)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![codecov](https://codecov.io/gh/architehc/selfware/branch/main/graph/badge.svg)](https://codecov.io/gh/architehc/selfware)

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

[continuous_work]
enabled = true
checkpoint_interval_tools = 10
checkpoint_interval_secs = 300
auto_recovery = true
max_recovery_attempts = 3

[retry]
max_retries = 5
base_delay_ms = 1000
max_delay_ms = 60000
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

- **54 Built-in Tools**: File tending, git cultivation, cargo crafting, code foraging
- **Multi-Agent Swarm**: Up to 16 concurrent agents with role specialization
- **Multi-layer Safety**: Path guardians, command sentinels, protected groves
- **Task Persistence**: Checkpoint seeds survive frost (crashes)
- **Self-Healing Recovery**: Error classification, exponential backoff with jitter, automatic escalation
- **Cognitive Architecture**: PDVR cycle with working memory
- **Selfware UI**: Warm amber tones, animated spinners, ASCII art banners
- **Multi-Model Support**: Works with Qwen3-Coder, Kimi K2.5, DeepSeek, and other local LLMs
- **Robust Tool Parser**: Handles multiple XML formats from different models
- **E2E Test Harness**: 7-scenario automated benchmark suite with scoring and screenshots
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
# All tests (~3,980 tests, ~2 min)
cargo test --all-features

# With resilience features (self-healing, recovery)
cargo test --features resilience

# Integration tests with real LLM
cargo test --features integration

# Specific test modules
cargo test --test unit            # 238 unit tests
cargo test --test e2e_tools_test  # 21 E2E tool tests
```

### Test Coverage

```bash
cargo tarpaulin --all-features --out Html
```

| Metric | Value |
|--------|-------|
| **Total Tests** | ~3,980 |
| **Test Targets** | lib (3,615) + unit (238) + e2e (21) + integration (5) + property (100) + doc (1) |

### E2E System Tests

A full E2E benchmark suite tests the agent against broken Rust projects using a local LLM:

```bash
# Run all 7 scenarios (requires local model at localhost:8000)
./system_tests/projecte2e/run_projecte2e.sh
```

| Difficulty | Scenario | What it tests |
|------------|----------|---------------|
| Easy | Calculator, String ops | Simple bug fixes (3-4 bugs each) |
| Medium | JSON merge, BitSet | Logic bugs requiring analysis |
| Hard | Scheduler, Event bus | Multi-file bugs across modules |
| Swarm | Multi-chat session | Agent spawning and coordination |

Produces scored reports (0-100), ANSI terminal recordings, and error analysis under `system_tests/projecte2e/reports/`.

### Project Structure

```
src/
├── agent/          # Core agent logic, checkpointing, execution
├── tools/          # 54 tool implementations
├── api/            # LLM client with timeout and retry
├── ui/             # Selfware aesthetic (style, animations, banners)
├── analysis/       # Code analysis, BM25 search, vector store
├── cognitive/      # PDVR cycle, working/episodic memory
├── config/         # Configuration management
├── devops/         # Container support, process manager
├── observability/  # Telemetry and tracing
├── orchestration/  # Multi-agent swarm, planning, workflows
├── safety/         # Path validation, sandboxing, threat modeling
├── self_healing/   # Error classification, recovery executor, backoff
├── session/        # Checkpoint persistence
├── testing/        # Verification, contract testing, workflow DSL
├── memory.rs       # Memory management
├── tool_parser.rs  # Robust multi-format XML parser
└── token_count.rs  # Token estimation
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

- Built for [Kimi K2.5](https://kimi.moonshot.cn/), [Qwen3-Coder](https://qwenlm.github.io/), and other local LLMs
- Inspired by the [AiSocratic](https://aisocratic.org/) movement
- UI philosophy: software should feel like a warm workshop, not a cold datacenter

---

```
    "Tend your garden. The code will grow."
                                    — selfware proverb
```
