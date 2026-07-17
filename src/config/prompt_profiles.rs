//! Prompt profiles for benchmark and agent modes.
//!
//! Profiles encapsulate system-prompt and task-prompt generation so that
//! different evaluation pipelines (SWE-bench Pro, local ad-hoc fixes, etc.)
//! can use tailored prompts without scattering prompt strings across the
//! codebase.

#[cfg(feature = "bench-harness")]
use crate::bench_harness::swebench_pro::dataset::{coerce_string_list, Instance};

#[cfg(feature = "bench-harness")]
pub type SwebenchProInstance = Instance;

/// Named prompt generation strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptProfile {
    /// The generic agent prompt (broad, conversational).
    Default,
    /// Tight, tool-oriented prompt optimized for Qwen3.6 local models on
    /// SWE-bench Pro.
    SwebenchPro,
}

impl PromptProfile {
    /// System prompt for the profile.
    pub fn system_prompt(&self) -> String {
        match self {
            PromptProfile::Default => {
                // The default system prompt is injected by the agent layer;
                // returning empty here lets the caller fall back to its own
                // default.
                String::new()
            }
            PromptProfile::SwebenchPro => {
                "You are a software repair agent. Your job is to produce the \
                 simplest correct patch for the issue described.\n\n\
                 RULES:\n\
                 1. Output a valid tool call ONLY, or a concise final answer.\n\
                 2. NEVER write prose, reasoning, or explanations before a tool XML tag.\n\
                 3. Make the smallest code change that resolves the issue.\n\
                 4. Verify honestly; do not gold-plate or refactor unrelated code.\n\
                 5. Do NOT modify test files.\n\
                 6. Run relevant tests and confirm they pass before finishing.\n"
                    .to_string()
            }
        }
    }

    /// Task prompt for a SWE-bench Pro instance.
    ///
    /// * `mode` – `"diagnostic"` includes fail-to-pass tests and selected test
    ///   files; `"official"` omits them.
    #[cfg(feature = "bench-harness")]
    pub fn task_prompt(&self, inst: &SwebenchProInstance, mode: &str) -> String {
        match self {
            PromptProfile::Default => build_default_prompt(inst, mode),
            PromptProfile::SwebenchPro => build_swebench_pro_prompt(inst, mode),
        }
    }
}

#[cfg(feature = "bench-harness")]
fn build_default_prompt(inst: &SwebenchProInstance, mode: &str) -> String {
    let trimmed = inst.problem_statement.trim();
    // Unwrap at most ONE balanced surrounding pair of quotes (a wrapping
    // artifact) instead of greedily stripping every leading/trailing quote,
    // which mangled statements that legitimately begin or end with a quote.
    let problem = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed)
        .replace("\\n", "\n");

    if mode == "official" {
        format!(
            "[mode: official]\n\n\
             You are working on a real codebase in the current directory. \
             Resolve this issue:\n\n{problem}\n\n\
             Steps:\n\
             1. Read the codebase to understand the expected behavior.\n\
             2. Make the smallest code change that resolves the issue.\n\
             3. Do NOT modify the test files themselves.\n\
             4. Run tests if possible to verify your fix.\n\
             5. When done, summarize what you changed."
        )
    } else {
        let tests = coerce_string_list(&inst.selected_test_files_to_run);
        let fail = coerce_string_list(&inst.fail_to_pass);

        let fail_str = fail
            .iter()
            .map(|t| format!("  - {}", t))
            .collect::<Vec<_>>()
            .join("\n");
        let test_str = tests.join(", ");

        format!(
            "[mode: diagnostic]\n\n\
             You are working on a real codebase in the current directory. \
             Resolve this issue:\n\n{problem}\n\n\
             The fix needs to make these tests pass:\n{fail_str}\n\n\
             Relevant test files: {test_str}\n\n\
             Steps:\n\
             1. Read the failing test files to understand the expected behavior.\n\
             2. Read the implementation files mentioned in the tests.\n\
             3. Make the smallest code change that resolves the issue.\n\
             4. Do NOT modify the test files themselves.\n\
             5. Run the failing tests if possible to verify your fix.\n\
             6. When done, summarize what you changed."
        )
    }
}

#[cfg(feature = "bench-harness")]
fn build_swebench_pro_prompt(inst: &SwebenchProInstance, mode: &str) -> String {
    let trimmed = inst.problem_statement.trim();
    // Unwrap at most ONE balanced surrounding pair of quotes (a wrapping
    // artifact) instead of greedily stripping every leading/trailing quote,
    // which mangled statements that legitimately begin or end with a quote.
    let problem = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed)
        .replace("\\n", "\n");

    if mode == "official" {
        format!(
            "[mode: official]\n\n\
             Resolve the issue in the current directory.\n\n\
             ISSUE:\n{problem}\n\n\
             CONTRACT:\n\
             - Valid tool call ONLY, or concise final summary.\n\
             - NO prose before tool XML.\n\
             - Simplest correct patch; verify honestly; do not gold-plate.\n\
             - Do NOT modify test files.\n\
             - Run relevant tests and confirm they pass before finishing."
        )
    } else {
        let tests = coerce_string_list(&inst.selected_test_files_to_run);
        let fail = coerce_string_list(&inst.fail_to_pass);

        let fail_str = if fail.is_empty() {
            "  (none specified)".to_string()
        } else {
            fail.iter()
                .map(|t| format!("  - {}", t))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let test_str = if tests.is_empty() {
            "(none specified)".to_string()
        } else {
            tests.join(", ")
        };

        format!(
            "[mode: diagnostic]\n\n\
             Resolve the issue in the current directory.\n\n\
             ISSUE:\n{problem}\n\n\
             FAIL-TO-PASS TESTS:\n{fail_str}\n\n\
             RELEVANT TEST FILES: {test_str}\n\n\
             CONTRACT:\n\
             - Valid tool call ONLY, or concise final summary.\n\
             - NO prose before tool XML.\n\
             - Simplest correct patch; verify honestly; do not gold-plate.\n\
             - Do NOT modify test files.\n\
             - Run the failing tests and confirm they pass before finishing."
        )
    }
}

#[cfg(all(test, feature = "bench-harness"))]
mod tests {
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
}
