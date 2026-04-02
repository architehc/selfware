# 122B System Test Matrix

This matrix runs 10 parallel Selfware headless sessions against the configured 122B endpoint.

Each scenario gets:

- an isolated copy of the `guided_scheduler_lab` sandbox
- a distinct prompt variant
- its own `run.log`, `exit_code.txt`, and post-run sandbox state

Use:

- `./long_run_tests/system_matrix_20260401/run_matrix.sh`
- `./long_run_tests/system_matrix_20260401/summarize_matrix.sh`

The goal is not only to see whether tasks complete, but also whether Selfware:

- keeps acting beyond early iterations
- avoids invalid or unavailable tools
- makes durable edits
- verifies with `cargo test`
- updates `RUN_NOTES.md`
