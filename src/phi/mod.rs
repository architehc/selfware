//! Phi's observation layer.
//!
//! Status: observe-only. These modules record what the human/agent loop is
//! doing and report it. Nothing here influences agent behaviour, and nothing
//! should until recorded sessions have been evaluated.

pub mod ledger;

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod ledger_tests;
