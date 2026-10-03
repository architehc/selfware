//! Hermetic integration tests that exercise public APIs and the real CLI.
//!
//! This target deliberately has no `required-features`, so default and
//! `extras` CI runs execute it. Endpoint-dependent integration tests remain in
//! `tests/integration/mod.rs` behind the `integration` feature.

#[path = "integration/cli_tests.rs"]
mod cli_tests;
#[path = "integration/errors_tests.rs"]
mod errors_tests;
#[path = "integration/live_context_tests.rs"]
mod live_context_tests;
#[path = "integration/supervision_tests.rs"]
mod supervision_tests;
