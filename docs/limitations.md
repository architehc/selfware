# Known Limitations & Model Requirements

Selfware is powerful but not magic. This document explains what works, what doesn't, and what model capabilities are needed for each feature.

## Model Size Requirements by Feature

Not every feature works with every model. Here's what to expect:

| Feature | Minimum Model | Recommended | Notes |
|---------|--------------|-------------|-------|
| Simple file edits | 1.5B | 4B | Basic search/replace |
| Bug fixes (easy) | 4B | 9B | Single-file, clear errors |
| Bug fixes (hard) | 9B | 27B+ | Multi-file, subtle logic |
| Test generation | 4B | 9B | Quality scales with size |
| Refactoring | 9B | 27B+ | Needs architectural understanding |
| Multi-step tasks | 9B | 27B+ | Planning + execution |
| Tool calling (native) | 4B | 9B | Requires backend support (sglang/vllm) |
| Tool calling (XML) | 1.5B | 4B | Works with any backend |
| Swarm coordination | 27B+ | 35B+ | Multiple agents need strong reasoning |
| Evolution engine | 27B+ | 35B+ | Hypothesis generation requires deep understanding |
| Code generation (new project) | 4B | 9B | Templates help weaker models |
| Security audit | 27B+ | 35B+ | Requires security domain knowledge |
| Visual analysis (VLM) | 7B VLM | 30B VLM | Requires vision-capable model |

## What Selfware Cannot Do

### Fundamental Limitations
- **Cannot guarantee correct code** — LLM output is probabilistic, not proven
- **Cannot replace code review** — generated code should always be reviewed by a human
- **Cannot understand business context** — it knows code, not your business requirements
- **Cannot access the internet** (unless you configure MCP servers) — fully local by design
- **No true task decomposition** — the agent runs a ReAct-style tool-use loop with a fixed Plan→Execute template; it does not decompose a goal into a tracked subtask plan. Break large goals into concrete tasks yourself.

### Safety Limitations
- **Cannot prevent all prompt injection** — adversarial inputs may influence the agent
- **Cannot guarantee sandbox escape is impossible** — defense-in-depth, not absolute containment
- **Cannot detect all malicious patterns** — shell command filtering is regex-based, not a full parser
- **Shell bypasses file path-denies** — allow/deny path globs govern the *file* tools; `shell_exec` is governed separately (its cwd plus a regex command filter), so a shell command (e.g. `cat`) can read files the file-deny globs would block (`.env`, `~/.ssh`). There is no OS-level sandbox (namespaces/seccomp); containment is cooperative. For unattended runs, stay in Normal mode and keep secrets out of the working tree.
- **Cannot prevent TOCTOU races in all cases** — mitigated but not eliminated

### Performance Limitations
- **Slow with small models** — 1.5B models may take minutes per response
- **Context window limits** — very large codebases may not fit in context even at 128K tokens
- **Memory usage scales with context** — 128K context on a 27B model needs significant RAM
- **No GPU sharing** — selfware doesn't manage GPU allocation between agents

### Feature Limitations
- **Evolution engine is experimental** — feature-gated, requires careful configuration
- **Computer control is platform-dependent** — full support on macOS, limited on Linux/Windows
- **PTY shell doesn't work on Windows** — uses Unix pseudo-terminals
- **LSP requires language servers installed** — rust-analyzer, pyright, etc. must be on PATH
- **Visual verification requires VLM** — standard text models cannot analyze screenshots

## Weaker Model Tips

If you're running a 4B or smaller model:

1. **Use templates** — `selfware init` with `--template rust` scaffolds the project correctly so the model only fills in business logic
2. **Use Plan Mode** — `/plan` lets the model think before acting, reducing errors
3. **Keep tasks small** — "add a function that does X" works better than "build the entire auth system"
4. **Enable native function calling** — `native_function_calling = true` in config, with sglang/vllm backend. XML parsing is less reliable with small models
5. **Lower max_iterations** — prevent long loops: `max_iterations = 20`
6. **Use the interview mode** — structured questions help the model understand what you want
7. **Check the doctor** — `selfware doctor` ensures your environment is set up correctly

## Benchmark Reproduction

To reproduce the SAB 90/100 score:

```bash
# Requirements:
# - Qwen3-Coder-Next-FP8 on an H100 80GB (or equivalent)
# - sglang with native tool calling
# - 131072 context length

# Start the backend
python -m sglang.launch_server \
  --model-path Qwen/Qwen3-Coder-Next-FP8 \
  --context-length 131072 \
  --tool-call-parser qwen \
  --reasoning-parser qwen3 \
  --port 8000

# Configure selfware
cat > selfware.toml << 'EOF'
endpoint = "http://localhost:8000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 65536

[agent]
native_function_calling = true
max_iterations = 100
step_timeout_secs = 600
EOF

# Run the benchmark
export ENDPOINT="http://localhost:8000/v1"
export MODEL="Qwen/Qwen3-Coder-Next-FP8"
export MAX_PARALLEL=6
bash system_tests/projecte2e/run_full_sab.sh
```

Results vary by model, quantization, and context length. The 90/100 score was achieved with:
- Model: Qwen3-Coder-Next-FP8 (FP8 quantization)
- Context: 131072 tokens
- Backend: sglang with native tool calling
- Hardware: RTX 6000 Pro
- Rounds: 27 rounds, 323 scenario runs
