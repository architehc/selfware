# Project E2E CLI Tests

This suite runs end-to-end tests of `selfware` as a user would, using a local OpenAI-compatible model endpoint.

## Scenarios

| Name | Difficulty | Description | Bugs |
|---|---|---|---|
| `easy_calculator` | Easy | Fix a calculator library | 3 bugs: wrong multiply, div-by-zero, broken pow |
| `easy_string_ops` | Easy | Fix string utilities | 4 bugs: byte reverse, off-by-one truncate, title case, word count |
| `medium_json_merge` | Medium | Fix recursive JSON merge | 1 bug: shallow merge instead of recursive |
| `medium_bitset` | Medium | Fix a BitSet data structure | 4 bugs: shift overflow, inverted clear, wrong union op, skip(1) |
| `hard_scheduler` | Hard | Fix a scheduling crate | 3 bugs: missing day unit, no whitespace trim, overflow |
| `hard_event_bus` | Hard | Fix a multi-file event bus | 5 bugs across 3 files: Display, prefix match, seq, count |
| `swarm_session` | Swarm | Multi-agent coordination | Tests agent spawning and inter-agent commands |

## What it does

- Builds `selfware` with `--all-features` in release mode.
- Creates fresh test projects from templates in `work/`.
- Runs `selfware` headless (`-p`) on each project with YOLO mode (`-y`).
- Captures ANSI terminal output via `script` for screenshots.
- Validates each project via `cargo test -q` before and after agent execution.
- Runs one `multi-chat` swarm interaction scenario.
- Produces scored report + raw logs under `reports/<timestamp>/`.

## Scoring

Each coding scenario is scored 0-100:
- **70 points**: Tests pass after agent run
- **20 points**: Tests were broken before and fixed by agent
- **10 points**: Agent exited cleanly (no timeout, exit 0)

## Prerequisites

- Local model API reachable at `http://localhost:8000/v1/models`.
- `timeout`, `cargo`, and Rust toolchain installed.

## Run

```bash
./system_tests/projecte2e/run_projecte2e.sh
```

## Outputs

- Latest report symlink: `system_tests/projecte2e/reports/latest`
- Summary markdown: `reports/<timestamp>/summary.md`
- Raw scenario logs: `reports/<timestamp>/logs/`
- Terminal screenshots: `reports/<timestamp>/screenshots/`
- Scenario workdirs after run: `work/`
