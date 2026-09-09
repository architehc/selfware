//! Recipe cards: known-correct config templates for the `selfware boot`
//! recovery assistant.
//!
//! A raw tiny model hallucinates configs (wrong endpoints, invalid TOML,
//! dangerous `allowed_paths = ["*"]`), so the boot path is DETERMINISTIC:
//! every config `boot` emits is rendered from one of these cards, never from
//! model output.

use anyhow::{bail, Result};

use crate::cli::init_wizard::toml_quote;

/// A known-good backend configuration, verified against a live endpoint.
pub struct RecipeCard {
    /// Lookup name (`selfware boot` recommends cards by this name).
    pub name: &'static str,
    /// OpenAI-compatible base URL (ends in `/v1`).
    pub endpoint: &'static str,
    /// Served model id. Empty means "detect from the server's `/v1/models`"
    /// (vLLM/SGLang/LM Studio serve whatever the user loaded).
    pub model: &'static str,
    /// Per-response output budget.
    pub max_tokens: usize,
    /// Context window. For server-detected cards this is the fallback used
    /// when the server does not report one.
    pub context_length: usize,
    /// Emit `[agent] native_function_calling`.
    pub native_function_calling: bool,
    /// Sampling temperature written into the config.
    pub temperature: f32,
    /// Free-text context shown during the wizard.
    pub notes: &'static str,
    /// One-line troubleshooting hint shown when `llm-doctor` fails on this card.
    pub hint: &'static str,
}

/// Every card `selfware boot` can recommend. Order is the recommendation
/// order shown in the wizard.
pub const CARDS: &[RecipeCard] = &[
    RecipeCard {
        name: "openrouter-free",
        endpoint: "https://openrouter.ai/api/v1",
        model: "nvidia/nemotron-3-ultra-550b-a55b:free",
        max_tokens: 65_536,
        context_length: 1_000_000,
        native_function_calling: true,
        temperature: 1.0,
        notes: "Free OpenRouter slug with a 1M-token window; needs an OpenRouter API key.",
        hint: "The free tier rate-limits: a 429 means wait a minute and retry, or switch to the paid slug (drop the `:free` suffix).",
    },
    RecipeCard {
        name: "openrouter-inkling",
        endpoint: "https://openrouter.ai/api/v1",
        model: "thinkingmachines/inkling:free",
        max_tokens: 262_144,
        context_length: 1_000_000,
        native_function_calling: true,
        temperature: 1.0,
        notes: "Free OpenRouter slug with a very large output budget (256k max_tokens).",
        hint: "The free tier rate-limits: a 429 means wait a minute and retry, or switch to the paid slug (drop the `:free` suffix).",
    },
    RecipeCard {
        name: "ollama",
        endpoint: "http://localhost:11434/v1",
        model: "qwen3",
        max_tokens: 8_192,
        context_length: 32_768,
        native_function_calling: true,
        temperature: 0.7,
        notes: "Local Ollama server. `model` must match a tag you have pulled (`ollama list`).",
        hint: "Check `ollama list` for the exact served tag and that `ollama serve` is running; Ollama defaults to a small num_ctx, raise it if you need the full window.",
    },
    RecipeCard {
        name: "vllm",
        endpoint: "http://localhost:8000/v1",
        model: "",
        max_tokens: 8_192,
        context_length: 32_768,
        native_function_calling: true,
        temperature: 0.7,
        notes: "Local vLLM/SGLang server. `model` is whatever `/v1/models` reports; `context_length` should match the server's `--max-model-len`.",
        hint: "Ask the server what it serves: `curl localhost:8000/v1/models`. context_length must match the server's --max-model-len.",
    },
    RecipeCard {
        name: "lmstudio",
        endpoint: "http://localhost:1234/v1",
        model: "",
        max_tokens: 8_192,
        context_length: 32_768,
        native_function_calling: true,
        temperature: 0.7,
        notes: "Local LM Studio server. `model` is whatever `/v1/models` reports (the loaded model).",
        hint: "In LM Studio start the local server (Developer tab) and load a model; context_length should match the context slider.",
    },
];

/// Look up a card by its exact name.
pub fn find_card(name: &str) -> Option<&'static RecipeCard> {
    CARDS.iter().find(|c| c.name == name)
}

impl RecipeCard {
    /// Whether this card's model id must come from the server (or the user)
    /// rather than the card itself.
    pub fn needs_model_detection(&self) -> bool {
        self.model.is_empty()
    }

    /// Render the config body for this card. `model_override` /
    /// `context_override` replace the card values (server-detected or
    /// user-entered); a card with an empty `model` REQUIRES an override.
    ///
    /// The emitted body is exactly what `Config::load` will parse — callers
    /// must run it through [`crate::config::Config::validate_generated_toml`]
    /// before writing (done by the wizard's emit step).
    pub fn render_config(
        &self,
        model_override: Option<&str>,
        context_override: Option<usize>,
    ) -> Result<String> {
        let model = model_override.unwrap_or(self.model);
        if model.trim().is_empty() {
            bail!(
                "recipe card '{}' has no built-in model id — detect it from the server or ask the user",
                self.name
            );
        }
        let context_length = context_override.unwrap_or(self.context_length);
        Ok(format!(
            r#"# Selfware configuration — written by `selfware boot`
# Recipe card: {name}
# {notes}
# Troubleshooting: {hint}

endpoint = {endpoint}
model = {model}
max_tokens = {max_tokens}
context_length = {context_length}
temperature = {temperature}

[agent]
native_function_calling = {native_fc}

[safety]
allowed_paths = ["./**"]
"#,
            name = self.name,
            notes = self.notes,
            hint = self.hint,
            endpoint = toml_quote(self.endpoint),
            model = toml_quote(model.trim()),
            max_tokens = self.max_tokens,
            context_length = context_length,
            temperature = self.temperature,
            native_fc = self.native_function_calling,
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/boot/cards_test.rs"]
mod tests;
