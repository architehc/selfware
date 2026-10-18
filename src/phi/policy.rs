//! Policy-driven Fox Digital Twin ("Phi") meta-agent engine.
//!
//! Bridges the revision-keyed [`Ledger`] and tool
//! [`observer`](super::observer) with the animated Fox Digital Twin in `/phi/`:
//!
//! 1. **Outbound Mediation ([`PhiMediator::evaluate_outbound`])**:
//!    Inspects measured [`Ledger`] obligations and session friction before a
//!    prompt is sent to Selfware. Never silently rewrites the user's prompt:
//!    returns explicit [`PromptAugmentation`]s (`DemandDisproof`,
//!    `RequireEvidence`, `NarrowScope` per `AGENTS.md` Rule 5) and an optional
//!    overridable [`PolicyGate`] (`RequireReading`).
//! 2. **Inbound Annotation ([`PhiMediator::evaluate_inbound`])**:
//!    Flags [`AnnotationKind::Capitulation`], [`AnnotationKind::UnsourcedClaim`],
//!    and [`AnnotationKind::ScopeCreep`] when model responses assert without
//!    checkable citations (`file:line`, line ranges, commit SHA, or test names).
//! 3. **Idle Stewardship ([`PhiSteward::propose`])**:
//!    Ranks what to do next when Selfware pauses:
//!    `Repair (0) > Verify (1) > Repay (2) > Orient (3) > Explore (4)`.
//!    Verification always outranks production; without measured evidence in the
//!    [`Ledger`] or observation journal, `propose()` returns an empty list.
//! 4. **Attention Economy & Fox Twin Posture ([`PhiPresence`], [`FoxPosture`])**:
//!    Routes signals across `Silent -> Ambient -> Glance -> Interrupt` so
//!    ignoring makes Phi quieter (never louder), and maps ledger health directly
//!    onto the animated Fox Digital Twin's posture (`Skeptical`, `Unimpressed`,
//!    `Error`, `Working`, `Flow`, `Guard`, `Greeting`).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use super::activity::ActivityPhase;
use super::ledger::{Ledger, ObligationKind};
use super::observer::ObservationRecord;

/// Operating mode of the Phi meta-agent policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PhiPolicyMode {
    /// Passively record ledger obligations and publish Fox Twin posture/receipts.
    #[default]
    ObserveOnly,
    /// Surface inbound annotations and idle steward proposals without altering prompts.
    Advise,
    /// Append labelled `[Phi — loop constraints]` augmentations to outbound prompts.
    Mediate,
    /// Enforce [`PolicyGate`] pauses when unverified debt exceeds policy thresholds.
    Enforce,
}

/// Declarative policy governing how the Fox Digital Twin meta-agent interacts
/// with Selfware.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhiPolicy {
    /// Active enforcement level.
    pub mode: PhiPolicyMode,
    /// Unreviewed diff count that triggers `RequireEvidence` (`file:line` citations).
    pub unreviewed_evidence_threshold: usize,
    /// Unverified line count that triggers `NarrowScope` (Rule 5 bug-class naming).
    pub debt_narrow_scope_lines: usize,
    /// Unverified line count that triggers a `RequireReading` gate.
    pub debt_gate_lines: usize,
    /// Unsourced reversal count that triggers `DemandDisproof`.
    pub capitulation_threshold: usize,
    /// Generated line count in a single turn that flags `ScopeCreep`.
    pub scope_line_limit: usize,
    /// Generated line count threshold exposed to the `/phi/` web companion (`bulk_generation_lines`).
    #[serde(default = "default_bulk_generation_lines")]
    pub bulk_generation_lines: usize,
    /// Maximum number of `Interrupt` cards Phi may spend per session unless refunded.
    pub interrupt_budget: usize,
    /// Idle quiet window (ms) before the web Steward speaks (`steward_quiet_ms`).
    #[serde(default = "default_steward_quiet_ms")]
    pub steward_quiet_ms: u64,
    /// Minimum quiet window (ms) after user/agent activity before `Glance` or `Interrupt`.
    pub quiet_after_activity_ms: u64,
    /// Minimum cooldown (ms) between spoken utterances (`Glance` or `Interrupt`).
    pub glance_cooldown_ms: u64,
    /// Maximum number of ranked proposals returned by [`PhiSteward`].
    pub max_proposals: usize,
}

fn default_bulk_generation_lines() -> usize {
    400
}

fn default_steward_quiet_ms() -> u64 {
    12_000
}

impl Default for PhiPolicy {
    fn default() -> Self {
        Self {
            mode: PhiPolicyMode::ObserveOnly,
            unreviewed_evidence_threshold: 2,
            debt_narrow_scope_lines: 80,
            debt_gate_lines: 160,
            capitulation_threshold: 2,
            scope_line_limit: 400,
            bulk_generation_lines: default_bulk_generation_lines(),
            interrupt_budget: 3,
            steward_quiet_ms: default_steward_quiet_ms(),
            quiet_after_activity_ms: 4_000,
            glance_cooldown_ms: 45_000,
            max_proposals: 4,
        }
    }
}

impl PhiPolicy {
    /// Construct a [`PhiPolicy`] from default thresholds, optionally overriding
    /// `mode` via `SELFWARE_PHI_MODE` (`observe`, `advise`, `mediate`, `enforce`).
    pub fn from_env() -> Self {
        let mut policy = Self::default();
        if let Ok(raw) = std::env::var("SELFWARE_PHI_MODE") {
            policy.mode = match raw.trim().to_ascii_lowercase().as_str() {
                "observe" | "observe_only" | "shadow" => PhiPolicyMode::ObserveOnly,
                "advise" | "advisor" => PhiPolicyMode::Advise,
                "mediate" | "mediator" => PhiPolicyMode::Mediate,
                "enforce" | "strict" | "gate" => PhiPolicyMode::Enforce,
                _ => PhiPolicyMode::ObserveOnly,
            };
        }
        policy
    }

    /// Compute a bounded `[0.0, 1.0]` verification debt ratio from measured
    /// [`Ledger`] obligations (`AGENTS.md` Rule 4: derived from actual obligation
    /// counts and line deltas, never from ungrounded guesses).
    pub fn measured_debt_ratio(&self, ledger: &Ledger) -> f64 {
        let outstanding = ledger.outstanding();
        if outstanding.is_empty() {
            return 0.0;
        }
        let unreviewed_lines = ledger.outstanding_lines(ObligationKind::UnreviewedChange);
        let untested_lines = ledger.outstanding_lines(ObligationKind::UnconfirmedCoverage);
        let removal_obligations = outstanding
            .iter()
            .filter(|o| o.kind == ObligationKind::BrokenByRemoval)
            .count();
        let unknown_size = ledger.outstanding_unknown_size(ObligationKind::UnreviewedChange)
            + ledger.outstanding_unknown_size(ObligationKind::UnconfirmedCoverage);

        let effective_lines = unreviewed_lines
            .max(untested_lines)
            .saturating_add(removal_obligations * 40)
            .saturating_add(unknown_size * 40);
        let scale = self.debt_gate_lines.max(1) as f64;
        (effective_lines as f64 / scale).clamp(0.0, 1.0)
    }
}

// ============================================================================
// 1. Mediator (Outbound Prompt Augmentation + Gates & Inbound Annotation)
// ============================================================================

/// Kind of outbound prompt mediation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediationKind {
    DemandDisproof,
    RequireEvidence,
    NarrowScope,
    RequireReading,
}

/// A labelled, inspectable constraint proposed for an outbound prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptAugmentation {
    pub kind: MediationKind,
    pub reason: String,
    pub text: String,
}

/// An overridable gate raised before generating more code on top of unverified debt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyGate {
    pub id: String,
    pub kind: MediationKind,
    pub question: String,
    pub reason: String,
}

/// Result of evaluating an outbound prompt against [`PhiPolicy`] and [`Ledger`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundMediation {
    /// Original prompt text, always preserved unchanged.
    pub prompt: String,
    /// Explicit augmentations with their measured reasons.
    pub augmentations: Vec<PromptAugmentation>,
    /// Optional gate when debt exceeds `policy.debt_gate_lines`.
    pub gate: Option<PolicyGate>,
}

impl OutboundMediation {
    /// Compose the outbound prompt with `[Phi — loop constraints]` when
    /// `augmentations` is non-empty.
    pub fn compose(&self) -> String {
        if self.augmentations.is_empty() {
            return self.prompt.clone();
        }
        let additions: Vec<&str> = self.augmentations.iter().map(|a| a.text.as_str()).collect();
        format!(
            "{}\n\n[Phi — loop constraints]\n{}",
            self.prompt,
            additions.join("\n")
        )
    }
}

/// Kind of inbound model-response annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Capitulation,
    UnsourcedClaim,
    ScopeCreep,
}

/// Severity level for annotations and presence routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSeverity {
    Low,
    Medium,
    High,
}

impl SignalSeverity {
    /// Evidence bar (number of observations required before speaking).
    pub fn base_quota(self) -> usize {
        match self {
            SignalSeverity::High => 2,
            SignalSeverity::Medium => 3,
            SignalSeverity::Low => 4,
        }
    }
}

/// Annotation attached to an inbound model turn before the operator acts on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboundAnnotation {
    pub kind: AnnotationKind,
    pub severity: SignalSeverity,
    pub excerpt: Option<String>,
    pub note: String,
}

/// Bidirectional mediator between the operator/supervisor and Selfware.
#[derive(Debug, Clone, Default)]
pub struct PhiMediator {
    pub policy: PhiPolicy,
    overridden_gates: BTreeSet<String>,
}

impl PhiMediator {
    pub fn new(policy: PhiPolicy) -> Self {
        Self {
            policy,
            overridden_gates: BTreeSet::new(),
        }
    }

    /// Record an explicit operator override for a gate ID (e.g. `"gate:debt"`).
    /// Once overridden, Phi does not ask twice in the same session.
    pub fn override_gate(&mut self, gate_id: impl Into<String>) {
        self.overridden_gates.insert(gate_id.into());
    }

    /// Evaluate an outbound prompt against the measured [`Ledger`] and session
    /// capitulation count.
    pub fn evaluate_outbound(
        &self,
        prompt: &str,
        ledger: &Ledger,
        capitulations: usize,
    ) -> OutboundMediation {
        let mut augmentations = Vec::new();
        let mut gate = None;

        if capitulations >= self.policy.capitulation_threshold {
            augmentations.push(PromptAugmentation {
                kind: MediationKind::DemandDisproof,
                reason: format!(
                    "{capitulations} reversals in this session carried no new evidence."
                ),
                text: "Before answering: state what observation would prove your previous \
                       answer wrong. If you cannot name one, say so explicitly instead of \
                       revising your position."
                    .to_string(),
            });
        }

        let unreviewed_count = ledger
            .outstanding()
            .into_iter()
            .filter(|o| o.kind == ObligationKind::UnreviewedChange)
            .count();
        if unreviewed_count >= self.policy.unreviewed_evidence_threshold {
            augmentations.push(PromptAugmentation {
                kind: MediationKind::RequireEvidence,
                reason: format!(
                    "{unreviewed_count} diffs from this session have not been read by anyone."
                ),
                text: "Cite file:line for every claim about existing behaviour. Where you \
                       have not read the code you are describing, say that rather than \
                       inferring it."
                    .to_string(),
            });
        }

        let unreviewed_lines = ledger.outstanding_lines(ObligationKind::UnreviewedChange);
        let untested_lines = ledger.outstanding_lines(ObligationKind::UnconfirmedCoverage);
        let unverified_lines = unreviewed_lines.max(untested_lines);
        let debt_ratio = self.policy.measured_debt_ratio(ledger);

        if unverified_lines >= self.policy.debt_narrow_scope_lines || debt_ratio > 0.55 {
            augmentations.push(PromptAugmentation {
                kind: MediationKind::NarrowScope,
                reason: format!(
                    "Unverified debt is {:.2} ({unverified_lines} unverified lines); broad changes here are hard to review.",
                    debt_ratio
                ),
                text: "State the bug class before changing anything, and cover every instance \
                       of it — a fix applied to one of several implementations leaves the \
                       others silently wrong. Keep everything OUTSIDE that class out of this \
                       change, and list the files you intend to touch first so the review has \
                       a shape."
                    .to_string(),
            });
        }

        let gate_id = "gate:debt";
        if (unverified_lines >= self.policy.debt_gate_lines || debt_ratio > 0.72)
            && !self.overridden_gates.contains(gate_id)
        {
            gate = Some(PolicyGate {
                id: gate_id.to_string(),
                kind: MediationKind::RequireReading,
                question:
                    "Nothing has read or verified the last few changes. Generate more on top of them anyway?"
                        .to_string(),
                reason: format!(
                    "debt {:.2} · {} unreviewed diffs ({} unverified lines)",
                    debt_ratio, unreviewed_count, unverified_lines
                ),
            });
        }

        OutboundMediation {
            prompt: prompt.to_string(),
            augmentations,
            gate,
        }
    }

    /// Annotate an inbound model response for unsourced claims, sycophantic
    /// capitulation after pushback, or scope creep.
    pub fn evaluate_inbound(
        &self,
        response: &str,
        after_pushback: bool,
        generated_lines: usize,
    ) -> Vec<InboundAnnotation> {
        let mut annotations = Vec::new();
        let sourced = response_is_sourced(response);

        if after_pushback && !sourced && response_capitulates(response) {
            annotations.push(InboundAnnotation {
                kind: AnnotationKind::Capitulation,
                severity: SignalSeverity::High,
                excerpt: None,
                note: "It agreed and changed position without citing anything. Agreement is \
                       not verification — the earlier answer may have been the correct one."
                    .to_string(),
            });
        }

        if !sourced {
            if let Some(excerpt) = first_unsourced_claim(response) {
                annotations.push(InboundAnnotation {
                    kind: AnnotationKind::UnsourcedClaim,
                    severity: SignalSeverity::Medium,
                    excerpt: Some(excerpt.clone()),
                    note: format!(
                        "\"{excerpt}\" is asserted, not shown. Ask what it checked to know that."
                    ),
                });
            }
        }

        if generated_lines > self.policy.scope_line_limit {
            annotations.push(InboundAnnotation {
                kind: AnnotationKind::ScopeCreep,
                severity: SignalSeverity::Medium,
                excerpt: None,
                note: format!(
                    "{generated_lines} lines came back from one prompt. That is more than \
                     anyone reviews carefully, and it is where the expensive mistakes hide."
                ),
            });
        }

        annotations
    }
}

/// Shared citation detector matching `phi_friction.js` (`CITATION_MARKERS`).
/// A bare "tests passed" is NOT a citation; concrete `file.rs:123`, `L10-L20`,
/// commit hashes, or `test_foo ... ok` are required.
pub fn response_is_sourced(text: &str) -> bool {
    static CITATION_RE: OnceLock<Regex> = OnceLock::new();
    let re = CITATION_RE.get_or_init(|| {
        Regex::new(
            r"(?ix)
            \b[\w./-]+\.(?:rs|js|ts|py|toml|md|json|sh|c|h|go):\d+\b
            | \b(?:line|lines)\s+\d+(?:\s*[-–]\s*\d+)?\b
            | \bL\d+(?:-L?\d+)?\b
            | \b[0-9a-f]{7,40}\b
            | \btest\s+[\w:]+\s+\.\.\.\s+(?:ok|FAILED)\b
            ",
        )
        .expect("valid citation regex")
    });
    re.is_match(text)
}

fn response_capitulates(text: &str) -> bool {
    static CAPITULATION_RE: OnceLock<Regex> = OnceLock::new();
    let re = CAPITULATION_RE.get_or_init(|| {
        Regex::new(r"(?i)\byou(?:'|\s+a)?re\s+(?:absolutely\s+)?right\b|\bgood catch\b|\bmy (?:apologies|mistake)\b")
            .expect("valid capitulation regex")
    });
    re.is_match(text)
}

fn first_unsourced_claim(text: &str) -> Option<String> {
    static UNSOURCED_RE: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = UNSOURCED_RE.get_or_init(|| {
        vec![
            Regex::new(
                r"(?i)\b(?:should|ought to|will probably|likely)\s+(?:work|be fine|fix|handle)\b",
            )
            .expect("valid unsourced regex 1"),
            Regex::new(r"(?i)\bi\s+(?:believe|think|assume|suspect)\b")
                .expect("valid unsourced regex 2"),
            Regex::new(r"(?i)\b(?:this|that)\s+is\s+(?:correct|right|fine|safe)\b")
                .expect("valid unsourced regex 3"),
            Regex::new(r"(?i)\bnow\s+(?:works|fixed|resolved)\b").expect("valid unsourced regex 4"),
        ]
    });
    for re in patterns {
        if let Some(m) = re.find(text) {
            return Some(m.as_str().to_string());
        }
    }
    None
}

// ============================================================================
// 2. Steward (Evidence-Backed Idle Proposals)
// ============================================================================

/// Strict priority order of Steward proposals:
/// `Repair (0) < Verify (1) < Repay (2) < Orient (3) < Explore (4)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    /// Something is red right now (failed test run or broken removal obligation).
    Repair = 0,
    /// Code was produced, but nothing has reviewed or covered it.
    Verify = 1,
    /// Artifacts or unattributed mutations left behind by fast generation.
    Repay = 2,
    /// Session state or check status is incomplete/unknown.
    Orient = 3,
    /// Books are clear; genuinely free to pick up the next piece of work.
    Explore = 4,
}

impl ProposalKind {
    pub fn rank(self) -> u8 {
        self as u8
    }
}

/// An evidence-backed proposal for what Selfware or the operator should do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StewardProposal {
    pub id: String,
    pub kind: ProposalKind,
    pub rank: u8,
    pub title: String,
    pub rationale: String,
    pub evidence: Vec<String>,
    pub target_path: Option<String>,
    pub suggested_prompt: String,
}

/// Idle-turn steward that ranks verification and repair ahead of new production.
#[derive(Debug, Clone, Default)]
pub struct PhiSteward {
    pub max_proposals: usize,
    dismissed: BTreeSet<String>,
}

impl PhiSteward {
    pub fn new(max_proposals: usize) -> Self {
        Self {
            max_proposals: max_proposals.max(1),
            dismissed: BTreeSet::new(),
        }
    }

    pub fn dismiss(&mut self, id: impl Into<String>) {
        self.dismissed.insert(id.into());
    }

    pub fn reset(&mut self) {
        self.dismissed.clear();
    }

    /// Generate ranked, evidence-backed proposals from the current [`Ledger`]
    /// and observation journal. Returns an empty Vec if there is no evidence.
    pub fn propose(
        &self,
        ledger: &Ledger,
        observations: &[ObservationRecord],
    ) -> Vec<StewardProposal> {
        let mut out = Vec::new();

        // 1. REPAIR: Failed verification runs in the observation journal.
        let failed_runs: Vec<&ObservationRecord> = observations
            .iter()
            .filter(|r| r.outcome.as_deref() == Some("Failed"))
            .collect();
        if !failed_runs.is_empty() {
            let last_failed = failed_runs.last().unwrap();
            let cmd_label = last_failed
                .command
                .as_deref()
                .unwrap_or(last_failed.tool.as_str());
            out.push(StewardProposal {
                id: format!("repair:run:{}", last_failed.turn),
                kind: ProposalKind::Repair,
                rank: ProposalKind::Repair.rank(),
                title: format!(
                    "Fix {} recorded failing run{}",
                    failed_runs.len(),
                    if failed_runs.len() == 1 { "" } else { "s" }
                ),
                rationale: "A red check is the cheapest signal available. Everything built on top of a failing run compounds the repair cost."
                    .to_string(),
                evidence: vec![
                    format!("{} failed run(s)", failed_runs.len()),
                    format!("turn {} ({})", last_failed.turn, cmd_label),
                ],
                target_path: None,
                suggested_prompt: format!(
                    "Diagnose the failing check from turn {} (`{}`) and propose the minimal fix.",
                    last_failed.turn, cmd_label
                ),
            });
        }

        // 2. REPAIR / VERIFY: Outstanding obligations in the revision-keyed ledger.
        let outstanding = ledger.outstanding();
        let broken_removals: Vec<_> = outstanding
            .iter()
            .filter(|o| o.kind == ObligationKind::BrokenByRemoval)
            .collect();
        if !broken_removals.is_empty() {
            let first = broken_removals[0];
            let path_str = first.path.display().to_string();
            out.push(StewardProposal {
                id: format!("repair:removal:{path_str}"),
                kind: ProposalKind::Repair,
                rank: ProposalKind::Repair.rank(),
                title: format!("Verify dependents after removing {path_str}"),
                rationale: "Deleting code retires obligations on the deleted lines but raises a regression obligation on everything that referenced them."
                    .to_string(),
                evidence: broken_removals
                    .iter()
                    .take(3)
                    .map(|o| format!("removed {}", o.path.display()))
                    .collect(),
                target_path: Some(path_str.clone()),
                suggested_prompt: format!(
                    "Check all dependents of removed path `{path_str}` and run a scoped test to confirm nothing broke."
                ),
            });
        }

        let unreviewed: Vec<_> = outstanding
            .iter()
            .filter(|o| o.kind == ObligationKind::UnreviewedChange)
            .collect();
        let unreviewed_lines = ledger.outstanding_lines(ObligationKind::UnreviewedChange);
        if !unreviewed.is_empty() {
            let first_path = unreviewed[0].path.display().to_string();
            out.push(StewardProposal {
                id: "verify:unread".to_string(),
                kind: ProposalKind::Verify,
                rank: ProposalKind::Verify.rank(),
                title: format!("Read {first_path} before the next change"),
                rationale: format!(
                    "{} diff(s) ({} lines) went in unreviewed. Passing tests only prove the bug is not where the tests look.",
                    unreviewed.len(),
                    unreviewed_lines
                ),
                evidence: vec![
                    format!("{} unreviewed obligation(s)", unreviewed.len()),
                    format!("{unreviewed_lines} unreviewed line(s)"),
                    first_path.clone(),
                ],
                target_path: Some(first_path.clone()),
                suggested_prompt: format!(
                    "Walk through what changed in `{first_path}` and flag any edge cases or invariants that were not verified."
                ),
            });
        }

        let untested: Vec<_> = outstanding
            .iter()
            .filter(|o| o.kind == ObligationKind::UnconfirmedCoverage)
            .collect();
        let untested_lines = ledger.outstanding_lines(ObligationKind::UnconfirmedCoverage);
        if !untested.is_empty() {
            let first_path = untested[0].path.display().to_string();
            out.push(StewardProposal {
                id: "verify:coverage".to_string(),
                kind: ProposalKind::Verify,
                rank: ProposalKind::Verify.rank(),
                title: format!("Run a scoped test covering {first_path}"),
                rationale: format!(
                    "{} path(s) ({} lines) have no confirmed test coverage after modification.",
                    untested.len(),
                    untested_lines
                ),
                evidence: vec![
                    format!("{} uncovered obligation(s)", untested.len()),
                    format!("{untested_lines} line(s) without confirmed coverage"),
                    first_path.clone(),
                ],
                target_path: Some(first_path.clone()),
                suggested_prompt: format!(
                    "Run or write a scoped test that directly exercises the changes in `{first_path}`."
                ),
            });
        }

        // 3. ORIENT: Unrecorded / opaque mutations in the observation journal.
        let opaque_mutations = observations.iter().filter(|r| r.may_have_mutated).count();
        if opaque_mutations > 0 {
            out.push(StewardProposal {
                id: "orient:opaque-mutations".to_string(),
                kind: ProposalKind::Orient,
                rank: ProposalKind::Orient.rank(),
                title: "Inspect opaque filesystem mutations".to_string(),
                rationale: "One or more shell or formatting commands may have mutated files without naming them, making the ledger a floor rather than a total."
                    .to_string(),
                evidence: vec![format!("{opaque_mutations} opaque or unattributed mutation(s)")],
                target_path: None,
                suggested_prompt:
                    "Run `git status` and `git diff --stat` to reconcile any unattributed workspace changes."
                        .to_string(),
            });
        }

        out.retain(|item| !self.dismissed.contains(&item.id));
        out.sort_by_key(|item| item.rank);
        out.truncate(self.max_proposals.max(1));
        out
    }
}

// ============================================================================
// 3. Presence & Animated Fox Digital Twin Posture (`PhiPresence`, `FoxPosture`)
// ============================================================================

/// Communication channel in Phi's attention economy, ordered by increasing cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceChannel {
    /// Nothing emitted. Default for transient noise.
    Silent = 0,
    /// Animated Fox Digital Twin posture only (ears, tail, eyes, colour). Free and peripheral.
    Ambient = 1,
    /// One quiet status line beside Phi. No modal or action required.
    Glance = 2,
    /// Action card / prompt gate. Strictly budgeted per session.
    Interrupt = 3,
}

/// Canonical expressions of the animated Fox Digital Twin (`phi_expression.js` / `phi_rig.js`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoxEmotion {
    Greeting,
    Thinking,
    Working,
    Success,
    Error,
    Idle,
    Curious,
    Evolve,
    Flow,
    Guard,
    Spark,
    Sleep,
    Skeptical,
    Unimpressed,
}

/// Posture snapshot driving the animated Fox Digital Twin in `/phi/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoxPosture {
    pub emotion: FoxEmotion,
    pub note: Option<String>,
    pub debt_ratio: f64,
    pub channel: PresenceChannel,
}

/// Derive the animated Fox Digital Twin's posture from the measured [`Ledger`],
/// [`ActivityPhase`], and [`PhiPolicy`].
pub fn derive_fox_posture(
    ledger: &Ledger,
    phase: ActivityPhase,
    policy: &PhiPolicy,
    channel: PresenceChannel,
) -> FoxPosture {
    let debt_ratio = policy.measured_debt_ratio(ledger);
    derive_fox_posture_with_debt(
        debt_ratio,
        ledger.outstanding().is_empty(),
        !ledger.evidence().is_empty(),
        phase,
        channel,
    )
}

/// Derive the animated Fox Digital Twin's posture from summarized receipt counts
/// when the full in-memory [`Ledger`] has already been projected into an activity
/// receipt.
#[allow(clippy::too_many_arguments)]
pub fn derive_fox_posture_from_counts(
    outstanding: usize,
    unreviewed_lines: usize,
    untested_lines: usize,
    unknown_size_obligations: usize,
    passed_runs: usize,
    phase: ActivityPhase,
    policy: &PhiPolicy,
    channel: PresenceChannel,
) -> FoxPosture {
    let debt_ratio = if outstanding == 0 {
        0.0
    } else {
        let effective_lines = unreviewed_lines
            .max(untested_lines)
            .saturating_add(unknown_size_obligations * 40);
        let scale = policy.debt_gate_lines.max(1) as f64;
        (effective_lines as f64 / scale).clamp(0.0, 1.0)
    };
    derive_fox_posture_with_debt(
        debt_ratio,
        outstanding == 0,
        passed_runs > 0,
        phase,
        channel,
    )
}

fn derive_fox_posture_with_debt(
    debt_ratio: f64,
    no_outstanding: bool,
    has_evidence: bool,
    phase: ActivityPhase,
    channel: PresenceChannel,
) -> FoxPosture {
    // Debt overrides optimistic expressions: a session can be green and still
    // be unverified, and the Fox Digital Twin's body should say so.
    let (emotion, note) = if debt_ratio > 0.70 {
        (
            FoxEmotion::Unimpressed,
            Some("nothing here is verified".to_string()),
        )
    } else if debt_ratio > 0.45 {
        (
            FoxEmotion::Skeptical,
            Some("more written than read".to_string()),
        )
    } else {
        match phase {
            ActivityPhase::Failed => (FoxEmotion::Error, Some("something is red".to_string())),
            ActivityPhase::Partial | ActivityPhase::Abandoned => (
                FoxEmotion::Skeptical,
                Some("nothing verified this".to_string()),
            ),
            ActivityPhase::Running => (FoxEmotion::Working, None),
            ActivityPhase::Completed => {
                if no_outstanding && has_evidence {
                    (FoxEmotion::Flow, None)
                } else {
                    (FoxEmotion::Greeting, None)
                }
            }
        }
    };

    // Lean PT4 formal invariants (`formal/HarnessAndTwinBounds.lean`):
    // - `flow_requires_clean_verified`: Flow is never emitted unless debt <= 0.45,
    //   no obligations remain, evidence is present, and the phase is Completed.
    // - `high_debt_overrides_phase`: debt > 0.70 always forces Unimpressed.
    debug_assert!(
        emotion != FoxEmotion::Flow
            || (debt_ratio <= 0.45
                && no_outstanding
                && has_evidence
                && matches!(phase, ActivityPhase::Completed)),
        "PT4 invariant violated: Flow posture emitted without clean verified completion"
    );
    debug_assert!(
        debt_ratio <= 0.70 || emotion == FoxEmotion::Unimpressed,
        "PT4 invariant violated: debt_ratio > 0.70 must produce Unimpressed posture"
    );

    FoxPosture {
        emotion,
        note,
        debt_ratio,
        channel,
    }
}

#[derive(Debug, Clone)]
struct TrackedSignal {
    seen: usize,
    spoke: usize,
    ignored: usize,
    dismissed: bool,
    quota: usize,
    last_channel: PresenceChannel,
}

/// Attention-economy router ensuring the Fox Digital Twin earns the right to speak.
#[derive(Debug, Clone)]
pub struct PhiPresence {
    pub budget: usize,
    pub quiet_after_activity_ms: u64,
    pub glance_cooldown_ms: u64,
    tracked: BTreeMap<String, TrackedSignal>,
    last_spoke_at_ms: u64,
    last_activity_at_ms: u64,
    busy: bool,
}

impl PhiPresence {
    pub fn new(policy: &PhiPolicy) -> Self {
        Self {
            budget: policy.interrupt_budget,
            quiet_after_activity_ms: policy.quiet_after_activity_ms,
            glance_cooldown_ms: policy.glance_cooldown_ms,
            tracked: BTreeMap::new(),
            last_spoke_at_ms: 0,
            last_activity_at_ms: 0,
            busy: false,
        }
    }

    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    pub fn note_activity(&mut self, now_ms: u64) {
        self.last_activity_at_ms = now_ms;
    }

    /// Route a signal (`id`, `severity`) to the loudest channel it has earned at `now_ms`.
    pub fn route(&mut self, id: &str, severity: SignalSeverity, now_ms: u64) -> PresenceChannel {
        if id.is_empty() {
            return PresenceChannel::Silent;
        }
        let base = severity.base_quota();
        let state = self.tracked.entry(id.to_string()).or_insert(TrackedSignal {
            seen: 0,
            spoke: 0,
            ignored: 0,
            dismissed: false,
            quota: base,
            last_channel: PresenceChannel::Silent,
        });
        state.seen += 1;
        if state.quota == 0 {
            state.quota = base;
        }

        if state.dismissed {
            state.last_channel = PresenceChannel::Ambient;
            return PresenceChannel::Ambient;
        }

        let settled = !self.busy
            && now_ms.saturating_sub(self.last_activity_at_ms) >= self.quiet_after_activity_ms;
        if !settled {
            state.last_channel = PresenceChannel::Ambient;
            return PresenceChannel::Ambient;
        }

        if state.seen < state.quota
            || (self.last_spoke_at_ms > 0
                && now_ms.saturating_sub(self.last_spoke_at_ms) < self.glance_cooldown_ms)
        {
            state.last_channel = PresenceChannel::Ambient;
            return PresenceChannel::Ambient;
        }

        let may_interrupt = severity != SignalSeverity::Low && state.spoke >= 1 && self.budget > 0;
        let channel = if may_interrupt {
            self.budget -= 1;
            PresenceChannel::Interrupt
        } else {
            PresenceChannel::Glance
        };

        state.spoke += 1;
        state.quota = state.seen + base * (1 + state.spoke + state.ignored * 2);
        self.last_spoke_at_ms = now_ms;
        state.last_channel = channel;
        channel
    }

    /// The operator closed/ignored the signal without acting: Phi gets quieter, never louder.
    pub fn ignore(&mut self, id: &str) {
        let base = SignalSeverity::Medium.base_quota();
        let state = self.tracked.entry(id.to_string()).or_insert(TrackedSignal {
            seen: 0,
            spoke: 0,
            ignored: 0,
            dismissed: false,
            quota: base,
            last_channel: PresenceChannel::Silent,
        });
        state.ignored += 1;
        state.quota += base * state.ignored * 2;
    }

    /// Permanently dismiss a signal for the rest of the session.
    pub fn dismiss(&mut self, id: &str) {
        let base = SignalSeverity::Medium.base_quota();
        let state = self.tracked.entry(id.to_string()).or_insert(TrackedSignal {
            seen: 0,
            spoke: 0,
            ignored: 0,
            dismissed: true,
            quota: base,
            last_channel: PresenceChannel::Ambient,
        });
        state.dismissed = true;
    }

    /// Operator acted on the signal: reset its counters and refund the interrupt budget.
    pub fn acknowledge(&mut self, id: &str) {
        if let Some(state) = self.tracked.get_mut(id) {
            state.seen = 0;
            state.ignored = 0;
            state.spoke = 0;
            state.quota = 0;
        }
        self.budget += 1;
    }
}
