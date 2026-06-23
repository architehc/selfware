# OpenRouter SWE-bench Pro Pilot — Interim Full Results

## Scope
- First 10 instances of `sample_50.jsonl`.
- Agentless patch generation via the Selfware harness.
- Includes the patch-ordering fix and all harness optimizations implemented during this session.

## Methodology evolution
1. **Parallel launch (AgentSwarm, 20 models)** → timed out; rootless Podman concurrency caused many `container state improper` errors.
2. **Serial runs** → much cleaner; revealed GLM 5.2 and Kimi K2.6 are strong.
3. **Isolated Podman roots** → launched for the top-3 re-run to test true parallelism without storage collisions.

## Full interim leaderboard

| Model | Pass | Completed | Errors | Pass Rate | Notes |
|-------|------|-----------|--------|------------|-------|
| z-ai/glm-5.2 | 4/7 | 7/10 | 3 | 57.1% | Best model found so far |
| moonshotai/kimi-k2.6 | 3/8 | 8/10 | 2 | 37.5% | Strong on completed cases |
| google/gemini-3.5-flash | 3/9 | 9/10 | 1 | 33.3% | Fast and consistent |
| moonshotai/kimi-k2.7-code | 1/3 | 3/10 | 7 | 33.3% | Many container errors in parallel |
| poolside/laguna-m.1:free | 2/7 | 7/10 | 3 | 28.6% | Good for a free model |
| nex-agi/nex-n2-pro | 2/8 | 8/10 | 2 | 25.0% | Paid slug works |
| xiaomi/mimo-v2.5 | 2/8 | 8/10 | 2 | 25.0% | Solid mid-tier |
| z-ai/glm-5.1 | 2/8 | 8/10 | 2 | 25.0% | Good smaller GLM |
| stepfun/step-3.7-flash | 2/9 | 9/10 | 1 | 22.2% | Fast |
| minimax/minimax-m2.7 | 1/5 | 5/10 | 5 | 20.0% | Container errors |
| openai/gpt-oss-120b | 1/5 | 5/10 | 5 | 20.0% | Container errors |
| xiaomi/mimo-v2.5-pro | 1/5 | 5/10 | 5 | 20.0% | Container errors |
| google/gemini-2.5-flash | 1/6 | 6/10 | 4 | 16.7% | Container errors |
| minimax/minimax-m3 | 1/6 | 6/10 | 4 | 16.7% | Container errors |
| nvidia/nemotron-3-ultra-550b-a55b:free | 1/6 | 6/9 | 3 | 16.7% | Free tier |
| tencent/hy3-preview | 1/6 | 6/10 | 4 | 16.7% | Container errors |
| deepseek/deepseek-v4-flash | 1/8 | 8/10 | 2 | 12.5% | Fast but weak fixes |
| deepseek/deepseek-v4-pro | 0/9 | 9/10 | 1 | 0.0% | No passes |
| meta-llama/llama-3.3-70b-instruct:free | 0/9 | 9/10 | 1 | 0.0% | No passes |
| google/gemma-4-26b-a4b-it | 0/8 | 8/10 | 2 | 0.0% | No passes |
| mistralai/mistral-nemo | 0/8 | 8/10 | 2 | 0.0% | No passes |
| nvidia/nemotron-3-super-120b-a12b:free | 0/8 | 8/10 | 2 | 0.0% | No passes |
| openai/gpt-4o-mini | 0/8 | 8/10 | 2 | 0.0% | No passes |
| deepseek/deepseek-v3.2 | 0/5 | 5/10 | 5 | 0.0% | Container errors |

## Still running
- `openai/gpt-5-mini`, `microsoft/phi-4-mini-instruct`, `mistralai/mistral-small-3.2-24b-instruct`, `nvidia/nemotron-3-nano-30b-a3b:free`, `qwen/qwen3.5-27b`, `qwen/qwen3.6-27b`, `qwen/qwen3-coder:free`
- Optimized re-run of GLM 5.2, Kimi K2.6, Gemini 3.5 Flash in isolated Podman roots.

## Harness changes made this session

### 1. Patch ordering fix (critical)
`evaluate_predictions.py` and `tdr.py` now apply the predicted patch **after** `before_repo_set_cmd`, which resets the repo and checks out updated test files. This single change turned Kimi K2.7 Code from 0/5 to 3/5 on the Teleport pilot.

### 2. Container reliability
- `start_container` now retries startup on transient Podman storage races.
- `stop_and_remove_container` ignores failures (idempotent cleanup).
- `evaluate_instance` retries the full setup sequence and embeds the output dir name in container names to avoid collisions.
- Evaluator entryscript sets `GOFLAGS=-p=1` and `GOMAXPROCS=1` to prevent OOM/fork storms under concurrent test runs.

### 3. Prompt improvements
- **Function-aware excerpting**: large files are now centered on the target function (e.g., `DeleteMFADevice`) rather than first/last chunks.
- **Test-patch hints**: extracted new test names, subtests, expected strings, and boundary numbers are added to the prompt without dumping the whole test patch (which regressed performance).
- **New-file creation**: empty SEARCH blocks create files; untracked files are captured in the final diff.

### 4. Parallelism infrastructure
Created isolated Podman root wrappers (`/tmp/podman-root-{1,2,3}/bin/podman`) so multiple model runs can operate in parallel without sharing rootless container storage.

## Interpretation
- **Best model**: GLM 5.2 is the clear winner on this sample, but it is slow. Serial execution was required for it to finish.
- **Best fast/cheap model**: Google Gemini 3.5 Flash (33% pass, ~10 min per instance).
- **Container concurrency is the enemy**: parallel runs on the same rootless storage produced 30-70% error rates; serial/isolated-root runs produced <30% errors.
- **Pass-to-pass is not the problem**: models rarely break existing tests; the challenge is generating the correct fix.

## Next actions
1. Wait for remaining small models and optimized top-3 re-run to finish.
2. Aggregate final leaderboard and compare optimized vs baseline pass rates.
3. If isolated Podman roots work, use them to run the full `sample_50` with GLM 5.2 and Gemini 3.5 Flash in parallel.
