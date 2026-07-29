# Debloat State (token baseline)

> Baseline measured 2026-07-27 (post waves 1-4, −6,027 source lines landed).
> Update this file after every debloat round; the goal is trend, not perfection.
> Numbers are **measured tokens** (`token_count::estimate_content_tokens`),
> not heuristics (AGENTS.md rule 4).

## Totals

| Metric | Baseline | Previous (2026-07-25) | Delta |
|---|---|---|---|
| Full (all Code nodes) | **953,601** | 1,359,882 | −406k total — **under 1M** (ops/presentation tooling-classified) |
| Lite (skeletons/verbatim) | 158,316 | 172,243 | −8% vs first measurement |
| Compact | ~1.04M | 1,099,370 | — |
| Map (component cards) | ~24.5k | 26,949 | — |
| Production files | 342 in tiers (376 total incl. tooling) | 380 | bench/vlm → Auxiliary |
| Source lines (repo) | ~205k | ~211k | −6,027 (waves 1-4) |

Lite density: 16.6% of full. Map density: ~1.7% (16.3k). Full product logic fits a 1M window with ~46k headroom; the 0.70 auto-fit still picks Lite at 1M (Full at 0.95+ fit_ratio).

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
| bench_harness + vlm_bench excluded from production tiers (they're tooling, not product code) | ~~−77k full, −10k lite~~ | [done] 2026-07-28 — measured −80.7k/−10.3k |
| swl/ classified as tooling (kept for CLI/orchestration, excluded from tiers) | ~~−46k full~~ | [done] 2026-07-28 — measured −45.7k/−9.3k lite |
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
