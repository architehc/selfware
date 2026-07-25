# Model Matrix Bench — 2026-07-25

Endpoint: `https://openrouter.ai/api/v1` via `examples/endpoint_smoke.rs` (7 checks per model).

| model | window | pass | plain_chat ms | streaming ms | tool_call | thinking | usage tokens |
|---|---|---|---|---|---|---|---|
| poolside/laguna-s-2.1 | 1048576 | 7/7 | 347 | 394 | PASS | PASS | 74+3=77 |
| z-ai/glm-5.2 | 1048576 | 7/7 | 3350 | 1447 | PASS | PASS | 68+148=216 |
| moonshotai/kimi-k3 | 1048576 | 7/7 | 3471 | 4130 | PASS | PASS | 153+61=214 |
| xiaomi/mimo-v2.5 | 1050000 | 7/7 | 5358 | 5889 | PASS | PASS | 66+93=159 |
| deepseek/deepseek-v4-flash | 1048576 | 7/7 | 823 | 286 | PASS | PASS | 62+3=65 |
| deepseek/deepseek-v4-pro | 1048576 | 7/7 | 13160 | 3418 | PASS | PASS | 63+23=86 |
| tencent/hy3 | 262144 | 7/7 | 2968 | 1780 | PASS | PASS | 72+4=76 |
| nvidia/nemotron-3-ultra-550b-a55b:free | 1000000 | 4/7 | 880 | 3719 | FAIL | FAIL | 75+28=103 |
| stepfun/step-3.7-flash | 262144 | 7/7 | 580 | 529 | PASS | PASS | 75+67=142 |
| minimax/minimax-m3 | 1048576 | 7/7 | 990 | 586 | PASS | PASS | 225+29=254 |
| qwen/qwen3.7-max | 1000000 | 7/7 | 3483 | 3676 | PASS | PASS | 73+143=216 |
| qwen/qwen3.6-27b | 262144 | 7/7 | 5327 | 1749 | PASS | PASS | 71+197=268 |
| google/gemma-4-31b-it | 262144 | 7/7 | 585 | 10945 | PASS | PASS | 76+1=77 |

Checks: endpoint_reachable backend_classify plain_chat streaming tool_call tool_followup thinking_parse.
