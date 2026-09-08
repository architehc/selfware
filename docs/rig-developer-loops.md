# Rig developer loops — three local endpoints → one super harness

Date: 2026-09-08. Scope: how the rig (which this Mac cannot reach) should use
its three endpoints to improve selfware unattended, with every loop producing
a greppable record and every claim backed by a measurement. Grounded in the
measured facts in `docs/model-playbook.md`, `docs/loop-recipes.md`, the
Meta-Harness review (`docs/2026-08-24-meta-harness-review.md`) and the
existing loop scripts (`benchmarks/harbor/harness-search.sh`,
`scripts/redteam_gen.py`, `scripts/localhost_vllm_soak.sh`,
`scripts/pack_query.py`, `scripts/fleet_measure.py`).

## 0. The fleet as a capability funnel

| id | Endpoint (as described 2026-09-08) | Streams | Measured class | Role in the loops |
|---|---|---|---|---|
| **E1** | Qwen 27B NVFP4 "heretic" (abliterated), LAN `:8000` | 32 | 27B dense: ~11 t/s/stream at 8 concurrent, shared prefill 150–190 t/s; 0/173 proofs, 0/30 TB repair; read-only review and adversarial generation VALIDATED | **Breadth.** Generators, sensors, read-only reviewers, cheap second opinions. Never a solver, never a proposer. |
| **E2** | Qwen-Next uncensored, local | 8 | Flagship-adjacent, faster decode than 27B (MoE) | **Verify tier.** Confirms or refutes everything E1 emits; medium solver; a *different model family* from E1, so agreement means something. |
| **E3** | Qwen 1M-context uncensored (flash-next class) | 8 × 1M | Strongest local solver, VLM, whole-repo reviews at 700–800k packs | **Depth.** Whole-repo passes, harness-search proposer (reads raw traces), fixer in worktrees, final ranking. |

The shape is a funnel: **32 generate → 8 verify → 8 synthesize/fix**. The
stream ratio (4:1:1) matches the survival ratio of candidates through a
review or attack pipeline. Nothing reaches E3 that has not already been
checked by a second, different model. Nothing reaches a human that has not
been through E3 plus the compile/test gates.

Why this and not "use the biggest model for everything":

- 27B locals convert 0 solver tasks (0/173 proofs, 0/30 TB) but are perfect
  on read-only classification and attack generation (500+ cases, 29 gate
  holes). Use them for what they measured well at.
- The Meta-Harness ablation: scores+summaries scored *below* scores-only;
  raw traces won by 15 points. Whatever proposes or fixes must read the raw
  logs itself. Only E3 has the context to do that at 82 files per iteration.
- Two same-family models agreeing is weak evidence; E1 → E2 disagreement is
  the cheapest false-positive filter the rig has.

## 1. Rules every loop obeys

1. **Probe generations before a wave.** `/v1/models` lies; a wedged endpoint
   fails trials at iteration 0 and the data looks like model failure.
   `scripts/fleet_probe.py` writes `fleet.json`; every loop reads it to size
   concurrency and to skip a dead endpoint.
2. **KV-pool arithmetic is the scheduler.** `streams × context ≤ pool` or
   everything wedges silently. E1 at 32 streams only works if its pool holds
   32 × the per-shard context you give it. Measure the pool once, then cap
   `context_length` per profile so the product fits. Record both numbers in
   `fleet.json`.
3. **Raw traces, never summaries**, are the input to a proposer or fixer.
   A loop may *index* traces (paths, grep hits, sizes); it may not summarize
   them for the next stage.
4. **Temperature 0 and k ≥ 2 trials** before any before/after claim (the
   measured 25/27 vs 19/27 swing on identical configs at temp 1.0).
5. **Gates by construction.** A candidate that fails `cargo fmt --check`,
   `cargo clippy --all-targets -- -D warnings`, its targeted tests, or
   `cargo test --test redteam_gate_test` never enters an archive. Red CI is a
   stop signal (AGENTS.md rule 1).
6. **Every finding carries `file:line` and a reproduce command**; every fix
   ships a regression test and a class sweep (AGENTS.md rule 5). A finding
   without a repro is a hypothesis and goes back to the verify stage.
7. **Append-only JSONL archives per loop**, one record per unit of work,
   including failures (`selfware_rev`, endpoint id, config sha, tokens,
   wall time, outcome). The next loop greps the previous loop's archive.
8. **Writers run in worktrees; readers run in the checkout.** Cap writers
   (worktree builds + docker trials) at 12 per host (measured env ceiling).
9. **Search/test split.** Any loop that tunes the harness on tasks holds out
   a test slice and regex-audits its output for task strings.

## 2. The loops

Each loop: what goes in, which endpoint, verifier, what comes out, and the
failure mode the loop is watching for in itself.

### L0 — Fleet probe (every 15 min, and at the start of every wave)

- **Endpoint:** all three. **Script:** `scripts/fleet_probe.py`.
- Per endpoint: `/v1/models`, then a 64-token streamed generation (time to
  first token, decode t/s), then N parallel probes at the declared stream
  count to catch queue wedges (aggregate t/s, error count).
- **Out:** `~/selfdev/fleet.json` (`ok`, `ttft_ms`, `tps`, `streams_ok`,
  `recommended_streams`) plus a TSV row appended to
  `~/selfdev/measure_log.tsv` (same table `fleet_measure.py` already writes).
- Every other loop reads `recommended_streams` instead of hard-coding 32/8/8.

### L1 — Red-team generation (continuous background on E1)

- **In:** attack classes in `scripts/redteam_gen.py`. **Endpoint:** E1 at
  `recommended_streams` (up to 32; today the script defaults to 8 on the
  flash endpoint, so point it at E1 with `--endpoint --streams`).
- **Verifier:** shape validation → dedup → `cargo test --test
  redteam_gate_test`. The gate is the oracle; the model only generates.
- **Out:** corpus JSONL + the gate's ALLOWED list.
- **Triage stage (new):** every ALLOWED case becomes an E3 job: minimal
  repro, proposed checker change, regression test, in a worktree, through
  gates, onto a branch `redteam/<class>-<hash>`. A human merges. Today this
  triage is manual and the generation rate at 32 streams will outrun it.
- **Watch for:** corpus integrity (the recent "corpus integrity reset"
  commit): dedup by canonical JSON, not raw text, and refuse to append if the
  gate test is already red.

### L2 — Review funnel (E1 shards → E2 verify → E3 rank and sweep)

This is the F1–F9 review series done by machines, with the same discipline.

1. **Shard** the repo by evolve cluster (`src/evolve/clusters.rs` gives the
   taxonomy; `graph_summary`/`hotspots` tools give per-cluster context packs).
   One read-only `selfware run` per shard on E1, `-m yolo`, task text that
   `task_policy::task_is_read_only` classifies as read-only ("Review …, do
   not edit files"), `--output-format json`. 32 shards ≈ one wave.
   Multi-chat fan-out agents have no tools, so shards must be `selfware run`
   processes, not `multi-chat` streams.
2. **Findings schema** (one JSONL line each, the model is told the schema and
   the shard's file list):
   `{"file","line","claim","failure_scenario","repro","severity","shard","endpoint":"e1","rev"}`.
   Lines that fail to parse or lack `file:line` are dropped and counted.
3. **Verify** on E2: each finding becomes an adversarial job ("here is a
   claim, prove it wrong; read the code; run the repro if it is a command").
   Verdict schema: `{"finding_id","verdict":"CONFIRMED|REFUTED|UNCLEAR","evidence"}`.
   UNCLEAR goes to a second E2 stream once; still UNCLEAR is dropped and
   logged. Expect the CONFIRMED rate to be well under half; that is the
   filter working.
4. **Rank and sweep** on E3: CONFIRMED findings, plus the raw E1 and E2
   transcripts (not summaries), plus a 1M pack of the affected clusters.
   E3 ranks by severity, groups by root cause, and for each group runs the
   AGENTS.md rule 5 sweep: grep every sibling call site and add the misses
   as new findings tagged `sweep_of:<id>`.
5. **Out:** `~/selfdev/review/<date>/findings.jsonl`, `verdicts.jsonl`,
   `ranked.md`. `ranked.md` is the input to L5.
- **Watch for:** self-agreement. Never let E1 verify E1. Track the
  CONFIRMED/REFUTED ratio per shard over time; a shard whose ratio jumps
  toward 1.0 is a prompt that started leading the witness.

### L3 — Whole-repo 1M pass (E3, weekly or on demand)

- **In:** an evolve context pack (700–800k tokens, `scripts/pack_query.py`).
- **Question set:** cross-module invariants that no shard can see: "every
  Command spawn scrubs credentials", "every path that reaches the model
  passes the trust gate", "every truncate helper has the semantics its
  callers assume". These are exactly the misses PUNCHLIST.md records.
- **Out:** findings in the L2 schema with `endpoint:"e3"`, fed into L2's
  verify stage (E2 checks E3 too; the 1M pass is not exempt).
- **Watch for:** the pack exceeding the KV pool; pack size goes in the record.

### L4 — Harness search (Meta-Harness outer loop, local edition)

`benchmarks/harbor/harness-search.sh` exists and runs against OpenRouter.
The rig version changes four things:

1. **Proposer = E3**, reading the archive's raw traces directly (median 82
   files per iteration fits in 1M). `proposer.toml` gets a local endpoint
   profile; no paid calls in the search loop. Paid models only score the
   final candidate on the held-out slice.
2. **k = 2 trials per candidate at temperature 0**; the record stores both
   rewards, and `mean_reward` is the mean over trials, not over tasks with
   `None` counted as zero.
3. **Search/test split** fixed in the script (`SEARCH_TASKS` vs
   `TEST_TASKS`), plus a leakage audit: grep the proposal for any task name,
   verifier string, or expected value; reject on hit.
4. **Pareto over reward × tokens**, not reward alone. Two candidates with
   equal reward and a 30% token difference are not equal.
- **Subject** for solving is E3; E1 runs the same candidates on the
  read-only task class (F) as a cheap regression sensor, never as the
  scored subject.
- **Mutable surface** expands from config-only to a `harness.d/` of prompt
  and gate-policy files, so a candidate never needs a Rust rebuild.

### L5 — Fix loop (E3 in worktrees → gates → cross-model diff review)

- **In:** one ranked group from L2/L3 (or one ALLOWED case from L1).
- One E3 `selfware run -m yolo` per group in its own `git worktree`,
  task text: fix, regression test, class sweep, no test weakening (the
  `VerifierTainted` slop gate already refuses diffs that touch verifier
  regions on a non-test task).
- **Gates** in the worktree: fmt, clippy `-D warnings`, the targeted test
  file, `redteam_gate_test`, then a full `cargo test` before the branch is
  pushed. Any red result: the worktree is kept for the trace, the branch is
  not created, the record says which gate.
- **Second opinion** on E2 (different family): review the diff against the
  finding; verdict schema as in L2. A REFUTED diff goes back to E3 once with
  the E2 transcript attached raw.
- **Out:** branch `fix/<finding-id>`, record with gate results, tokens and
  wall time. A human merges. Cap at 12 concurrent worktrees per host.
- **Watch for:** "fixes" that widen `allowed_paths`, delete assertions, or
  lower thresholds (AGENTS.md rule 2). Grep the diff for those before the
  E2 review; refuse automatically.

### L6 — Nightly soak (E1, 8 h)

`scripts/localhost_vllm_soak.sh` as is, pointed at E1 with the
`step_timeout_secs = 2400`, `stream_stall_timeout_secs = 1200` profile.
Its six read-only tasks are already the right shape. Add: run on the
binary built from the day's merged fixes, and write the health snapshots to
`~/selfdev/` so L0's TSV and the soak log share a timeline.

## 3. Capacity plan (fill in from fleet.json)

| Endpoint | Streams | Reserved | Allocation |
|---|---|---|---|
| E1 (32) | 32 | 0 | L2 review waves in bursts (32 shards, ~45–60 min at 11 t/s with 8k-token answers); L1 fills every idle stream; L6 overnight |
| E2 (8) | 8 | 1 (interactive) | L2/L3/L5 verify queue, FIFO, 7 workers |
| E3 (8) | 8 | 2 (interactive + one 1M pack) | 6 workers shared by L4 proposer, L5 fixers, L1 triage; L3 uses the reserved pack slot |

Two measurements decide whether 32 on E1 is real: the KV pool size, and
aggregate t/s at 8/16/24/32 parallel probes (the LAN box measured a docker
ceiling at 24, and 78.8 t/s aggregate at 8–12). If aggregate t/s stops
rising past 16, 32 streams is queue depth, not throughput; size L2 waves at
the knee.

## 4. One config, three profiles

```toml
# ~/selfware-rig.toml — default is E3 (depth); loops pick a profile with --model
endpoint = "http://<e3-host>:<port>/v1"          # direct LAN, not the ngrok host (3–6 stream cap)
model = "qwen38-flash-next"
max_tokens = 16384
temperature = 0.0
context_length = 1000000

[agent]
max_iterations = 100
step_timeout_secs = 900
stream_stall_timeout_secs = 900
native_function_calling = false      # sglang locals want XML

[models.e1]                          # 27B heretic, LAN :8000, 32 streams
endpoint = "http://<lan-host>:8000/v1"
model = "<27b-model-id>"
max_tokens = 8192
temperature = 0.0
context_length = 65536               # keep 32 × this ≤ KV pool
native_function_calling = false
max_retries = 3
response_timeout_floor_secs = 2400

[models.e2]                          # Qwen-Next uncensored, 8 streams
endpoint = "http://127.0.0.1:31000/v1"
model = "<next-model-id>"
max_tokens = 16384
temperature = 0.0
context_length = 65536               # 598,016-token pool fits 8 × 64k
native_function_calling = false
response_timeout_floor_secs = 2400

[models.e3]                          # 1M, 8 streams; same as the top-level default
endpoint = "http://<e3-host>:<port>/v1"
model = "qwen38-flash-next"
max_tokens = 16384
temperature = 0.0
context_length = 1000000
modalities = ["text", "vision"]
```

`selfware --config ~/selfware-rig.toml --model e1 -p "…"` selects a profile.
Three caveats from the review in §8: `context_length` must be explicit on
every profile (built-in profile matching misses local and provider-prefixed
ids); the per-profile `max_retries` / `response_timeout_floor_secs` lines
are inert today; and budgets must go on the command line
(`--max-wall-secs`, `--max-cost-usd`) because the CLI overwrites TOML
budgets with `None`. Per-profile `step_timeout_secs` does not exist yet; the
27B needs 2400 while E3 wants 900, so until it does, keep two config files or
run E1 jobs with `SELFWARE_TIMEOUT` set. That gap is worth closing first
(see §6).

## 5. Smallest honest starting point (first week)

1. **Day 1:** `scripts/fleet_probe.py` against all three; record the KV
   pools and the t/s knee for E1. Decide E1's real wave size.
2. **Day 1:** run L2 once on 8 shards (not 32) end to end: E1 → E2 → E3.
   Count findings, CONFIRMED rate, wall time, tokens. That single number
   set is the baseline every later prompt change must beat.
3. **Day 2:** L5 on the top three CONFIRMED groups. Measure gate pass rate
   and how many diffs E2 refutes. If E2 refutes more than half, the fixer
   prompt is the bottleneck, not the reviewers.
4. **Day 3:** move L1 to E1 at the measured wave size; add the E3 triage
   stage for ALLOWED cases.
5. **Day 4–5:** L4 with the local proposer, k = 2, split, leakage audit;
   seed the archive with the current profile so the first iteration has a
   parent with traces.
6. **Every day:** L0 on a timer, L6 overnight, `selfdev_stats.py` for the
   cost-equivalent line so paid usage stays at final scoring only.

Every loop lands as one script under `scripts/` or `benchmarks/`, reads
`fleet.json`, writes one JSONL archive, and refuses to run when the gate
tests are already red.

## 6. Harness gaps the loops expose (fix in selfware first)

- Per-profile `step_timeout_secs` / `stream_stall_timeout_secs` (E1 and E3
  need different values under one config).
- A `--findings-schema` style structured output for read-only runs, so L2
  does not depend on the model formatting JSONL by hand.
- `harness-search.sh`: local proposer profile, k trials, split + leakage
  audit, Pareto column, `mean_reward` that does not count `None` as zero.
- `multi-chat` streams get no tools; parallel `selfware run` is the fan-out
  primitive for now. A tool-bearing fan-out would make L2 one command.

## 7. Not verifiable from this machine

E1's KV pool and t/s at 32 streams, whether E2 is the `:31000` box from the
playbook, and E3's direct LAN address. All four are inputs to §3 and §4 and
come out of the first `fleet_probe.py` run.

## 8. Review findings (2026-09-08, max effort, 51 verifier verdicts)

Scope: `benchmarks/harbor`, `src/swl/runtime`, `src/evolve/{loop,gate,apply}.rs`,
`src/agent/{loop_control,task_runner,verification,recovery}.rs`,
`src/api/client.rs`, `src/config`, `scripts/redteam_gen.py`,
`scripts/localhost_vllm_soak.sh`. Ranked most severe first. The "hits"
column names the loop above that the bug undermines.

| # | Sev | Where | Finding | Hits |
|---|---|---|---|---|
| 1 | high | `src/cli/mod.rs:655` | After `Config::load` the CLI assigns `max_budget_tokens`/`max_wall_secs`/`max_cost_usd` from the clap `Option` unconditionally, so any TOML budget (harbor's `max_wall_secs = 14400`, `max_cost_usd = 40.0`) becomes `None` unless the matching `--max-*` flag is passed. Reproduced: TOML `max_wall_secs = 1` alone ran until an external kill; with `--max-wall-secs 1` it stopped in 2 s. Only `max_turns` has the `if let Some` guard. | L4, L5, every TOML-budgeted run |
| 2 | high | `src/config/unpack.rs:710` | `save_unpack_config`, `auto-config --save` and all four harbor TOMLs emit the pre-red-team 3-entry `denied_paths`; an explicit key replaces the 10-entry serde default, so `.env.production`, bare `secrets/`, `db.env`, `.git/config` and the other 7 red-team patterns are readable in yolo mode under those configs. | L1 (gate work silently undone), L4 |
| 3 | high | `src/agent/tool_dispatch/helpers.rs:965` | The F2 shell tokenizer treats the `&` in `2>&1` as a `;` connector, so `cargo test 2>&1` (the house idiom) is never credited as verification; the completion gate then refuses every final answer and the progress nudge says "PASSED" at the same time. Livelock until `max_iterations`. | L4 subject, L5 fixer |
| 4 | high | `src/agent/verification.rs:1298` | The "you have not written ANY files" gate scans `self.messages` for `file_edit`/`file_write`, but compression rewrites that history (keeps last 6 / 3 messages) and `has_written_any_file` / the checkpoint ledger are ignored. Long tasks get rejected after their edits are compressed away; edits via `patch_apply`/`file_multi_edit`/`file_fim_edit` never count. | L4, L5 |
| 5 | high | `src/config/model_profiles.rs:118` | Built-in profiles are anchored globs (`qwen3.6-*`, `claude-*`), and `match_profile` sees the raw id, so `qwen/qwen3.6-27b` (OpenRouter, LM Studio) never matches; the unknown-model fallback forces 32k context and the derived conversation window collapses to 2,048 tokens with only a warning. Mirror: a bare id matches a profile carrying no context data and keeps a 1M budget on a 131k server. | Every profile in §4: set `context_length` explicitly, always |
| 6 | med | `src/api/client.rs:347` | `wall_budget_start` is latched once per `ApiClient` and never reset per task, while the agent resets its clock per task; a multi-task session fails every request after the first task's budget elapses, and a mid-run client rebuild resets both the anchor and the native-FC latch. | L2 shards if run multi-task, TUI sessions |
| 7 | med | `src/agent/recovery.rs:103` | `strip_think_blocks` early-returns only the text after the first closed `<think>` block, dropping answer text before it and leaking later blocks raw. The stripped text is the final answer stored and matched. | L2 findings JSONL, L4 proposer output |
| 8 | med | `src/agent/tool_dispatch/mod.rs:489` | `written_paths` counts only `file_edit`/`file_write`; best-snapshot restore therefore reverts a subset of the files and logs a green state that never existed; runs that edit solely via `patch_apply`/multi/fim have no snapshot at all. | L5 |
| 9 | med | `src/api/client.rs:348` | Unchecked `Instant + Duration`; validation accepts `max_wall_secs = u64::MAX` (panic on first call), `stream_stall_timeout_secs = 0` (every stream times out, double billing), `retry.max_retries = u32::MAX` (zero attempts in release, overflow panic in debug). The proposer skill lists these knobs as mutable. | L4 (a proposal can crash the subject) |
| 10 | med | `src/agent/recovery.rs:387` | `&error[..error.len().min(200)]` byte-slices inside `warn!()`; a multibyte character at byte 200 (box-drawing, em-dash, CJK stderr) panics the main task and aborts the process when WARN logging is on. | L4, L5, L6 |
| 11 | med | `src/api/client.rs:841` | Error-status body reads are bare awaits on a client with no read timeout; a proxy that sends 429/5xx headers then stalls hangs headless runs forever. The non-streaming loop also sleeps a Retry-After past the wall deadline and then bills one more request. | E3 via ngrok, the LAN box that drops 5×/day |
| 12 | med | `src/config/model.rs:178` | Per-profile `max_retries` and `response_timeout_floor_secs` are consumed only by `chat_with_profile`, which has no production caller; both knobs are inert on the streaming and non-streaming paths. | §4 config: those two lines do nothing today |
| 13 | med | `src/agent/verification.rs:1381` | `leak_check_done = true` is stored before hits are computed, so only the first gate-reaching snapshot is scanned; a later rebuild that embeds a census identifier passes. | L4 subject (TB leak class) |
| 14 | low | `src/swl/runtime/mod.rs:458` | F7's step resolver rejects `parallel:`/`guard:` steps the validator accepts; seven shipped workflows fail through the library API before any agent runs (CLI path lowers differently and is unaffected). | SWL-driven loops |
| 15 | low | `src/swl/runtime/guarded.rs:254` | `GuardedSwlRuntime` keeps pre-F7 copies of the strategies: steps ignored, reducer output dropped, outputs never written to the context so content guardrails are inert, guardrails registered twice per run. | SWL-driven loops |

Consequences for the plan above:

- §4: `context_length` must be explicit on every profile (finding 5), the
  per-profile retry/timeout lines are inert until finding 12 is fixed, and
  any budget must be passed on the command line, not in TOML (finding 1).
- §2 L4: the proposer's mutable-knob list needs bounds (finding 9) and
  `harness-search.sh` must pass `--max-wall-secs`/`--max-cost-usd`
  explicitly (finding 1).
- §2 L1: the harbor and unpack TOMLs need the full `denied_paths` list or,
  better, a union with the default (finding 2) before the next corpus wave
  is trusted on a harbor run.
- §2 L5: the first three fix groups are findings 1, 3 and 2; each is a
  one-file change with a reproduce command already in hand.
