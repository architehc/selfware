//! Grounding-enforced GLM/OpenRouter review support.
//!
//! Model prose is never returned as an unstructured authority. The response
//! must cite evidence IDs created from exact workspace lines, and items with
//! missing or unknown citations are removed before they reach the UI.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::api::{ApiClient, Message, ThinkingMode, Usage};
use crate::config::Config;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedReview {
    pub model: String,
    pub claims: Vec<GroundedClaim>,
    pub recommendations: Vec<GroundedRecommendation>,
    pub evidence: Vec<GroundingEvidence>,
    pub evidence_complete: bool,
    /// Structural citation integrity only: every retained item cites known
    /// evidence IDs. This does not claim semantic entailment.
    pub grounding_valid: bool,
    pub citation_valid: bool,
    pub grounding_scope: String,
    pub semantic_validation: String,
    pub hallucination_free_guarantee: bool,
    pub rejected_items: usize,
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

        let response = self
            .client
            .chat(vec![system, user], None, ThinkingMode::Disabled)
            .await
            .context("grounded model review failed")?;
        let text = response
            .choices
            .first()
            .map(|choice| choice.message.content.text_all())
            .ok_or_else(|| anyhow!("model returned no review choice"))?;
        let payload = parse_model_review(&text)?;
        let (claims, recommendations, rejected_items) = validate_grounding(payload, &evidence);

        Ok(GroundedReview {
            model: response.model,
            claims,
            recommendations,
            evidence,
            evidence_complete,
            grounding_valid: rejected_items == 0,
            citation_valid: rejected_items == 0,
            grounding_scope: "snapshot_and_citation_integrity_only".to_string(),
            semantic_validation: "not_performed".to_string(),
            hallucination_free_guarantee: false,
            rejected_items,
            usage: response.usage.into(),
        })
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
