//! Interactive `selfware boot` wizard: recommend a recipe card, emit its
//! config, then verify it with `llm-doctor` and loop on failure.
//!
//! The question/answer flow goes through the [`BootIo`] abstraction so tests
//! can script answers headlessly ([`ScriptedIo`]). The doctor invocation is
//! injected as a closure for the same reason.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::cards::{find_card, RecipeCard};

/// Max doctor rounds before the wizard stops retrying.
const MAX_VERIFY_ATTEMPTS: usize = 3;

/// Terminal I/O abstraction for the wizard.
pub trait BootIo {
    /// Print a line to the user.
    fn say(&mut self, message: &str);
    /// Show `prompt` and return the user's answer (trimmed; empty on EOF,
    /// mirroring `read_line` behaviour so defaults apply).
    fn ask(&mut self, prompt: &str) -> Result<String>;
}

/// Real stdin/stdout implementation.
pub struct StdinIo;

impl BootIo for StdinIo {
    fn say(&mut self, message: &str) {
        println!("{}", message);
    }

    fn ask(&mut self, prompt: &str) -> Result<String> {
        print!("{}", prompt);
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim().to_string())
    }
}

/// Scripted answers for headless tests. Answers are consumed FIFO; running
/// out behaves like EOF (empty answer → the default branch), and everything
/// `say` prints is captured in `said` for assertions.
#[derive(Default)]
pub struct ScriptedIo {
    answers: std::collections::VecDeque<String>,
    pub said: Vec<String>,
}

impl ScriptedIo {
    pub fn new(answers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            answers: answers.into_iter().map(Into::into).collect(),
            said: Vec::new(),
        }
    }
}

impl BootIo for ScriptedIo {
    fn say(&mut self, message: &str) {
        self.said.push(message.to_string());
    }

    fn ask(&mut self, _prompt: &str) -> Result<String> {
        Ok(self.answers.pop_front().unwrap_or_default())
    }
}

/// Model info reported by a running server's `/v1/models`.
pub struct DetectedModel {
    pub id: String,
    pub context_length: Option<usize>,
}

/// What the interview decided: the card plus any user/server overrides.
pub struct WizardPlan {
    pub card: &'static RecipeCard,
    pub model_override: Option<String>,
    pub context_override: Option<usize>,
    /// OpenRouter key to store in the OS keyring (never written to config).
    pub api_key: Option<String>,
    pub config_path: PathBuf,
}

/// Where boot writes its config by default — the GLOBAL config path, not
/// `./selfware.toml`: boot is recovery/onboarding, not project setup, and a
/// checkout-local file would be subject to untrusted-repo stripping.
pub fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config").join("selfware").join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("selfware.toml"))
}

/// Run the question/answer flow. `detect` probes a local server's
/// `/v1/models` for cards that don't pin a model id; tests inject a stub.
///
/// Returns `Ok(None)` when the user declines setup ("nothing yet" branch).
pub fn interview(
    io: &mut dyn BootIo,
    detect: &dyn Fn(&RecipeCard) -> Option<DetectedModel>,
) -> Result<Option<WizardPlan>> {
    io.say("selfware boot — recovery setup assistant");
    io.say("Every config here comes from a verified recipe card, never from a model's guess.");
    io.say("");
    io.say("What do you have to run models with?");
    io.say("  [1] An OpenRouter API key");
    io.say("  [2] A local server (Ollama / vLLM / SGLang / LM Studio)");
    io.say("  [3] Nothing yet");
    let choice = io.ask("  > ")?;

    let (card, model_override, context_override, api_key) = match choice.trim() {
        "1" => {
            io.say("Which OpenRouter recipe?");
            io.say("  [1] openrouter-free  — nemotron-3-ultra free slug, 1M context (recommended)");
            io.say("  [2] openrouter-inkling — inkling free slug, 256k output budget");
            let pick = io.ask("  > ")?;
            let card = if pick.trim() == "2" {
                find_card("openrouter-inkling").unwrap()
            } else {
                find_card("openrouter-free").unwrap()
            };
            let key = io.ask(
                "  Paste your OpenRouter API key (stored in your OS keyring, blank to skip): ",
            )?;
            let api_key = (!key.is_empty()).then_some(key);
            (card, None, None, api_key)
        }
        "2" => {
            io.say("Which local server?");
            io.say("  [1] Ollama (http://localhost:11434/v1)");
            io.say("  [2] vLLM / SGLang (http://localhost:8000/v1)");
            io.say("  [3] LM Studio (http://localhost:1234/v1)");
            let pick = io.ask("  > ")?;
            let card = match pick.trim() {
                "2" => find_card("vllm").unwrap(),
                "3" => find_card("lmstudio").unwrap(),
                _ => find_card("ollama").unwrap(),
            };
            if card.needs_model_detection() {
                match detect(card) {
                    Some(detected) => {
                        io.say(&format!("  Detected served model: {}", detected.id));
                        (card, Some(detected.id), detected.context_length, None)
                    }
                    None => {
                        io.say("  Could not query /v1/models — is the server running?");
                        let mut model = String::new();
                        for _ in 0..3 {
                            model = io.ask("  Model id to use (exactly as served): ")?;
                            if !model.is_empty() {
                                break;
                            }
                            io.say("  A model id is required for this card.");
                        }
                        if model.is_empty() {
                            bail!("card '{}' needs a model id and none was given", card.name);
                        }
                        (card, Some(model), None, None)
                    }
                }
            } else {
                let answer = io.ask(&format!("  Model tag to use [{}]: ", card.model))?;
                let model_override = (!answer.is_empty()).then_some(answer);
                (card, model_override, None, None)
            }
        }
        _ => {
            io.say("");
            io.say("No server and no key: the cheapest working setup is OpenRouter's free tier —");
            io.say("create a key at https://openrouter.ai/keys and selfware points at it.");
            let go = io.ask("  Set up the openrouter-free recipe now? [y/N]: ")?;
            if !go.eq_ignore_ascii_case("y") {
                io.say("  No problem. Re-run `selfware boot` any time; `selfware boot --check` self-tests.");
                return Ok(None);
            }
            let key = io.ask(
                "  Paste your OpenRouter API key (stored in your OS keyring, blank to skip): ",
            )?;
            let api_key = (!key.is_empty()).then_some(key);
            (find_card("openrouter-free").unwrap(), None, None, api_key)
        }
    };

    io.say("");
    io.say(&format!(
        "  Recipe card '{}' → {} (model: {})",
        card.name,
        card.endpoint,
        model_override.as_deref().unwrap_or(card.model)
    ));
    io.say(&format!("  {}", card.notes));

    Ok(Some(WizardPlan {
        card,
        model_override,
        context_override,
        api_key,
        config_path: default_config_path(),
    }))
}

/// Render, validate, and write the plan's config. The body is validated the
/// way the loader would (`Config::validate_generated_toml`) BEFORE it touches
/// disk, so boot can never persist a config selfware rejects. Parent
/// directories are created as needed.
pub fn emit_config(plan: &WizardPlan) -> Result<PathBuf> {
    let body = plan
        .card
        .render_config(plan.model_override.as_deref(), plan.context_override)?;
    crate::config::Config::validate_generated_toml(&body)?;
    if let Some(parent) = plan.config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&plan.config_path, &body)
        .with_context(|| format!("writing {}", plan.config_path.display()))?;
    Ok(plan.config_path.clone())
}

/// Structured outcome of one doctor round against the emitted config.
pub struct DoctorOutcome {
    pub ok: bool,
    /// "name: detail" lines for every FAIL-level check.
    pub failing_checks: Vec<String>,
}

/// Run the doctor against the emitted config and loop on failure: show the
/// failing checks plus the card's troubleshooting hint, then offer retry /
/// edit model settings / quit. `doctor` is injected so tests never touch the
/// network; it receives the config path.
///
/// Returns `Ok(true)` when the config verifies.
pub async fn verify_loop<F, Fut>(
    io: &mut dyn BootIo,
    plan: &mut WizardPlan,
    doctor: &F,
) -> Result<bool>
where
    F: Fn(PathBuf) -> Fut,
    Fut: std::future::Future<Output = Result<DoctorOutcome>>,
{
    for attempt in 1..=MAX_VERIFY_ATTEMPTS {
        io.say("");
        io.say(&format!(
            "  Verifying {} with llm-doctor (attempt {}/{})...",
            plan.config_path.display(),
            attempt,
            MAX_VERIFY_ATTEMPTS
        ));
        let outcome = doctor(plan.config_path.clone()).await?;
        if outcome.ok {
            io.say("  Config verified — you're set. Run `selfware` to start.");
            return Ok(true);
        }
        io.say("  Doctor found problems:");
        for line in &outcome.failing_checks {
            io.say(&format!("    FAIL {}", line));
        }
        io.say(&format!("  Hint: {}", plan.card.hint));
        if attempt == MAX_VERIFY_ATTEMPTS {
            io.say("  Out of retries. The config stays on disk — fix the backend and run");
            io.say("  `selfware llm-doctor` to re-verify, or `selfware boot` to start over.");
            return Ok(false);
        }
        let action = io.ask("  [r]etry, [e]dit model/context, [q]uit: ")?;
        match action.to_ascii_lowercase().as_str() {
            "e" => {
                let current_model = plan
                    .model_override
                    .clone()
                    .unwrap_or_else(|| plan.card.model.to_string());
                let model = io.ask(&format!("  Model id [{}]: ", current_model))?;
                if !model.is_empty() {
                    plan.model_override = Some(model);
                }
                let current_ctx = plan.context_override.unwrap_or(plan.card.context_length);
                let ctx = io.ask(&format!("  Context length [{}]: ", current_ctx))?;
                if !ctx.is_empty() {
                    let parsed: usize = ctx
                        .parse()
                        .context("context length must be a positive integer")?;
                    plan.context_override = Some(parsed);
                }
                emit_config(plan)?;
                io.say("  Config updated; re-running doctor.");
            }
            "r" => {}
            _ => {
                io.say(
                    "  Stopping. The config stays on disk; `selfware llm-doctor` re-verifies it.",
                );
                return Ok(false);
            }
        }
    }
    Ok(false)
}

/// Real `/v1/models` probe for local-server cards: 2s budget, first listed
/// model wins. Any failure (server down, non-JSON, empty list) is `None` and
/// the wizard falls back to asking.
fn detect_served_model(card: &RecipeCard) -> Option<DetectedModel> {
    detect_model_at(card.endpoint)
}

fn detect_model_at(endpoint: &str) -> Option<DetectedModel> {
    let url = format!("{}/models", endpoint.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    let body: serde_json::Value = client.get(&url).send().ok()?.json().ok()?;
    let model = body.get("data")?.as_array()?.first()?;
    let id = model.get("id")?.as_str()?.to_string();
    Some(DetectedModel {
        id,
        context_length: model
            .get("max_model_len")
            .or_else(|| model.get("context_length"))
            .and_then(|value| value.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0),
    })
}

// Both terminal input and reqwest's blocking detector must execute outside
// the Tokio runtime. Keep this adapter shared by the real wizard and tests.
async fn interview_off_runtime<I, D>(mut io: I, detect: D) -> Result<(I, Option<WizardPlan>)>
where
    I: BootIo + Send + 'static,
    D: Fn(&RecipeCard) -> Option<DetectedModel> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let plan = interview(&mut io, &detect)?;
        Ok((io, plan))
    })
    .await
    .context("running the setup interview")?
}

/// Load the config boot just wrote and run the full doctor against it. An
/// unreachable endpoint surfaces as an `Err` from the doctor inner loop —
/// fold that into a failed outcome (the wizard reports, it doesn't crash).
async fn doctor_for_path(path: &Path) -> Result<DoctorOutcome> {
    let config = crate::config::Config::load(Some(path.to_string_lossy().as_ref()))
        .context("loading the config boot just wrote")?;
    match crate::llm_doctor::run_llm_doctor_report(&config).await {
        Ok(report) => {
            let failing_checks = report
                .config_checks
                .iter()
                .chain(report.endpoint_reachable.iter())
                .chain(report.model_available.iter())
                .filter(|c| matches!(c.status, crate::llm_doctor::CheckOutcome::Fail))
                .map(|c| format!("{}: {}", c.name, c.detail))
                .collect();
            Ok(DoctorOutcome {
                ok: !report.had_failures,
                failing_checks,
            })
        }
        Err(e) => Ok(DoctorOutcome {
            ok: false,
            failing_checks: vec![format!("{:#}", e)],
        }),
    }
}

/// `selfware boot` with no flags: interview → emit → verify loop.
pub async fn run_boot_wizard() -> Result<()> {
    run_boot_wizard_for_path(None).await
}

/// Repair an explicitly selected configuration, or use the global default.
pub async fn run_boot_wizard_for_path(config_path: Option<PathBuf>) -> Result<()> {
    // Same fail-fast as `selfware init`: without a terminal every answer is
    // EOF and the wizard would persist an all-defaults config the user never
    // asked for.
    if !std::io::stdin().is_terminal() {
        bail!(
            "`selfware boot` is an interactive wizard and needs a terminal on stdin. \
             Use `selfware boot --check` for a headless self-test."
        );
    }

    let (mut io, plan) = interview_off_runtime(StdinIo, detect_served_model).await?;
    let Some(mut plan) = plan else {
        return Ok(());
    };
    if let Some(path) = config_path {
        plan.config_path = path;
    }

    if plan.config_path.exists() {
        let answer = io.ask(&format!(
            "  {} already exists. Overwrite? [y/N]: ",
            plan.config_path.display()
        ))?;
        if !answer.eq_ignore_ascii_case("y") {
            io.say("  Aborted. Existing configuration preserved.");
            return Ok(());
        }
    }

    emit_config(&plan)?;
    io.say(&format!(
        "  Config written to {}",
        plan.config_path.display()
    ));

    // Store the key in the OS keyring, never the config — same handling as
    // the init wizard.
    if let Some(ref key) = plan.api_key {
        match crate::config::save_api_key_to_keyring(plan.card.endpoint, key) {
            Ok(()) => io.say("  API key saved to your OS keyring."),
            Err(e) => io.say(&format!(
                "  Could not save to keyring ({}). Set SELFWARE_API_KEY=<key> instead.",
                e
            )),
        }
    } else if !plan.card.endpoint.contains("localhost") {
        io.say(
            "  No key stored: set SELFWARE_API_KEY=<key> before running, or the endpoint will 401.",
        );
    }

    let doctor = |p: PathBuf| async move { doctor_for_path(&p).await };
    let ok = verify_loop(&mut io, &mut plan, &doctor).await?;
    if !ok {
        bail!("boot: config did not pass llm-doctor — see hints above");
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/boot/wizard_test.rs"]
mod tests;
