//! `selfware boot` — recovery/setup assistant.
//!
//! The critical path is DETERMINISTIC: configs come from recipe cards
//! ([`cards`]) with known-correct values, verified by `llm-doctor` after
//! writing. The tiny local model ([`model`], [`chat`]) is optional,
//! freeform-only, and never emits config that isn't from a card.

pub mod cards;
pub mod chat;
pub mod check;
pub mod model;
pub mod wizard;
