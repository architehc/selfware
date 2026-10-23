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

pub use cards::{find_card, RecipeCard, CARDS};
pub use chat::{retrieve_bundled_snippet, retrieve_snippet};
pub use check::{BootCheck, BootCheckStatus};
pub use model::{
    boot_dir, model_path_in, sha256_file, verify_model_file, MODEL_FILE_NAME, MODEL_SHA256,
    MODEL_SIZE_BYTES, MODEL_URL, SERVER_ALIAS, SERVER_PORT,
};
pub use wizard::{BootIo, DetectedModel, ScriptedIo, StdinIo, WizardPlan};
