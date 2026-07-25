# Model Matrix Bench — 2026-07-25

Endpoint: `https://openrouter.ai/api/v1` via `examples/endpoint_smoke.rs` (7 checks per model).

| model | window | pass | plain_chat ms | streaming ms | tool_call | thinking | usage tokens |
|---|---|---|---|---|---|---|---|
| poolside/laguna-s-2.1 | 1048576 | 7/7 | 289 | 276 | PASS | PASS | 74+3=77 |
| z-ai/glm-5.2 | 1048576 | 7/7 | 2226 | 1664 | PASS | PASS | 70+150=220 |
| moonshotai/kimi-k3 | 1048576 | 7/7 | 5664 | 4029 | PASS | PASS | 153+69=222 |
| xiaomi/mimo-v2.5 | 1050000 | 7/7 | 10381 | 3100 | PASS | PASS | 66+125=191 |
| deepseek/deepseek-v4-flash | 1048576 | 7/7 | 513 | 642 | PASS | PASS | 62+3=65 |
| deepseek/deepseek-v4-pro | 1048576 | 7/7 | 3303 | 3135 | PASS | PASS | 63+27=90 |
| tencent/hy3 | 262144 | 7/7 | 2472 | 1729 | PASS | PASS | 72+4=76 |
| nvidia/nemotron-3-ultra-550b-a55b:free | 1000000 | 7/7 | 1470 | 14050 | PASS | PASS | 75+28=103 |
| stepfun/step-3.7-flash | 262144 | 7/7 | 411 | 5459 | PASS | PASS | 75+41=116 |
| minimax/minimax-m3 | 1048576 | 7/7 | 927 | 2116 | PASS | PASS | 225+29=254 |

Checks: endpoint_reachable backend_classify plain_chat streaming tool_call tool_followup thinking_parse.
