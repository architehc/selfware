# Context-selection benchmark for coding tasks

A standalone experiment that measures **how to build an LLM context for a coding
task under a fixed context-window budget**, and how that trade-off changes as the
budget grows. The corpus is selfware's own source tree (`src/**/*.rs`), and the
"gold" context for each task is the set of files the corresponding fix actually
touches (mined from `IMPROVE_03_CONTEXT_MEMORY.md`) — so the benchmark is
self-labeling against the codebase.

It does **not** modify the core agent. It reuses selfware's own primitives:

| Primitive | Source | Role here |
|-----------|--------|-----------|
| `analysis::bm25::BM25Index` | `src/analysis/bm25.rs` | cross-file relevance ranking |
| `token_count::estimate_content_tokens` | `src/token_count.rs` | the same token accounting the agent uses |
| `agent::context_map::extract_rust_skeleton` | `src/agent/context_map.rs` | L2 signature view + item line boundaries for excerpts |

All model calls go through the configured model only — **GLM-5.2 via OpenRouter**
(`selfware.toml`), no fallback.

## The context-building function

`build_context(query, budget, strategy, breadth_frac)` ranks the corpus for `query`
(BM25), then packs it into `budget` tokens under one of five strategies:

- **`full-bm25`** — whole files at full fidelity in rank order until the budget is
  spent. Max detail, min breadth.
- **`hybrid`** (skeleton-then-full) — top files full; when the next file doesn't fit
  full, fall back to its skeleton. Big full files can still exhaust the budget first.
- **`skeleton`** — skeletons only. Max breadth, no implementation detail.
- **`reserve`** — cap depth (full files) at `(1 - breadth_frac)` of the budget, then
  guarantee the remaining `breadth_frac` for skeletons of further files. Fixes the
  breadth starvation `hybrid` suffers at tight budgets.
- **`excerpt`** — **extract the most interesting parts of each file automatically**:
  carve the file into function/impl/struct blocks at skeleton item boundaries, score
  each block by query term-frequency, and pack only the top blocks (with
  `// ... N lines elided ...` markers). Real code, but far cheaper than whole files.

## Coding use cases (gold sets)

Ten real coding problems mined from `IMPROVE_03_CONTEXT_MEMORY.md`, each mapped to
the file(s) a correct fix touches — e.g. *"fix the O(N²) eviction in `add_message`"*
→ `src/memory/mod.rs`; *"integrate semantic RAG into `ContextMap` file ranking"* →
`src/agent/context_map.rs` + `src/cognitive/rag.rs`. See `problems()` in `main.rs`.

## Metrics (per problem × budget × strategy)

- **recall_any** — gold file present at any level (did we surface the right file?)
- **recall_full** — gold file present at **full** fidelity
- **recall_code** — gold file present with real code (**full or excerpt**)
- **goldCode%** — real-code tokens of gold included ÷ gold's full-token mass
  (credits excerpts proportionally — the key metric for `excerpt`)
- **precision** — fraction of selected files that are gold (noise measure)
- **utilization** — `tokens_used / budget`

## Running

```bash
cargo build --release --features context-select --bin context_select_bench

# Retrieval metrics across budgets (cheap, deterministic, no API calls)
./target/release/context_select_bench \
    --budgets 8000,32000,128000,1048576 \
    --breadth-frac 0.35 \
    --json experiments/context_select/results/retrieval.json

# End-to-end: incrementally bigger context requests to GLM-5.2.
# Sweeps the first N problems across ascending budgets and reports pass@budget.
# Costs API tokens, so it's opt-in.
source ~/.openrouter_env
./target/release/context_select_bench \
    --e2e 4 --e2e-budgets 8000,32000,128000 --e2e-strategy excerpt
```

## What the retrieval sweep shows (aggregate, mean over 10 problems)

| strategy  | 8K | 32K | 128K | 1M | metric |
|-----------|----|-----|------|----|--------|
| full-bm25 | 0.00 | 0.10 | 0.32 | **0.87** | recall_code |
| skeleton  | 0.23 | 0.32 | 0.87 | 1.00 | recall_any (recall_code = 0) |
| reserve   | 0.00 | 0.32 | 0.32 | 1.00 | recall_any |
| excerpt   | 0.10 | 0.10 | 0.32 | **1.00** | recall_code (goldCode% 83%) |

- **`full-bm25` plateaus at ~0.87 even at 1M** — the full selfware source is larger
  than a 1M-token window, so low-ranked gold files never fit at full fidelity. Bigger
  windows do not remove the need for selection.
- **`skeleton` reaches full breadth** (recall_any → 1.0) but carries no code
  (`recall_code = 0`): it finds the file but not the fix detail.
- **`reserve` guarantees breadth**: at 32K it lifts recall_any from `hybrid`'s 0.10 to
  0.32, and at 1M reaches recall_any 1.00 while still bringing real code for 52% of
  gold files — the breadth quota is doing its job.
- **`excerpt` is the standout**: at 1M it hits **recall_code 1.00 / goldCode% 83%** —
  real code for *every* gold file, covering 83% of the needed code, where whole-file
  packing tops out at 0.87. Compact extraction wins exactly where full fidelity can't.

## End-to-end sweep (GLM-5.2, `excerpt`, 4 problems)

```
Pass@budget (located a gold file), mean over 4 problems:
   8000 tok   2/4   50%
  32000 tok   1/4   25%   (one call hit a transient API error, counted as miss)
 128000 tok   2/4   50%
```

**This is a deliberately honest negative result, and it teaches something.** The
`gold in ctx` counter is 0/n at 8K–32K almost everywhere, yet GLM-5.2 still names the
right file — it infers selfware's conventional layout (`src/<area>/mod.rs`) from its
own priors, *not* from what retrieval placed in the window. So **file-naming is too
easy to be a context-length signal**; the pass@budget curve is dominated by model
priors + noise, which is why it is non-monotonic.

Takeaways:
1. **Trust the retrieval metrics** (`recall_code`, `goldCode%`) to compare context
   builders — they scale cleanly with budget and strategy. Treat the file-naming e2e
   as a loose sanity check, not a scoring axis.
2. **A context-sensitive e2e needs an in-file task** the model cannot guess — e.g.
   "quote the exact function signature and line you would change" or "produce the
   unified diff." That is the natural next extension of `run_e2e`.

## Files

- `main.rs` — corpus builder, `build_context`, the five strategies, excerpt
  extraction (`extract_relevant_spans`), metrics, the problem set, and the GLM-5.2
  end-to-end sweep.
- `results/` — JSON output from `--json`.
