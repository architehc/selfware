# Debloat State (token baseline)

> Baseline measured 2026-07-27 (post waves 1-4, −6,027 source lines landed).
> Update this file after every debloat round; the goal is trend, not perfection.
> Numbers are **measured tokens** (`token_count::estimate_content_tokens`),
> not heuristics (AGENTS.md rule 4).

## Totals

| Metric | Baseline | Previous (2026-07-25) | Delta |
|---|---|---|---|
| Full (all Code nodes) | 1,408,580 | 1,359,882 | +48,698 (multi-lang Code + new features) |
| Lite (skeletons/verbatim) | 225,094 | 172,243 | +52,851 (non-rs now shipped) |
| Compact | ~1.10M | 1,099,370 | — |
| Map (component cards) | ~27k | 26,949 | — |
| Production files | 376 | 380 | −4 |
| Source lines (repo) | ~205k | ~211k | −6,027 (waves 1-4) |

Lite density: 16.0% of full (84.0% reduction). Map density: ~1.9%.

## Per-module tokens (top 15 by full size)

| Module | Files | Full | Lite | Reduction |
|---|---|---|---|---|
| agent | 34 | 214,917 | 19,442 | 91.0% |
| tools | 44 | 155,819 | 22,746 | 85.4% |
| evolve | 39 | 146,377 | 52,914 | 63.9% |
| cognitive | 27 | 114,148 | 22,707 | 80.1% |
| ui | 34 | 99,003 | 16,305 | 83.5% |
| bench_harness | 24 | 61,730 | 7,297 | 88.2% |
| safety | 14 | 51,429 | 6,205 | 87.9% |
| cli | 4 | 48,110 | 2,983 | 93.8% |
| swl | 19 | 45,772 | 9,297 | 79.7% |
| config | 16 | 39,768 | 4,469 | 88.8% |
| analysis | 7 | 39,530 | 7,228 | 81.7% |
| orchestration | 14 | 39,488 | 4,898 | 87.6% |
| evolution | 8 | 33,631 | 4,207 | 87.5% |
| testing | 6 | 31,233 | 4,210 | 86.5% |
| api | 5 | 23,520 | 2,912 | 87.6% |

Full per-module table: `curl :7777/api/context/map` (evolve server) or
`cargo run --release --example tier_bench` for tier totals.

## Reduction potential (open)

| Item | Tokens at stake | Status |
|---|---|---|
| bench_harness + vlm_bench excluded from production tiers (they're tooling, not product code) | −77k full, −10k lite | [open] — classification change |
| swl/ (experimental) — exclude or lower to orchestration executor (CONSOLIDATION_PLAN #1) | −46k full | [open] — owner decision |
| tokens.rs dead subsystem | ~~1,489 src lines~~ | [done] 2026-07-28 |
| tier_allocator duplicate tier system | ~~1,200 lines~~ | [done] 2026-07-28 |
| Dead deps (lru, tokio-test) + ~30 dead items | ~~895 lines~~ | [done] 2026-07-28 |
| server.rs / app.js splits (seams mapped) | readability, not tokens | [open] — no urgency |
| edit_history 21 unwired API items | ~400 lines | [open] — under documented blanket |

## Method (for future baselines)

```bash
./target/release/selfware self-evolve -p 7777 -c selfware.toml &
curl -s http://127.0.0.1:7777/api/context/map | jq -r '.cards[] | [.component, .tokens, .lite_tokens] | @tsv'
# tier totals: cargo run --release --example tier_bench
```
