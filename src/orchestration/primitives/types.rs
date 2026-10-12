use std::collections::BTreeSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) const MAX_ARTIFACTS: usize = 8;
pub(crate) const MAX_ROUNDS: u8 = 3;
pub(crate) const MAX_FINDINGS_PER_REPORT: usize = 12;
pub(crate) const MAX_MESSAGES_PER_REPORT: usize = 4;
pub(crate) const MAX_MESSAGE_EVIDENCE: usize = 4;
pub(crate) const MAX_TOTAL_MESSAGES: usize =
    MAX_ARTIFACTS * (MAX_ROUNDS as usize - 1) * MAX_MESSAGES_PER_REPORT;
pub(crate) const MAX_EVIDENCE_QUOTE_TOKENS: usize = 256;
pub(crate) const MAX_IDENTIFIERS_PER_EVIDENCE: usize = 12;
pub(crate) const MAX_IDENTIFIER_BYTES: usize = 64;
pub(crate) const MAX_FOCUS_SYMBOL_BYTES: usize = 128;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum GraphStatus {
    Loaded {
        revision: String,
        sha256: String,
    },
    Missing,
    Rejected {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RepositoryBinding {
    /// Hash of the canonical repository root. The local absolute path is not
    /// emitted in reports, while capabilities still cannot be replayed in a
    /// different checkout with identical files.
    pub repository_id: String,
    /// Git HEAD tree when the root belongs to a repository. Dirty bytes are
    /// intentionally bound separately by each assignment's `base_sha256`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_tree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_sha256: Option<String>,
    pub activation_census_sha256: String,
    pub activation_census_complete: bool,
    pub activation_ranking_complete: bool,
    pub working_set_sha256: String,
    pub plan_sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ReadAttestationSummary {
    pub attestation_id: String,
    pub exact_snapshot_read: bool,
    pub write: bool,
    pub execute: bool,
    pub network: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct PrimitiveAssignment {
    /// Opaque run-local identity used in model-visible routing.
    pub key: String,
    /// Advisory graph identity. It never grants read or message authority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_node_id: Option<String>,
    pub path: String,
    pub classification: String,
    pub base_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_symbol: Option<String>,
    pub reasons: Vec<String>,
    pub read_attestation: ReadAttestationSummary,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ActivationPlan {
    pub run_id: String,
    pub task_sha256: String,
    pub selection_mode: String,
    pub max_rounds: u8,
    pub graph: GraphStatus,
    pub binding: RepositoryBinding,
    pub assignments: Vec<PrimitiveAssignment>,
    /// Message authority selected by the trusted activation policy. The
    /// broker treats these routes as opaque grants and never derives routes
    /// from advisory graph data.
    pub allowed_routes: BTreeSet<AllowedRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum PlanError {
    #[error("artifact plan has no run identity")]
    MissingRunIdentity,
    #[error("artifact plan has {0} assignments; maximum is {MAX_ARTIFACTS}")]
    TooManyArtifacts(usize),
    #[error("artifact plan has {0} rounds; expected 1 through {MAX_ROUNDS}")]
    InvalidRounds(u8),
    #[error("duplicate artifact key '{0}'")]
    DuplicateKey(String),
    #[error("duplicate artifact owner for '{0}'")]
    DuplicateOwner(String),
    #[error("duplicate read attestation '{0}'")]
    DuplicateAttestation(String),
    #[error("message route '{from}' -> '{to}' names an inactive artifact")]
    InactiveRoute { from: String, to: String },
    #[error("message route '{0}' targets its own sender")]
    SelfRoute(String),
    #[error("unsafe artifact path '{path}': {reason}")]
    UnsafePath { path: String, reason: String },
    #[error("artifact '{0}' has an invalid or oversized focus symbol")]
    InvalidFocusSymbol(String),
    #[error("invalid SHA-256 binding in {0}")]
    InvalidDigest(&'static str),
}

impl ActivationPlan {
    pub(crate) fn validate(&self) -> Result<(), PlanError> {
        if self.run_id.trim().is_empty() {
            return Err(PlanError::MissingRunIdentity);
        }
        if self.assignments.len() > MAX_ARTIFACTS {
            return Err(PlanError::TooManyArtifacts(self.assignments.len()));
        }
        if !(1..=MAX_ROUNDS).contains(&self.max_rounds) {
            return Err(PlanError::InvalidRounds(self.max_rounds));
        }
        for (name, digest) in [
            ("task", self.task_sha256.as_str()),
            ("repository", self.binding.repository_id.as_str()),
            (
                "activation census",
                self.binding.activation_census_sha256.as_str(),
            ),
            ("working set", self.binding.working_set_sha256.as_str()),
            ("plan", self.binding.plan_sha256.as_str()),
        ] {
            if !is_sha256(digest) {
                return Err(PlanError::InvalidDigest(name));
            }
        }
        if self
            .binding
            .graph_sha256
            .as_deref()
            .is_some_and(|digest| !is_sha256(digest))
        {
            return Err(PlanError::InvalidDigest("graph"));
        }
        for assignment in &self.assignments {
            if !is_sha256(&assignment.base_sha256) {
                return Err(PlanError::InvalidDigest("artifact"));
            }
            validate_relative_artifact_path(&assignment.path)?;
            if assignment
                .focus_symbol
                .as_deref()
                .is_some_and(|symbol| !valid_focus_symbol(symbol))
            {
                return Err(PlanError::InvalidFocusSymbol(assignment.key.clone()));
            }
        }

        let mut keys = BTreeSet::new();
        let mut owners = BTreeSet::new();
        let mut grants = BTreeSet::new();
        for assignment in &self.assignments {
            if !keys.insert(assignment.key.clone()) {
                return Err(PlanError::DuplicateKey(assignment.key.clone()));
            }
            if !owners.insert(assignment.path.clone()) {
                return Err(PlanError::DuplicateOwner(assignment.path.clone()));
            }
            if !grants.insert(assignment.read_attestation.attestation_id.clone()) {
                return Err(PlanError::DuplicateAttestation(
                    assignment.read_attestation.attestation_id.clone(),
                ));
            }
            if !assignment.read_attestation.exact_snapshot_read
                || assignment.read_attestation.write
                || assignment.read_attestation.execute
                || assignment.read_attestation.network
            {
                return Err(PlanError::UnsafePath {
                    path: assignment.path.clone(),
                    reason: "read attestation is not strictly read-only".into(),
                });
            }
        }
        for route in &self.allowed_routes {
            if route.from == route.to {
                return Err(PlanError::SelfRoute(route.from.clone()));
            }
            if self.assignment(&route.from).is_none() || self.assignment(&route.to).is_none() {
                return Err(PlanError::InactiveRoute {
                    from: route.from.clone(),
                    to: route.to.clone(),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn assignment(&self, key: &str) -> Option<&PrimitiveAssignment> {
        self.assignments.iter().find(|item| item.key == key)
    }

    pub(crate) fn route_allowed(&self, from: &str, to: &str) -> bool {
        self.allowed_routes.contains(&AllowedRoute {
            from: from.to_string(),
            to: to.to_string(),
        })
    }
}

pub(crate) fn valid_focus_symbol(symbol: &str) -> bool {
    !symbol.is_empty()
        && symbol.len() <= MAX_FOCUS_SYMBOL_BYTES
        && symbol
            .chars()
            .all(|character| character == '_' || character.is_alphanumeric())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AllowedRoute {
    pub from: String,
    pub to: String,
}

pub(crate) fn validate_relative_artifact_path(path: &str) -> Result<(), PlanError> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains('\\')
        || path.chars().any(|character| {
            character.is_control() || crate::evolve::context_trust::is_hidden_unicode(character)
        })
    {
        return Err(PlanError::UnsafePath {
            path: path.to_string(),
            reason: "path must be a non-empty normalized repository-relative path".into(),
        });
    }
    let parsed = Path::new(path);
    for component in parsed.components() {
        let Component::Normal(name) = component else {
            return Err(PlanError::UnsafePath {
                path: path.to_string(),
                reason: "absolute, parent, prefix, and current-directory components are forbidden"
                    .into(),
            });
        };
        let lowered = name.to_string_lossy().to_ascii_lowercase();
        if crate::safety::source_context::is_private_discovery_dir_name(name) {
            return Err(PlanError::UnsafePath {
                path: path.to_string(),
                reason: format!("private state component '{lowered}' is forbidden"),
            });
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Severity {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawFinding {
    pub line: usize,
    pub severity: Severity,
    pub title: String,
    pub evidence_quote: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawEvidenceClaim {
    pub line: usize,
    pub quote: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MessageKind {
    DependencyQuestion,
    InterfaceConstraint,
    TestImpact,
    Risk,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawMessageProposal {
    pub to: String,
    pub kind: MessageKind,
    pub evidence: Vec<RawEvidenceClaim>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawPrimitiveReport {
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<RawFinding>,
    #[serde(default)]
    pub messages: Vec<RawMessageProposal>,
}

/// Broker-produced evidence metadata. Source excerpts and model-provided
/// quotes never cross the primitive boundary; bounded identifiers may.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub(crate) struct BrokeredEvidence {
    pub line: usize,
    pub sha256: String,
    pub identifiers: Vec<String>,
}

/// A metadata-only message stamped by the trusted broker. Every identity and
/// binding field comes from the frozen plan rather than model output.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct PrimitiveMessage {
    pub sequence: u64,
    pub round: u8,
    pub from: String,
    pub to: String,
    pub kind: MessageKind,
    pub sender_path: String,
    pub sender_sha256: String,
    pub evidence: Vec<BrokeredEvidence>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct TaintedText {
    pub text: String,
    pub provenance: &'static str,
    pub trusted: bool,
    #[serde(default)]
    pub neutralized_findings: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EvidenceRef {
    pub path: String,
    pub line: usize,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EvidenceMatchedFinding {
    pub reported_severity: Severity,
    pub reported_title: TaintedText,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct SourceScope {
    pub start_line: usize,
    pub end_line: usize,
    pub total_lines: usize,
    pub content_tokens: usize,
    pub whole_file: bool,
    pub trust_gate_neutralized: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkerCallStatus {
    Analyzed,
    FormatFailed,
    TransportFailed,
    TimedOut,
    ContextUnavailable,
    BudgetSkipped,
    TokenAccountingUnavailable,
    CostAccountingUnavailable,
    KillswitchStopped,
    StaleBeforeCall,
    StaleAfterCall,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub(crate) struct AccountedUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub(crate) struct UsageReport {
    /// Usage included in the run ledger. Consult the completeness flags before
    /// treating zero or partial values as the full provider-side consumption.
    pub accounted: AccountedUsage,
    /// True only when every attempt reported prompt, completion, and total.
    /// A measured fallback does not make this true.
    pub token_accounting_complete: bool,
    pub unknown_token_attempts: usize,
    /// Attempts that charged a measured fallback. Omitted from JSON when zero
    /// so older report readers keep the previous shape.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fallback_token_attempts: usize,
    /// Tokens charged from measured fallbacks. They are not provider usage.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fallback_tokens: usize,
    pub cost_accounting_complete: bool,
    pub unmetered_cost_attempts: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct WorkerTurnReport {
    pub artifact: String,
    pub path: String,
    pub round: u8,
    pub status: WorkerCallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<TaintedText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_matched_findings: Vec<EvidenceMatchedFinding>,
    pub rejected_findings: usize,
    pub messages_sent: usize,
    pub rejected_messages: usize,
    pub source: SourceScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounted_usage: Option<AccountedUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ArtifactRunStatus {
    AnalysisComplete,
    AnalysisPartial,
    AnalysisFailed,
    NotRun,
    BindingChanged,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskOutcome {
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceEgress {
    LocalEndpoint,
    RemoteExplicitlyAuthorized,
    RemotePlaintextExplicitlyAuthorized,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AuthorityReport {
    pub read_only: bool,
    pub model_tools: bool,
    pub model_filesystem_write: bool,
    pub model_command_execution: bool,
    pub model_network_tools: bool,
    pub source_egress: SourceEgress,
    pub source_content_transmission_attempted: bool,
    pub relative_paths_transmission_attempted: bool,
    pub content_hashes_transmission_attempted: bool,
    pub acceptance_owner: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct ArtifactRunReport {
    pub status: ArtifactRunStatus,
    pub task_outcome: TaskOutcome,
    pub task: String,
    pub authority: AuthorityReport,
    pub evidence_validation: &'static str,
    pub plan: ActivationPlan,
    pub rounds_requested: u8,
    pub rounds_completed: u8,
    pub turns_planned: usize,
    pub calls_scheduled: usize,
    pub turns_analyzed: usize,
    pub evidence_matched_findings: usize,
    pub rejected_findings: usize,
    pub messages: Vec<PrimitiveMessage>,
    pub turns: Vec<WorkerTurnReport>,
    pub usage: UsageReport,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

impl ArtifactRunReport {
    pub(crate) fn analysis_succeeded(&self) -> bool {
        self.status == ArtifactRunStatus::AnalysisComplete
    }

    pub(crate) fn render_text(&self, quiet: bool) -> String {
        let mut out = String::new();
        use std::fmt::Write as _;
        let _ = writeln!(
            out,
            "Artifact harness: {:?}; {} of {} scheduled model calls analyzed ({} turn(s) planned); {} evidence-checked finding(s)",
            self.status,
            self.turns_analyzed,
            self.calls_scheduled,
            self.turns_planned,
            self.evidence_matched_findings
        );
        let _ = writeln!(
            out,
            "Authority: read-only, tool-free model calls; task outcome not evaluated; source egress {:?}; payload transmission attempted {}",
            self.authority.source_egress,
            self.authority.source_content_transmission_attempted
        );
        let _ = writeln!(
            out,
            "Binding: plan {}, census {}, working set {}",
            &self.plan.binding.plan_sha256[..12],
            &self.plan.binding.activation_census_sha256[..12],
            &self.plan.binding.working_set_sha256[..12]
        );
        let _ = writeln!(
            out,
            "Selection census: {}",
            if self.plan.binding.activation_census_complete {
                "complete within the configured discovery limits"
            } else {
                "incomplete because a configured discovery limit or checked discovery failure was encountered"
            }
        );
        let _ = writeln!(
            out,
            "Activation ranking: {}",
            if self.plan.binding.activation_ranking_complete {
                "complete within the measured token limits"
            } else {
                "incomplete at a measured token limit"
            }
        );
        match &self.plan.graph {
            GraphStatus::Loaded { sha256, .. } => {
                let _ = writeln!(out, "Graph: loaded ({})", &sha256[..12]);
            }
            GraphStatus::Missing => {
                let _ = writeln!(out, "Graph: missing; activation used the bounded localizer");
            }
            GraphStatus::Rejected { reason, .. } => {
                let _ = writeln!(out, "Graph: rejected — {}", inline_text(reason));
            }
        }
        let fallback_tokens = self.usage.fallback_tokens;
        let fallback_token_attempts = self.usage.fallback_token_attempts;
        let fallback_note = if fallback_tokens > 0 || fallback_token_attempts > 0 {
            format!(
                "; {fallback_tokens} measured fallback token(s) across {fallback_token_attempts} attempt(s), not provider usage"
            )
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "Accounting: {} token(s) accounted, coverage {} ({} unknown attempt(s){fallback_note}); cost {} ({} unmetered attempt(s))",
            self.usage.accounted.total_tokens,
            if self.usage.token_accounting_complete {
                "complete"
            } else {
                "incomplete"
            },
            self.usage.unknown_token_attempts,
            if self.usage.cost_accounting_complete {
                "complete"
            } else {
                "incomplete"
            },
            self.usage.unmetered_cost_attempts
        );
        if quiet {
            for turn in &self.turns {
                write_turn_coverage(&mut out, turn);
            }
            if let Some(reason) = &self.stop_reason {
                let _ = writeln!(out, "Stopped: {}", inline_text(reason));
            }
            return out;
        }
        for turn in &self.turns {
            let _ = writeln!(
                out,
                "\n{} ({}) — {:?}",
                inline_text(&turn.path),
                turn.artifact,
                turn.status
            );
            write_turn_coverage(&mut out, turn);
            if let Some(summary) = &turn.summary {
                let _ = writeln!(
                    out,
                    "  analysis (untrusted): {}",
                    inline_text(&summary.text)
                );
            }
            for finding in &turn.evidence_matched_findings {
                let _ = writeln!(
                    out,
                    "  model-reported {:?} {}:{} — {} (untrusted assessment; quote matched)",
                    finding.reported_severity,
                    inline_text(&finding.evidence.path),
                    finding.evidence.line,
                    inline_text(&finding.reported_title.text)
                );
            }
            if let Some(error) = &turn.error {
                let _ = writeln!(out, "  unavailable: {}", inline_text(error));
            }
        }
        if !self.messages.is_empty() {
            let _ = writeln!(
                out,
                "\nBrokered metadata messages: {} (no model prose, quote, or source excerpt forwarded)",
                self.messages.len()
            );
        }
        if !self.plan.warnings.is_empty() {
            let _ = writeln!(out, "\nActivation warnings:");
            for warning in &self.plan.warnings {
                let _ = writeln!(out, "  - {}", inline_text(warning));
            }
        }
        if let Some(reason) = &self.stop_reason {
            let _ = writeln!(out, "\nStopped: {}", inline_text(reason));
        }
        out
    }
}

fn write_turn_coverage(out: &mut String, turn: &WorkerTurnReport) {
    use std::fmt::Write as _;
    if turn.status != WorkerCallStatus::Analyzed {
        return;
    }
    let _ = writeln!(
        out,
        "  source lines {}-{} of {}",
        turn.source.start_line, turn.source.end_line, turn.source.total_lines
    );
    if turn.source.trust_gate_neutralized > 0 {
        let _ = writeln!(
            out,
            "  redacted findings: {}",
            turn.source.trust_gate_neutralized
        );
    }
}

fn inline_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() || crate::evolve::context_trust::is_hidden_unicode(character)
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "../../../tests/unit/orchestration/primitives/types_test.rs"]
mod tests;
