# Model & Endpoint Playbook

Measured facts from the September 2026 endpoint program. Every entry is
something we learned by running, not by reading marketing pages. Update it
when a measurement changes.

## 1. The fleet

| Endpoint | Model | Role | Measured |
|---|---|---|---|
| `localhost:31000` | qwen38-unc-kt (NVFP4 abliterated KT) | Red-team slow sensor, TB sensor, VLM fallback | 598,016-token KV pool; ~11 t/s per stream at 8 concurrent, 150–190 t/s shared prefill; 65k-context step can take 30 min — needs `step_timeout_secs = 2400`, `stream_stall_timeout_secs = 1200` |
| `192.168.137.1:8000` | qwen38-uncensored | Attack generation (the best), TB sensor | 12 × 262K pool; drops ~5×/day — **opportunistic only, never the critical path**; `step_timeout_secs = 2400`, stall 1200 |
| `llm.selfware.design` | qwen38-flash-next (`dealignai/Qwen3.8-Flash-Next-CYBERSECURITY-BF16PLE` — dealigned cybersecurity fine-tune, served name `qwen38-flash-next`) | Strongest local solver, VLM, 1M reviews | profile defaults measured 2026-09-24: `context_length = 163840` (largest real agent prompt 127k; TTFT 13 s at 99k vs 40 s at 257k, decode falls to 17 tok/s; 823,538 tokens remain demonstrable on 2026-09-15 routing), `max_tokens = 24576` (only max_tokens bounds reasoning; longest real need 13.4k), `agent.max_call_secs = 1628` (a full 24,576-token call at the slowest measured whole-call rate, 15.1 tok/s with prefill — val083; the former 600 s assumed ~42 tok/s and killed a legitimate 701 s, 13,799-token turn under load; max_tokens bounds runaways); concurrency pins `max_streams = 8` (server `max_running_requests = 8`) and `max_global = 16` (8 permits left for tool execution); **ngrok free tier caps ~3–6 agent streams** (400s under waves); stall 900. *Note: server `/get_server_info` reports model_path `dealignai/Qwen3.8-Flash-Next-CYBERSECURITY-BF16PLE` (not stock Qwen); benchmarks and red-team evaluations reflect this dealigned fine-tune.* |
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

## 7. llm.selfware.design quotas (qwen38 profile, measured 2026-09-27)

Single source of truth: `src/config/model_profiles.rs`
(`qwen38_defaults_profile`, `QWEN38_WORKLOAD_QUOTAS`, `QWEN38_MEASURED`).
`selfware llm-doctor` prints the active table (Step 0b) and every run
summary lists turns per workload with the quota each ran under. Harness:
`scripts/endpoint_quota_bench.py` (streaming only; `/tokenize` counts
reasoning tokens exactly, since SGLang reports `usage.reasoning_tokens = 0`
for this model). Server at measurement: SGLang 0.5.9,
`max_running_requests = 8`, `chunked_prefill_size = 8192`, radix cache
enabled but no measurable reuse (see below).

### Per-turn quota table

| Workload (turn kind) | enable_thinking | max_tokens | Why (measured) |
|---|---|---|---|
| planning — first call of a task | **off** | **12,288** | opened with a tool call 12/12 off vs 10/12 on (the 2 misses answered a citation task "from memory", 71–96 s); on cost 13–828 reasoning tokens per plan. 12,288 covers the largest visible output in 2,142 recorded turns (7,788 tokens) |
| mechanical — after a read-only tool batch (a review's reading phase) | on | session (24,576) | replays favoured off, live reviews did not — see "Live runs"; opt in with `[workloads.mechanical] enable_thinking = false` |
| edit — after an edit/write/shell/check/test batch | on | session | 4/4 vs 4/4 equivalent replies, median 300 reasoning tokens — no evidence to take thinking from code edits |
| synthesis — the review's synthesis phase (`Agent::review_phase`), a turn after a gate/correction directive, or not after a tool batch | on | **16,384** | longest successful answer/report 14,105 tokens (recorded answer turns p99 10,431, max 12,362 of 287); a live synthesis spent ~21k reasoning tokens on an EMPTY answer under 24,576 — the cap ends such a runaway ~8k tokens sooner and the step-down retry answers with thinking off |

Escalation: a thinking-off turn (planning, or mechanical when opted in)
whose reply carries no tool call is about to become the final answer. The
reply is discarded and the same request is re-sent once under the
`synthesis` quota (a `workload_escalated` turn decision; the run summary
counts it). A live review with thinking-off read turns and no escalation
ended on the fragment ", not part of the file content" as its "answer".

Reasoning-budget step-down: qwen38's profile records `reasoning_effort` as
not honored (`reasoning_effort_honored = false`), so a turn that spends its
whole budget on hidden reasoning is retried once with thinking OFF, not at
a "lower" effort. With the old `reasoning_effort = "xhigh"` pin the retry
went to "medium" — which the model ignores — and could burn the budget
again (a live slugify review spent ~22 min in one synthesis turn: ~80k
characters of reasoning, an empty answer, then a retry "at lower reasoning
effort").

Precedence per field (most specific first): `[workloads.<kind>]` in your
config → a user `extra_body` pin (`enable_thinking` under
`chat_template_kwargs` holds for **every** turn, so the profile's per-turn
toggle is not applied; an explicit top-level `max_tokens` likewise) → the
profile table. `llm-doctor` and the run summary print a `!` line naming any
profile quota an explicit setting overrode. Per-turn thinking is sent as
`chat_template_kwargs.enable_thinking` (the rendered history is unchanged;
only the generation prompt differs).

### Turn replay (recorded agent request bodies, `max_tokens` 24,576, 2 reps)

| Kind | Mode | n | completion median / max | reasoning median / max | generation s median | total s median / max | valid |
|---|---|---|---|---|---|---|---|
| mechanical (34–59k prompts) | on | 6 | 175 / 337 | 97 / 294 | 13.5 | 20.2 / 39 | 6/6 |
| mechanical | off | 6 | 57 / 94 | 0 | 1.6 | 10.2 / 27 | 6/6 |
| planning (6 first turns, 4.5–23k) | on | 12 | 79 / 1,224 | 39 / 828 | 2.5 | 6.4 / 96 | 10/12 tool-first |
| planning | off | 12 | 47 / 183 | 0 | 2.0 | 4.3 / 73 | 12/12 tool-first |
| edit (44–50k) | on | 4 | 523 / 650 | 300 / 595 | 19.6 | 23.8 / 32 | 4/4 same call as off |
| edit | off | 4 | 147 / 295 | 0 | 6.7 | 11.4 / 32 | 4/4 |

Final-report decision points — 7 recorded turns where the original run
wrote its review (34–119k prompts), 2 reps per mode:

| Mode | wrote the report | kept reading | malformed call | reasoning tokens | total s per turn |
|---|---|---|---|---|---|
| on | 5/14 | 9/14 | 0 | 54–10,754 | 18–606 |
| off | 2/14 | 11/14 | 1/14 | 0 | 3–223 |

Thinking on reaches "write it now" sooner, and pays for every decision:
three replays spent 4,535–10,754 reasoning tokens (3.5–10 min) and then
read more anyway. Thinking off keeps reading at ~25 s a turn and writes
later. When a report was written, citation accuracy held: off 58/64 and
60/69 `file:line` citations resolve (one 88k-prompt audit, 148 s and 223 s)
vs on 48/53 and 34/36 on the same audit (301 s, 252 s); the other
thinking-on reports resolved 41/41, 35/36 and 26/26 (268–606 s).
Tool-call validity across all continuation replays: off 17/18, on 18/18.
On replays alone, thinking off looked right for read turns; the live runs
below did not confirm it, so read turns keep thinking on by default.

Greeting ("hi" after the real 21k-token system prompt, 3 reps): thinking
on 56–80 reasoning tokens, ~9 s; off ~1.7 s ("Hi! I'm ready to help…" or a
`context_status` call). With the escalation a tool-less greeting is
re-asked with thinking on, so a greeting costs ~1.7 s more than before —
the planning gain is on tasks, not chat.

### Live runs (slugify checkout, `run -m yolo`, 2026-09-27)

One sample per row — high variance: the two thinking-on baselines differ by
more than 3.7×. Reasoning tokens are estimated from the session log's
reasoning characters at the measured 3.85 characters/token. "Before" =
v0.9.3 with the tracked config (`reasoning_effort = "xhigh"` pinned,
thinking on every turn); the final row runs this change rebased on the
review coverage gate (inventory, `review_phase`).

| Run | Table | Wall s | Turns | Tokens | Reasoning (est.) | Outcome |
|---|---|---|---|---|---|---|
| review before 1 | all on | 968 | 14 | 268,834 | ~33.8k | report, 4 findings, 15 citations, 0 wrong |
| review before 2 | all on | 3,600 (limit) | 38 | 946,166 | ~53.0k | no answer — killed at the limit; slowest call 871 s |
| review, read turns off, no escalation | mech off | 288 | 18 | 287,341 | ~6.8k | "answer" was a 6-word thinking-off fragment, 0 citations |
| review, read turns off + escalation | mech off | 2,151 | 49 | 1,837,161 | ~20.8k | weak report (16 citations, 1 wrong); a thinking-off turn FIM-edited `slugify.py` mid-review (see below); ~10 min lost to stream errors caused by this session's own 350k/500k probe |
| review, read + planning off + escalation | mech/plan off | 1,200 | 34 | 1,025,620 | ~31.0k | report, 13 citations, 0 wrong; 5 of 20 read turns escalated (three 1-character replies) |
| **review, final table + coverage gate** | plan off, synthesis 16,384 | **910** | **9** | **236,314** | **~21.5k** | report, 15 citations, 0 wrong; coverage 10/10 relevant files; no file touched |
| edit before 1 | all on | 1,327 | 62 | 2,829,507 | ~34.0k | code correct (all tests pass) but run FAILED: `UNATTRIBUTED_FAILURE_LOOP` on a malformed verification command (`python3 pytest …`) |
| edit, read turns off | mech off | 337 | 29 | 697,210 | ~2.3k | completed, but `max_words` works only on `algorithm='modern'`; the default path silently ignores it |
| edit, read turns off + escalation | mech off | 279 | 19 | 312,595 | ~0.7k | correct on both paths, tests pass |
| **edit, final table** | plan off, synthesis 16,384 | **286** | **26** | **497,685** | **~4.6k** | correct on both paths, tests pass |

The edit speedups are not attributable to the quotas alone: the baseline
lost most of its time to the verification loop. The final review is one
sample; it also ran with the coverage gate, which changed how the run read
(9 turns reading several files per call).

Two harness defects surfaced (not quota issues; reported, not fixed here):
`file_read` output showed the line `tokens = text.split(DEFAULT_SEPARATOR)`
as `env_token=[REDACTED]` (the secret scrubber matched `tokens =`, and ate
the newline) and HTML-escaped `&`/`<` (`&amp;`, `&lt;`). Both baseline and
changed runs saw them; in one run a model "repaired" the redacted line and
dropped the `&` from `MODERN_HEX_PATTERN`, and the test suite did not
catch it.

### Thinking-budget controls (one reasoning-heavy prompt, 2 reps each)

No budget knob is honored. `thinking_budget = 128` and
`max_thinking_tokens = 128`, in `chat_template_kwargs` or at the top level,
produced 1,373–8,192 reasoning tokens (two hit the 8,192 cap).
`reasoning_effort` low vs xhigh overlap (643–8,192 vs 7,072–8,192), the same
spread as no setting (3,244–8,192). Only `enable_thinking = false` (0
reasoning tokens, answer still correct 2/2) and `max_tokens` bound
reasoning. The tracked `selfware-llm-selfware-design.toml` therefore no
longer pins `reasoning_effort = "xhigh"`.

### Context size (synthetic source prompts, 400-token answer, thinking off; measured while three other streams of this session ran — idle-server rows below)

| Prompt tokens | TTFT cold s | TTFT repeat s | prefill tok/s | decode tok/s |
|---|---|---|---|---|
| 23,114 | 4.5 | 3.5 | 5.2–6.7k | 40.9 |
| 48,282 | 6.4 | 6.5 | 7.4–7.6k | 40.5 |
| 77,329 | 10.0 | 10.2 | 7.6–7.8k | 38.7 |
| 108,672 | 14.0 | 14.3 | 7.6–7.8k | 38.1 |
| 124,172 | 16.3 | 19.4 | 6.4–7.6k | 37.6 |
| 148,890 | 20.8 | 25.5 | 5.8–7.2k | 36.4 |
| 170,122 | 23.2 | 27.0 | 6.3–7.3k | 35.7 |
| 208,405 | 28.2 | 33.8 | 6.2–7.4k | 34.2 |

Idle server (one stream): 51,592 → TTFT 6.9 / 6.5 s, decode 55 tok/s;
105,977 → 13.4 / 15.0 s, 42–51 tok/s; 157,853 → 21.7 / 28.2 s, 36–47 tok/s.

Prefill is linear with no knee, and an identical repeat is no faster: no
prefix-cache reuse, so **every turn pays its whole prompt** (~13–14 s per
100k tokens). The 163,840 window stays; the per-turn cost is governed by
the compaction point.

### Endpoint-level quotas

| Quota | Value | Basis |
|---|---|---|
| `context_length` | 163,840 | table above; largest real agent prompt 127k (2026-09-24) |
| `max_tokens` (reserved output) | 24,576 | longest real completion 13,799 (val083), 14,105 on replay |
| `agent.max_call_secs` | 1,628 | 24,576 tokens at the slowest whole-call rate, 15.1 tok/s (val083); scales with a larger max_tokens, including a `[workloads]` one |
| history budget (`max_context_tokens`) | 106,496 | 163,840 − 24,576 − 20% margin (32,768) |
| `agent.context_content_ratio` (compaction point) | **0.80** → 85,196 | history budget − p99 per-turn prompt growth (21,221; p50 962, p90 5,658, max 41,677 over 1,206 growing steps, val082–val090): compaction starts as late as one p99 step allows without overshooting the hard budget. Was the global 0.75 (79,872). |

Also measured, not changed: the 20% safety margin (32,768 tokens) is far
above the estimator's measured error — for histories ≥ 20k estimated
tokens the server's prompt count exceeded selfware's estimate by at most
3,877 tokens (p99 3,500, n = 858), and for ≥ 80k it was always below the
estimate (server/estimate p99 0.992, n = 154). Shrinking it would grow every
prompt (and its prefill) — a separate decision.

### Server limits and the 60-second first-byte cut (2026-09-27)

`/v1/models` and `/get_server_info` report a 1,000,000-token context;
the KV pool (`max_total_num_tokens`, shared by all 8 slots) is 735,153
tokens; `/tokenize` reports `max_model_len` 262,144 (the tokenizer config,
not the serving limit — 823,538 tokens were accepted on 2026-09-15). A
claimed "743,908" was not observed.

Streaming requests whose first byte takes more than ~60 s are cut by the
gateway (`Response ended prematurely` at 62–63 s): SGLang sends nothing
until prefill completes. A cold 367k-token prompt and a 500k-token prompt
(twice) were cut at 62–63 s with 3 other streams running; the 367k repeat
then answered with a 4.6 s TTFT (prefix reused from the cut request, which
the server finished) but decoded at 8.2 tok/s. At the measured single-stream
prefill rate a cold prompt above ~430–470k tokens cannot reach its first
byte inside 60 s even on an idle server, and under load the ceiling is
lower. These large-prompt probes also broke two live streams of this
session's own review run (stream errors at the same minutes) — do not
repeat them on the shared endpoint during other work.

So the "350–500k review context" proposal fails on this endpoint: 500k
never returned; 350k returned only warm; and without prefix reuse every
turn of a review would pay ~48–68 s of prefill (a 100-turn review at an
average 300k prompt spends ~68 min in prefill alone, against ~12 s per
turn at the 85k compaction point). The window stays 163,840.

Decode depends on prompt size and load. On an otherwise idle server
(one stream, thinking off): 63.2–63.8 tok/s at a 30-token prompt (3
reps), 54–55 at 52k, 42–51 at 106k, 36–47 at 158k; with three of this
session's other streams running, 41 at 23k down to 34 at 208k (table
above); under the heavy shared load recorded in val083, 15–20 tok/s whole
call. `agent.max_call_secs` stays sized for the slowest (15.1).

Hypotheses from an external analysis, tested: "~58 tok/s decode" — true
only for short prompts on an idle server (63.5 at 30 tokens, 55 at 52k);
agent turns carry 20–120k prompts on a shared server (34–51 measured,
15–20 under load). "~140k prefill in 5–8 s" — measured 20.8 s at 148,890
tokens and 21.7 s at 157,853 idle (no prefix reuse). "Mapping turns at
4,096 max_tokens" — the largest visible output of a turn is 7,788 tokens
and a thinking-on report turn needed 14,105, so a 4,096 cap would cut a
read turn that becomes the answer; read turns keep the session cap.
"Compaction at 120k" — above the 106,496-token history budget this window
leaves. "Planning: reduced thinking" — no budget knob exists, so planning
is thinking OFF (measured above). "Chat/greetings thinking off, 2–4k
output" — a greeting is a planning turn (off, 12,288); its tool-less reply
is re-asked with thinking on by the escalation, measured above. "Edit
turns at 8–16k output" — recorded edit turns needed up to 16,535
completion tokens (p99 13,799), so edits keep the 24,576 session cap.
"Review output 16–32k" — synthesis is capped at 16,384 (longest successful
report 14,105; a runaway at ~21k reasoning tokens produced nothing).

The "compaction at N" on the status bar, `/ctx`, `/stats` and the `/compact`
target now all read the compressor's own threshold
(`Agent::compaction_threshold`); they used to show the hard trim budget
(`max_context_tokens`), which on a 1M window read "compaction at 796k"
while compaction started at 597k.
