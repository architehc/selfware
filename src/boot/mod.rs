//! `selfware boot` — recovery/setup assistant.
//!
//! The critical path is DETERMINISTIC: configs come from recipe cards
//! ([`cards`]) with known-correct values, verified by `llm-doctor` after
//! writing. The tiny local model ([`model`], [`chat`]) is optional,
//! freeform-only, and never emits config that isn't from a card.

pub mod cards;
pub mod chat;
pub mod check;
pub mod endpoint_scout;
pub mod model;
pub mod wizard;

pub use cards::{find_card, RecipeCard, CARDS};
pub use chat::{retrieve_bundled_snippet, retrieve_snippet};
pub use check::{BootCheck, BootCheckStatus};
pub use endpoint_scout::{
    build_functiongemma_scout_prompt, candidate_endpoints_for_config,
    ensure_functiongemma_scout_server, is_endpoint_down_error, parse_functiongemma_selected_index,
    probe_openai_endpoint, rank_best_candidate_index, scout_and_select_endpoint,
    EndpointScoutReport, ScoutedEndpoint,
};
pub use model::{
    boot_dir, build_server_args_with_context, bundled_llama_server_candidates,
    bundled_model_candidates, current_platform_llama_runtime_spec, ensure_background_boot_server,
    ensure_background_boot_server_for_model, ensure_background_boot_server_for_model_with_context,
    ensure_llama_server, ensure_model_spec, find_boot_model_spec, find_bundled_model,
    find_llama_server, find_llama_server_in_path, is_boot_server_endpoint, is_boot_server_healthy,
    llama_runtime_spec_for, measure_memory_snapshot, model_path_in, pick_boot_model_for_memory,
    resolve_boot_model_context, sha256_file, shipped_boot_model_spec,
    shutdown_background_boot_server, spawn_server_with_context, verify_any_boot_model_file,
    verify_model_file, verify_model_file_spec, BootMemorySnapshot, BootModelSpec,
    LlamaServerRuntimeSpec, BOOT_MODELS, LFM3_1GB_MODEL_FILE_NAME, LFM3_1GB_MODEL_SHA256,
    LFM3_1GB_MODEL_SIZE_BYTES, LFM3_1GB_MODEL_URL, LFM3_CTX_128K, LFM3_CTX_256K, LFM3_CTX_64K,
    LFM3_SLQ_MODEL_FILE_NAME, LFM3_SLQ_MODEL_SHA256, LFM3_SLQ_MODEL_SIZE_BYTES, LFM3_SLQ_MODEL_URL,
    LFM3_SUPPORTED_CONTEXTS, LLAMA_SERVER_BIN_NAME, LLAMA_SERVER_RUNTIMES, MODEL_FILE_NAME,
    MODEL_SHA256, MODEL_SIZE_BYTES, MODEL_URL, SERVER_ALIAS, SERVER_PORT, SHIPPED_BOOT_MODEL_ID,
};
pub use wizard::{BootIo, DetectedModel, ScriptedIo, StdinIo, WizardPlan};
