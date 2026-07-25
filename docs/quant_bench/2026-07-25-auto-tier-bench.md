# Auto-Tier Benchmark — 2026-07-25

Measured on this repo (`selfware`, 376 production files) with
`cargo run --release --example tier_bench`. Graph scan (~7-10 s) excluded from
per-tier timings.

## Per-tier measurement (performance + token accuracy)

| tier | measured tokens | time | io reads | heuristic tokens | measured/heuristic |
|---|---|---|---|---|---|
| full_extended | 2,841,587 | 0.0 ms | 0 | 2,841,588 | 1.00x |
| full | 1,352,203 | 0.0 ms | 0 | 1,352,203 | 1.00x |
| compact | 1,092,973 | 555 ms | 376 | 1,108,820 | 0.99x |
| lite | 171,498 | 87 ms | 376 | 243,396 | **0.70x** |
| map | 26,773 | 24 ms | 0 | – | – |

Takeaways:

- Full/FullExtended come from scan-time counts — zero I/O, zero measurable time.
- The old 0.18 signature fraction **overestimates Lite by 42%** (243k vs 171k
  measured). Auto-tier decisions now use the measured value, so a 262k-window
  model correctly gets Lite (the heuristic alone would have wrongly rejected it:
  243k > 180k usable, but 171k fits).
- Compact's 0.82 fraction is accurate (0.99x) — comments really are ~18% here.

## Ladder resolution per model window (`auto`)

| window | usable budget | resolved | measured | fits | time | io reads |
|---|---|---|---|---|---|---|
| 8,192 | 4,300 | map | 26,773 | **false** (warning path) | 57 ms | 752 |
| 32,768 | 20,070 | map | 26,773 | **false** (warning path) | 57 ms | 752 |
| 131,072 | 88,883 | map | 26,773 | true | 59 ms | 752 |
| 262,144 | 180,633 | lite | 171,498 | true | 45 ms | 752 |
| 1,048,576 | 731,136 | lite | 171,498 | true | 45 ms | 752 |
| 3,000,000 | 2,097,152 | full | 1,352,203 | true | 0.0 ms | **0** |
| 4,500,000 | 3,147,152 | full_extended | 2,841,587 | true | 0.0 ms | **0** |

Takeaways:

- Resolution costs <60 ms worst case on this repo, and **zero I/O** once a
  scan-counted tier (Full/FullExtended) fits — the lazy short-circuit working.
- On a 1M-window model this repo caps at Lite: Full (1.35M) exceeds the 731k
  usable budget. The map→lite flip sits between 128k and 262k windows.
- Below ~40k windows even Map (27k) exceeds the budget — the `fits: false`
  warning path, never silent truncation.
