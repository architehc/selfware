# Micro Evolution Guide: Qwen3.5-0.8B on Minimal Hardware

Run selfware evolution on edge devices, Raspberry Pi, or CPU-only machines using the smallest Qwen3.5 models.

## Overview

| Model | RAM Required | Storage | Tokens/sec | Evolution Success* |
|-------|--------------|---------|------------|-------------------|
| Qwen3.5-0.8B-Q8 | 2GB | 1.2GB | 15-30 | 1-5% |
| Qwen3.5-1.5B-Q8 | 3GB | 2GB | 10-20 | 2-8% |
| Qwen3.5-3B-Q4 | 4GB | 2.5GB | 8-15 | 5-15% |

*Success = hypotheses passing cargo check + tests

## Quick Start

### 1. Download Model (GGUF format for llama.cpp/llmfit)

```bash
# Using huggingface-cli
pip install huggingface-hub
huggingface-cli download \
  Qwen/Qwen3.5-0.8B-Instruct-GGUF \
  --local-dir ./models \
  --include "*q8_0.gguf"

# Or download manually:
wget https://huggingface.co/Qwen/Qwen3.5-0.8B-Instruct-GGUF/resolve/main/qwen3.5-0.8b-instruct-q8_0.gguf \
  -O models/qwen3.5-0.8b-q8_0.gguf
```

### 2. Option A: llama.cpp Server

```bash
# Build llama.cpp server
 git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp
make server -j$(nproc)

# Start server
./server \
  -m ../models/qwen3.5-0.8b-q8_0.gguf \
  -c 16384 \
  --host 0.0.0.0 \
  --port 8080 \
  -t 4  # Threads

# Test:
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "local", "messages": [{"role": "user", "content": "Hello"}]}'
```

### 3. Option B: llmfit (Recommended for Resource Constraints)

```bash
# Install llmfit
pip install llmfit

# Start with Qwen3.5-0.8B
llmfit serve \
  --model Qwen/Qwen3.5-0.8B-Instruct \
  --quantization q8_0 \
  --max-context 16384 \
  --device cpu

# Or use GGUF directly:
llmfit serve \
  --model ./models/qwen3.5-0.8b-q8_0.gguf \
  --backend llama.cpp \
  --max-context 16384
```

### 4. Run Selfware Evolution

```bash
# Use the micro configuration
cd /path/to/selfware
SELFWARE_CONFIG=selfware-micro.toml selfware evolve

# Or override for testing:
SELFWARE_CONFIG=selfware-micro.toml selfware evolve \
  --generations 50 \
  --population 1 \
  --parallel 1
```

## Configuration: selfware-micro.toml

```toml
endpoint = "http://localhost:8080/v1"
model = "qwen3.5-0.8b"
context_length = 16384
max_tokens = 2048
temperature = 0.3

[agent]
token_budget = 8000
token_safety_margin = 2000

[evolution]
# Single-file mutations only
prompt_logic = ["src/cognitive/state.rs"]
tool_code = ["src/tools/file.rs"]
cognitive = []
```

## How Micro Mode Works

### Automatic Detection

Micro mode activates automatically when the model name contains:
- `0.8b`, `1.5b`, `1b`, `2b`, `3b`
- `tiny`, `small`

### Adaptations

| Feature | Standard Mode | Micro Mode |
|---------|--------------|------------|
| Context size | 45K chars | 12K chars |
| Max files | All targets | 3 smallest |
| Population | 5-10 | 1-3 |
| Max edits/hypothesis | Unlimited | 2 |
| Prompt length | Detailed | Compressed |
| Evaluation | Parallel | Sequential |

### Simplified Prompts

**Standard prompt**: ~1500 tokens with detailed instructions
**Micro prompt**: ~300 tokens, example-driven:

```json
{
  "description": "fix off-by-one error",
  "edits": [
    {"file": "src/foo.rs", "search": "for i in 0..n", "replace": "for i in 0..=n"}
  ],
  "target_files": ["src/foo.rs"]
}
```

## Optimization Tips

### 1. File Selection

Choose small, self-contained files:

```toml
# Good: Small, focused files (<500 lines)
prompt_logic = ["src/cognitive/state.rs"]
tool_code = ["src/tools/file.rs"]

# Bad: Large, complex files
tool_code = ["src/agent/mod.rs"]  # 2000+ lines
```

### 2. Iteration Strategy

With small models, prefer many generations with small population:

```bash
# Better: 100 gens × 1 hypothesis = 100 attempts
selfware evolve --generations 100 --population 1

# Worse: 10 gens × 5 hypotheses = 50 attempts, more failures
selfware evolve --generations 10 --population 5
```

### 3. Temperature Tuning

| Temp | Effect | Use Case |
|------|--------|----------|
| 0.1-0.3 | Deterministic, repetitive | Fixing known issues |
| 0.4-0.6 | Balanced exploration | General evolution |
| 0.7-1.0 | High randomness, many FROSTs | Escaping local optima |

### 4. Quantization Trade-offs

| Format | Size | Quality | Speed |
|--------|------|---------|-------|
| Q4_0 | 450MB | ⚠️ Poor | Fastest |
| Q4_K_M | 500MB | OK | Fast |
| Q5_K_M | 600MB | Good | Medium |
| Q8_0 | 800MB | ✅ Best | Slower |
| F16 | 1.6GB | Native | Slowest |

**Recommendation**: Q8_0 for evolution - the quality improvement outweighs speed cost.

## Troubleshooting

### Issue: "JSON parse error" in hypotheses

**Cause**: 0.8B model producing malformed JSON
**Fix**: Lower temperature, enable thinking mode:
```toml
temperature = 0.2
[evolution.llm]
enable_thinking = true
```

### Issue: All hypotheses FROST (fail compile)

**Cause**: Search strings don't match exactly
**Fix**: Manually verify one edit works, then restart

### Issue: Out of memory

**Cause**: Context too large for RAM
**Fix**: Reduce context_length:
```toml
context_length = 8192  # Instead of 16384
```

### Issue: llmfit connection refused

**Cause**: llmfit not running or wrong port
**Fix**: Check `llmfit status` or restart server

## Expected Results

With Qwen3.5-0.8B on a 4-core CPU:

```
Generation 1:  1 hypothesis, 1 FROST (compile error)
Generation 2:  1 hypothesis, 1 FROST (test fail)
Generation 3:  1 hypothesis, 1 GROW (compiled, no improvement)
...
Generation 27: 1 hypothesis, 1 BLOOM! 🌸
   → Fixed off-by-one in token counting
   → SAB score: 50.0 → 51.2
```

**Realistic expectation**: 1 BLOOM per 20-50 generations with 0.8B model.

## Comparison: Cloud vs Edge

| Metric | Cloud (122B) | Edge (0.8B) |
|--------|--------------|-------------|
| Cost/hour | $2-5 | $0 (electricity) |
| Privacy | ❌ Data leaves device | ✅ Local only |
| Internet | Required | Not required |
| Speed/gen | 30 min | 5 min |
| Success rate | 40-70% | 1-5% |
| Best for | Architecture changes | Bug fixes, small tweaks |

## Next Steps

1. **Start small**: Run 10 generations with 0.8B to verify setup
2. **Scale up**: If successful, try 3B or 7B for better success rates
3. **Hybrid approach**: Use 0.8B for exploration, 32B for validation
4. **Contribute**: Share your BLOOMs to improve the model!

## References

- [llmfit GitHub](https://github.com/AlexsJones/llmfit)
- [llama.cpp](https://github.com/ggerganov/llama.cpp)
- [Qwen3.5 HuggingFace](https://huggingface.co/Qwen)
- [GGUF Format](https://github.com/ggerganov/ggml/blob/master/docs/gguf.md)
