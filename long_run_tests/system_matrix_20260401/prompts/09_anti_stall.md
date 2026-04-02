This is a long-running autonomous task. Keep making concrete progress and do not stall on repeated invalid tool calls.

If a tool call is blocked or unavailable, switch strategy immediately.

Goal:

- fix the known failing tests
- run `cargo test`
- add 3 more edge-case tests
- run `cargo test` again
- update `RUN_NOTES.md`

Do not finish until all of that is done.
