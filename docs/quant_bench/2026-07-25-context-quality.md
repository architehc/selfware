# Context Quality Bench — 2026-07-25

Judge model: `moonshotai/kimi-k3` (max_tokens=2048, temperature=0).
Artifacts: `/tmp/context-quality-bench/tiers/{map,lite}.txt` from `examples/tier_bench.rs --dump`.

## Per-tier score

| tier | correct |
|---|---|
| map | 5/5 |
| lite | 5/5 |

## Per-question results

| question | map | lite | latency |
|---|---|---|---|
| Which function resolves the auto context mode to a concrete tier? | CORRECT | CORRECT | map 19.849466s / lite 16.050533s |
| What type does the fit_tier function return? | CORRECT | CORRECT | map 21.617275s / lite 13.112150s |
| Which struct measures the real token cost of each context tier? | CORRECT | CORRECT | map 16.483678s / lite 20.485843s |
| Which module strips comments and cfg(test) blocks to reduce source? | CORRECT | CORRECT | map 7.683006s / lite 11.308667s |
| What does the Map tier emit for each component? | CORRECT | CORRECT | map 37.101410s / lite 38.038490s |

**Takeaway:** Map and Lite tiers answer equally well (5/5 each) — the cheaper tier preserves answer quality on this question set.
