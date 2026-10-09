use super::*;
use crate::orchestration::primitives::types::{
    AllowedRoute, GraphStatus, PrimitiveAssignment, ReadAttestationSummary, RepositoryBinding,
};
use std::collections::BTreeSet;

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn assignment(key: &str, path: &str, hash: char) -> PrimitiveAssignment {
    PrimitiveAssignment {
        key: key.to_string(),
        graph_node_id: None,
        path: path.to_string(),
        classification: "rust_source".to_string(),
        base_sha256: digest(hash),
        focus_symbol: None,
        reasons: vec!["test".to_string()],
        read_attestation: ReadAttestationSummary {
            attestation_id: format!("read-{key}"),
            exact_snapshot_read: true,
            write: false,
            execute: false,
            network: false,
        },
    }
}

fn plan() -> ActivationPlan {
    ActivationPlan {
        run_id: "run-test".to_string(),
        task_sha256: digest('1'),
        selection_mode: "trusted-test-policy".to_string(),
        max_rounds: 3,
        graph: GraphStatus::Missing,
        binding: RepositoryBinding {
            repository_id: digest('2'),
            head_tree: None,
            graph_sha256: None,
            activation_census_sha256: digest('3'),
            activation_census_complete: true,
            activation_ranking_complete: true,
            working_set_sha256: digest('4'),
            plan_sha256: digest('5'),
        },
        assignments: vec![
            assignment("artifact-001", "src/a.rs", 'a'),
            assignment("artifact-002", "src/b.rs", 'b'),
        ],
        allowed_routes: BTreeSet::from([AllowedRoute {
            from: "artifact-001".to_string(),
            to: "artifact-002".to_string(),
        }]),
        warnings: Vec::new(),
    }
}

fn proposal(quote: &str) -> RawMessageProposal {
    RawMessageProposal {
        to: "artifact-002".to_string(),
        kind: MessageKind::InterfaceConstraint,
        evidence: vec![RawEvidenceClaim {
            line: 7,
            quote: quote.to_string(),
        }],
    }
}

fn delivered_lines() -> Vec<(usize, String)> {
    vec![(7, "pub fn stable_interface() {}".to_string())]
}

#[test]
fn broker_stamps_identity_and_emits_metadata_without_model_text() {
    let plan = plan();
    let mut broker = MessageBroker::new(&plan);
    let accepted = broker
        .accept_one(
            "artifact-001",
            1,
            proposal("pub fn stable_interface() {}"),
            &delivered_lines(),
        )
        .unwrap();

    assert_eq!(accepted.from, "artifact-001");
    assert_eq!(accepted.to, "artifact-002");
    assert_eq!(accepted.sender_path, "src/a.rs");
    assert_eq!(accepted.sender_sha256, digest('a'));
    assert_eq!(accepted.sequence, 1);
    assert_eq!(accepted.round, 1);
    assert_eq!(accepted.evidence[0].line, 7);
    assert_eq!(
        accepted.evidence[0].sha256,
        format!("{:x}", Sha256::digest(b"pub fn stable_interface() {}"))
    );
    assert_eq!(
        accepted.evidence[0].identifiers,
        vec!["pub", "fn", "stable_interface"]
    );

    let serialized = serde_json::to_string(&accepted).unwrap();
    assert!(!serialized.contains("pub fn stable_interface() {}"));
    assert!(!serialized.contains("quote"));
    assert!(!serialized.contains("body"));
}

#[test]
fn broker_enforces_only_the_frozen_plan_route() {
    let plan = plan();
    let lines = delivered_lines();
    let mut broker = MessageBroker::new(&plan);
    assert_eq!(
        broker.accept_one(
            "artifact-999",
            1,
            proposal("pub fn stable_interface() {}"),
            &lines,
        ),
        Err(BrokerReject::InactiveSender)
    );

    let reverse = RawMessageProposal {
        to: "artifact-001".to_string(),
        ..proposal("pub fn stable_interface() {}")
    };
    assert_eq!(
        broker.accept_one("artifact-002", 1, reverse, &lines),
        Err(BrokerReject::RouteNotDeclared)
    );

    let inactive = RawMessageProposal {
        to: "artifact-404".to_string(),
        ..proposal("pub fn stable_interface() {}")
    };
    assert_eq!(
        broker.accept_one("artifact-001", 1, inactive, &lines),
        Err(BrokerReject::InactiveRecipient)
    );
}

#[test]
fn broker_requires_the_exact_cited_sender_line() {
    let plan = plan();
    let lines = delivered_lines();
    let mut broker = MessageBroker::new(&plan);

    assert_eq!(
        broker.accept_one("artifact-001", 1, proposal("stable_interface"), &lines,),
        Err(BrokerReject::EvidenceMismatch)
    );
    let wrong_line = RawMessageProposal {
        evidence: vec![RawEvidenceClaim {
            line: 8,
            quote: "pub fn stable_interface() {}".to_string(),
        }],
        ..proposal("pub fn stable_interface() {}")
    };
    assert_eq!(
        broker.accept_one("artifact-001", 1, wrong_line, &lines),
        Err(BrokerReject::EvidenceMismatch)
    );

    let duplicate_lines = vec![
        (7, "pub fn stable_interface() {}".to_string()),
        (7, "pub fn stable_interface() {}".to_string()),
    ];
    assert_eq!(
        broker.accept_one(
            "artifact-001",
            1,
            proposal("pub fn stable_interface() {}"),
            &duplicate_lines,
        ),
        Err(BrokerReject::EvidenceMismatch)
    );
}

#[test]
fn broker_rejects_zero_and_final_round_messages() {
    let plan = plan();
    let lines = delivered_lines();
    let mut broker = MessageBroker::new(&plan);
    for round in [0, plan.max_rounds] {
        assert_eq!(
            broker.accept_one(
                "artifact-001",
                round,
                proposal("pub fn stable_interface() {}"),
                &lines,
            ),
            Err(BrokerReject::RoundLimit)
        );
    }
}

#[test]
fn broker_rejects_missing_oversized_and_duplicate_evidence() {
    let plan = plan();
    let lines = delivered_lines();
    let mut broker = MessageBroker::new(&plan);
    let empty = RawMessageProposal {
        evidence: Vec::new(),
        ..proposal("pub fn stable_interface() {}")
    };
    assert_eq!(
        broker.accept_one("artifact-001", 1, empty, &lines),
        Err(BrokerReject::EvidenceBudget)
    );

    let oversized_quote = "word ".repeat(MAX_EVIDENCE_QUOTE_TOKENS * 4);
    assert_eq!(
        broker.accept_one("artifact-001", 1, proposal(&oversized_quote), &lines,),
        Err(BrokerReject::EvidenceBudget)
    );

    broker
        .accept_one(
            "artifact-001",
            1,
            proposal("pub fn stable_interface() {}"),
            &lines,
        )
        .unwrap();
    assert_eq!(
        broker.accept_one(
            "artifact-001",
            1,
            proposal("pub fn stable_interface() {}"),
            &lines,
        ),
        Err(BrokerReject::Duplicate)
    );
}

#[test]
fn derived_identifiers_are_ascii_deduplicated_and_bounded() {
    let oversized = "a".repeat(MAX_IDENTIFIER_BYTES + 1);
    let mut source = format!("{oversized} stable stable café");
    for index in 0..(MAX_IDENTIFIERS_PER_EVIDENCE + 5) {
        source.push_str(&format!(" symbol_{index}"));
    }

    let identifiers = derive_identifiers(&source);
    assert_eq!(identifiers[0], "stable");
    assert_eq!(
        identifiers.iter().filter(|item| *item == "stable").count(),
        1
    );
    assert!(identifiers.len() <= MAX_IDENTIFIERS_PER_EVIDENCE);
    assert!(identifiers
        .iter()
        .all(|item| item.len() <= MAX_IDENTIFIER_BYTES && item.is_ascii()));
    assert!(!identifiers.contains(&oversized));
    assert!(!identifiers.iter().any(|item| item == "caf"));
}

#[test]
fn report_and_inbox_helpers_preserve_only_accepted_messages() {
    let plan = plan();
    let mut broker = MessageBroker::new(&plan);
    let mut proposals = vec![proposal("pub fn stable_interface() {}"); 2];
    proposals.extend(std::iter::repeat_n(
        proposal("pub fn stable_interface() {}"),
        MAX_MESSAGES_PER_REPORT,
    ));
    let outcome = broker.accept_report("artifact-001", 1, proposals, &delivered_lines());
    assert_eq!(outcome.accepted.len(), 1);
    assert_eq!(outcome.rejected, MAX_MESSAGES_PER_REPORT + 1);

    let inboxes = inbox_by_recipient(&outcome.accepted);
    assert_eq!(inboxes["artifact-002"], outcome.accepted);
}
