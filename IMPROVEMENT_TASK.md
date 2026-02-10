The test coverage is below 90%. You need to:

1. Read the tarpaulin output to find uncovered lines
2. Add unit tests for uncovered functions in:
   - src/tools/file.rs
   - src/tools/git.rs  
   - src/agent/context.rs
   - src/safety.rs
3. Focus on error path testing (the '?' operator branches)
4. Add mock-based tests for API calls
5. Run cargo_tarpaulin again to verify >90%

Use cargo_test frequently to ensure tests pass.
