# SWE-bench Pro 10-model smoke leaderboard

**Dataset:** `system_tests/swe_bench_pro/sample_10.jsonl` (10 instances)  
**Date:** 2026-06-28  
**Harness:** `run_selfware.py` with P0 routing-gate, SEARCH/REPLACE, compile-gate, and evaluator fixes applied.

## Overall results

| Model | Passed | Compl | Err | F2P | P2P |
|-------|--------|-------|-----|-----|-----|
| deepseek-v3.2 | **2/10** | 8 | 2 | 34/109 (31.2%) | 882/930 (94.8%) |
| poolside-laguna-xs.2-free-sweap | **2/10** | 7 | 3 | 21/109 (19.3%) | 714/930 (76.8%) |
| nova-lite | 1/10 | 5 | 5 | 2/109 (1.8%) | 573/930 (61.6%) |
| cohere-north-mini-code-free-sweap | 1/10 | 2 | 8 | 1/109 (0.9%) | 60/930 (6.5%) |
| gemma-3-12b | 0/10 | 5 | 5 | 0/109 (0.0%) | 683/930 (73.4%) |
| mistral-nemo | 0/10 | 3 | 7 | 0/109 (0.0%) | 649/930 (69.8%) |
| granite-4.1-8b | 0/10 | 2 | 8 | 0/109 (0.0%) | 469/930 (50.4%) |
| llama-3.1-8b | 0/10 | 2 | 8 | 0/109 (0.0%) | 373/930 (40.1%) |
| lfm-2.5-1.2b-thinking-free-sweap | 0/10 | 1 | 9 | 0/109 (0.0%) | 191/930 (20.5%) |
| qwen2.5-7b | 0/10 | 0 | 10 | 0/109 (0.0%) | 0/930 (0.0%) |

*Passed* = instances where every fail-to-pass test passed and no pass-to-pass test regressed.  
*Compl* = completed evaluations; *Err* = errored (empty patch, patch apply failure, no tests executed, etc.).  
*F2P* = fail-to-pass tests converted (higher is better).  
*P2P* = previously-passing tests still passing (higher is better).

## Diagnostic counters

| Model | Empty patch | Compile gate rejected | Recovery fired | Recovery succeeded | No-op patch | Compile/test-setup fail | F2P failed | P2P regressed |
|-------|-------------|----------------------|----------------|--------------------|-------------|-------------------------|------------|---------------|
| deepseek-v3.2 | 0 | 0 | 0 | 0 | 0 | 2 | 6 | 2 |
| poolside-laguna-xs.2-free-sweap | 1 | 0 | 0 | 0 | 0 | 2 | 5 | 1 |
| nova-lite | 3 | 0 | 0 | 0 | 0 | 2 | 4 | 3 |
| cohere-north-mini-code-free-sweap | 8 | 0 | 0 | 0 | 0 | 0 | 1 | 0 |
| gemma-3-12b | 3 | 0 | 0 | 0 | 0 | 2 | 5 | 1 |
| mistral-nemo | 6 | 0 | 0 | 0 | 0 | 1 | 3 | 1 |
| granite-4.1-8b | 6 | 0 | 0 | 0 | 0 | 2 | 2 | 1 |
| llama-3.1-8b | 8 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| lfm-2.5-1.2b-thinking-free-sweap | 9 | 0 | 0 | 0 | 0 | 0 | 1 | 0 |
| qwen2.5-7b | 8 | 0 | 0 | 0 | 0 | 2 | 0 | 0 |

*Note:* Recovery and compile-gate counters are read from prediction `metadata`, which was added after this swarm ran. The existing predictions were generated without metadata, so those columns show 0. Future runs will populate them.

## Solved instances by model

- **deepseek-v3.2** (2)
  - `instance_internetarchive__openlibrary-4a5d2a7d24c9e4c11d3069220c0685b736d5ecde-v13642507b4fc1f8d234172bf8129942da2c2ca26`
  - `instance_qutebrowser__qutebrowser-f631cd4422744160d9dcf7a0455da532ce973315-v35616345bb8052ea303186706cec663146f0f184`
- **poolside-laguna-xs.2-free-sweap** (2)
  - `instance_qutebrowser__qutebrowser-f91ace96223cac8161c16dd061907e138fe85111-v059c6fdc75567943479b23ebca7c07b5e9a7f34c`
  - `instance_internetarchive__openlibrary-4a5d2a7d24c9e4c11d3069220c0685b736d5ecde-v13642507b4fc1f8d234172bf8129942da2c2ca26`
- **nova-lite** (1)
  - `instance_internetarchive__openlibrary-4a5d2a7d24c9e4c11d3069220c0685b736d5ecde-v13642507b4fc1f8d234172bf8129942da2c2ca26`
- **cohere-north-mini-code-free-sweap** (1)
  - `instance_internetarchive__openlibrary-4a5d2a7d24c9e4c11d3069220c0685b736d5ecde-v13642507b4fc1f8d234172bf8129942da2c2ca26`

The OpenLibrary instance was the only one solved by more than one model.

## Observations

- **Top performers:** `deepseek-v3.2` and `poolside-laguna-xs.2-free-sweap` each solved 2/10 instances. `deepseek-v3.2` is not a small model and used the multi-turn tool loop; it also has the best F2P conversion (31.2%).
- **Best small model:** `poolside-laguna-xs.2-free-sweap` matched deepseek on solved count and had the strongest fail-to-pass conversion among the small models (19.3%).
- **Honest scoring is working:** After the harness fixes, pass rates are no longer inflated by leaked test patches or silently-partial SEARCH/REPLACE blocks. Scores reflect actual model capability on this sample.
- **Empty patches are the dominant loss mode:** 52 of 100 predictions were empty (mostly on the weaker small models). The new metadata counters will let us measure how many of those are recovered by the empty-patch escalation in future runs.
- **Repo-specific gaps remain:** NodeBB and ansible instances scored 0 across all 10 models, suggesting either repo-specific harness gaps (test command / oracle parsing) or that these instances are above small-model capability.

## Raw data

Per-model evaluation reports and `predictions.jsonl` files are in:

```
system_tests/swe_bench_pro/swarm_2026_06_27_full/<model>/
system_tests/swe_bench_pro/swarm_2026_06_27_full/<model>_eval/
```
