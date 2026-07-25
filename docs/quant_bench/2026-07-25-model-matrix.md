# Model Matrix Bench — 2026-07-25

Endpoint: `https://openrouter.ai/api/v1` via `examples/endpoint_smoke.rs` (7 checks per model).

| model | window | pass | plain_chat ms | streaming ms | tool_call | thinking | usage tokens |
|---|---|---|---|---|---|---|---|
| poolside/laguna-s-2.1 | 1048576 | 4/7 | 205 | 153 | PASS | PASS | -- |
| z-ai/glm-5.2 | 1048576 | 7/7 | 3147 | 995 | PASS | PASS | 68+172=240 |
| moonshotai/kimi-k3 | 1048576 | 7/7 | 3205 | 3705 | PASS | PASS | 153+45=198 |
| xiaomi/mimo-v2.5 | 1050000 | 6/7 | 6120 | 30000 | PASS | PASS | 66+233=299 |
| deepseek/deepseek-v4-flash | 1048576 | 7/7 | 1651 | 1033 | PASS | PASS | 62+28=90 |
| deepseek/deepseek-v4-pro | 1048576 | 7/7 | 3119 | 2275 | PASS | PASS | 63+30=93 |
| tencent/hy3 | 262144 | 7/7 | 873 | 445 | PASS | PASS | 72+4=76 |
| nvidia/nemotron-3-ultra-550b-a55b:free | 1000000 | 7/7 | 2913 | 2304 | PASS | PASS | 75+28=103 |
| stepfun/step-3.7-flash | 262144 | 7/7 | 2500 | 1611 | PASS | PASS | 75+79=154 |
| minimax/minimax-m3 | 1048576 | 7/7 | 1724 | 2032 | PASS | PASS | 212+2=214 |
| qwen/qwen3.7-max | 1000000 | 7/7 | 4441 | 3921 | PASS | PASS | 73+165=238 |
| qwen/qwen3.6-27b | 262144 | 7/7 | 5830 | 2372 | PASS | PASS | 71+316=387 |
| google/gemma-4-31b-it | 262144 | 7/7 | 479 | 267 | PASS | PASS | 76+1=77 |
| google/gemma-4-26b-a4b-it:free | 262144 | 7/7 | 840 | 2492 | PASS | PASS | 76+2=78 |

Checks: endpoint_reachable backend_classify plain_chat streaming tool_call tool_followup thinking_parse.
