# OpenRouter SWE-bench Pro Pilot — Final Report

## Scope
- First 10 instances of `sample_50.jsonl`.
- Agentless patch generation via the Selfware harness.
- Includes the patch-ordering fix and prompt/container optimizations.

## Final leaderboard

| Rank | Model | Pass | Completed | Errors | Pass Rate |
|------|-------|------|-----------|--------|------------|
| 1 | z-ai/glm-5.2 | 4/7 | 7/10 | 3 | 57.1% |
| 2 | google/gemini-3.5-flash_opt | 4/9 | 9/10 | 1 | 44.4% |
| 3 | moonshotai/kimi-k2.6 | 3/8 | 8/10 | 2 | 37.5% |
| 4 | google/gemini-3.5-flash | 3/9 | 9/10 | 1 | 33.3% |
| 5 | moonshotai/kimi-k2.6_opt | 3/9 | 9/10 | 1 | 33.3% |
| 6 | openai/gpt-5-mini | 3/9 | 9/10 | 1 | 33.3% |
| 7 | z-ai/glm-5.2_opt | 3/9 | 9/10 | 1 | 33.3% |
| 8 | moonshotai/kimi-k2.7-code | 1/3 | 3/10 | 7 | 33.3% |
| 9 | poolside/laguna-m.1:free | 2/7 | 7/10 | 3 | 28.6% |
| 10 | nex-agi/nex-n2-pro | 2/8 | 8/10 | 2 | 25.0% |
| 11 | xiaomi/mimo-v2.5 | 2/8 | 8/10 | 2 | 25.0% |
| 12 | z-ai/glm-5.1 | 2/8 | 8/10 | 2 | 25.0% |
| 13 | stepfun/step-3.7-flash | 2/9 | 9/10 | 1 | 22.2% |
| 14 | minimax/minimax-m2.7 | 1/5 | 5/10 | 5 | 20.0% |
| 15 | openai/gpt-oss-120b | 1/5 | 5/10 | 5 | 20.0% |
| 16 | xiaomi/mimo-v2.5-pro | 1/5 | 5/10 | 5 | 20.0% |
| 17 | google/gemini-2.5-flash | 1/6 | 6/10 | 4 | 16.7% |
| 18 | minimax/minimax-m3 | 1/6 | 6/10 | 4 | 16.7% |
| 19 | nvidia/nemotron-3-ultra-550b-a55b:free | 1/6 | 6/9 | 3 | 16.7% |
| 20 | tencent/hy3-preview | 1/6 | 6/10 | 4 | 16.7% |
| 21 | deepseek/deepseek-v4-flash | 1/8 | 8/10 | 2 | 12.5% |
| 22 | nvidia/nemotron-3-nano-30b-a3b:free | 1/9 | 9/10 | 1 | 11.1% |
| 23 | deepseek/deepseek-v4-pro | 0/9 | 9/10 | 1 | 0.0% |
| 24 | meta-llama/llama-3.3-70b-instruct:free | 0/9 | 9/10 | 1 | 0.0% |
| 25 | microsoft/phi-4-mini-instruct | 0/9 | 9/10 | 1 | 0.0% |
| 26 | mistralai/mistral-small-3.2-24b-instruct | 0/9 | 9/10 | 1 | 0.0% |
| 27 | google/gemma-4-26b-a4b-it | 0/8 | 8/10 | 2 | 0.0% |
| 28 | mistralai/mistral-nemo | 0/8 | 8/10 | 2 | 0.0% |
| 29 | nvidia/nemotron-3-super-120b-a12b:free | 0/8 | 8/10 | 2 | 0.0% |
| 30 | openai/gpt-4o-mini | 0/8 | 8/10 | 2 | 0.0% |
| 31 | deepseek/deepseek-v3.2 | 0/5 | 5/10 | 5 | 0.0% |

## Key findings

1. **GLM 5.2 is the strongest model on this sample** (57.1% pass rate on completed instances).
2. **Serial execution beats parallel** for rootless Podman: it eliminated most `container state improper` errors.
3. **Gemini 3.5 Flash** is the best fast/cheap model at 33-44% depending on prompt variant.
4. **GPT-5-mini** is the best small model at 33%.
5. Most free/small models (Llama 3.3 70B, Phi-4 Mini, Mistral Small, Gemma-4) scored 0%.
6. Pass-to-pass rates are near 100% across the board; the bottleneck is fix correctness.
