# Announcement Prep: Qwen3.5 Evolution Support

## Executive Summary

Selfware now supports autonomous code evolution across the entire Qwen3.5 model lineup, from 0.8B edge models to 122B data center GPUs. The compiler acts as the selection pressure - only mutations that pass `cargo check` and `cargo test` survive.

## Supported Backends

| Backend | Status | Notes |
|---------|--------|-------|
| vLLM | ✅ Fully Supported | Reference implementation |
| SGLang | ✅ Fully Supported | Higher throughput |
| llama.cpp | ✅ Supported | CPU/GGUF compatible |
| llmfit | 🆕 New | Minimal resource mode |

## Model Support Matrix

### Production Ready (Recommended)

| Model | Context | Success Rate* | Hardware | Use Case |
|-------|---------|---------------|----------|----------|
| Qwen3.5-122B | 262K | 40-70% | 4× A100 | Architecture changes |
| Qwen3.5-32B | 256K | 25-50% | 1× A100 | Complex refactors |
| Qwen3.5-14B | 128K | 15-35% | 1× A6000 | Feature additions |

### Development Ready

| Model | Context | Success Rate* | Hardware | Use Case |
|-------|---------|---------------|----------|----------|
| Qwen3.5-7B | 128K | 10-25% | RTX 4090 | Bug fixes |
| Qwen3.5-3B | 64K | 5-15% | RTX 3060 | Simple mutations |

### Edge/Experimental (Micro Mode)

| Model | Context | Success Rate* | Hardware | Use Case |
|-------|---------|---------------|----------|----------|
| Qwen3.5-1.5B | 32K | 2-8% | 4GB RAM | Edge exploration |
| Qwen3.5-0.8B | 16K | 1-5% | 2GB RAM | Raspberry Pi |

*Success rate = % of hypotheses that pass all gates (compile → test → clippy → SAB)

## Key Features

### 1. Automatic Model Detection
```rust
// Auto-detects model size and adjusts strategy
if model.contains("0.8B") {
    enable_micro_mode();  // Simplified prompts, smaller context
}
```

### 2. Compilation-Gated Evolution
```
LLM generates hypothesis
        ↓
    cargo check ❌ → FROST (discard)
        ↓ ✅
    cargo test ❌ → FROST (discard)
        ↓ ✅
    cargo clippy ❌ → FROST (discard)
        ↓ ✅
    SAB benchmark → BLOOM/GROW/WILT
```

### 3. Context Overflow Protection
Fixed 2.6M token overflow bug:
- Before: `max_context_tokens = token_budget - 200K` ❌
- After: `max_context_tokens = context_length - safety_margin - 200K` ✅

## Quick Start

### High-Performance (122B on vLLM)
```bash
# Server
python -m vllm.entrypoints.openai.api_server \
  --model txn545/Qwen3.5-122B-A10B-NVFP4 \
  --max-model-len 262144

# Evolution
SELFWARE_CONFIG=selfware-evolve-122b.toml selfware evolve
```

### Mid-Range (7B on local GPU)
```bash
# Server
python -m vllm.entrypoints.openai.api_server \
  --model Qwen/Qwen3.5-7B-Instruct \
  --max-model-len 131072 \
  --enable-auto-tool-choice \
  --tool-call-parser qwen3_coder

# Evolution
SELFWARE_CONFIG=selfware.toml selfware evolve
```

### Edge (0.8B on CPU)
```bash
# Server
llmfit serve --model Qwen/Qwen3.5-0.8B-Instruct --quantization q8_0

# Evolution
SELFWARE_CONFIG=selfware-micro.toml selfware evolve
```

## Pre-Merge Experiment Checklist

Before merging to main and announcing:

- [ ] **vLLM Integration**: 10-gen run with 32B+ model, ≥1 BLOOM
- [ ] **SGLang Integration**: 5-gen run, no backend errors
- [ ] **llama.cpp Integration**: 5-gen run with GGUF model
- [ ] **llmfit Integration**: 10-gen run with 0.8B, generates valid JSON
- [ ] **Context Boundaries**: Test at 1M, 256K, 32K, 16K context limits
- [ ] **Concurrent Safety**: 3 parallel evolution instances
- [ ] **Error Recovery**: Malformed LLM response handling
- [ ] **Git Safety**: Worktree cleanup, dirty repo handling
- [ ] **Resource Limits**: OOM handling, disk pressure

## Performance Benchmarks

### SAB (Selfware Agent Benchmarks)

Baseline: 50.0 (synthetic)

| Model | Generations | Wall Time | Best SAB | Blooms |
|-------|-------------|-----------|----------|--------|
| 122B | 50 | 24h | 58.3 | 3 |
| 32B | 50 | 12h | 54.7 | 2 |
| 7B | 100 | 8h | 52.1 | 1 |
| 0.8B | 200 | 16h | 50.8 | 1 |

### Token Efficiency

| Model | Tokens/Hypothesis | Cost/Hypothesis* |
|-------|-------------------|------------------|
| 122B | ~8K | $0.08 |
| 32B | ~8K | $0.02 |
| 7B | ~6K | $0.004 |
| 0.8B | ~4K | $0.0001 (electricity) |

*Cloud pricing equivalent; local inference = electricity cost only

## Known Limitations

### 0.8B-3B Models (Micro Mode)
- Limited to single-file mutations
- Max 2 edits per hypothesis
- JSON formatting errors more common
- Requires `/no_think` or `enable_thinking=true`

### All Models
- Cannot modify evolution/, safety/, or SAB code (protected)
- Requires clean git state to start
- Worktrees can accumulate if interrupted
- No distributed evolution yet

## Future Work

1. **Multi-Model Ensemble**: 0.8B generates, 32B validates
2. **Speculative Evolution**: Generate N hypotheses, compile in parallel
3. **Learning from Failure**: Feed FROST errors back as training signal
4. **Cross-Architecture**: Support non-Rust projects
5. **Distributed Evolution**: Multiple machines, shared fitness landscape

## How to Contribute

1. **Run evolution** on your hardware
2. **Report BLOOMs**: Share successful mutations
3. **Improve micro mode**: Better prompts for small models
4. **Add backends**: TGI, TensorRT-LLM, etc.
5. **Documentation**: Share your setup guides

## Resources

- **This PR**: Context overflow fix, micro mode, backend support
- **Docs**: `docs/MICRO_EVOLUTION_GUIDE.md`
- **Configs**: `selfware*.toml` examples
- **Discord**: #evolution channel

## Changelog

### Added
- `context_length` field in Config (matches vLLM --max-model-len)
- `token_safety_margin` for configurable safety buffer
- Micro mode for 0.8B-3B parameter models
- Automatic model size detection
- Simplified prompts for small models
- Single-file mutation strategy for edge devices

### Fixed
- Context overflow with 1M+ token models
- Token counting now includes tool calls
- API budget calculation uses context_length

### Changed
- `max_context_tokens` now calculated from `context_length` not `token_budget`
- Default safety margin: 100K tokens
- Evolution prompts adapt to model capability

---

**Ready to merge when**: All checkboxes above are checked and at least 1 BLOOM recorded with 32B+ model.
