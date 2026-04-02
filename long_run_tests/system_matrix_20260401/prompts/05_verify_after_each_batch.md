You are fixing a small Rust library locally.

Rules:

- Start with `cargo test`.
- After every meaningful edit batch, run `cargo test` again.
- Prefer small, correct fixes over big refactors.
- Do not finish until all tests pass.
- After the existing failures are fixed, add at least 3 more edge-case tests and rerun `cargo test`.
- Update `RUN_NOTES.md` with bugs fixed, tests added, and remaining risks.
