# SAB Round 1 Report

## Summary

| Metric | Value |
|--------|-------|
| Date | 20260301-010659 |
| Model | Qwen/Qwen3-Coder-Next-FP8 |
| Endpoint | https://crazyshit.ngrok.io/v1 |
| Scenarios | 12 |
| Completed | 11 |
| Passed | 7/11 |
| Average Score | 60/100 |
| Overall Rating | **🌿 GROW** |
| Note | perf_optimization excluded (post-validation hung) |

## Results

| Scenario | Difficulty | Score | Rating | Duration | Timeout |
|----------|-----------|-------|--------|----------|---------|
| codegen_task_runner | hard | 100/100 | 🌸 BLOOM | 217s | no |
| easy_calculator | easy | 100/100 | 🌸 BLOOM | 60s | no |
| easy_string_ops | easy | 100/100 | 🌸 BLOOM | 143s | no |
| medium_bitset | medium | 100/100 | 🌸 BLOOM | 118s | no |
| medium_json_merge | medium | 100/100 | 🌸 BLOOM | 235s | no |
| security_audit | hard | 90/100 | 🌸 BLOOM | 490s | yes |
| testgen_ringbuf | medium | 80/100 | 🌿 GROW | 173s | no |
| expert_async_race | expert | 0/100 | ❄️ FROST | 610s | yes |
| hard_event_bus | hard | 0/100 | ❄️ FROST | 610s | yes |
| hard_scheduler | hard | 0/100 | ❄️ FROST | 490s | yes |
| refactor_monolith | medium | 0/100 | ❄️ FROST | 490s | yes |

BLOOM: 6  GROW: 1  WILT: 0  FROST: 4
