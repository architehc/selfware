# Model Matrix Bench — 2026-07-25

Endpoint: `https://openrouter.ai/api/v1` via `examples/endpoint_smoke.rs`. Declared = OpenRouter
supported_parameters/modalities metadata; measured = live probes (7 checks per model,
8 when the multimodal re-probe runs).

## Measured capabilities

| model | window | pass | multimodal | plain_chat ms | streaming ms | tool_call | thinking | usage tokens |
|---|---|---|---|---|---|---|---|---|
| poolside/laguna-s-2.1 | 1048576 | 5/7 | — | 339 | 254 | PASS | PASS | 74+3=77 |
| z-ai/glm-5.2 | 1048576 | 7/7 | — | 775 | 1374 | PASS | PASS | 68+3=71 |
| moonshotai/kimi-k3 | 1048576 | 8/8 | PASS | 3372 | 3318 | PASS | PASS | 153+20=173 |
| xiaomi/mimo-v2.5 | 1050000 | 8/8 | PASS | 763 | 1193 | PASS | PASS | 66+28=94 |
| deepseek/deepseek-v4-flash | 1048576 | 7/7 | — | 1082 | 533 | PASS | PASS | 62+3=65 |
| deepseek/deepseek-v4-pro | 1048576 | 7/7 | — | 1293 | 1288 | PASS | PASS | 62+21=83 |
| tencent/hy3 | 262144 | 7/7 | — | 1914 | 11956 | PASS | PASS | 72+4=76 |
| nvidia/nemotron-3-ultra-550b-a55b:free | 1000000 | 5/7 | — | 3731 | 766 | FAIL | PASS | 75+28=103 |
| stepfun/step-3.7-flash | 262144 | 8/8 | PASS | 2957 | 3289 | PASS | PASS | 75+78=153 |
| minimax/minimax-m3 | 1048576 | 8/8 | PASS | 1022 | 1988 | PASS | PASS | 225+29=254 |
| qwen/qwen3.7-max | 1000000 | 7/7 | — | 3636 | 4429 | PASS | PASS | 73+160=233 |
| qwen/qwen3.6-27b | 262144 | 8/8 | PASS | 7809 | 3664 | PASS | PASS | 71+90=161 |
| google/gemma-4-31b-it | 262144 | 8/8 | PASS | 339 | 636 | PASS | PASS | 89+2=91 |
| google/gemma-4-26b-a4b-it:free | 262144 | 8/8 | PASS | 9363 | 2549 | PASS | PASS | -- |

Checks: endpoint_reachable backend_classify plain_chat streaming tool_call tool_followup thinking_parse (+ multimodal for vision-capable models).

## Declared capabilities

| model | modalities | tools | structured outputs | reasoning | price in/out per M tokens |
|---|---|---|---|---|---|
| poolside/laguna-s-2.1 | text | yes | no | yes | $0.10/$0.20 |
| z-ai/glm-5.2 | text | yes | yes | yes | $0.71/$2.23 |
| moonshotai/kimi-k3 | text+image | yes | yes | yes | $3.00/$15.00 |
| xiaomi/mimo-v2.5 | text+audio+image+video | yes | yes | yes | $0.14/$0.28 |
| deepseek/deepseek-v4-flash | text | yes | yes | yes | $0.09/$0.19 |
| deepseek/deepseek-v4-pro | text | yes | yes | yes | $0.43/$0.87 |
| tencent/hy3 | text | yes | yes | yes | $0.13/$0.53 |
| nvidia/nemotron-3-ultra-550b-a55b:free | text | yes | no | yes | $0.00/$0.00 |
| stepfun/step-3.7-flash | text+image+video | yes | yes | yes | $0.20/$1.15 |
| minimax/minimax-m3 | text+image+video | yes | yes | yes | $0.30/$1.20 |
| qwen/qwen3.7-max | text | yes | yes | yes | $1.48/$4.42 |
| qwen/qwen3.6-27b | text+image+video | yes | yes | yes | $0.29/$2.40 |
| google/gemma-4-31b-it | image+text+video | yes | yes | yes | $0.14/$0.40 |
| google/gemma-4-26b-a4b-it:free | image+text+video | yes | yes | yes | $0.00/$0.00 |
