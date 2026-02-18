# Selfware Experience Improvement Backlog

Use this backlog to prioritize improvements discovered during long-run sessions.

## P0 - Reliability

1. Add explicit `--resume-last` to quickly continue interrupted long-run tasks.
2. Persist per-task timing and token metrics in machine-readable JSON (not only console logs).
3. Add a built-in long-run mode with automatic phase checkpoints and periodic health pings.

## P1 - Swarm Experience

1. Integrate swarm/multi-agent execution into the primary `chat` loop, not as a separate mode only.
2. Stream real agent events into TUI dashboard by default (tool calls, retries, recovery steps).
3. Add role-level summaries at the end of each swarm task (architect/coder/tester/reviewer outcomes).

## P1 - Qwen-like CLI Flow

1. Provide a minimal prompt profile with less decorative language for coding-heavy sessions.
2. Improve slash-command discoverability and keep command names consistent between TUI and basic chat.
3. Add a native `--tasks-file` option for scripted batches without external shell loops.

## P2 - Operator Feedback

1. Show compact per-step outcome lines: action, file/tool, duration, status.
2. Add a built-in run comparison command (current vs previous run metrics).
3. Add suggested next actions when recovery happens repeatedly.
