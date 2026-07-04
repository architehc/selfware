//! Testing module
//!
//! This module contains testing and verification functionality including:
//! - API testing
//! - Contract testing
//! - Code verification
//! - Code review
//! - Mock LLM API server (test-only)

pub mod api_testing;
pub mod code_review;
pub mod contract_testing;
pub mod language_qa;
#[cfg(test)]
pub mod mock_api;
pub mod qa_profiles;
pub mod verification;
pub mod visual_verification;
