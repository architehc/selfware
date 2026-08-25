# Review: Meta-Harness (arXiv 2603.28052) vs the selfware harness-evolution loop

Date: 2026-08-24. Reviewer: analysis of the full text + this repo's measured state.
Paper: Lee, Nair, Zhang, Lee, Khattab, Finn — *Meta-Harness: an outer-loop
system that searches over harness code for LLM applications* (Stanford/MIT/
KRAFTON, March 2026).

## What the paper claims

An agentic proposer (Claude Code + Opus 4.6) searches over **harness code**
(the code deciding what to store/retrieve/show the model) using a filesystem
of all prior candidates: source + scores + **full execution traces**. Results:
+7.7pts over ACE on online text classification with 4x less context; +4.7pts
avg across five held-out models on IMO-level retrieval-augmented math; on
TerminalBench-2 the discovered harness beats hand-engineered Terminus-KIRA
(76.4% vs 74.7% on Opus 4.6; #1 among Haiku 4.5 agents at 37.6%).

The load-bearing ablation (their Table 3): scores-only feedback reaches
34.6 median / 41.3 best; scores + LLM-generated summaries 34.9 / 38.7
(summaries HURT); full raw traces 50.0 / 56.7. **Compressed feedback loses to
raw traces, and summaries are worse than nothing.** Proposer reads a median of
82 files per iteration, ~40% of them raw execution traces.

## How today's selfware work maps

We spent the day doing Meta-Harness *manually* on Terminal-Bench 3.0:

| Paper component | Our equivalent (built 2026-08-23/24) |
|---|---|
| Evaluation on a contested benchmark | Harbor + TB 3.0 locally, `benchmarks/harbor/` |
| Execution traces per candidate | `jobs/<ts>/<task>__/agent/selfware.txt` + verifier stdout |
| Proposer inspecting traces | Me, reading those logs and forming causal hypotheses |
| Harness edits from diagnosis | Loops 5-10 (monologue cutoff, audit, census, firewall, pin-abort) |
| Interface validation | cargo gates + Harbor verifier |
| Pareto / candidate tracking | ad-hoc (mental), not a filesystem artifact |

The loop orderings matched the paper's observed proposer behavior closely —
e.g. the "confounded edit" isolation (the 25/27 vs 19/27 cargo swing traced to
temp-1.0 variance, then temperature pinned to 0.0) is exactly the behavior
their Appendix A highlights.

## Where we diverge — and lose

1. **The proposer reads summaries, not traces.** My grounded reviews and the
   three-model consult fed GLM-5.3/Fable/Qwen *compressed descriptions* of the
   failures. The paper's central result says that's the weak interface: their
   scores+summary ablation underperformed even scores-only. The traces existed
   on disk the whole time — the proposer should grep them directly.
2. **The outer loop is manual.** I was the proposer. It works, but it doesn't
   scale, doesn't run unattended, and its judgments aren't recorded as
   candidates-with-evidence in a filesystem archive.
3. **No candidate archive.** The paper's filesystem D (code + scores + traces
   per candidate, persisted, greppable) is what lets the proposer reference 20+
   prior candidates per step. Our jobs/ dirs have the raw material but no
   indexed candidate manifest (config diff + code diff + scores + trace path).
4. **No search/test split discipline.** We iterated ON the watched tasks
   (cargo-flight-dispatch et al.) — fine for harness debugging, but a harness
   tuned to a task's verifier is overfit; the paper holds out test sets and
   audits for task-string leakage into evolved harnesses.
5. **Single metric.** Rewards-only; the paper Pareto-tracks accuracy vs cost
   (we have per-task token/cost in the logs but don't frontier-track).

## Improvements list (ranked)

1. **Automated outer loop over harness candidates** (the paper itself): a
   proposer agent (selfware can play this role — dogfood) with a skill pointing
   at `harbor-agents/jobs/`; each candidate = a harness variant (config +
   prompt/code patch) evaluated by Harbor on a fixed search-set slice; every
   iteration appends `{candidate diff, scores, trace paths}` to the archive.
2. **Raw-trace access for the proposer** — no summaries. The proposer gets
   shell access to the jobs/ filesystem and reads traces itself (the paper:
   median 82 files/iteration). Our trust-gate + path scoping already make this
   safe to allow.
3. **Candidate manifest** (`candidates.jsonl`): id, parent, diff hash, per-task
   rewards, token/cost totals, trace paths. Pareto frontier over reward × cost.
4. **Search/test split + leakage audit**: fix a search subset (e.g. 12 tasks),
   hold the rest; regex-audit evolved harness text for task-specific strings.
5. **Temp-0 pinned configs for all benchmark runs** (done today in the harbor
   profiles) and k>1 trials per candidate before any before/after claim —
   measured need: 25/27 vs 19/27 swing on identical configs at temp 1.0.
6. **Harness-as-code surface for search**: today the mutable surface is
   selfware.toml + prompts. The paper searches single-file harness *programs*.
   Our equivalent: a `harness.d/` of overridable prompt/gate-policy files the
   proposer can edit without touching Rust (compile-free candidates =
   evaluation in minutes, not rebuilds).
7. **Keep the gates green by construction**: candidates that break
   fmt/clippy/tests never enter the archive (their interface-validation step).

## Smallest honest starting point

A `scripts/harness-search.sh` that: (1) runs the current harness on the search
slice via Harbor, (2) writes the candidate record to the archive, (3) launches
`selfware run` as the proposer with a skill file describing the archive layout
and the rules (read traces, propose one bounded diff, no test-set peeking),
(4) applies the proposal to a worktree, gates it, evaluates, repeats. That is
the paper's Algorithm 1 with selfware as both proposer and subject.
