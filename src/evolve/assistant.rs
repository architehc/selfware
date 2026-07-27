//! Grounding-enforced GLM/OpenRouter review support.
//!
//! Model prose is never returned as an unstructured authority. The response
//! must cite evidence IDs created from exact workspace lines, and items with
//! missing or unknown citations are removed before they reach the UI.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;

use crate::api::{ApiClient, Message, ThinkingMode, Usage};
use crate::config::Config;

/// Repair instruction appended as a user message after one malformed reply
/// (spec §2.1). The original system + user messages and the assistant's
/// malformed reply are kept as context.
pub const REVIEW_REPAIR_PROMPT: &str = "Your previous reply was not valid JSON matching the required schema. Respond with ONLY the JSON object.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingEvidence {
    pub id: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub excerpt: String,
    pub content_hash: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedClaim {
    pub text: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionHop {
    pub order: usize,
    pub action: String,
    pub target: String,
    pub evidence_ids: Vec<String>,
    pub verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedRecommendation {
    pub title: String,
    pub rationale: String,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub hops: Vec<ActionHop>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ModelReview {
    #[serde(default)]
    claims: Vec<GroundedClaim>,
    #[serde(default)]
    recommendations: Vec<GroundedRecommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub cost: Option<f64>,
}

impl From<Usage> for ReviewUsage {
    fn from(value: Usage) -> Self {
        Self {
            prompt_tokens: value.prompt_tokens,
            completion_tokens: value.completion_tokens,
            total_tokens: value.total_tokens,
            cost: value.cost,
        }
    }
}

/// Typed protocol failure of the grounded review path (spec §2.1). Every
/// variant keeps the model telemetry from the last chat response — telemetry
/// is never dropped, even when the model output itself is unusable.
///
/// `GroundedAssistant::review` keeps its `anyhow::Result` signature and
/// returns this as the (downcastable) error, so HTTP handlers can recover the
/// typed outcome via `err.downcast_ref::<ReviewProtocolError>()` and map it
/// to a 422 with [`ReviewProtocolError::body`].
#[derive(Debug)]
pub enum ReviewProtocolError {
    /// Output was not parseable as the review schema after one repair.
    Invalid {
        detail: String,
        model: String,
        latency_ms: u128,
        usage: ReviewUsage,
    },
    /// Parseable, but zero claims, zero recommendations, zero rejections.
    Empty {
        model: String,
        latency_ms: u128,
        usage: ReviewUsage,
    },
    /// Every item was rejected by grounding: zero surviving, some rejected.
    Ungrounded {
        rejected_items: usize,
        model: String,
        latency_ms: u128,
        usage: ReviewUsage,
    },
    /// The evidence trust gate refused the send: a high-severity injection
    /// finding in non-trusted content. Happens before any model call, so
    /// there is no model telemetry to retain (no request was made).
    TrustBlocked { findings: Vec<TrustGateFinding> },
}

/// One blocking trust-gate finding (subset of context_trust::InjectionFinding
/// plus the source path it shipped from).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustGateFinding {
    pub path: String,
    pub kind: String,
    pub severity: String,
    pub line: usize,
    pub excerpt: String,
}

/// Compact summary of the evidence trust scan, attached to every review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustGateSummary {
    pub sources_scanned: usize,
    pub findings: usize,
    /// Worst risk_score across scanned sources (0 clean .. 100 suspicious).
    pub worst_risk_score: u32,
}

impl ReviewProtocolError {
    /// The exact 422 JSON body per spec §2.1.
    pub fn body(&self) -> serde_json::Value {
        match self {
            Self::Invalid {
                detail,
                model,
                latency_ms,
                usage,
            } => serde_json::json!({
                "error": "model_output_invalid",
                "detail": detail,
                "model": model,
                "latency_ms": latency_ms,
                "usage": usage,
            }),
            Self::Empty {
                model,
                latency_ms,
                usage,
            } => serde_json::json!({
                "error": "model_output_empty",
                "model": model,
                "latency_ms": latency_ms,
                "usage": usage,
            }),
            Self::Ungrounded {
                rejected_items,
                model,
                latency_ms,
                usage,
            } => serde_json::json!({
                "error": "model_output_ungrounded",
                "rejected_items": rejected_items,
                "model": model,
                "latency_ms": latency_ms,
                "usage": usage,
            }),
            Self::TrustBlocked { findings } => serde_json::json!({
                "error": "context_trust_blocked",
                "findings": findings,
            }),
        }
    }
}

impl std::fmt::Display for ReviewProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { detail, .. } => {
                write!(f, "model output invalid after one repair: {detail}")
            }
            Self::Empty { .. } => write!(f, "model returned an empty review"),
            Self::Ungrounded { rejected_items, .. } => write!(
                f,
                "model review fully ungrounded: {rejected_items} item(s) rejected"
            ),
            Self::TrustBlocked { findings } => write!(
                f,
                "evidence trust gate blocked the send: {} high-severity finding(s) in non-trusted content",
                findings.len()
            ),
        }
    }
}

impl std::error::Error for ReviewProtocolError {}

/// Classify an evidence path for trust scanning: first-party Rust is trusted
/// code (rules downgrade to informational); everything else is data — it
/// should never carry instructions, so injection patterns stay hot.
fn trust_classification(path: &str) -> &'static str {
    if path.ends_with(".rs") {
        "rust_source"
    } else {
        "data"
    }
}

/// Scan what is about to reach the model. High-severity findings in content
/// whose trust level is not `Trusted` block the send (the seed invariant:
/// untrusted content never reaches the model unflagged). Trusted first-party
/// code can legitimately contain these patterns — those are reported, not
/// blocked.
pub fn gate_evidence_trust(
    evidence: &[GroundingEvidence],
) -> Result<TrustGateSummary, ReviewProtocolError> {
    use super::context_trust::{analyze_source, TrustLevel};

    let mut blocking: Vec<TrustGateFinding> = Vec::new();
    let mut total_findings = 0usize;
    let mut worst_risk = 0u32;
    let mut scanned: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for chunk in evidence {
        let classification = trust_classification(&chunk.path);
        let report = analyze_source(
            &chunk.path,
            super::context_trust::SourceKind::Workspace,
            classification,
            &chunk.excerpt,
        );
        scanned.insert(chunk.path.as_str());
        total_findings += report.findings.len();
        worst_risk = worst_risk.max(report.risk_score);
        let trusted = super::context_trust::trust_level(
            super::context_trust::SourceKind::Workspace,
            classification,
        ) == TrustLevel::Trusted;
        // Findings are excerpt-relative (the excerpt starts at
        // chunk.start_line): offset back to real file lines.
        let line_offset = chunk.start_line.saturating_sub(1);
        blocking.extend(
            report
                .findings
                .iter()
                .filter(|f| {
                    f.severity == "high"
                        // hidden_unicode is never legitimate in source, even
                        // trusted first-party code (e.g. reviewing a hostile
                        // PR branch): it blocks regardless of provenance.
                        && (!trusted || f.kind == "hidden_unicode")
                })
                .map(|f| TrustGateFinding {
                    path: chunk.path.clone(),
                    kind: f.kind.clone(),
                    severity: f.severity.clone(),
                    line: f.line + line_offset,
                    excerpt: f.excerpt.clone(),
                }),
        );
    }

    if !blocking.is_empty() {
        return Err(ReviewProtocolError::TrustBlocked { findings: blocking });
    }
    Ok(TrustGateSummary {
        sources_scanned: scanned.len(),
        findings: total_findings,
        worst_risk_score: worst_risk,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedReview {
    pub model: String,
    pub claims: Vec<GroundedClaim>,
    pub recommendations: Vec<GroundedRecommendation>,
    pub evidence: Vec<GroundingEvidence>,
    /// Coverage of *files*: false only when a selected file could not be read.
    /// It does NOT mean every line of every file was shipped — long files are
    /// excerpted by design. Read this as "no file omitted", not "everything
    /// included" (review finding #9, 2026-07-26).
    pub evidence_complete: bool,
    /// Structural citation integrity only: every retained item cites known
    /// evidence IDs. This does not claim semantic entailment.
    pub grounding_valid: bool,
    pub citation_valid: bool,
    pub grounding_scope: String,
    pub semantic_validation: String,
    pub hallucination_free_guarantee: bool,
    pub rejected_items: usize,
    /// Derived trust state (spec §3.1), computed once at construction:
    /// "verified" (citation-valid, complete evidence, semantic validation
    /// performed — unreachable while semantic validation is unimplemented),
    /// "structural" (citation-valid, complete evidence, structural checks
    /// only), or "degraded" (anything else still returning 200).
    pub trust_state: String,
    /// Evidence trust-gate summary (what was scanned before the send).
    pub trust_gate: TrustGateSummary,
    pub usage: ReviewUsage,
}

#[derive(Clone)]
pub struct GroundedAssistant {
    client: ApiClient,
    configured_model: String,
}

impl GroundedAssistant {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            client: ApiClient::new(config)?,
            configured_model: config.model.clone(),
        })
    }

    pub fn configured_model(&self) -> &str {
        &self.configured_model
    }

    /// A single non-grounded chat turn for generative tasks such as pair
    /// evolution suggestions. Unlike [`review`](Self::review), the output is not
    /// validated against evidence IDs — the caller supplies whatever context the
    /// model should reason over. Returns (text, model, usage).
    pub async fn freeform(
        &self,
        system: &str,
        user: &str,
    ) -> Result<(String, String, ReviewUsage)> {
        if user.trim().is_empty() {
            bail!("freeform prompt cannot be empty");
        }
        let response = self
            .client
            .chat(
                vec![Message::system(system), Message::user(user)],
                None,
                ThinkingMode::Disabled,
            )
            .await
            .context("model suggestion call failed")?;
        let text = response
            .choices
            .first()
            .map(|choice| choice.message.content.text_all())
            .ok_or_else(|| anyhow!("model returned no choice"))?;
        Ok((text, response.model, response.usage.into()))
    }

    pub async fn review(
        &self,
        question: &str,
        evidence: Vec<GroundingEvidence>,
        evidence_complete: bool,
    ) -> Result<GroundedReview> {
        self.review_with_orientation(question, evidence, evidence_complete, None)
            .await
    }

    /// Grounded review with an optional non-citeable workspace orientation
    /// (architectural taxonomy + component map) prepended as navigation context.
    /// The orientation lets the model place the cited evidence in the wider tree
    /// without loading every file — claims still bind to supplied evidence IDs.
    pub async fn review_with_orientation(
        &self,
        question: &str,
        evidence: Vec<GroundingEvidence>,
        evidence_complete: bool,
        orientation: Option<&str>,
    ) -> Result<GroundedReview> {
        if question.trim().is_empty() {
            bail!("review question cannot be empty");
        }
        if evidence.is_empty() {
            bail!("grounded review requires source evidence");
        }

        // Trust gate: scan what is about to reach the model. High-severity
        // findings in non-trusted content refuse the send before any API call.
        let trust_gate = gate_evidence_trust(&evidence)?;

        let evidence_json = serde_json::to_string_pretty(&evidence)?;
        let system = Message::system(
            "You are a code-review engine. Use only the supplied evidence for \
             claims. A `Workspace orientation` section may precede the question: \
             it is a non-citeable map of the codebase's architecture and public \
             symbols, for navigation only — never cite it, and ground every claim \
             in a supplied evidence ID. \
             Return one JSON object and no markdown. Schema: \
             {\"claims\":[{\"text\":string,\"evidence_ids\":[string]}],\
             \"recommendations\":[{\"title\":string,\"rationale\":string,\
             \"evidence_ids\":[string],\"hops\":[{\"order\":number,\
             \"action\":string,\"target\":string,\"evidence_ids\":[string],\
             \"verification\":string}]}]}. Every item and hop must cite at least \
             one supplied evidence ID. Every recommendation must contain at least \
             two valid action hops numbered contiguously from 1. If evidence is \
             insufficient, omit the item.",
        );
        let user = Message::user(match orientation {
            Some(text) if !text.trim().is_empty() => format!(
                "Workspace orientation (background, not citeable):\n{}\n\nQuestion:\n{}\n\nGrounding evidence:\n{}",
                text.trim(),
                question.trim(),
                evidence_json
            ),
            _ => format!(
                "Question:\n{}\n\nGrounding evidence:\n{}",
                question.trim(),
                evidence_json
            ),
        });

        let started = Instant::now();
        let mut messages = vec![system, user];
        // One budgeted repair, no loops: attempt 0 is the original call;
        // attempt 1 reuses the same token budget with the malformed reply and
        // the repair instruction appended. A second parse failure is final.
        // Usage is ACCUMULATED across attempts — a repair roughly doubles
        // prompt cost and cost reporting must stay honest (AGENTS.md §3).
        let mut total_usage = ReviewUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cost: None,
        };
        let (payload, response) = loop {
            let response = self
                .client
                .chat(messages.clone(), None, ThinkingMode::Disabled)
                .await
                .context("grounded model review failed")?;
            {
                let usage: ReviewUsage = response.usage.clone().into();
                total_usage.prompt_tokens += usage.prompt_tokens;
                total_usage.completion_tokens += usage.completion_tokens;
                total_usage.total_tokens += usage.total_tokens;
                total_usage.cost = match (total_usage.cost, usage.cost) {
                    (Some(a), Some(b)) => Some(a + b),
                    (a, b) => a.or(b),
                };
            }
            let parsed = response
                .choices
                .first()
                .map(|choice| choice.message.content.text_all())
                .ok_or_else(|| anyhow!("model returned no review choice"))
                .and_then(|text| parse_model_review(&text));
            match parsed {
                Ok(payload) => break (payload, response),
                Err(error) => {
                    if messages.len() > 2 {
                        // Already repaired once — report the typed failure with
                        // telemetry from this last chat response.
                        return Err(ReviewProtocolError::Invalid {
                            detail: format!("{error:#}"),
                            model: response.model,
                            latency_ms: started.elapsed().as_millis(),
                            usage: total_usage,
                        }
                        .into());
                    }
                    let previous = response
                        .choices
                        .first()
                        .map(|choice| choice.message.content.text_all())
                        .unwrap_or_default();
                    messages.push(Message::assistant(previous));
                    messages.push(Message::user(REVIEW_REPAIR_PROMPT));
                }
            }
        };
        let (claims, recommendations, rejected_items) = validate_grounding(payload, &evidence);
        if claims.is_empty() && recommendations.is_empty() {
            let latency_ms = started.elapsed().as_millis();
            return Err(if rejected_items == 0 {
                ReviewProtocolError::Empty {
                    model: response.model,
                    latency_ms,
                    usage: total_usage,
                }
            } else {
                ReviewProtocolError::Ungrounded {
                    rejected_items,
                    model: response.model,
                    latency_ms,
                    usage: total_usage,
                }
            }
            .into());
        }

        let citation_valid = rejected_items == 0;
        let semantic_validation = "not_performed";
        Ok(GroundedReview {
            model: response.model,
            claims,
            recommendations,
            evidence,
            evidence_complete,
            grounding_valid: rejected_items == 0,
            citation_valid,
            grounding_scope: "snapshot_and_citation_integrity_only".to_string(),
            semantic_validation: semantic_validation.to_string(),
            hallucination_free_guarantee: false,
            rejected_items,
            trust_state: trust_state(citation_valid, evidence_complete, semantic_validation)
                .to_string(),
            trust_gate,
            usage: total_usage,
        })
    }
}

/// The spec §3.1 trust-state table. `verified` is reserved: it needs semantic
/// validation, which nothing performs today, so reachable states are
/// `structural` (clean, complete, structural checks only) and `degraded`
/// (rejected items or incomplete evidence on an otherwise successful review).
fn trust_state(
    citation_valid: bool,
    evidence_complete: bool,
    semantic_validation: &str,
) -> &'static str {
    if citation_valid && evidence_complete {
        if semantic_validation == "performed" {
            "verified"
        } else {
            "structural"
        }
    } else {
        "degraded"
    }
}

/// Create line-addressed evidence chunks from an exact document snapshot.
pub fn evidence_from_document(
    path: &str,
    content: &str,
    content_hash: &str,
    max_lines: usize,
) -> (Vec<GroundingEvidence>, bool) {
    let (evidence, complete, _) =
        evidence_from_document_excluding_ranges(path, content, content_hash, max_lines, &[]);
    (evidence, complete)
}

/// Create exact evidence while omitting inclusive one-based line ranges.
/// Remaining excerpts preserve their original line numbers and bytes.
pub fn evidence_from_document_excluding_ranges(
    path: &str,
    content: &str,
    content_hash: &str,
    max_lines: usize,
    excluded_ranges: &[(usize, usize)],
) -> (Vec<GroundingEvidence>, bool, usize) {
    const CHUNK_LINES: usize = 40;
    let lines: Vec<&str> = content.lines().collect();
    let mut excluded = vec![false; lines.len()];
    for &(start, end) in excluded_ranges {
        if start == 0 || start > end {
            continue;
        }
        let first = start.saturating_sub(1).min(lines.len());
        let last = end.min(lines.len());
        excluded[first..last].fill(true);
    }
    let excluded_lines = excluded.iter().filter(|value| **value).count();
    let eligible_lines = lines.len().saturating_sub(excluded_lines);
    let take = eligible_lines.min(max_lines);
    let mut evidence = Vec::new();
    let mut cursor = 0usize;
    let mut included = 0usize;
    while cursor < lines.len() && included < take {
        while cursor < lines.len() && excluded[cursor] {
            cursor += 1;
        }
        if cursor >= lines.len() {
            break;
        }
        let start = cursor;
        let mut indices = Vec::new();
        while cursor < lines.len()
            && !excluded[cursor]
            && indices.len() < CHUNK_LINES
            && included < take
        {
            indices.push(cursor);
            cursor += 1;
            included += 1;
        }
        let end = indices.last().map(|index| index + 1).unwrap_or(start + 1);
        let excerpt = indices
            .iter()
            .map(|index| format!("{:>6} | {}", index + 1, lines[*index]))
            .collect::<Vec<_>>()
            .join("\n");
        evidence.push(GroundingEvidence {
            id: format!("E{}", evidence.len() + 1),
            path: path.to_string(),
            start_line: start + 1,
            end_line: end,
            excerpt,
            content_hash: content_hash.to_string(),
            source: "workspace_snapshot".to_string(),
        });
    }
    (evidence, take == eligible_lines, excluded_lines)
}

fn parse_model_review(text: &str) -> Result<ModelReview> {
    let trimmed = text.trim();
    if let Ok(review) = serde_json::from_str(trimmed) {
        return Ok(review);
    }
    let start = trimmed
        .find('{')
        .context("model response did not contain JSON")?;
    let end = trimmed
        .rfind('}')
        .context("model response did not contain complete JSON")?;
    serde_json::from_str(&trimmed[start..=end]).context("model returned invalid review JSON")
}

/// Parse model JSON and discard every claim, recommendation, or hop that does
/// not cite the supplied evidence set. This is public for contract tests and
/// for non-HTTP actors that share the same grounding gateway.
pub fn validate_review_json(
    text: &str,
    evidence: &[GroundingEvidence],
) -> Result<(Vec<GroundedClaim>, Vec<GroundedRecommendation>, usize)> {
    Ok(validate_grounding(parse_model_review(text)?, evidence))
}

fn validate_grounding(
    payload: ModelReview,
    evidence: &[GroundingEvidence],
) -> (Vec<GroundedClaim>, Vec<GroundedRecommendation>, usize) {
    let known: HashSet<&str> = evidence.iter().map(|item| item.id.as_str()).collect();
    let valid_ids =
        |ids: &[String]| !ids.is_empty() && ids.iter().all(|id| known.contains(id.as_str()));
    let mut rejected = 0usize;

    let claims = payload
        .claims
        .into_iter()
        .filter(|claim| {
            let valid = valid_ids(&claim.evidence_ids) && !claim.text.trim().is_empty();
            if !valid {
                rejected += 1;
            }
            valid
        })
        .collect();

    let recommendations = payload
        .recommendations
        .into_iter()
        .filter_map(|mut recommendation| {
            if !valid_ids(&recommendation.evidence_ids)
                || recommendation.title.trim().is_empty()
                || recommendation.rationale.trim().is_empty()
            {
                rejected += 1;
                return None;
            }
            recommendation.hops.retain(|hop| {
                let valid = hop.order > 0
                    && !hop.action.trim().is_empty()
                    && !hop.target.trim().is_empty()
                    && !hop.verification.trim().is_empty()
                    && valid_ids(&hop.evidence_ids);
                if !valid {
                    rejected += 1;
                }
                valid
            });
            recommendation.hops.sort_by_key(|hop| hop.order);
            let contiguous = recommendation.hops.len() >= 2
                && recommendation
                    .hops
                    .iter()
                    .enumerate()
                    .all(|(index, hop)| hop.order == index + 1);
            if !contiguous {
                rejected += 1;
                return None;
            }
            Some(recommendation)
        })
        .collect();

    (claims, recommendations, rejected)
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/assistant_trust_gate_test.rs"]
mod assistant_trust_gate_test;
