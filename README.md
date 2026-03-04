# Selfware

[![CI](https://github.com/architehc/selfware/actions/workflows/ci.yml/badge.svg)](https://github.com/architehc/selfware/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/selfware)](https://crates.io/crates/selfware)
[![Docs.rs](https://docs.rs/selfware/badge.svg)](https://docs.rs/selfware)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![codecov](https://codecov.io/gh/architehc/selfware/branch/main/graph/badge.svg)](https://codecov.io/gh/architehc/selfware)

```
       /\___/\
      ( o   o )    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
      (  =^=  )    selfware — Your Personal AI Workshop
       )     (     Software you own. Software that knows you.
      (       )    Software that lasts.
     ( |     | )   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
      \|     |/
```

An **agentic coding harness** for local LLMs that runs entirely on your hardware. 54 tools, multi-agent swarm, evolution engine, TUI dashboard, and a fox mascot — all local-first, no cloud required.

> **TL;DR** — Point it at any OpenAI-compatible endpoint (vLLM, Ollama, llama.cpp, LM Studio), give it a task, and watch it autonomously read, plan, edit, test, and commit code. Then let the evolution engine improve itself.

---

## What It Looks Like

### Interactive Chat

```
╭─── selfware workshop ────────────────────────────────────╮
│                                                           │
│   /\___/\                                                 │
│  ( o   o )  Welcome to your workshop!                     │
│  (  =^=  )  What shall we tend to today?                  │
│   )     (                                                 │
│                                                           │
│  you> Add unit tests for the auth module                  │
│                                                           │
│  🌿 Planning...                                           │
│  🔍 Reading src/auth/mod.rs                               │
│  ✍️  Writing tests/auth_test.rs                            │
│  🧪 Running cargo test... 12 passed                       │
│  📦 Committing: "Add 12 unit tests for auth module"       │
│                                                           │
│  🌸 BLOOM — Task complete!                                │
╰───────────────────────────────────────────────────────────╯
```

### TUI Dashboard (`selfware --tui`)

```
┌─ Selfware Dashboard ──────────────────────────────────────────────┐
│ ┌─ Agent Status ─────────┐  ┌─ Token Usage ──────────────────┐   │
│ │ State: WORKING         │  │ ████████████░░░░ 75% (37k/50k) │   │
│ │ Tool:  file_edit       │  │ Budget remaining: 13,000 tokens │   │
│ │ Step:  7 / 100         │  └─────────────────────────────────┘   │
│ │ Time:  2m 34s          │                                        │
│ └────────────────────────┘  ┌─ Digital Garden ───────────────┐   │
│ ┌─ Message Stream ───────┐  │ src/                           │   │
│ │ Reading auth/mod.rs... │  │  🌳 mod.rs      [THRIVING]    │   │
│ │ Found 3 functions      │  │  🌿 handler.rs  [GROWING]     │   │
│ │ Writing test file...   │  │  🌱 utils.rs    [SEEDLING]    │   │
│ │ Running tests...       │  │                                │   │
│ └────────────────────────┘  └────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────────┘
```

### Evolution Engine (`selfware evolve`)

```
╭─── Evolution Daemon ──────────────────────────────────────╮
│                                                           │
│  Generation 1 / 3                                         │
│  ├─ Hypothesis 1: Cache token lookups     → 🌸 BLOOM     │
│  ├─ Hypothesis 2: Optimize FIM joining    → 🌸 BLOOM     │
│  ├─ Hypothesis 3: Refactor parse logic    → ❄️  FROST     │
│  └─ Hypothesis 4: Inline hot path         → 🌸 BLOOM     │
│                                                           │
│  SAB Fitness: 50 → 60 (+10.0)                            │
│  Committed: "Gen 1 BLOOM: Cache token lookups"            │
│                                                           │
│  3/4 edits applied · 3/3 compiled · 3/3 tests passed     │
╰───────────────────────────────────────────────────────────╯
```

### Multi-Agent Swarm (`selfware multi-chat`)

```
╭─── Swarm: 4 agents active ───────────────────────────────╮
│                                                           │
│  🏗️  Architect  → Designing module structure              │
│  💻 Coder      → Implementing auth handler                │
│  🧪 Tester     → Writing integration tests                │
│  🔍 Reviewer   → Reviewing PR #42                         │
│                                                           │
│  Progress: ████████████████░░░░ 80%                       │
╰───────────────────────────────────────────────────────────╯
```

> **Screenshots & GIFs**: See the [`docs/`](docs/) directory for full-resolution screenshots and animated GIFs of each mode in action.

---

## Quick Start

### 1. Install Selfware

**Option A: Download prebuilt binary (recommended)**

```bash
# Linux/macOS one-liner
OS=$(uname -s | tr '[:upper:]' '[:lower:]' | sed 's/darwin/macos/')
ARCH=$(uname -m | sed 's/arm64/aarch64/')
curl -fsSL "https://github.com/architehc/selfware/releases/latest/download/selfware-${OS}-${ARCH}.tar.gz" | tar -xz
sudo mv selfware /usr/local/bin/
```

| Platform | Architecture | Download |
|----------|--------------|----------|
| **Linux** | x86_64 (Intel/AMD) | [selfware-linux-x86_64.tar.gz](https://github.com/architehc/selfware/releases/latest) |
| **Linux** | aarch64 (ARM64) | [selfware-linux-aarch64.tar.gz](https://github.com/architehc/selfware/releases/latest) |
| **macOS** | Apple Silicon (M1–M4) | [selfware-macos-aarch64.tar.gz](https://github.com/architehc/selfware/releases/latest) |
| **macOS** | Intel | [selfware-macos-x86_64.tar.gz](https://github.com/architehc/selfware/releases/latest) |
| **Windows** | x86_64 | [selfware-windows-x86_64.zip](https://github.com/architehc/selfware/releases/latest) |

**Option B: Install via Cargo**

```bash
cargo install selfware
```

**Option C: Build from source**

```bash
git clone https://github.com/architehc/selfware.git
cd selfware
cargo build --release --all-features
./target/release/selfware --help
```

**Option D: Docker**

```bash
docker build -t selfware .
docker run --rm -it -v $(pwd):/workspace selfware chat
```

### 2. Set Up a Local LLM

Selfware needs an **OpenAI-compatible API endpoint**. Pick any backend:

| Backend | Best For | One-liner |
|---------|----------|-----------|
| **[vLLM](https://docs.vllm.ai/)** | Fast inference, GPU servers | `vllm serve Qwen/Qwen3-Coder-Next-FP8` |
| **[Ollama](https://ollama.ai/)** | Easy setup, any hardware | `ollama run qwen2.5-coder` |
| **[llama.cpp](https://github.com/ggerganov/llama.cpp)** | GGUF models, minimal deps | `./llama-server -m model.gguf -c 65536` |
| **[LM Studio](https://lmstudio.ai/)** | GUI, Windows/Mac | Download → load model → start server |
| **[MLX](https://github.com/ml-explore/mlx-examples)** | Apple Silicon native | `mlx_lm.server --model mlx-community/Qwen3.5-Coder-35B-A3B-4bit` |
| **[SGLang](https://github.com/sgl-project/sglang)** | High throughput, tool calling | `python -m sglang.launch_server --model Qwen/Qwen3-Coder-Next-FP8` |

> For finding and downloading the best local models, see **[Unsloth Model Zoo](https://unsloth.ai/docs/models/qwen3.5)** — they provide optimized quantized versions ready to run.

### 3. Configure

Create `selfware.toml` in your project directory:

```toml
# Your local workshop
endpoint = "http://localhost:8000/v1"    # Your LLM backend
model = "Qwen/Qwen3-Coder-Next-FP8"     # Model name
max_tokens = 65536
temperature = 0.7

[safety]
allowed_paths = ["./**", "/home/*/projects/**"]
denied_paths = ["**/.env", "**/secrets/**"]
protected_branches = ["main"]

[agent]
max_iterations = 100
step_timeout_secs = 600         # 10 min per step

[continuous_work]
enabled = true
checkpoint_interval_tools = 10  # Checkpoint every 10 tool calls
auto_recovery = true

[retry]
max_retries = 5
base_delay_ms = 1000
max_delay_ms = 60000
```

Or use the setup wizard:

```bash
selfware init
```

### 4. Start Coding

```bash
# Interactive chat
selfware chat

# Run a specific task
selfware run "Add unit tests for the auth module"

# Multi-agent mode (4 concurrent agents)
selfware multi-chat

# Analyze your codebase
selfware analyze ./src

# View your code as a living garden
selfware garden

# Full TUI dashboard
selfware --tui
```

---

## Recommended Models & Hardware

### Qwen3.5 — Hardware Requirements

[Qwen3.5](https://unsloth.ai/docs/models/qwen3.5) is highly recommended for selfware. It's a strong coder with excellent instruction following and thinking capabilities. Here are the total **VRAM + RAM** requirements at different quantization levels:

| Qwen3.5 Model | 3-bit | 4-bit | 6-bit | 8-bit | BF16 |
|----------------|-------|-------|-------|-------|------|
| **0.8B + 2B** | 3 GB | 3.5 GB | 5 GB | 7.5 GB | 9 GB |
| **4B** | 4.5 GB | 5.5 GB | 7 GB | 10 GB | 14 GB |
| **9B** | 5.5 GB | 6.5 GB | 9 GB | 13 GB | 19 GB |
| **27B** | 14 GB | 17 GB | 24 GB | 30 GB | 54 GB |
| **35B-A3B** (MoE) | 17 GB | 22 GB | 30 GB | 38 GB | 70 GB |
| **122B-A10B** (MoE) | 60 GB | 70 GB | 106 GB | 132 GB | 245 GB |
| **397B-A17B** (MoE) | 180 GB | 214 GB | 340 GB | 512 GB | 810 GB |

> Source: [Unsloth — Qwen3.5 Inference Requirements](https://unsloth.ai/docs/models/qwen3.5)

The **MoE models** (35B-A3B, 122B-A10B, 397B-A17B) only activate a fraction of parameters per token, making them significantly faster at inference despite their large parameter count.

### GPU Servers (vLLM / llama.cpp / SGLang)

| Model | Quant | VRAM | Recommended GPU | Context | SAB Score |
|-------|-------|------|-----------------|---------|-----------|
| **Qwen3-Coder-Next-FP8** | FP8 | 80 GB | H100 / A100 80 GB | 1M | **90/100** (27 rounds) |
| **Qwen3.5-Coder 35B-A3B** | Q4_K_M | 22 GB | **RTX 5090** (32 GB) | 32–128K | Best value |
| **Qwen3.5 27B** | Q4 | 17 GB | RTX 4090 / 3090 (24 GB) | 32–64K | Strong |
| **LFM2 24B-A2B** | 4-bit | 13 GB | RTX 4090 / 3090 (24 GB) | 32–64K | Good |
| **Qwen3.5 9B** | Q4 | 6.5 GB | RTX 4060 Ti (16 GB) | 16–32K | Decent |
| **LFM2.5 1.2B** | Q8 | 1.25 GB | Any GPU | 8–16K | Prototyping |

### Apple Silicon (MLX / Ollama / llama.cpp)

Mac uses unified memory — your total RAM determines what you can run:

| RAM | Recommended Model | Quant | Context | Use Case |
|-----|-------------------|-------|---------|----------|
| **96–128 GB** | Qwen3.5 35B-A3B | Q8 | 64–128K | Full SAB, production coding |
| **64 GB** | Qwen3.5 35B-A3B | Q4_K_M | 32–64K | Most scenarios, good context |
| **32 GB** | Qwen3.5 27B or LFM2 24B-A2B | 4-bit | 16–32K | Everyday coding |
| **24 GB** | Qwen3.5 9B | Q4 | 16–32K | Moderate tasks |
| **16 GB** | Qwen3.5 4B or LFM2.5 1.2B | Q8 | 8–16K | Lightweight, fast feedback |

> **Context window matters.** SAB scenarios work best with >=32K context. Adjust `max_tokens` in `selfware.toml` to match your model's context.

### Quick Setup Examples

```bash
# H100 with vLLM (reference setup, 90/100 SAB)
vllm serve Qwen/Qwen3-Coder-Next-FP8 --max-model-len 131072

# RTX 5090 with Qwen3.5 35B MoE (llama.cpp)
./llama-server -m qwen3.5-coder-35b-a3b-q4_k_m.gguf \
  -c 65536 -ngl 99 --port 8000

# RTX 4090 with Qwen3.5 27B (vLLM)
vllm serve Qwen/Qwen3.5-27B-AWQ --max-model-len 32768

# Mac M2/M3/M4 with MLX
mlx_lm.server --model mlx-community/Qwen3.5-Coder-35B-A3B-4bit \
  --port 8000

# Any machine with Ollama
ollama run qwen2.5-coder:14b

# Ultra-light (CPU or weak GPU)
ollama run qwen2.5-coder:1.5b
```

---

## Features

### 54 Built-in Tools

Selfware gives the LLM a full toolkit for autonomous coding:

| Category | Tools | Examples |
|----------|-------|---------|
| **File Tending** | Read, write, edit, search, tree | `file_read`, `file_write`, `file_edit`, `directory_tree` |
| **Git Cultivation** | Status, diff, commit, branch, log | `git_status`, `git_diff`, `git_commit`, `git_checkpoint` |
| **Cargo Workshop** | Test, check, clippy, fmt, build | `cargo_test`, `cargo_check`, `cargo_clippy`, `cargo_fmt` |
| **Code Foraging** | Grep, glob, symbol search | `grep_search`, `glob_find`, `symbol_search` |
| **Shell** | Execute commands with safety checks | `shell_exec` |
| **Analysis** | AST parsing, complexity, BM25 | `code_analysis`, `bm25_search` |
| **Knowledge** | Web fetch, documentation lookup | `web_fetch`, `knowledge_query` |
| **FIM Editing** | Fill-in-the-Middle AI code replacement | `file_fim_edit` |

### Multi-Agent Swarm

Up to **16 concurrent agents** with role specialization:

```bash
selfware multi-chat -n 8
```

Roles: **Architect**, **Coder**, **Tester**, **Reviewer**, **DevOps**, **Security** — each with its own context and tool access. The swarm coordinator distributes tasks and merges results.

### Task Persistence & Recovery

Tasks survive crashes via automatic checkpointing:

```bash
# Start a long task
selfware run "Refactor the entire authentication system"

# Power outage? System crash? No problem.
selfware journal          # Browse saved checkpoints
selfware resume <task-id> # Pick up exactly where you left off
```

### Cognitive Architecture

The agent thinks in PDVR cycles with working memory:

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

**Working Memory** tracks current plan, active hypothesis, open questions, and discovered facts. **Episodic Memory** learns from past sessions — what worked, your preferences, project patterns.

### Multi-Layer Safety

```
Request → Path Guardian → Command Sentinel → Protected Groves → Execute
```

- **Path validation**: Allowed/denied path globs, no escape from workspace
- **Command filtering**: Dangerous commands blocked by default
- **Protected branches**: Prevent force-push to main
- **SSRF protection**: URL validation on web requests
- **Evolution safety**: Cannot modify its own fitness function, SAB suite, or safety module

### Warm Terminal Aesthetic

Four color themes for your workshop:

| Theme | Style | Flag |
|-------|-------|------|
| **Amber** (default) | Warm amber, soil brown, garden green | `--theme amber` |
| **Ocean** | Cool blues and teals | `--theme ocean` |
| **Minimal** | Clean grayscale | `--theme minimal` |
| **High Contrast** | Accessibility-focused | `--theme high-contrast` |

Status messages use garden metaphors:
- **BLOOM** — Success, fresh growth
- **GROW** — Progress, on the right track
- **WILT** — Warning, needs attention
- **FROST** — Error, needs warmth

---

## Evolution Engine — Recursive Self-Improvement

The evolution engine is selfware's most unique feature: it uses an LLM to generate code improvements to itself, then verifies them through compilation and testing. Only improvements that pass `cargo check` + `cargo test` survive.

### How It Works

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Generate    │────▶│  Apply       │────▶│  Verify      │
│  Hypotheses  │     │  Edits       │     │  (compile +  │
│  (LLM call)  │     │  (search/    │     │   test)      │
└──────────────┘     │   replace)   │     └──────┬───────┘
       ▲             └──────────────┘            │
       │                                         ▼
┌──────┴───────┐                          ┌──────────────┐
│  History +   │◀─────────────────────────│  Select or   │
│  Telemetry   │     fitness improved?    │  Rollback    │
└──────────────┘                          └──────────────┘
```

1. **Generate**: LLM reads your mutation target files and proposes N hypotheses as search-and-replace edits
2. **Apply**: Each hypothesis's edits are applied with fuzzy whitespace matching
3. **Verify**: `cargo check` → `cargo test` — if either fails, the hypothesis is rolled back
4. **Select**: If all tests pass and fitness improves, the change is committed as a **BLOOM**

### Running Evolution

```bash
# Build with the self-improvement feature
cargo build --release --features self-improvement

# Run 3 generations with 4 hypotheses each
./target/release/selfware evolve --generations 3 --population 4

# Dry run (show config, don't execute)
./target/release/selfware evolve --dry-run
```

### Configuring Mutation Targets

In `selfware.toml`, specify which files the evolution engine is allowed to modify:

```toml
[evolution]
# Prompt construction logic
prompt_logic = [
    "src/agent/planning.rs",
    "src/agent/loop_control.rs",
    "src/orchestration/planning.rs",
]

# Tool implementations
tool_code = [
    "src/tools/file.rs",
    "src/tools/search.rs",
    "src/tool_parser.rs",
]

# Cognitive architecture
cognitive = [
    "src/cognitive/memory_hierarchy.rs",
    "src/cognitive/episodic.rs",
    "src/memory.rs",
]

# Config keys the agent may tune
config_keys = ["temperature", "max_tokens", "token_budget"]
```

### Safety Invariants

The evolution engine **CANNOT** modify:

| Protected Path | Reason |
|----------------|--------|
| `src/evolution/` | Cannot modify its own evolution logic |
| `src/safety/` | Cannot weaken safety checks |
| `system_tests/` | Cannot modify its own benchmark suite |
| `benches/sab_*` | Cannot game fitness measurements |

These are enforced at the code level via `PROTECTED_PATHS` in `src/evolution/mod.rs`.

### Evolution Output

The engine writes a JSONL event log to `.evolution-log.jsonl` for every generation:

```jsonl
{"event":"generation_start","generation":1,"timestamp":"2026-03-04T12:25:00Z"}
{"event":"hypothesis_result","generation":1,"hypothesis":"Cache token lookups","applied":true,"compiled":true,"tests_passed":true,"rating":"BLOOM"}
{"event":"generation_end","generation":1,"blooms":3,"frosts":1,"fitness_delta":10.0}
```

Successful improvements are auto-committed to the repo with descriptive messages:

```
Gen 1 BLOOM: Cache token lookups in FIM string joining
Gen 2 BLOOM: Optimize search-replace dispatch
Gen 3 BLOOM: Inline hot path in token counter
```

---

## SAB — Selfware Agentic Benchmark

A **12-scenario agentic coding benchmark** that measures how well a local LLM can autonomously fix bugs, write tests, refactor code, and optimize performance through selfware's agent loop.

### Scenarios

| Difficulty | Scenario | What It Tests |
|------------|----------|---------------|
| Easy | `easy_calculator` | Simple arithmetic bug fixes (3–4 bugs) |
| Easy | `easy_string_ops` | String manipulation bugs |
| Medium | `medium_json_merge` | JSON deep merge logic |
| Medium | `medium_bitset` | Bitwise operations and edge cases |
| Medium | `testgen_ringbuf` | Write 15+ tests for an untested ring buffer |
| Medium | `refactor_monolith` | Split a 210-line monolith into 4 modules |
| Hard | `hard_scheduler` | Multi-file scheduler with duration parsing |
| Hard | `hard_event_bus` | Event system with async subscribers |
| Hard | `security_audit` | Replace 5 vulnerable functions with secure alternatives |
| Hard | `perf_optimization` | Fix 5 O(n^2)/exponential algorithms |
| Hard | `codegen_task_runner` | Implement 12 `todo!()` method stubs |
| Expert | `expert_async_race` | Fix 4 concurrency bugs in a Tokio task pool |

### Scoring

Each scenario scores 0–100:
- **70 pts** — all tests pass after agent edits
- **20 pts** — agent also fixes intentionally broken tests
- **10 pts** — clean exit (no crash, no timeout)

Round ratings: **BLOOM** (>=85) · **GROW** (>=60) · **WILT** (>=30) · **FROST** (<30)

### Benchmark Results — Qwen3-Coder-Next-FP8

Tested on NVIDIA H100 via vLLM, 6 parallel scenarios, 27 rounds (323 scenario runs):

| Metric | Value |
|--------|-------|
| Steady-state average (R2–R27) | **90/100** |
| Peak phase (R9–R27) | **91/100** |
| Best round | **96/100** (achieved 8 times) |
| Perfect rounds (12/12 pass) | **16 out of 27** |
| BLOOM rounds (>=85) | **22 out of 27** |
| S-tier scenarios (100% reliable) | 5 of 12 |

<details>
<summary>Full round-by-round results</summary>

| Round | Score | Rating | Passed |
|-------|-------|--------|--------|
| R1 | 60/100 | GROW | 7/11 |
| R2 | 96/100 | BLOOM | 12/12 |
| R3 | 70/100 | GROW | 9/12 |
| R4 | 87/100 | BLOOM | 11/12 |
| R5 | 79/100 | GROW | 10/12 |
| R6 | 81/100 | GROW | 10/12 |
| R7 | 87/100 | BLOOM | 11/12 |
| R8 | 89/100 | BLOOM | 11/12 |
| R9 | 95/100 | BLOOM | 12/12 |
| R10 | 95/100 | BLOOM | 12/12 |
| R11 | 96/100 | BLOOM | 12/12 |
| R12 | 87/100 | BLOOM | 11/12 |
| R13 | 96/100 | BLOOM | 12/12 |
| R14 | 88/100 | BLOOM | 11/12 |
| R15 | 95/100 | BLOOM | 12/12 |
| R16 | 95/100 | BLOOM | 12/12 |
| R17 | 95/100 | BLOOM | 12/12 |
| R18 | 96/100 | BLOOM | 12/12 |
| R19 | 96/100 | BLOOM | 12/12 |
| R20 | 96/100 | BLOOM | 12/12 |
| R21 | 89/100 | BLOOM | 11/12 |
| R22 | 87/100 | BLOOM | 11/12 |
| R23 | 96/100 | BLOOM | 12/12 |
| R24 | 87/100 | BLOOM | 11/12 |
| R25 | 90/100 | BLOOM | 11/12 |
| R26 | 95/100 | BLOOM | 12/12 |
| R27 | 73/100 | GROW | 9/12 |

</details>

### Scenario Reliability

| Tier | Scenarios | Pass Rate |
|------|-----------|-----------|
| **S** (100%) | `easy_calculator`, `easy_string_ops`, `medium_json_merge`, `perf_optimization`, `codegen_task_runner` | 100% |
| **A** (>80%) | `hard_scheduler`, `hard_event_bus`, `expert_async_race`, `medium_bitset` | 89–96% |
| **B** (50–80%) | `security_audit`, `testgen_ringbuf`, `refactor_monolith` | 70–74% |

### Running Your Own Benchmark

```bash
export ENDPOINT="http://localhost:8000/v1"
export MODEL="Qwen/Qwen3-Coder-Next-FP8"
export MAX_PARALLEL=6

bash system_tests/projecte2e/run_full_sab.sh

# Results in system_tests/projecte2e/reports/<timestamp>/
```

---

## CLI Reference

| Command | Alias | Description |
|---------|-------|-------------|
| `selfware chat` | `c` | Interactive chat session |
| `selfware multi-chat` | `m` | Multi-agent swarm chat |
| `selfware run <task>` | `r` | Execute a specific task |
| `selfware analyze <path>` | `a` | Survey codebase structure |
| `selfware garden` | | View code as a digital garden |
| `selfware journal` | `j` | Browse checkpoint entries |
| `selfware resume <id>` | | Resume from checkpoint |
| `selfware status` | | Show workshop stats |
| `selfware workflow <file>` | `w` | Run a YAML workflow |
| `selfware init` | | Setup wizard |
| `selfware evolve` | | Run evolution engine* |
| `selfware improve` | | Self-improvement pass* |
| `selfware demo` | | Run animated demo** |
| `selfware dashboard` | | Launch TUI dashboard** |

\* Requires `--features self-improvement`
\*\* Requires `--features tui`

### Global Flags

| Flag | Description |
|------|-------------|
| `-p <PROMPT>` | Headless mode: run prompt and exit |
| `-C <DIR>` | Set working directory |
| `-m <MODE>` | Execution mode: `normal`, `auto-edit`, `yolo`, `daemon` |
| `-y` | Shortcut for `--mode=yolo` |
| `--tui` | Launch TUI dashboard |
| `--theme <THEME>` | Color theme: `amber`, `ocean`, `minimal`, `high-contrast` |
| `--compact` | Dense output, less chrome |
| `-v, --verbose` | Detailed tool output |
| `--show-tokens` | Display token usage after each response |
| `--ascii` | ASCII-only output (no emoji) |
| `--no-color` | Disable colored output |

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `SELFWARE_ENDPOINT` | LLM API endpoint | `http://localhost:8000/v1` |
| `SELFWARE_MODEL` | Model name | `Qwen/Qwen3-Coder-Next-FP8` |
| `SELFWARE_API_KEY` | API key (if required) | None |
| `SELFWARE_MAX_TOKENS` | Max tokens per response | `65536` |
| `SELFWARE_TEMPERATURE` | Sampling temperature | `0.7` |
| `SELFWARE_TIMEOUT` | Request timeout (seconds) | `600` |
| `SELFWARE_DEBUG` | Enable debug logging | Disabled |
| `SELFWARE_ASCII` | Force ASCII-only mode | Disabled |
| `NO_COLOR` | Disable colors (standard) | Disabled |

---

## Slow Model Support

Designed for local LLMs on consumer hardware. The agent will wait patiently:

```
Model Speed          Timeout Setting
─────────────────────────────────────
> 10 tok/s           300s (5 min)
1-10 tok/s           3600s (1 hour)
< 1 tok/s            14400s (4 hours)
0.08 tok/s           Works! Be patient.
```

---

## Project Structure

```
src/
├── agent/          Core agent logic, checkpointing, execution
├── tools/          54 tool implementations (file, git, cargo, search, shell, FIM)
├── api/            LLM client with timeout, retry, streaming
├── ui/             Terminal aesthetic (themes, animations, banners, fox mascot)
│   └── tui/        Full ratatui dashboard (garden view, swarm widgets, particles)
├── analysis/       Code analysis, BM25 search, vector store
├── cognitive/      PDVR cycle, working/episodic memory, RAG, token budget
├── config/         Configuration management (TOML + env + CLI)
├── devops/         Container support, process manager
├── evolution/      Recursive self-improvement engine (feature-gated)
│   ├── daemon.rs   Main evolution loop + LLM hypothesis generation
│   ├── fitness.rs  SAB-based fitness scoring
│   ├── sandbox.rs  Isolated evaluation environments
│   └── tournament.rs  Parallel hypothesis evaluation
├── observability/  OpenTelemetry tracing, Prometheus metrics
├── orchestration/  Multi-agent swarm, planning, workflows
├── safety/         Path validation, command filtering, sandboxing
├── self_healing/   Error classification, recovery, exponential backoff
├── session/        Checkpoint persistence
├── testing/        Verification, contract testing, workflow DSL
├── memory.rs       Memory management
├── tool_parser.rs  Robust multi-format XML parser
└── token_count.rs  Token estimation
```

---

## Development

### Run Tests

```bash
# All tests (~5,200 tests)
cargo test --all-features

# Quick unit tests only
cargo test --lib --all-features

# Evolution engine tests (95 tests)
cargo test --features self-improvement evolution::
cargo test --features self-improvement --test evolution_integration_test

# With resilience features
cargo test --features resilience

# Integration tests with real LLM
cargo test --features integration
```

### Test Coverage

| Metric | Value |
|--------|-------|
| **Total Tests** | ~5,200 |
| **Line Coverage** | ~82% |
| **Test Targets** | lib (4,784) + external (254) + integration (124) + doc (1) + property (5) |

### Code Quality

```bash
cargo clippy --all-features -- -D warnings
cargo fmt -- --check
cargo llvm-cov --lib --all-features --summary-only
```

---

## Troubleshooting

**"Connection refused"** — Is your LLM backend running?
```bash
curl http://localhost:8000/v1/models
```

**"Request timeout"** — Increase timeout for slow models:
```toml
[agent]
step_timeout_secs = 14400  # 4 hours
```

**"Safety check failed"** — Check `allowed_paths` in your config. The agent only accesses paths you permit.

**Evolution produces no BLOOMs** — Common causes:
- Model response truncated → increase `max_tokens` in config
- Thinking mode consuming tokens → the engine disables it automatically with `/no_think`
- Patch context mismatch → the engine uses fuzzy whitespace matching to handle this

---

## License

MIT License

## Acknowledgments

- Built for [Qwen3-Coder](https://qwenlm.github.io/), [Kimi K2.5](https://kimi.moonshot.cn/), [LFM2](https://www.liquid.ai/), and other local LLMs
- Model downloads and quantizations via [Unsloth](https://unsloth.ai/docs/models/qwen3.5)
- Inspired by the [AiSocratic](https://aisocratic.org/) movement
- UI philosophy: software should feel like a warm workshop, not a cold datacenter

---

```
    "Tend your garden. The code will grow."
                                    — selfware proverb
```
