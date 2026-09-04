# Model & Endpoint Playbook

Measured facts from the September 2026 endpoint program. Every entry is
something we learned by running, not by reading marketing pages. Update it
when a measurement changes.

## 1. The fleet

| Endpoint | Model | Role | Measured |
|---|---|---|---|
| `localhost:31000` | qwen38-unc-kt (NVFP4 abliterated KT) | Red-team slow sensor, TB sensor, VLM fallback | 598,016-token KV pool; ~11 t/s per stream at 8 concurrent, 150–190 t/s shared prefill; 65k-context step can take 30 min — needs `step_timeout_secs = 2400`, `stream_stall_timeout_secs = 1200` |
| `192.168.137.1:8000` | qwen38-uncensored | Attack generation (the best), TB sensor | 12 × 262K pool; drops ~5×/day — **opportunistic only, never the critical path**; `step_timeout_secs = 2400`, stall 1200 |
| `llm.selfware.design` | qwen38-flash-next | Strongest local solver, VLM, 1M reviews | 1M ctx × 8 streams; **ngrok free tier caps ~3–6 agent streams** (400s under waves); stall 900 |
| OpenRouter | z-ai/glm-5.3 | Paid heavy solver (proofs, deep TB) | Solved TB4 coq-block-bound; $55–86 per deep trial at 2000-iter budgets — needs per-trial $ caps |
| OpenRouter | google/gemini-3.8-flash | **The vero workhorse** | First model to discharge Lean specs (2/9 primepy), then **11/11 bankledger perfect score** same day, first TB3 solve (1.0). $0.75/$3.75 per M, 1M ctx, VLM |

## 2. Vero (Lean 4) scoreboard

| Model | primepy (9 specs) | Note |
|---|---|---|
| google/gemini-3.8-flash | **2/9** then **11/11 bankledger PERFECT** | Also 1/19 munkres, 0/15 toposort, first TB3 solve (1.0). Vero/proof champion on bank-type instances. |
| tencent/hy4-preview | **4/9** primepy, **0/11** bankledger | Wins small-arithmetic instances, zero on ledger-style. Instance × model is decisive. |

### Measured envelopes
- **gemini-3.8-flash closes instances with <20 specs**: 11/11 (11 specs), 1/19, 2/9 — but 0/20, 0/23, 0/26, 0/27, 0/40, 0/43, 0/53. Size beats type as the predictor.
- **OpenRouter burn at full fleet ≈ $100/h**: $200 lasted ~2h (gemini TB3-70 + ~15 probes). Paid runs need per-trial budgets or they 402 mid-wave.
| deepseek/deepseek-v4-pro-0813 | **4/9** primepy | bankledger running. |
| tencent/hy4-preview | running | |
| z-ai/glm-5.3 | running | |
| meta/muse-spark-1.3-contributor | 0/9 | Never committed to writing. |
| z-ai/glm-5.3 | 0/9 primepy (9/9 attempts failed) | Maximum engagement, zero correctness here — but solved TB4 coq-block-bound. Instance × model again. |
| z-ai/glm-5.3-flash | 0/9 (2 attempts) | Engager class. |
| qwen/qwen3.8-flash | 0/9 | Ghost class (never writes). |
| minimax/minimax-m3:free | 0/9 | Two stacked issues: 400 on native FC (auto-fallback latches XML correctly) AND "tool call result does not follow tool call (2013)" — a history-pairing error the format flip cannot fix. Dead lane. |
| z-ai/glm-5.2:free | 429 persistent | Free-tier upstream congestion. Dead lane. |
| 27B locals (unc-kt, uncensored) | 0/173 across 8 instances | Reads forever, writes nothing. Not a Lean model. |

## 3. Harness lessons encoded in 0.7.2

- **Verifier detection is per-ecosystem.** Lean repos verify with
  `lake build`, never `cargo_check`. The stale-verification rescue now
  detects `lakefile.toml`/`lakefile.lean` and `lake build` is a first-class
  verification prefix. (gemini probe: the gate ran cargo_check on a Lean
  project and reported nonsense failures.)
- **Tag-free output**: qwen3 reasoning parser can return the whole answer
  as `reasoning_content` (empty `content`) — the agent promotes reasoning
  to content (`assistant_response.rs`).
- **Per-endpoint timeouts**: `ModelProfile.max_retries`,
  `response_timeout_floor_secs`, `agent.stream_stall_timeout_secs`.
- **Vision works only when the schema doesn't ask the model to guess
  infrastructure**: `vision_analyze` requires only `prompt`; endpoint/model
  inject from the vision profile.
- **Slop gate**: diffs touching verifier regions (tests/CI/runners) fail
  completion (`VerifierTainted`).

## 4. Endpoint compat traps (all measured)

- **Native FC support varies per model**: 400 "Provider returned error" on
  m3:free with `native_function_calling = true`. Hosted flagships (GLM,
  Gemini) want native; sglang locals want XML. TODO: auto-fallback on
  tool-schema 400s.
- **Free tiers rate-limit upstream** (`z-ai/glm-5.2:free` 429 persistently,
  even staggered). Don't schedule work on them; keep them as fallbacks only.
- **KV-pool arithmetic is the real scheduler**: streams × context ≤ pool,
  or everything wedges silently (queue hangs look like dead endpoints).
  270336 fit 4 × 64k + 1 spare; 598016 fits 8 × 64k + change.
- **Harbor kills the whole job on one GPU task** — always launch waves with
  the non-GPU task list (`/tmp/tb3_nongpu_tasks.txt`).
- **4× timeout multiplier on 8h-base tasks = 32h zombie trials.** Multiply
  deliberately, not reflexively.
- **Two `harbor run`s on the same dataset at the same second starve each
  other** (dataset lock). Launch waves sequentially.
- **ngrok free tier**: fine for interactive + ~3 streams; 6+ streams of agent
  traffic 400s within an hour.
- **Environment-layer ceiling ≈ 12 concurrent docker trials per host**: the
  -n 24 LAN wave (measured 2026-09-04) produced 57 RuntimeError + 4
  env-start-timeouts out of 70 — docker/build contention, not KV or model.
  Right-size waves at -n 8–12 (also the measured 78.8 t/s aggregate point).

## 5. What works (do more of)

- **Write→build→fix cadence** (gemini's win): models that verify after
  nearly every write convert; models that read for 50 steps convert nothing.
  The harness should push this (write-early directive + verify-after-write).
- **Uncensored locals for adversarial generation**: 500+ attack cases, 29
  gate holes closed. Hosted models refuse this work by policy.
- **Free fleet for iteration, paid only for final scoring**: $85/day saved
  at OpenRouter rates and rising.

## 6. What doesn't (stop doing)

- 27B local models on Lean/proof tasks (0/173 measured).
- 27B local models as TB solvers (0 wins in 30+ trials across two models).
- Vero probes sharing a single config path (parallel writes corrupted it —
  now content-addressed per model).
- `harbor run` waves against a wedged endpoint: trials die at iteration 0
  and the data looks like model failure. Probe generations, not just
  `/v1/models`, before launching.
