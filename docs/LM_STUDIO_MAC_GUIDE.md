# Running Selfware on Mac with LM Studio

A step-by-step guide to running selfware with local models on macOS using LM Studio. Includes RAM-based model recommendations and per-tier configuration files.

---

## 1. Prerequisites

- **macOS** with Apple Silicon (M1/M2/M3/M4) or Intel
- **Rust toolchain** (if installing via cargo):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Selfware** installed:
  ```bash
  cargo install --git https://github.com/architehc/selfware
  ```
- **LM Studio** downloaded from [https://lmstudio.ai/](https://lmstudio.ai/)

---

## 2. Download & Configure Your Model (Critical Setup)

Follow these steps exactly to ensure tool calling works correctly.

### Step-by-step in LM Studio:

1. Open LM Studio and go to the **Search** tab.
2. Search for your desired Qwen3.5 model (see the RAM table in section 3 below).
3. Download the recommended quantization for your RAM.
4. Go to **LLMs** in the menu bar.
5. Select your downloaded model.
6. In the right panel, find **"Prompt Template"**.
7. **Switch from Jinja to Manual.**
8. In the Manual dropdown, select **"ChatML"**.
9. Start the server (default port: **1234**).

### Why Manual ChatML?

LM Studio's Jinja templates can mangle the XML tags that selfware uses for tool calling. When the Jinja template reformats or escapes XML content, the tool parser fails to recognize tool calls. Manual ChatML mode preserves them correctly, ensuring reliable tool execution.

---

## 3. Choose Your Model (by RAM)

Mac uses unified memory -- your total RAM determines what model you can run. Pick the row that matches your machine:

| RAM | Recommended (Q4) | Alternative (Q8) | Context Window | Notes |
|-----|-------------------|-------------------|----------------|-------|
| **8 GB** | Qwen3.5-4B Q4 (5.5 GB) | Qwen3.5-0.8B Q8 (3.5 GB) | 4-8K | Minimum viable. Simple tasks only. |
| **16 GB** | Qwen3.5-9B Q4 (6.5 GB) | Qwen3.5-4B Q8 (10 GB) | 8-16K | Light coding tasks. |
| **32 GB** | Qwen3.5-27B Q4 (17 GB) | Qwen3.5-9B Q8 (13 GB) | 16-32K | Everyday coding. Good balance. |
| **64 GB** | Qwen3.5-35B-A3B Q4 (22 GB) | Qwen3.5-27B Q8 (30 GB) | 32-64K | Strong coding. MoE = fast. |
| **96 GB** | Qwen3.5-35B-A3B Q8 (38 GB) | Qwen3.5-122B-A10B Q4 (70 GB) | 64-128K | Production quality. |
| **128 GB** | Qwen3.5-122B-A10B Q4 (70 GB) | Qwen3.5-122B-A10B Q8 (132 GB) | 128-256K | Maximum capability. |

> **MoE models** (35B-A3B, 122B-A10B) only activate a fraction of parameters per token -- faster inference despite large size. If your RAM allows it, prefer these for the best speed-to-quality ratio.

---

## 4. Create Your First Project

```bash
mkdir my-project && cd my-project
git init
```

Then create a `selfware.toml` configuration file in the project root. You can use any editor:

```bash
nano selfware.toml
```

Paste the configuration for your RAM tier from section 5 below, save, and exit.

---

## 5. Per-Tier selfware.toml Configs

Pick the configuration that matches your RAM. Each config is tuned for the model size, context window, and timeout appropriate for that tier.

### 8 GB RAM -- Qwen3.5-4B Q4

```toml
# selfware.toml -- 8 GB Mac (Qwen3.5-4B Q4)
endpoint = "http://localhost:1234/v1"
model = "qwen/qwen3.5-4b"
native_function_calling = false
streaming = true
max_tokens = 4096
temperature = 0.2
token_budget = 100000

[agent]
max_iterations = 50
step_timeout_secs = 600    # 10 min -- small model is relatively fast

[safety]
allowed_paths = ["./**"]
denied_paths = ["**/.env", "**/secrets/**"]

[retry]
max_retries = 3
base_delay_ms = 1000
max_delay_ms = 30000
```

### 16 GB RAM -- Qwen3.5-9B Q4

```toml
# selfware.toml -- 16 GB Mac (Qwen3.5-9B Q4)
endpoint = "http://localhost:1234/v1"
model = "qwen/qwen3.5-9b"
native_function_calling = false
streaming = true
max_tokens = 8192
temperature = 0.2
token_budget = 150000

[agent]
max_iterations = 75
step_timeout_secs = 600    # 10 min per step

[safety]
allowed_paths = ["./**"]
denied_paths = ["**/.env", "**/secrets/**"]

[retry]
max_retries = 3
base_delay_ms = 1000
max_delay_ms = 30000
```

### 32 GB RAM -- Qwen3.5-27B Q4

```toml
# selfware.toml -- 32 GB Mac (Qwen3.5-27B Q4)
endpoint = "http://localhost:1234/v1"
model = "qwen/qwen3.5-27b"
native_function_calling = false
streaming = true
max_tokens = 16384
temperature = 0.2
token_budget = 250000

[agent]
max_iterations = 100
step_timeout_secs = 900    # 15 min -- larger model, slower per token

[safety]
allowed_paths = ["./**"]
denied_paths = ["**/.env", "**/secrets/**"]

[retry]
max_retries = 5
base_delay_ms = 1000
max_delay_ms = 60000
```

### 64 GB RAM -- Qwen3.5-35B-A3B Q4 (MoE)

```toml
# selfware.toml -- 64 GB Mac (Qwen3.5-35B-A3B Q4, MoE)
endpoint = "http://localhost:1234/v1"
model = "qwen/qwen3.5-35b-a3b"
native_function_calling = false
streaming = true
max_tokens = 32768
temperature = 0.2
token_budget = 350000

[agent]
max_iterations = 100
step_timeout_secs = 1200   # 20 min -- MoE is fast despite size

[safety]
allowed_paths = ["./**"]
denied_paths = ["**/.env", "**/secrets/**"]

[retry]
max_retries = 5
base_delay_ms = 1000
max_delay_ms = 60000
```

### 96 GB RAM -- Qwen3.5-35B-A3B Q8

```toml
# selfware.toml -- 96 GB Mac (Qwen3.5-35B-A3B Q8)
endpoint = "http://localhost:1234/v1"
model = "qwen/qwen3.5-35b-a3b"
native_function_calling = false
streaming = true
max_tokens = 65536
temperature = 0.2
token_budget = 400000

[agent]
max_iterations = 100
step_timeout_secs = 1200   # 20 min

[safety]
allowed_paths = ["./**"]
denied_paths = ["**/.env", "**/secrets/**"]

[retry]
max_retries = 5
base_delay_ms = 1000
max_delay_ms = 60000
```

### 128 GB RAM -- Qwen3.5-122B-A10B Q4 (MoE)

```toml
# selfware.toml -- 128 GB Mac (Qwen3.5-122B-A10B Q4, MoE)
endpoint = "http://localhost:1234/v1"
model = "qwen/qwen3.5-122b-a10b"
native_function_calling = false
streaming = true
max_tokens = 131072
temperature = 0.2
token_budget = 500000

[agent]
max_iterations = 100
step_timeout_secs = 1800   # 30 min -- large model, be patient

[safety]
allowed_paths = ["./**"]
denied_paths = ["**/.env", "**/secrets/**"]

[retry]
max_retries = 5
base_delay_ms = 1000
max_delay_ms = 60000
```

---

## 6. Start Coding

With LM Studio running and your `selfware.toml` in place, start an interactive session:

```bash
selfware chat
```

You should see the selfware workshop banner and a prompt. Type a task and the agent will begin planning and executing.

---

## 7. Verify Tool Calling Works

After starting `selfware chat`, try this as your first message:

```
list files in this directory
```

The agent should call `directory_tree` or `shell_exec` to list files and return the result. If it does, tool calling is working correctly.

If the agent responds with a text description instead of actually calling a tool, check that:
- The Prompt Template in LM Studio is set to **Manual ChatML** (not Jinja).
- `native_function_calling` is set to `false` in your `selfware.toml`.
- The model is actually loaded and the server is running.

---

## 8. LM Studio Settings Tips

These settings are found in the LM Studio server configuration panel:

| Setting | Recommendation | Why |
|---------|---------------|-----|
| **KV Cache Quantization** | Set to **Q4** | Reduces memory used by the context cache, letting you fit larger context windows in the same RAM. |
| **GPU Offload** | **100%** for Apple Silicon | Apple Silicon uses unified memory -- the GPU and CPU share the same RAM, so offloading everything to the GPU is optimal. |
| **Flash Attention** | **Enable** if available | Speeds up attention computation and reduces memory usage for long contexts. |
| **Batch Size** | Default (512) | The default is fine for single-user local usage. No need to increase. |

---

## 9. Troubleshooting

### Model too slow

- Try a smaller model or Q4 quantization instead of Q8.
- Reduce `max_tokens` in your config to limit generation length.
- MoE models (35B-A3B) are faster than dense models (27B) at similar quality.

### Out of memory

- Reduce the context window (`max_tokens`) in `selfware.toml`.
- Enable KV cache quantization (Q4) in LM Studio.
- Switch to a smaller model -- see the RAM table in section 3.

### Tool calls not working

- Check that the Prompt Template in LM Studio is set to **Manual ChatML** (not Jinja). This is the most common cause.
- Verify `native_function_calling = false` in `selfware.toml`.
- Restart both LM Studio and selfware after changing the prompt template.

### Connection refused

- Verify LM Studio's server is running (green indicator in the UI).
- Check that the port matches your config -- LM Studio defaults to **1234**, not 8000.
- Test the connection:
  ```bash
  curl http://localhost:1234/v1/models
  ```

### "missing field path" errors

This is a known parser issue that can occur when models generate malformed XML in tool calls, especially with HTML content. Update to the latest selfware version:

```bash
cargo install selfware --force
```

### Model outputs gibberish or loops

- Lower the temperature to `0.1` or `0.15`.
- Try a different quantization (Q4_K_M often works better than Q4_K_S).
- Ensure you are using a recent Qwen3.5 model -- older Qwen2/Qwen3 models have weaker instruction following.

---

## Summary

1. Install selfware and LM Studio.
2. Download a Qwen3.5 model sized for your RAM.
3. Set the Prompt Template to **Manual ChatML** in LM Studio.
4. Create a `selfware.toml` with the config for your tier.
5. Run `selfware chat` and verify tool calling works.

For GPU server setups (vLLM, SGLang, llama.cpp), see the main [README](../README.md).
