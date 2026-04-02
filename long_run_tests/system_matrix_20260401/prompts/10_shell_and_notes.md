Use `shell_exec` early to run `cargo test`, then use file tools to fix the code.

After the crate is green:

- add at least 3 extra edge-case tests
- rerun `cargo test`
- write concise notes to `RUN_NOTES.md`

Completion gate for yourself:

- tests green
- extra tests added
- notes updated
