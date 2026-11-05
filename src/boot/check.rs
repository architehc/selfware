//! `selfware boot --check`: self-test for the boot assistant and the current
//! config. Every line is PASS / FAIL / SKIP; the command never hard-fails —
//! callers inspect the results and decide the exit code.

use std::path::{Path, PathBuf};

use crate::config::Config;

use super::chat;
use super::model;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootCheckStatus {
    Pass,
    Fail,
    Skip,
}

impl BootCheckStatus {
    pub fn label(self) -> &'static str {
        match self {
            BootCheckStatus::Pass => "PASS",
            BootCheckStatus::Fail => "FAIL",
            BootCheckStatus::Skip => "SKIP",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BootCheck {
    pub name: String,
    pub status: BootCheckStatus,
    pub detail: String,
}

impl BootCheck {
    pub fn pass(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: BootCheckStatus::Pass,
            detail: detail.into(),
        }
    }

    pub fn fail(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: BootCheckStatus::Fail,
            detail: detail.into(),
        }
    }

    pub fn skip(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: BootCheckStatus::Skip,
            detail: detail.into(),
        }
    }

    pub fn render(&self) -> String {
        format!("[{}] {} — {}", self.status.label(), self.name, self.detail)
    }
}

/// Check 1: model file present and sha256-verified. SKIP (with a note) when
/// the model was never downloaded — it's optional.
pub fn check_model_file(dir: &Path) -> BootCheck {
    let default_path = model::model_path_in(dir);
    let candidate = if default_path.is_file() {
        Some(default_path)
    } else {
        model::BOOT_MODELS
            .iter()
            .map(|spec| dir.join(spec.file_name))
            .find(|p| p.is_file())
    };
    let Some(path) = candidate else {
        if dir == model::boot_dir() {
            if let Some(bundled) = model::find_bundled_model() {
                return BootCheck::pass(
                    "boot model file",
                    format!("{} (sha256 ok)", bundled.display()),
                );
            }
        }
        return BootCheck::skip(
            "boot model file",
            "not downloaded (optional — fetched on first `selfware boot --chat`)",
        );
    };
    match model::verify_model_file(&path) {
        Ok(true) => BootCheck::pass("boot model file", format!("{} (sha256 ok)", path.display())),
        Ok(false) => BootCheck::fail(
            "boot model file",
            format!(
                "{} sha256/size mismatch — delete it and re-run `selfware boot --chat`",
                path.display()
            ),
        ),
        Err(e) => BootCheck::fail("boot model file", format!("cannot verify: {:#}", e)),
    }
}

/// Check 2: llama-server discoverable via LLAMA_SERVER, bundled bin/, or PATH.
pub fn check_llama_server() -> BootCheck {
    match model::find_llama_server() {
        Some(path) => BootCheck::pass("llama-server", path.display().to_string()),
        None => BootCheck::fail("llama-server", model::LLAMA_INSTALL_HINT),
    }
}

/// Check 2b: host RAM and disk-space readiness for the configured or shipped
/// local model (`LFM3-1.7B-GSQ` 64k/128k/256k or `FunctionGemma 270M` 32k),
/// plus an actionable optimization tip.
pub fn check_resource_readiness(dir: &Path, config: Option<&Config>) -> BootCheck {
    let (model_id, ctx_size) = match config {
        Some(cfg) if model::find_boot_model_spec(&cfg.model).is_some() => {
            (Some(cfg.model.as_str()), Some(cfg.context_length))
        }
        _ => (Some(model::SHIPPED_BOOT_MODEL_ID), None),
    };
    let readiness = model::check_model_resource_readiness(model_id, ctx_size, dir);
    let mut detail = readiness.summary_line();
    if let Some(tip) = readiness.tips.first() {
        detail.push_str(" | ");
        detail.push_str(tip);
    }
    if readiness.is_ready() {
        BootCheck::pass("host RAM & disk readiness", detail)
    } else {
        BootCheck::fail("host RAM & disk readiness", detail)
    }
}

/// What the round-trip check should do, decided from the earlier results so
/// the planner is testable without touching the machine.
pub enum RoundTripPlan {
    Skip(&'static str),
    Run { server: PathBuf, model: PathBuf },
}

pub fn plan_round_trip(model_check: &BootCheck, server_check: &BootCheck) -> RoundTripPlan {
    if model_check.status != BootCheckStatus::Pass {
        return RoundTripPlan::Skip("no verified model");
    }
    if server_check.status != BootCheckStatus::Pass {
        return RoundTripPlan::Skip("no llama-server");
    }
    let boot = model::boot_dir();
    let default_model = model::model_path_in(&boot);
    let model = if default_model.is_file() {
        default_model
    } else {
        model::BOOT_MODELS
            .iter()
            .map(|spec| boot.join(spec.file_name))
            .find(|p| p.is_file())
            .or_else(model::find_bundled_model)
            .unwrap_or(default_model)
    };
    RoundTripPlan::Run {
        server: PathBuf::from(&server_check.detail),
        model,
    }
}

/// Check 3: boot the server and ask a canned question; PASS on any non-empty
/// reply. Skipped unless checks 1+2 passed.
async fn check_round_trip(plan: RoundTripPlan) -> BootCheck {
    let (server, model) = match plan {
        RoundTripPlan::Skip(reason) => return BootCheck::skip("canned round-trip", reason),
        RoundTripPlan::Run { server, model } => (server, model),
    };
    let result = async {
        let base_url = if model::is_boot_server_healthy(model::SERVER_PORT).await {
            format!("http://127.0.0.1:{}/v1", model::SERVER_PORT)
        } else {
            let guard = model::spawn_server(&server, &model, model::SERVER_PORT).await?;
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(120))
                .build()?;
            let body = chat::build_messages("Reply with exactly: boot-ok", None);
            return chat::ask_once(&client, &guard.base_url(), &body).await;
        };
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;
        let body = chat::build_messages("Reply with exactly: boot-ok", None);
        chat::ask_once(&client, &base_url, &body).await
    }
    .await;
    match result {
        Ok(reply) if !reply.is_empty() => BootCheck::pass(
            "canned round-trip",
            format!("assistant replied ({} chars)", reply.len()),
        ),
        Ok(_) => BootCheck::fail("canned round-trip", "assistant returned an empty reply"),
        Err(e) => BootCheck::fail("canned round-trip", format!("{:#}", e)),
    }
}

/// Check 4: the CURRENT config (the one this invocation loaded) passes
/// llm-doctor.
async fn check_current_config(config: &Config) -> BootCheck {
    let mut effective_config = config.clone();
    if effective_config.loaded_config_path().is_none()
        && effective_config.source_of("endpoint") == crate::config::ConfigSource::Default
        && effective_config.source_of("model") == crate::config::ConfigSource::Default
        && model::find_llama_server().is_some()
    {
        effective_config.endpoint = format!("http://127.0.0.1:{}/v1", model::SERVER_PORT);
        effective_config.apply_cli_model_override(model::SHIPPED_BOOT_MODEL_ID.to_string());
        effective_config.context_length = model::FUNCTIONGEMMA_CTX_32K;
        effective_config.sync_default_model_profile();
    }
    if model::is_boot_server_endpoint(&effective_config.endpoint) {
        match model::ensure_background_boot_server_for_model_with_context(
            Some(&effective_config.model),
            Some(effective_config.context_length),
        )
        .await
        {
            Ok(base_url) => {
                effective_config.endpoint = base_url;
                effective_config.sync_default_model_profile();
            }
            Err(e) => {
                return BootCheck::fail("current config llm-doctor", format!("{:#}", e));
            }
        }
    }
    match crate::llm_doctor::run_llm_doctor_report(&effective_config).await {
        Ok(report) if !report.had_failures => {
            BootCheck::pass("current config llm-doctor", "no failures")
        }
        Ok(_) => BootCheck::fail(
            "current config llm-doctor",
            "one or more FAIL checks above — run `selfware boot` to repair",
        ),
        Err(e) => BootCheck::fail("current config llm-doctor", format!("{:#}", e)),
    }
}

/// Run all checks; never returns Err. `true` when nothing failed (SKIP is
/// fine — the model is optional).
pub async fn run_boot_check(config: &Config) -> (Vec<BootCheck>, bool) {
    run_boot_check_with_config(Ok(config)).await
}

/// Configuration load errors are diagnostic results, so the remaining
/// recovery checks are still available when the current TOML is broken.
pub async fn run_boot_check_with_config(config: Result<&Config, &str>) -> (Vec<BootCheck>, bool) {
    let boot_dir = model::boot_dir();
    let model_check = check_model_file(&boot_dir);
    let server_check = check_llama_server();
    let resource_check = check_resource_readiness(&boot_dir, config.ok());
    let round_trip = check_round_trip(plan_round_trip(&model_check, &server_check)).await;
    let doctor = match config {
        Ok(config) => check_current_config(config).await,
        Err(error) => BootCheck::fail(
            "current config",
            crate::safety::redact::redact_secrets(error).into_owned(),
        ),
    };

    let checks = vec![
        model_check,
        server_check,
        resource_check,
        round_trip,
        doctor,
    ];
    let ok = checks.iter().all(|c| c.status != BootCheckStatus::Fail);
    (checks, ok)
}

#[cfg(test)]
#[path = "../../tests/unit/boot/check_test.rs"]
mod tests;
