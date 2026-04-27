# Quant comparison — selfware agent vs Qwen3.6 quants

Each cell is the result of running selfware end-to-end against the
scenario: bug injected, agent given the prompt, then `cargo test`.
✓ = post-validator passed and pre-validator failed (real fix). ✗ = either
the bug wasn't actually breaking, or the agent didn't fix it.

## Speed

| Quant | Endpoint | Median tok/s |
|-------|----------|-------------:|
| `Qwen3.6-27B-HauhauCS-IQ2_M` | `http://127.0.0.1:8000/v1` | 56.7 |
| `Qwen3.6-27B-HauhauCS-IQ3_M` | `http://127.0.0.1:8000/v1` | 48.8 |
| `Qwen3.6-27B-HauhauCS-IQ3_XS` | `http://127.0.0.1:8000/v1` | 52.0 |
| `Qwen3.6-27B-HauhauCS-IQ4_XS` | `http://127.0.0.1:8000/v1` | 44.3 |
| `Qwen3.6-27B-HauhauCS-Q2_K_P` | `http://127.0.0.1:8000/v1` | 50.8 |
| `Qwen3.6-27B-HauhauCS-Q3_K_P` | `http://127.0.0.1:8000/v1` | 43.9 |
| `Qwen3.6-27B-HauhauCS-Q4_K_P` | `http://127.0.0.1:8000/v1` | 39.2 |
| `Qwen3.6-27B-HauhauCS-Q5_K_P` | `http://127.0.0.1:8000/v1` | 34.3 |
| `Qwen3.6-27B-HauhauCS-Q6_K_P` | `http://127.0.0.1:8000/v1` | 31.6 |
| `Qwen3.6-27B-HauhauCS-Q8_K_P` | `http://127.0.0.1:8000/v1` | 24.0 |
| `Qwen3.6-35B-A3B-Q3_K_XL` | `http://127.0.0.1:8000/v1` | 125.0 |

## Scenario pass matrix

| Quant | Total | easy_calculator | easy_string_ops | medium_bitset | medium_json_merge | actor_pdvr | hard_event_bus | hard_scheduler | unsafe_scanner | viz_ascii_table | viz_maze_gen | viz_svg_chart |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `Qwen3.6-27B-HauhauCS-IQ2_M` | 2/11 | ✓ | ✗ (29s) | ✓ | ✗ (12s) | ✗ (8s) | ✗ | ✗ (99s) | ✗ | ✗ | ✗ (12s) | ✗ |
| `Qwen3.6-27B-HauhauCS-IQ3_M` | 2/11 | ✓ (31s) | ✗ (8s) | ✗ (11s) | ✗ (8s) | ✗ (8s) | ✗ | ✓ (24s) | ✗ (8s) | ✗ (8s) | ✗ (8s) | ✗ (8s) |
| `Qwen3.6-27B-HauhauCS-IQ3_XS` | 1/11 | ✗ (8s) | ✗ | ✗ (8s) | ✓ (42s) | ✗ (8s) | ✗ (28s) | ✗ | ✗ (12s) | ✗ | ✗ | ✗ (11s) |
| `Qwen3.6-27B-HauhauCS-IQ4_XS` | 6/11 | ✓ (72s) | ✗ (42s) | ✓ | ✓ | ✗ | ✗ (8s) | ✓ | ✓ | ✗ | ✗ | ✓ (58s) |
| `Qwen3.6-27B-HauhauCS-Q2_K_P` | 5/11 | ✓ (11s) | ✗ | ✓ | ✓ | ✗ (12s) | ✗ | ✗ (41s) | ✓ (100s) | ✗ | ✗ (8s) | ✓ (18s) |
| `Qwen3.6-27B-HauhauCS-Q3_K_P` | 3/11 | ✗ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✓ (100s) | ✗ | ✗ | ✓ |
| `Qwen3.6-27B-HauhauCS-Q4_K_P` | 6/11 | ✓ (34s) | ✗ | ✓ | ✓ | ✗ | ✗ | ✓ | ✓ (100s) | ✗ | ✗ | ✓ (16s) |
| `Qwen3.6-27B-HauhauCS-Q5_K_P` | 4/11 | ✗ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✓ (25s) | ✗ | ✓ | ✓ |
| `Qwen3.6-27B-HauhauCS-Q6_K_P` | 1/11 | ✓ (51s) | ✗ (11s) | ✗ | ✗ (9s) | ✗ (8s) | ✗ | ✗ (12s) | ✗ | ✗ | ✗ | ✗ |
| `Qwen3.6-27B-HauhauCS-Q8_K_P` | 2/11 | ✓ | ✗ | ✗ | ✓ (41s) | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| `Qwen3.6-35B-A3B-Q3_K_XL` | 4/11 | ✓ (59s) | ✗ (8s) | ✓ (98s) | ✓ | ✗ | ✗ (99s) | ✗ (99s) | ✗ | ✗ (99s) | ✗ (99s) | ✓ (34s) |

## Per-quant detail

### `Qwen3.6-27B-HauhauCS-IQ2_M`

- Model: `qwen3.6-27b-iq2m`
- Total duration: 2260.2s
- Speed: 56.7 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | timeout | ✓ | 300.4 | ? | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | success | ✗ | 138.8 | 29 | error[E0432]: unresolved imports `easy_string_ops::reverse`, `easy_string_ops::title_case`, `easy_string_ops::truncate`, `easy_string_ops::word_count` |
| `medium_bitset` | ✓ | timeout | ✓ | 300.4 | ? | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | success | ✗ | 13.6 | 12 | test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | success | ✗ | 11.6 | 8 | test result: FAILED. 5 passed; 14 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_event_bus` | ✓ | timeout | ✗ | 300.4 | ? | test result: FAILED. 2 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | nonzero(1) | ✗ | 220.3 | 99 | error[E0432]: unresolved imports `hard_scheduler::next_run_at`, `hard_scheduler::parse_duration`, `hard_scheduler::should_run` |
| `unsafe_scanner` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 16 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.5 | ? | test result: FAILED. 9 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | success | ✗ | 53.3 | 12 | test result: FAILED. 9 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | timeout | ✗ | 300.2 | ? | test result: FAILED. 0 passed; 8 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-IQ3_M`

- Model: `qwen3.6-27b-iq3m`
- Total duration: 932.3s
- Speed: 48.8 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | success | ✓ | 109.0 | 31 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | success | ✗ | 24.5 | 8 | test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_bitset` | ✓ | success | ✗ | 25.2 | 11 | test result: FAILED. 11 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | success | ✗ | 8.7 | 8 | test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | success | ✗ | 12.1 | 8 | test result: FAILED. 5 passed; 14 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_event_bus` | ✓ | timeout | ✗ | 300.0 | ? | test result: FAILED. 2 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | success | ✓ | 265.3 | 24 | test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `unsafe_scanner` | ✓ | success | ✗ | 46.5 | 8 | test result: FAILED. 16 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | success | ✗ | 52.8 | 8 | test result: FAILED. 7 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | success | ✗ | 16.6 | 8 | test result: FAILED. 9 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | success | ✗ | 48.9 | 8 | test result: FAILED. 0 passed; 8 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-IQ3_XS`

- Model: `qwen3.6-27b-iq3xs`
- Total duration: 1770.6s
- Speed: 52.0 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | success | ✗ | 15.1 | 8 | test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | timeout | ✗ | 300.3 | ? | error[E0432]: unresolved imports `easy_string_ops::reverse`, `easy_string_ops::title_case`, `easy_string_ops::truncate`, `easy_string_ops::word_count` |
| `medium_bitset` | ✓ | success | ✗ | 11.6 | 8 | test result: FAILED. 11 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | success | ✓ | 205.7 | 42 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | success | ✗ | 14.1 | 8 | test result: FAILED. 5 passed; 14 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_event_bus` | ✓ | success | ✗ | 100.2 | 28 | test result: FAILED. 5 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | timeout | ✗ | 300.4 | ? | test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `unsafe_scanner` | ✓ | success | ✗ | 100.1 | 12 | test result: FAILED. 17 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 9 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | timeout | ✗ | 300.1 | ? | error[E0599]: no method named `unwrap` found for mutable reference `&mut grid::Cell` in the current scope |
| `viz_svg_chart` | ✓ | success | ✗ | 101.5 | 11 | test result: FAILED. 0 passed; 8 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-IQ4_XS`

- Model: `qwen3.6-27b-iq4xs`
- Total duration: 2816.7s
- Speed: 44.3 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | success | ✓ | 243.5 | 72 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | success | ✗ | 149.3 | 42 | error[E0432]: unresolved imports `easy_string_ops::reverse`, `easy_string_ops::title_case`, `easy_string_ops::truncate`, `easy_string_ops::word_count` |
| `medium_bitset` | ✓ | timeout | ✓ | 300.2 | ? | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | timeout | ✓ | 300.5 | ? | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | timeout | ✗ | 300.1 | ? | test result: FAILED. 5 passed; 14 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_event_bus` | ✓ | success | ✗ | 26.3 | 8 | test result: FAILED. 2 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | timeout | ✓ | 300.3 | ? | test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `unsafe_scanner` | ✓ | timeout | ✓ | 300.0 | ? | test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.2 | ? | test result: FAILED. 7 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | timeout | ✗ | 300.1 | ? | test result: FAILED. 9 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | success | ✓ | 272.0 | 58 | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-Q2_K_P`

- Model: `qwen3.6-27b-q2kp`
- Total duration: 2252.0s
- Speed: 50.8 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | success | ✓ | 68.5 | 11 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | timeout | ✗ | 300.2 | ? | error[E0432]: unresolved imports `easy_string_ops::reverse`, `easy_string_ops::title_case`, `easy_string_ops::truncate`, `easy_string_ops::word_count` |
| `medium_bitset` | ✓ | timeout | ✓ | 300.0 | ? | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | timeout | ✓ | 300.2 | ? | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | success | ✗ | 32.3 | 12 | error[E0432]: unresolved import `actor_pdvr::actor` |
| `hard_event_bus` | ✓ | timeout | ✗ | 300.4 | ? | test result: FAILED. 5 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | success | ✗ | 139.9 | 41 | error[E0432]: unresolved imports `hard_scheduler::next_run_at`, `hard_scheduler::parse_duration`, `hard_scheduler::should_run` |
| `unsafe_scanner` | ✓ | nonzero(1) | ✓ | 240.5 | 100 | test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.4 | ? | test result: FAILED. 9 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | success | ✗ | 18.2 | 8 | test result: FAILED. 9 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | success | ✓ | 229.3 | 18 | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-Q3_K_P`

- Model: `qwen3.6-27b-q3kp`
- Total duration: 3261.8s
- Speed: 43.9 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | timeout | ✗ | 300.2 | ? | test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | timeout | ✗ | 300.1 | ? | test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_bitset` | ✓ | timeout | ✓ | 300.4 | ? | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | timeout | ✗ | 300.4 | ? | test result: FAILED. 5 passed; 14 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_event_bus` | ✓ | timeout | ✗ | 300.4 | ? | test result: FAILED. 2 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `unsafe_scanner` | ✓ | nonzero(1) | ✓ | 235.5 | 100 | test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.0 | ? | test result: FAILED. 8 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 9 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | timeout | ✓ | 300.1 | ? | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-Q4_K_P`

- Model: `qwen3.6-27b-q4kp`
- Total duration: 2991.4s
- Speed: 39.2 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | success | ✓ | 176.4 | 34 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | timeout | ✗ | 300.3 | ? | error[E0432]: unresolved imports `easy_string_ops::reverse`, `easy_string_ops::title_case`, `easy_string_ops::truncate`, `easy_string_ops::word_count` |
| `medium_bitset` | ✓ | timeout | ✓ | 300.1 | ? | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | timeout | ✓ | 300.4 | ? | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 6 passed; 13 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_event_bus` | ✓ | timeout | ✗ | 300.1 | ? | test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | timeout | ✓ | 300.4 | ? | test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `unsafe_scanner` | ✓ | nonzero(1) | ✓ | 259.1 | 100 | test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 9 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | timeout | ✗ | 300.1 | ? | test result: FAILED. 13 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | success | ✓ | 128.3 | 16 | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-Q5_K_P`

- Model: `qwen3.6-27b-q5kp`
- Total duration: 3301.1s
- Speed: 34.3 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | timeout | ✗ | 300.5 | ? | test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | timeout | ✗ | 300.1 | ? | error[E0432]: unresolved imports `easy_string_ops::reverse`, `easy_string_ops::title_case`, `easy_string_ops::truncate`, `easy_string_ops::word_count` |
| `medium_bitset` | ✓ | timeout | ✓ | 300.4 | ? | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | timeout | ✗ | 300.1 | ? | test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | timeout | ✗ | 300.4 | ? | error[E0432]: unresolved import `actor_pdvr::actor` |
| `hard_event_bus` | ✓ | timeout | ✗ | 300.1 | ? | test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | timeout | ✗ | 300.0 | ? | error[E0432]: unresolved imports `hard_scheduler::next_run_at`, `hard_scheduler::parse_duration`, `hard_scheduler::should_run` |
| `unsafe_scanner` | ✓ | success | ✓ | 271.0 | 25 | test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 7 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | timeout | ✓ | 300.4 | ? | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | timeout | ✓ | 300.2 | ? | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-Q6_K_P`

- Model: `qwen3.6-27b-q6kp`
- Total duration: 2383.9s
- Speed: 31.6 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | success | ✓ | 241.4 | 51 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | success | ✗ | 85.4 | 11 | error[E0282]: type annotations needed |
| `medium_bitset` | ✓ | timeout | ✗ | 300.2 | ? | test result: FAILED. 11 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | success | ✗ | 96.8 | 9 | test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | success | ✗ | 93.2 | 8 | test result: FAILED. 5 passed; 14 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_event_bus` | ✓ | timeout | ✗ | 300.5 | ? | test result: FAILED. 2 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | success | ✗ | 35.9 | 12 | error[E0432]: unresolved imports `hard_scheduler::next_run_at`, `hard_scheduler::parse_duration`, `hard_scheduler::should_run` |
| `unsafe_scanner` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 16 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.2 | ? | test result: FAILED. 7 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | timeout | ✗ | 300.5 | ? | test result: FAILED. 9 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | timeout | ✗ | 300.4 | ? | test result: FAILED. 0 passed; 8 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-27B-HauhauCS-Q8_K_P`

- Model: `qwen3.6-27b-q8kp`
- Total duration: 3232.4s
- Speed: 24.0 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | timeout | ✓ | 300.1 | ? | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_bitset` | ✓ | timeout | ✗ | 300.4 | ? | test result: FAILED. 11 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | success | ✓ | 194.5 | 41 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 5 passed; 14 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_event_bus` | ✓ | timeout | ✗ | 300.5 | ? | test result: FAILED. 2 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | timeout | ✗ | 300.0 | ? | test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `unsafe_scanner` | ✓ | timeout | ✗ | 300.2 | ? | test result: FAILED. 16 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | timeout | ✗ | 300.1 | ? | test result: FAILED. 7 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | timeout | ✗ | 300.3 | ? | test result: FAILED. 9 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | timeout | ✗ | 300.0 | ? | test result: FAILED. 0 passed; 8 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

### `Qwen3.6-35B-A3B-Q3_K_XL`

- Model: `qwen3.6-35b-a3b`
- Total duration: 2062.3s
- Speed: 125.0 tok/s

| Scenario | Pre-fail | Agent exit | Post-pass | Wall (s) | Steps | Validator |
|---|---|---|---|---:|---:|---|
| `easy_calculator` | ✓ | success | ✓ | 94.2 | 59 | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `easy_string_ops` | ✓ | success | ✗ | 5.6 | 8 | test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_bitset` | ✓ | nonzero(1) | ✓ | 152.0 | 98 | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `medium_json_merge` | ✓ | timeout | ✓ | 300.2 | ? | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `actor_pdvr` | ✓ | timeout | ✗ | 300.4 | ? | error[E0432]: unresolved imports `crate::state::Phase`, `crate::state::PhaseOutcome`, `crate::state::StateMachine` |
| `hard_event_bus` | ✓ | nonzero(1) | ✗ | 158.0 | 99 | test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `hard_scheduler` | ✓ | nonzero(1) | ✗ | 140.7 | 99 | error[E0432]: unresolved imports `hard_scheduler::next_run_at`, `hard_scheduler::parse_duration`, `hard_scheduler::should_run` |
| `unsafe_scanner` | ✓ | timeout | ✗ | 300.5 | ? | test result: FAILED. 17 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_ascii_table` | ✓ | nonzero(1) | ✗ | 121.6 | 99 | test result: FAILED. 9 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_maze_gen` | ✓ | nonzero(1) | ✗ | 183.5 | 99 | test result: FAILED. 13 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| `viz_svg_chart` | ✓ | success | ✓ | 290.2 | 34 | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

