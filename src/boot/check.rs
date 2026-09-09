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
    let path = model::model_path_in(dir);
    if !path.is_file() {
        return BootCheck::skip(
            "boot model file",
            "not downloaded (optional — fetched on first `selfware boot --chat`)",
        );
    }
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

/// Check 2: llama-server discoverable via LLAMA_SERVER or PATH.
pub fn check_llama_server() -> BootCheck {
    match model::find_llama_server() {
        Some(path) => BootCheck::pass("llama-server", path.display().to_string()),
        None => BootCheck::fail("llama-server", model::LLAMA_INSTALL_HINT),
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
    RoundTripPlan::Run {
        server: PathBuf::from(&server_check.detail),
        model: model::model_path_in(&model::boot_dir()),
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
        let guard = model::spawn_server(&server, &model, model::SERVER_PORT).await?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;
        let body = chat::build_messages("Reply with exactly: boot-ok", None);
        chat::ask_once(&client, &guard.base_url(), &body).await
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
    match crate::llm_doctor::run_llm_doctor_report(config).await {
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
    let model_check = check_model_file(&model::boot_dir());
    let server_check = check_llama_server();
    let round_trip = check_round_trip(plan_round_trip(&model_check, &server_check)).await;
    let doctor = check_current_config(config).await;

    let checks = vec![model_check, server_check, round_trip, doctor];
    let ok = checks.iter().all(|c| c.status != BootCheckStatus::Fail);
    (checks, ok)
}

#[cfg(test)]
#[path = "../../tests/unit/boot/check_test.rs"]
mod tests;
