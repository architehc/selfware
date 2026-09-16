//! Dual-analyst trace processing and log diagnosis for RSI.
//!
//! Implements redundant meta-tasks:
//! 1. [`AttributionAnalyst`]: Decouples transient environment friction (timeouts, 429s,
//!    network drops) from actionable reasoning gaps and compilation/syntax failures.
//! 2. [`SafetyInvariantAuditor`]: Verifies that proposed mitigations do not touch
//!    [`PROTECTED_PATHS`](crate::evolution::PROTECTED_PATHS), weaken test assertions,
//!    or introduce unsafe shell commands.
//!
//! A candidate playbook is ONLY generated when both analysts agree on an actionable,
//! safe, recurring failure pattern. Candidate playbooks are stored in
//! `.selfware/skill-candidates/` outside automatic active discovery.

use crate::agent::session_log::SessionLogEvent;
use crate::evolution::is_protected;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Classification of execution failure causes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureCategory {
    /// Transient network errors, rate limits, provider downtime, or socket drops.
    EnvironmentFriction,
    /// Compiler, lint, type-check, or language syntax error.
    SyntaxOrCompilation,
    /// Logical mistake, missing prerequisite, or incorrect tool parameter sequence.
    ReasoningGap,
    /// Tool execution error with explicit error message.
    ToolExecution,
    /// Access to protected paths, invariant regression, or unsafe command.
    SafetyInvariantViolation,
}

/// Finding produced by the Attribution Analyst.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionFinding {
    pub category: FailureCategory,
    pub description: String,
    pub is_actionable: bool,
    pub tool_name: Option<String>,
    pub error_snippet: String,
}

/// Attribution Analyst decoupling environment friction from actionable defects.
#[derive(Debug, Default, Clone)]
pub struct AttributionAnalyst;

impl AttributionAnalyst {
    pub fn new() -> Self {
        Self
    }

    /// Check whether an error string represents transient environment friction.
    pub fn is_environment_friction(msg: &str) -> bool {
        let lower = msg.to_ascii_lowercase();
        lower.contains("rate limit")
            || lower.contains("429")
            || lower.contains("too many requests")
            || lower.contains("timed out")
            || lower.contains("timeout")
            || lower.contains("connection refused")
            || lower.contains("connection reset")
            || lower.contains("dns resolution")
            || lower.contains("502 bad gateway")
            || lower.contains("503 service unavailable")
            || lower.contains("504 gateway timeout")
            || lower.contains("broken pipe")
            || lower.contains("network is unreachable")
    }

    /// Classify a failure message into a [`FailureCategory`].
    pub fn classify(&self, msg: &str, tool: Option<&str>) -> FailureCategory {
        if Self::is_environment_friction(msg) {
            return FailureCategory::EnvironmentFriction;
        }

        let lower = msg.to_ascii_lowercase();
        if lower.contains("error[e")
            || lower.contains("mismatched types")
            || lower.contains("cannot find")
            || lower.contains("expected `")
            || lower.contains("syntax error")
            || lower.contains("parse error")
            || lower.contains("unresolved import")
        {
            return FailureCategory::SyntaxOrCompilation;
        }

        if lower.contains("missing required")
            || lower.contains("invalid argument")
            || lower.contains("prerequisite")
            || lower.contains("not found in scope")
        {
            return FailureCategory::ReasoningGap;
        }

        if tool.is_some() {
            FailureCategory::ToolExecution
        } else {
            FailureCategory::ReasoningGap
        }
    }

    /// Analyze session log events to identify actionable failure patterns.
    pub fn analyze_events(&self, events: &[SessionLogEvent]) -> Vec<AttributionFinding> {
        let mut findings = Vec::new();

        for ev in events {
            if let Some(false) = ev.success {
                let error_text = ev
                    .result
                    .as_deref()
                    .or(ev.input.as_deref())
                    .unwrap_or("unknown error");

                let category = self.classify(error_text, ev.tool_name.as_deref());
                let is_actionable = category != FailureCategory::EnvironmentFriction;

                findings.push(AttributionFinding {
                    category,
                    description: format!(
                        "Failure in {}: {}",
                        ev.tool_name.as_deref().unwrap_or("session"),
                        error_text.chars().take(120).collect::<String>()
                    ),
                    is_actionable,
                    tool_name: ev.tool_name.clone(),
                    error_snippet: error_text.chars().take(300).collect(),
                });
            }
        }

        findings
    }
}

/// Audit result produced by the Safety & Invariant Auditor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyAuditResult {
    /// Safe to synthesize playbook candidate.
    Approved,
    /// Violates safety invariant or touches protected area.
    Rejected { reasons: Vec<String> },
}

/// Safety & Invariant Auditor enforcing protected paths and assertion integrity.
#[derive(Debug, Default, Clone)]
pub struct SafetyInvariantAuditor;

impl SafetyInvariantAuditor {
    pub fn new() -> Self {
        Self
    }

    /// Audit a proposed mitigation playbook against safety invariants.
    pub fn audit_candidate(
        &self,
        name: &str,
        content: &str,
        affected_paths: &[String],
    ) -> SafetyAuditResult {
        let mut reasons = Vec::new();

        // 1. Check affected paths against PROTECTED_PATHS
        for path_str in affected_paths {
            let p = Path::new(path_str);
            if is_protected(p) {
                reasons.push(format!(
                    "Proposed edit touches protected path '{}'",
                    path_str
                ));
            }
        }

        // 2. Check for assertion weakening patterns
        let lower = content.to_ascii_lowercase();
        if lower.contains("remove assertion")
            || lower.contains("delete test")
            || lower.contains("ignore test")
            || lower.contains("#[ignore]")
            || lower.contains("lower threshold")
        {
            reasons.push("Proposed mitigation appears to weaken assertions or drop test cases (AGENTS.md Rule 2 violation)".to_string());
        }

        // 3. Check for dangerous command execution patterns
        if lower.contains("rm -rf /")
            || lower.contains(":(){ :|:& };:")
            || lower.contains("curl ") && lower.contains("| sh")
            || lower.contains("wget ") && lower.contains("| sh")
        {
            reasons.push("Proposed mitigation contains unsafe shell execution command".to_string());
        }

        // 4. Validate name characters
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            reasons.push(format!(
                "Playbook candidate name '{}' contains invalid characters",
                name
            ));
        }

        if reasons.is_empty() {
            SafetyAuditResult::Approved
        } else {
            SafetyAuditResult::Rejected { reasons }
        }
    }
}

/// Consensus decision across Attribution Analyst and Safety Invariant Auditor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusResult {
    /// Both analysts agree: actionable defect diagnosed and verified safe.
    ConsensusReached {
        finding: AttributionFinding,
        candidate_playbook_content: String,
    },
    /// Attribution Analyst classified the error as transient environment friction.
    IgnoredEnvironmentFriction { reason: String },
    /// Safety Invariant Auditor rejected the candidate.
    SafetyRejected { reasons: Vec<String> },
    /// No actionable failures detected in session.
    NoActionableFailure,
}

/// Orchestrator for redundant dual-analyst meta-tasks.
pub struct DualAnalystEvaluator {
    attribution: AttributionAnalyst,
    safety: SafetyInvariantAuditor,
}

impl Default for DualAnalystEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl DualAnalystEvaluator {
    pub fn new() -> Self {
        Self {
            attribution: AttributionAnalyst::new(),
            safety: SafetyInvariantAuditor::new(),
        }
    }

    /// Evaluate session events and propose a safe candidate playbook if consensus is reached.
    pub fn evaluate(
        &self,
        events: &[SessionLogEvent],
        candidate_name: &str,
        affected_paths: &[String],
    ) -> ConsensusResult {
        let findings = self.attribution.analyze_events(events);

        if findings.is_empty() {
            return ConsensusResult::NoActionableFailure;
        }

        // Check if all findings are environment friction
        if findings.iter().all(|f| !f.is_actionable) {
            return ConsensusResult::IgnoredEnvironmentFriction {
                reason: "All observed errors are transient environment friction (network, timeouts, or 429s)".to_string(),
            };
        }

        // Pick the first actionable finding
        let Some(actionable) = findings.into_iter().find(|f| f.is_actionable) else {
            return ConsensusResult::NoActionableFailure;
        };

        // Draft playbook markdown
        let draft_playbook = format!(
            "# Playbook: {}\n\n\
            ## Problem Diagnosis\n\
            Category: {:?}\n\
            Description: {}\n\n\
            ## Observed Error Snippet\n\
            ```\n{}\n```\n\n\
            ## Required Mitigation Steps\n\
            1. Verify inputs and prerequisites before invoking `{}`.\n\
            2. Handle domain error states without assertion weakening.\n\
            3. Run `cargo check` and relevant tests to verify invariant preservation.\n",
            candidate_name,
            actionable.category,
            actionable.description,
            actionable.error_snippet,
            actionable.tool_name.as_deref().unwrap_or("tool")
        );

        // Run Safety Invariant Auditor
        match self
            .safety
            .audit_candidate(candidate_name, &draft_playbook, affected_paths)
        {
            SafetyAuditResult::Approved => {
                let frontmatter = format!(
                    "---\n\
                    name: {}\n\
                    description: {}\n\
                    verified: false\n\
                    candidate: true\n\
                    origin: distilled\n\
                    admitted: false\n\
                    ---\n\n",
                    candidate_name, actionable.description
                );
                let full_content = format!("{}{}", frontmatter, draft_playbook);
                ConsensusResult::ConsensusReached {
                    finding: actionable,
                    candidate_playbook_content: full_content,
                }
            }
            SafetyAuditResult::Rejected { reasons } => ConsensusResult::SafetyRejected { reasons },
        }
    }

    /// Save an agreed candidate playbook to the candidate store directory (`.selfware/skill-candidates/`).
    pub fn save_candidate_playbook(
        candidates_dir: &Path,
        candidate_name: &str,
        content: &str,
    ) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(candidates_dir)?;
        let file_path = candidates_dir.join(format!("{}.md", candidate_name));
        std::fs::write(&file_path, content)?;
        Ok(file_path)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/consolidation/trace_analyst_test.rs"]
mod tests;
