# SAB Scorecard: Qwen3.5-27B-FP8

## Summary

| Metric | Value |
|--------|-------|
| Date | 2026-03-15 |
| Model | Qwen/Qwen3.5-27B-FP8 |
| Backend | vLLM (v0.17.1rc1) |
| Hardware | 2x NVIDIA RTX 4090 (46 GB total) |
| Context | 262,144 tokens |
| KV Cache | FP8 |
| Mamba Cache | float16 |
| Concurrency | ~1x at 262K |
| Decode Speed | ~24 tok/s |
| Overall Score (raw) | 93/100 |
| Overall Score (weighted) | **95/100** |
| Overall Rating | **BLOOM** |
| Duration | 55m 49s (2 parallel) |

## Rating Distribution

| Rating | Count |
|--------|-------|
| BLOOM | 17 |
| GROW | 2 |
| WILT | 0 |
| FROST | 1 |

## Detailed Results

| Scenario | Difficulty | Score | Rating | Duration |
|----------|-----------|-------|--------|----------|
| `easy_calculator` | easy | 100/100 | BLOOM | 68s |
| `easy_string_ops` | easy | 100/100 | BLOOM | 116s |
| `medium_json_merge` | medium | 100/100 | BLOOM | 58s |
| `medium_bitset` | medium | 100/100 | BLOOM | 156s |
| `hard_scheduler` | hard | 100/100 | BLOOM | 78s |
| `hard_event_bus` | hard | 100/100 | BLOOM | 155s |
| `expert_async_race` | expert | 100/100 | BLOOM | 126s |
| `security_audit` | hard | 100/100 | BLOOM | 165s |
| `perf_optimization` | hard | 100/100 | BLOOM | 756s |
| `codegen_task_runner` | hard | 100/100 | BLOOM | 97s |
| `testgen_ringbuf` | medium | 80/100 | GROW | 329s |
| `refactor_monolith` | medium | 80/100 | GROW | 503s |
| `viz_svg_chart` | easy | 100/100 | BLOOM | 252s |
| `viz_ascii_table` | easy | 0/100 | FROST | 1810s (timeout) |
| `viz_histogram` | easy-medium | 100/100 | BLOOM | 126s |
| `viz_sparkline` | easy-medium | 100/100 | BLOOM | 77s |
| `viz_progress_bar` | medium | 100/100 | BLOOM | 233s |
| `viz_maze_gen` | medium | 100/100 | BLOOM | 164s |
| `unsafe_scanner` | hard | 100/100 | BLOOM | 174s |
| `actor_pdvr` | hard | 100/100 | BLOOM | 541s |

## Architecture Notes

Qwen3.5-27B uses a hybrid **Gated Attention + Gated DeltaNet** (linear attention)
architecture with 64 layers: 16 blocks of (3x DeltaNet + 1x Gated Attention).
This is similar to Mamba-style SSM hybrids but uses DeltaNet instead of Mamba for
the recurrent layers. vLLM handles this via its hybrid KV cache manager with
experimental prefix caching support (`mamba_cache_mode = "align"`).

## vLLM Serve Command

```bash
export VLLM_ALLOW_LONG_MAX_MODEL_LEN=1
export NCCL_P2P_DISABLE=1
export NCCL_IB_DISABLE=1
export NCCL_SHM_DISABLE=0

vllm serve Qwen/Qwen3.5-27B-FP8 \
    --tensor-parallel-size 2 \
    --kv-cache-dtype fp8 \
    --gpu-memory-utilization 0.88 \
    --max-model-len 262144 \
    --max-num-seqs 2 \
    --mamba-cache-dtype float16 \
    --enable-prefix-caching \
    --reasoning-parser qwen3 \
    --enable-auto-tool-choice \
    --tool-call-parser qwen3_coder \
    --served-model-name qwen3.5-27b \
    --trust-remote-code \
    --host 0.0.0.0 \
    --port 8000
```
