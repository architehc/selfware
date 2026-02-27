//! Integration tests for selfware
//!
//! Run with: cargo test --features integration
//!
//! Configure with environment variables:
//!   SELFWARE_ENDPOINT - API endpoint (default: http://localhost:8888/v1)
//!   SELFWARE_MODEL - Model name (default: unsloth/Kimi-K2.5-GGUF)
//!   SELFWARE_TIMEOUT - Request timeout in seconds (default: 300, use 900 for slow models)
//!   SELFWARE_SKIP_SLOW - Set to "1" to skip slow tests
//!
//! For deep testing with slow local models:
//!   SELFWARE_TIMEOUT=900 cargo test --features integration deep_

mod cli_tests;
mod conversation_tests;
mod deep_tests;
mod e2e_tests;
mod errors_tests;
mod extended_e2e;
mod helpers;
#[cfg(not(target_os = "windows"))]
mod interactive_tests;
mod model_format_tests;
mod qwen3_tests;
mod supervision_tests;
mod test_agent_loop;
mod tool_tests;

pub use helpers::*;
