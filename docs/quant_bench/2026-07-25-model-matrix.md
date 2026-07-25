# Model Matrix Bench — 2026-07-25

Endpoint: `https://openrouter.ai/api/v1` via `examples/endpoint_smoke.rs` (7 checks per model).

| model | window | pass | plain_chat ms | streaming ms | tool_call | thinking | usage tokens |
|---|---|---|---|---|---|---|---|
| poolside/laguna-s-2.1 | 1048576 | 7/7 | 327 | 262 | PASS | PASS | 74+3=77 |
| z-ai/glm-5.2 | 1048576 | 7/7 | 2702 | 845 | PASS | PASS | 68+148=216 |
| moonshotai/kimi-k3 | 1048576 | 7/7 | 3429 | 2756 | PASS | PASS | 153+52=205 |
| xiaomi/mimo-v2.5 | 1050000 | 7/7 | 5589 | 11349 | PASS | PASS | 66+120=186 |
| deepseek/deepseek-v4-flash | 1048576 | 7/7 | 1053 | 1338 | PASS | PASS | 62+31=93 |
| deepseek/deepseek-v4-pro | 1048576 | 7/7 | 880 | 810 | PASS | PASS | 62+3=65 |
| tencent/hy3 | 262144 | 7/7 | 1840 | 767 | PASS | PASS | 72+4=76 |
| nvidia/nemotron-3-ultra-550b-a55b:free | 1000000 | 7/7 | 733 | 21734 | PASS | PASS | 75+24=99 |
| stepfun/step-3.7-flash | 262144 | 7/7 | 593 | 647 | PASS | PASS | 75+67=142 |
| minimax/minimax-m3 | 1048576 | 7/7 | 496 | 764 | PASS | PASS | 225+29=254 |
| qwen/qwen3.7-max | 1000000 | 7/7 | 3938 | 7738 | PASS | PASS | 73+170=243 |
| qwen/qwen3.6-27b | 262144 | 7/7 | 1909 | 1075 | PASS | PASS | 71+109=180 |

Checks: endpoint_reachable backend_classify plain_chat streaming tool_call tool_followup thinking_parse.
