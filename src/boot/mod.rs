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
    boot_dir, bundled_llama_server_candidates, bundled_model_candidates,
    ensure_background_boot_server, ensure_background_boot_server_for_model, ensure_model_spec,
    find_boot_model_spec, find_bundled_model, find_llama_server, find_llama_server_in_path,
    is_boot_server_endpoint, is_boot_server_healthy, model_path_in, pick_boot_model_for_memory,
    sha256_file, shutdown_background_boot_server, verify_any_boot_model_file, verify_model_file,
    verify_model_file_spec, BootModelSpec, BOOT_MODELS, LFM3_1GB_MODEL_FILE_NAME,
    LFM3_1GB_MODEL_SHA256, LFM3_1GB_MODEL_SIZE_BYTES, LFM3_1GB_MODEL_URL, LFM3_SLQ_MODEL_FILE_NAME,
    LFM3_SLQ_MODEL_SHA256, LFM3_SLQ_MODEL_SIZE_BYTES, LFM3_SLQ_MODEL_URL, LLAMA_SERVER_BIN_NAME,
    MODEL_FILE_NAME, MODEL_SHA256, MODEL_SIZE_BYTES, MODEL_URL, SERVER_ALIAS, SERVER_PORT,
};
pub use wizard::{BootIo, DetectedModel, ScriptedIo, StdinIo, WizardPlan};
