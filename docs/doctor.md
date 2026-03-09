# Doctor Mode Guide

Selfware includes two diagnostic modes to help verify your setup:

- `selfware doctor` -- checks system dependencies and tool availability
- LLM Doctor -- verifies your LLM backend configuration

## System Doctor

Run the system doctor to check all dependencies:

```bash
selfware doctor
```

### What It Checks

The doctor runs checks across 10 categories, all in parallel:

#### Core (Required)

These must be present for selfware to function:

| Tool | Check Command |
|------|--------------|
| `rustc` | `rustc --version` |
| `cargo` | `cargo --version` |
| `git` | `git --version` |

If any core tool is missing, the overall status is **broken**.

#### Languages (Optional)

| Tool | Check Command |
|------|--------------|
| `node` | `node --version` or `nodejs --version` |
| `npm` | `npm --version` |
| `python` | `python3 --version` or `python --version` |
| `pip` | `pip3 --version` or `pip --version` |
| `go` | `go version` |

#### Rust Tools

| Tool | Check Command |
|------|--------------|
| `cargo-clippy` | `cargo clippy --version` |
| `cargo-fmt` | `cargo fmt --version` |
| `cargo-tarpaulin` | `cargo tarpaulin --version` |

#### Python Tools

| Tool | Check Command |
|------|--------------|
| `ruff` | `ruff --version` |
| `mypy` | `mypy --version` |
| `pytest` | `pytest --version` |
| `black` | `black --version` |
| `bandit` | `bandit --version` |

#### Node Tools

| Tool | Check Command |
|------|--------------|
| `npx` | `npx --version` |
| `eslint` | `eslint --version` |
| `prettier` | `prettier --version` |
| `tsc` | `tsc --version` |
| `vitest` | `vitest --version` |

#### Go Tools

| Tool | Check Command |
|------|--------------|
| `golangci-lint` | `golangci-lint --version` |
| `gofmt` | `gofmt -e /dev/null` |

#### Computer Control / Image Processing

| Tool | Check Command |
|------|--------------|
| `ImageMagick` | `convert --version` |
| `ffmpeg` | `ffmpeg -version` |
| `xcap` | Internal screen capture test |
| `Accessibility` | macOS: checks System Events permission |

#### Container Tools

| Tool | Check Command |
|------|--------------|
| `docker` | `docker --version` |
| `docker-compose` | `docker-compose --version` or `docker compose version` |

#### Browser Automation

| Tool | Check Command |
|------|--------------|
| `chromium/chrome` | `chromium --version` (or chrome variants) |
| `playwright` | `npx playwright --version` |

#### Security

| Tool | Check Command |
|------|--------------|
| `cargo-audit` | `cargo audit --version` |
| `safety` | `safety --version` |

### Output

Each check shows one of three statuses:

- **ok** (green checkmark) -- tool found, version detected
- **warning** (yellow triangle) -- optional tool not found
- **missing** (red X) -- required tool not found

### Overall Health

| Status | Meaning |
|--------|---------|
| **healthy** | All checks passed |
| **degraded** | Some optional tools missing |
| **broken** | Required tools missing -- selfware cannot function |

### Example Output

```
  Selfware Doctor -- System Diagnostics

  Core (Required):
    ok rustc (1.82.0)
    ok cargo (1.82.0)
    ok git (2.47.0)

  Languages (Optional):
    ok node (22.11.0)
    ok python (3.12.4)
    !! go -- Not found (optional)

  Rust Tools:
    ok cargo-clippy (0.1.82)
    ok cargo-fmt (1.7.1)
    !! cargo-tarpaulin -- Not found (optional)

  22/25 checks passed, 3 warnings, 0 missing
    Status: degraded -- some optional tools are missing
```

## LLM Doctor

The LLM doctor performs a deep diagnostic of your LLM backend. It runs automatically as part of `selfware doctor` when an endpoint is configured.

### What It Checks

The LLM doctor runs 6 diagnostic steps:

#### Step 1: Backend Detection

Probes the endpoint to identify the backend type:

| Backend | Detection Method |
|---------|-----------------|
| **sglang** | `/get_server_info` endpoint |
| **vllm** | `/version` endpoint with "vllm" in response |
| **ollama** | `/api/tags` endpoint |
| **llama.cpp** | `/health` endpoint with "slots" or "llama" |
| **LM Studio** | Model IDs containing "lm-studio" |
| **Unknown** | Falls back to "OpenAI-compatible" |

Lists all available models and their context lengths.

#### Step 2: Model Analysis

- Verifies the configured model exists in the backend
- Checks context length (recommends minimum 32K tokens)
- Provides backend-specific instructions for extending context length:
  - sglang: `--context-length 131072`
  - vllm: `--max-model-len 131072`
  - ollama: `num_ctx 131072` in Modelfile
  - llama.cpp: `-c 131072`
  - LM Studio: set in the UI

#### Step 3: Template / Chat Format Check

Verifies chat template configuration for your backend and model combination. Especially important for Qwen models that need proper tool-calling templates.

#### Step 4: Capability Assessment

Rates the model's quality tier based on model name/size:

| Tier | Models | Notes |
|------|--------|-------|
| **Excellent** | Qwen3.5-122B | Full tool use, vision, long context |
| **Very Good** | Qwen3-Coder | Optimized for coding tasks |
| **Good** | Other Qwen models | General purpose |
| **Limited** | Small models (<7B) | May struggle with multi-step tasks |
| **Unknown** | Unrecognized models | Run connection test to verify |

Checks feature compatibility: shell tools, file editing, multi-step workflows, tool calling, visual processing, long-context tasks.

#### Step 5: Connection Test

- Sends a test completion request and measures latency
- Estimates tokens-per-second throughput
- Tests native tool calling support

#### Step 6: Recommendations

Prints a summary with backend-specific optimization tips:

- sglang: `--enable-torch-compile` for better throughput
- vllm: `--enable-prefix-caching` for repeated prompts
- ollama: `OLLAMA_NUM_PARALLEL=1` for single-request throughput
- llama.cpp: `--mlock` to prevent swapping

## Troubleshooting Common Issues

### "Could not reach endpoint"

The LLM backend is not running or the endpoint URL is wrong.

```bash
# Check if the backend is running
curl http://localhost:8000/v1/models

# Common endpoints:
# sglang/vllm: http://localhost:8000/v1
# LM Studio:   http://localhost:1234/v1
# ollama:      http://localhost:11434/v1
```

### "Configured model not found"

The model name in `selfware.toml` does not match any model the backend is serving.

```bash
# List available models
curl http://localhost:8000/v1/models | jq '.data[].id'
```

Update `model` in your config to match exactly.

### "Tool calling not working"

The backend or model does not support native function calling. Solutions:

1. Set `native_function_calling = false` in `[agent]` (default). Selfware will embed tools in the system prompt instead.
2. For sglang: add `--tool-call-parser` flag when starting the server.
3. For vllm: add `--enable-auto-tool-choice` flag.

### "Context length too short"

The model is running with a small context window. Extend it:

```bash
# sglang
python -m sglang.launch_server --context-length 131072 ...

# vllm
python -m vllm.entrypoints.openai.api_server --max-model-len 131072 ...
```

### "High latency"

- Use a smaller or quantized model
- Enable torch compile (`--enable-torch-compile` for sglang)
- Use flash attention (`--enable-flashinfer` for sglang)
- Check GPU utilization with `nvidia-smi`

### "Permission denied" on config file

```bash
chmod 600 ~/.config/selfware/config.toml
```

If strict permissions are enabled, group/world-readable config files are rejected.
