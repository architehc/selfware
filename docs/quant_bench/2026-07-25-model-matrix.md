# Model Matrix Bench — 2026-07-25

Endpoint: `https://openrouter.ai/api/v1` via `examples/endpoint_smoke.rs` (7 checks per model).

| model | window | pass | plain_chat ms | streaming ms | tool_call | thinking | usage tokens |
|---|---|---|---|---|---|---|---|
| poolside/laguna-s-2.1 | 1048576 | 7/7 | 386 | 305 | PASS | PASS | 74+3=77 |
| z-ai/glm-5.2 | 1048576 | 7/7 | 3176 | 2893 | PASS | PASS | 68+126=194 |
| moonshotai/kimi-k3 | 1048576 | 7/7 | 2687 | 4217 | PASS | PASS | 153+20=173 |
| xiaomi/mimo-v2.5 | 1050000 | 6/7 | 30000 | 13402 | PASS | PASS | -- |
| deepseek/deepseek-v4-flash | 1048576 | 7/7 | 1587 | 3056 | PASS | PASS | 62+2=64 |
| deepseek/deepseek-v4-pro | 1048576 | 7/7 | 2139 | 2463 | PASS | PASS | 141+40=181 |
| tencent/hy3 | 262144 | 7/7 | 2147 | 476 | PASS | PASS | 72+4=76 |
| nvidia/nemotron-3-ultra-550b-a55b:free | 1000000 | 7/7 | 896 | 764 | PASS | PASS | 75+28=103 |
| stepfun/step-3.7-flash | 262144 | 7/7 | 638 | 2745 | PASS | PASS | 75+41=116 |
| minimax/minimax-m3 | 1048576 | 7/7 | 777 | 1639 | PASS | PASS | 225+29=254 |
| qwen/qwen3.7-max | 1000000 | 7/7 | 3849 | 6883 | PASS | PASS | 73+167=240 |

Checks: endpoint_reachable backend_classify plain_chat streaming tool_call tool_followup thinking_parse.
