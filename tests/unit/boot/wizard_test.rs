use super::{
    emit_config, interview, verify_loop, DetectedModel, DoctorOutcome, ScriptedIo, WizardPlan,
};
use crate::boot::cards::find_card;

#[tokio::test]
async fn real_model_probe_runs_outside_async_runtime() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 2048];
        let size = stream.read(&mut request).await.unwrap();
        assert!(String::from_utf8_lossy(&request[..size]).starts_with("GET /v1/models "));
        let body = r#"{"data":[{"id":"served-fixture","max_model_len":65536}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let (_, plan) = super::interview_off_runtime(ScriptedIo::new(["2", "2"]), move |_| {
        super::detect_model_at(&endpoint)
    })
    .await
    .unwrap();
    let plan = plan.unwrap();
    assert_eq!(plan.model_override.as_deref(), Some("served-fixture"));
    assert_eq!(plan.context_override, Some(65536));
    server.await.unwrap();
}

fn no_detect() -> impl Fn(&crate::boot::cards::RecipeCard) -> Option<DetectedModel> {
    |_| None
}

fn detected(id: &str) -> impl Fn(&crate::boot::cards::RecipeCard) -> Option<DetectedModel> + '_ {
    move |_| {
        Some(DetectedModel {
            id: id.to_string(),
            context_length: Some(65_536),
        })
    }
}

fn plan_in(dir: &tempfile::TempDir, card_name: &str) -> WizardPlan {
    WizardPlan {
        card: find_card(card_name).unwrap(),
        model_override: None,
        context_override: None,
        api_key: None,
        config_path: dir.path().join("nested").join("config.toml"),
    }
}

// ── Branch logic ────────────────────────────────────────────────────────────

#[test]
fn openrouter_branch_defaults_to_free_card() {
    let mut io = ScriptedIo::new(["1", "1", ""]);
    let plan = interview(&mut io, &no_detect()).unwrap().unwrap();
    assert_eq!(plan.card.name, "openrouter-free");
    assert!(plan.api_key.is_none());
    assert!(plan.model_override.is_none());
}

#[test]
fn openrouter_branch_selects_inkling_and_captures_key() {
    let mut io = ScriptedIo::new(["1", "2", "sk-test-key"]);
    let plan = interview(&mut io, &no_detect()).unwrap().unwrap();
    assert_eq!(plan.card.name, "openrouter-inkling");
    assert_eq!(plan.api_key.as_deref(), Some("sk-test-key"));
}

#[test]
fn local_ollama_branch_uses_card_default_model_on_blank() {
    let mut io = ScriptedIo::new(["2", "1", ""]);
    let plan = interview(&mut io, &no_detect()).unwrap().unwrap();
    assert_eq!(plan.card.name, "ollama");
    assert!(plan.model_override.is_none());
}

#[test]
fn local_ollama_branch_accepts_custom_tag() {
    let mut io = ScriptedIo::new(["2", "1", "qwen3:8b"]);
    let plan = interview(&mut io, &no_detect()).unwrap().unwrap();
    assert_eq!(plan.model_override.as_deref(), Some("qwen3:8b"));
}

#[test]
fn local_vllm_branch_uses_detected_model_and_context() {
    let mut io = ScriptedIo::new(["2", "2"]);
    let plan = interview(&mut io, &detected("served-qwen3"))
        .unwrap()
        .unwrap();
    assert_eq!(plan.card.name, "vllm");
    assert_eq!(plan.model_override.as_deref(), Some("served-qwen3"));
    assert_eq!(plan.context_override, Some(65_536));
    assert!(io.said.iter().any(|l| l.contains("served-qwen3")));
}

#[test]
fn local_vllm_branch_asks_when_detection_fails() {
    let mut io = ScriptedIo::new(["2", "2", "my-model"]);
    let plan = interview(&mut io, &no_detect()).unwrap().unwrap();
    assert_eq!(plan.model_override.as_deref(), Some("my-model"));
    assert!(plan.context_override.is_none());
}

#[test]
fn local_vllm_branch_bails_when_model_never_given() {
    let mut io = ScriptedIo::new(["2", "2", "", "", ""]);
    assert!(interview(&mut io, &no_detect()).is_err());
}

#[test]
fn local_lmstudio_branch_detects() {
    let mut io = ScriptedIo::new(["2", "3"]);
    let plan = interview(&mut io, &detected("lmstudio-loaded"))
        .unwrap()
        .unwrap();
    assert_eq!(plan.card.name, "lmstudio");
    assert_eq!(plan.model_override.as_deref(), Some("lmstudio-loaded"));
}

#[test]
fn nothing_branch_decline_aborts_cleanly() {
    let mut io = ScriptedIo::new(["3", "n"]);
    assert!(interview(&mut io, &no_detect()).unwrap().is_none());
    assert!(io.said.iter().any(|l| l.contains("Re-run `selfware boot`")));
}

#[test]
fn nothing_branch_accept_recommends_openrouter_free() {
    let mut io = ScriptedIo::new(["3", "y", "sk-free"]);
    let plan = interview(&mut io, &no_detect()).unwrap().unwrap();
    assert_eq!(plan.card.name, "openrouter-free");
    assert_eq!(plan.api_key.as_deref(), Some("sk-free"));
    // The recommendation was explained, not silently picked.
    assert!(io.said.iter().any(|l| l.contains("openrouter.ai")));
}

// ── Emit ────────────────────────────────────────────────────────────────────

#[test]
fn emitted_toml_parses_and_matches_the_card() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan_in(&dir, "openrouter-free");
    let path = emit_config(&plan).unwrap();
    // Parent dirs were created.
    assert!(path.parent().unwrap().is_dir());
    let body = std::fs::read_to_string(&path).unwrap();
    crate::config::Config::validate_generated_toml(&body).unwrap();
    let cfg: crate::config::Config = toml::from_str(&body).unwrap();
    assert_eq!(cfg.endpoint, plan.card.endpoint);
    assert_eq!(cfg.model, plan.card.model);
    assert_eq!(cfg.max_tokens, plan.card.max_tokens);
    assert_eq!(cfg.context_length, plan.card.context_length);
    assert!(body.contains("Recipe card: openrouter-free"));
    assert!(body.contains(plan.card.hint));
}

#[test]
fn emit_applies_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = plan_in(&dir, "ollama");
    plan.model_override = Some("qwen3:8b".to_string());
    plan.context_override = Some(131_072);
    let path = emit_config(&plan).unwrap();
    let cfg: crate::config::Config =
        toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(cfg.model, "qwen3:8b");
    assert_eq!(cfg.context_length, 131_072);
}

// ── Verify loop ─────────────────────────────────────────────────────────────

fn fail_outcome(detail: &str) -> DoctorOutcome {
    DoctorOutcome {
        ok: false,
        failing_checks: vec![format!("endpoint reachable: {}", detail)],
    }
}

fn pass_outcome() -> DoctorOutcome {
    DoctorOutcome {
        ok: true,
        failing_checks: Vec::new(),
    }
}

#[tokio::test]
async fn verify_loop_retries_then_passes() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = plan_in(&dir, "openrouter-free");
    emit_config(&plan).unwrap();
    let mut io = ScriptedIo::new(["r", "r"]);
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let doctor = |_p: std::path::PathBuf| {
        let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move {
            if n < 2 {
                Ok(fail_outcome("connection refused"))
            } else {
                Ok(pass_outcome())
            }
        }
    };
    let ok = verify_loop(&mut io, &mut plan, &doctor).await.unwrap();
    assert!(ok);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    // The card's troubleshooting hint was shown on failure.
    assert!(io.said.iter().any(|l| l.contains("429")));
    assert!(io
        .said
        .iter()
        .any(|l| l.contains("FAIL endpoint reachable")));
}

#[tokio::test]
async fn verify_loop_quit_stops_without_more_doctor_calls() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = plan_in(&dir, "ollama");
    emit_config(&plan).unwrap();
    let mut io = ScriptedIo::new(["q"]);
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let doctor = |_p: std::path::PathBuf| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async { Ok(fail_outcome("connection refused")) }
    };
    let ok = verify_loop(&mut io, &mut plan, &doctor).await.unwrap();
    assert!(!ok);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn verify_loop_edit_updates_plan_and_rewrites_config() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = plan_in(&dir, "ollama");
    emit_config(&plan).unwrap();
    let mut io = ScriptedIo::new(["e", "qwen3:8b", "65536"]);
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let doctor = |_p: std::path::PathBuf| {
        let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move {
            if n == 0 {
                Ok(fail_outcome("model not listed"))
            } else {
                Ok(pass_outcome())
            }
        }
    };
    let ok = verify_loop(&mut io, &mut plan, &doctor).await.unwrap();
    assert!(ok);
    assert_eq!(plan.model_override.as_deref(), Some("qwen3:8b"));
    assert_eq!(plan.context_override, Some(65_536));
    // The rewritten config on disk carries the edit.
    let cfg: crate::config::Config =
        toml::from_str(&std::fs::read_to_string(&plan.config_path).unwrap()).unwrap();
    assert_eq!(cfg.model, "qwen3:8b");
    assert_eq!(cfg.context_length, 65_536);
}

#[tokio::test]
async fn verify_loop_gives_up_after_max_attempts() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = plan_in(&dir, "lmstudio");
    plan.model_override = Some("m".to_string());
    emit_config(&plan).unwrap();
    let mut io = ScriptedIo::new(["r", "r"]); // 3rd attempt never asks
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let doctor = |_p: std::path::PathBuf| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async { Ok(fail_outcome("server down")) }
    };
    let ok = verify_loop(&mut io, &mut plan, &doctor).await.unwrap();
    assert!(!ok);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert!(io.said.iter().any(|l| l.contains("Out of retries")));
}
