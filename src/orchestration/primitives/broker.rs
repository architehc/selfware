use std::collections::{BTreeMap, HashSet};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::token_count::estimate_content_tokens;

use super::types::{
    ActivationPlan, BrokeredEvidence, MessageKind, PrimitiveMessage, RawEvidenceClaim,
    RawMessageProposal, MAX_EVIDENCE_QUOTE_TOKENS, MAX_IDENTIFIERS_PER_EVIDENCE,
    MAX_IDENTIFIER_BYTES, MAX_MESSAGES_PER_REPORT, MAX_MESSAGE_EVIDENCE, MAX_TOTAL_MESSAGES,
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum BrokerReject {
    #[error("sender is not active")]
    InactiveSender,
    #[error("recipient is not active")]
    InactiveRecipient,
    #[error("message route is not declared in the frozen plan")]
    RouteNotDeclared,
    #[error("message has an invalid number of evidence claims")]
    EvidenceBudget,
    #[error("message evidence does not exactly match the cited sender snapshot line")]
    EvidenceMismatch,
    #[error("message duplicates an earlier message")]
    Duplicate,
    #[error("run-wide message limit reached")]
    RunLimit,
    #[error("messages are forbidden outside non-final configured rounds")]
    RoundLimit,
}

#[derive(Debug, Default)]
pub(crate) struct BrokerOutcome {
    pub accepted: Vec<PrimitiveMessage>,
    pub rejected: usize,
}

/// Enforces the frozen plan's routing grants and converts model proposals
/// into metadata-only messages. It does not inspect graph data or infer new
/// routes. Model-provided prose and evidence quotes stop at this boundary.
pub(crate) struct MessageBroker<'a> {
    plan: &'a ActivationPlan,
    next_sequence: u64,
    seen: HashSet<String>,
    total: usize,
}

impl<'a> MessageBroker<'a> {
    pub(crate) fn new(plan: &'a ActivationPlan) -> Self {
        Self {
            plan,
            next_sequence: 1,
            seen: HashSet::new(),
            total: 0,
        }
    }

    pub(crate) fn accept_report(
        &mut self,
        sender: &str,
        round: u8,
        proposals: Vec<RawMessageProposal>,
        delivered_lines: &[(usize, String)],
    ) -> BrokerOutcome {
        let mut outcome = BrokerOutcome::default();
        for (position, proposal) in proposals.into_iter().enumerate() {
            if position >= MAX_MESSAGES_PER_REPORT {
                outcome.rejected += 1;
                continue;
            }
            match self.accept_one(sender, round, proposal, delivered_lines) {
                Ok(message) => outcome.accepted.push(message),
                Err(_reason) => outcome.rejected += 1,
            }
        }
        outcome
    }

    fn accept_one(
        &mut self,
        sender: &str,
        round: u8,
        proposal: RawMessageProposal,
        delivered_lines: &[(usize, String)],
    ) -> Result<PrimitiveMessage, BrokerReject> {
        let sender_assignment = self
            .plan
            .assignment(sender)
            .ok_or(BrokerReject::InactiveSender)?;
        if self.plan.assignment(&proposal.to).is_none() {
            return Err(BrokerReject::InactiveRecipient);
        }
        if !self.plan.route_allowed(sender, &proposal.to) {
            return Err(BrokerReject::RouteNotDeclared);
        }
        // Messages can only affect a later round. Round zero is invalid, and
        // accepting from the final round would create output that is never
        // delivered or checked by a recipient.
        if round == 0 || round >= self.plan.max_rounds {
            return Err(BrokerReject::RoundLimit);
        }
        if self.total >= MAX_TOTAL_MESSAGES {
            return Err(BrokerReject::RunLimit);
        }
        if proposal.evidence.is_empty() || proposal.evidence.len() > MAX_MESSAGE_EVIDENCE {
            return Err(BrokerReject::EvidenceBudget);
        }

        let mut evidence = Vec::with_capacity(proposal.evidence.len());
        let mut seen_evidence = HashSet::new();
        for claim in proposal.evidence {
            let verified = verify_evidence(claim, delivered_lines)?;
            if seen_evidence.insert((verified.line, verified.sha256.clone())) {
                evidence.push(verified);
            }
        }
        if evidence.is_empty() {
            return Err(BrokerReject::EvidenceBudget);
        }

        let digest = message_digest(sender, &proposal.to, proposal.kind, &evidence);
        if !self.seen.insert(digest) {
            return Err(BrokerReject::Duplicate);
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.total += 1;
        Ok(PrimitiveMessage {
            sequence,
            round,
            from: sender.to_string(),
            to: proposal.to,
            kind: proposal.kind,
            sender_path: sender_assignment.path.clone(),
            sender_sha256: sender_assignment.base_sha256.clone(),
            evidence,
        })
    }
}

fn verify_evidence(
    claim: RawEvidenceClaim,
    delivered_lines: &[(usize, String)],
) -> Result<BrokeredEvidence, BrokerReject> {
    if claim.line == 0
        || claim.quote.trim().is_empty()
        || estimate_content_tokens(&claim.quote) > MAX_EVIDENCE_QUOTE_TOKENS
    {
        return Err(BrokerReject::EvidenceBudget);
    }
    let snapshot_text = exact_snapshot_line(claim.line, &claim.quote, delivered_lines)
        .ok_or(BrokerReject::EvidenceMismatch)?;

    Ok(BrokeredEvidence {
        line: claim.line,
        sha256: format!("{:x}", Sha256::digest(snapshot_text.as_bytes())),
        identifiers: derive_identifiers(snapshot_text),
    })
}

/// Resolve one unambiguous, full-line evidence claim. Normalized substrings,
/// relocation, and model-authored variants are deliberately refused.
pub(super) fn exact_snapshot_line<'a>(
    line: usize,
    quote: &str,
    delivered_lines: &'a [(usize, String)],
) -> Option<&'a str> {
    let mut matches = delivered_lines
        .iter()
        .filter(|(candidate, _text)| *candidate == line);
    let (_line, snapshot_text) = matches.next()?;
    if matches.next().is_some() || quote != snapshot_text {
        return None;
    }
    Some(snapshot_text)
}

/// Extract a small, deterministic vocabulary that can help a recipient
/// correlate interfaces without forwarding source text. Only ASCII source
/// identifiers cross the broker boundary; oversized identifiers are dropped
/// rather than truncated into misleading names.
fn derive_identifiers(text: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let mut seen = HashSet::new();
    let bytes = text.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() && identifiers.len() < MAX_IDENTIFIERS_PER_EVIDENCE {
        if !(bytes[offset].is_ascii_alphabetic() || bytes[offset] == b'_') {
            offset += 1;
            continue;
        }
        let start = offset;
        offset += 1;
        while offset < bytes.len()
            && (bytes[offset].is_ascii_alphanumeric() || bytes[offset] == b'_')
        {
            offset += 1;
        }
        let candidate = &text[start..offset];
        let continues_as_unicode_identifier = text[offset..]
            .chars()
            .next()
            .is_some_and(|character| !character.is_ascii() && character.is_alphanumeric());
        if candidate.len() <= MAX_IDENTIFIER_BYTES
            && !continues_as_unicode_identifier
            && candidate.bytes().any(|byte| byte.is_ascii_alphanumeric())
            && seen.insert(candidate.to_string())
        {
            identifiers.push(candidate.to_string());
        }
    }
    identifiers
}

fn message_digest(
    from: &str,
    to: &str,
    kind: MessageKind,
    evidence: &[BrokeredEvidence],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(from.as_bytes());
    hasher.update([0]);
    hasher.update(to.as_bytes());
    hasher.update([0]);
    hasher.update([match kind {
        MessageKind::DependencyQuestion => 0,
        MessageKind::InterfaceConstraint => 1,
        MessageKind::TestImpact => 2,
        MessageKind::Risk => 3,
    }]);
    for claim in evidence {
        hasher.update(claim.line.to_le_bytes());
        hasher.update(claim.sha256.as_bytes());
        for identifier in &claim.identifiers {
            hasher.update([0]);
            hasher.update(identifier.as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

pub(crate) fn inbox_by_recipient(
    messages: &[PrimitiveMessage],
) -> BTreeMap<String, Vec<PrimitiveMessage>> {
    let mut inboxes: BTreeMap<String, Vec<PrimitiveMessage>> = BTreeMap::new();
    for message in messages {
        inboxes
            .entry(message.to.clone())
            .or_default()
            .push(message.clone());
    }
    inboxes
}

#[cfg(test)]
#[path = "../../../tests/unit/orchestration/primitives/broker_test.rs"]
mod tests;
