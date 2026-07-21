use super::*;
use serde_json::json;

fn dummy_instance() -> SwebenchProInstance {
    SwebenchProInstance {
        instance_id: "demo-1".into(),
        repo: "octocat/hello-world".into(),
        base_commit: "deadbeef".into(),
        problem_statement: "  fix bug ".into(),
        fail_to_pass: json!(["tests/test_a.py::test_x"]),
        selected_test_files_to_run: json!(["tests/test_a.py"]),
        repo_language: Some("python".into()),
        extra: Default::default(),
    }
}

#[test]
fn swebench_pro_system_prompt_is_concise() {
    let sys = PromptProfile::SwebenchPro.system_prompt();
    assert!(sys.contains("simplest correct patch"));
    assert!(sys.contains("valid tool call ONLY"));
    assert!(sys.contains("NEVER write prose, reasoning, or explanations before a tool XML tag"));
    assert!(sys.contains("Do NOT modify test files"));
    assert!(sys.contains("Verify honestly"));
}

#[test]
fn swebench_pro_diagnostic_includes_fail_to_pass() {
    let inst = dummy_instance();
    let prompt = PromptProfile::SwebenchPro.task_prompt(&inst, "diagnostic");
    assert!(prompt.contains("[mode: diagnostic]"));
    assert!(prompt.contains("tests/test_a.py::test_x"));
    assert!(prompt.contains("RELEVANT TEST FILES: tests/test_a.py"));
}

#[test]
fn swebench_pro_official_excludes_tests() {
    let inst = dummy_instance();
    let prompt = PromptProfile::SwebenchPro.task_prompt(&inst, "official");
    assert!(prompt.contains("[mode: official]"));
    assert!(!prompt.contains("tests/test_a.py::test_x"));
    assert!(!prompt.contains("RELEVANT TEST FILES:"));
    assert!(!prompt.contains("FAIL-TO-PASS"));
}

#[test]
fn swebench_pro_prompt_contains_tool_contract() {
    let inst = dummy_instance();
    for mode in &["diagnostic", "official"] {
        let prompt = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
        assert!(
            prompt.contains("Valid tool call ONLY"),
            "tool contract missing in {} mode",
            mode
        );
        assert!(
            prompt.contains("NO prose before tool XML"),
            "no-prose rule missing in {} mode",
            mode
        );
    }
}

#[test]
fn swebench_pro_prompt_contains_no_test_edit_rule() {
    let inst = dummy_instance();
    for mode in &["diagnostic", "official"] {
        let prompt = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
        assert!(
            prompt.contains("Do NOT modify test files"),
            "no-test-edit rule missing in {} mode",
            mode
        );
    }
}

#[test]
fn swebench_pro_prompt_contains_verification_requirement() {
    let inst = dummy_instance();
    for mode in &["diagnostic", "official"] {
        let prompt = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
        assert!(
            prompt.contains("confirm they pass before finishing"),
            "verification requirement missing in {} mode",
            mode
        );
    }
}

#[test]
fn default_profile_task_prompt_matches_legacy() {
    let inst = dummy_instance();
    let diag = PromptProfile::Default.task_prompt(&inst, "diagnostic");
    assert!(diag.contains("[mode: diagnostic]"));
    assert!(diag.contains("tests/test_a.py::test_x"));

    let off = PromptProfile::Default.task_prompt(&inst, "official");
    assert!(off.contains("[mode: official]"));
    assert!(!off.contains("tests/test_a.py::test_x"));
}
