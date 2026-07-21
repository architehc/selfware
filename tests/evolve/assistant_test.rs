use selfware::evolve::assistant::{
    evidence_from_document, evidence_from_document_excluding_ranges, validate_review_json,
};
use serde_json::json;

#[test]
fn test_document_evidence_is_line_addressed_and_reports_truncation() {
    let content = (1..=85)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    let (evidence, complete) =
        evidence_from_document("src/example.rs", &content, "snapshot-hash", 60);

    assert!(!complete);
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[0].id, "E1");
    assert_eq!((evidence[0].start_line, evidence[0].end_line), (1, 40));
    assert!(evidence[0].excerpt.starts_with("     1 | line 1"));
    assert!(evidence[0].excerpt.ends_with("    40 | line 40"));
    assert_eq!(evidence[1].id, "E2");
    assert_eq!((evidence[1].start_line, evidence[1].end_line), (41, 60));
    assert!(evidence[1].excerpt.starts_with("    41 | line 41"));
    assert!(evidence[1].excerpt.ends_with("    60 | line 60"));
    assert!(evidence.iter().all(|item| item.path == "src/example.rs"
        && item.content_hash == "snapshot-hash"
        && item.source == "workspace_snapshot"));
}

#[test]
fn test_document_evidence_marks_a_complete_snapshot() {
    let (evidence, complete) =
        evidence_from_document("src/lib.rs", "first\nsecond\n", "hash", usize::MAX);

    assert!(complete);
    assert_eq!(evidence.len(), 1);
    assert_eq!((evidence[0].start_line, evidence[0].end_line), (1, 2));
}

#[test]
fn test_document_evidence_excludes_ranges_without_changing_source_lines() {
    let content = (1..=12)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let (evidence, complete, excluded_lines) = evidence_from_document_excluding_ranges(
        "src/lib.rs",
        &content,
        "hash",
        usize::MAX,
        &[(3, 5), (9, 9)],
    );

    assert!(complete);
    assert_eq!(excluded_lines, 4);
    assert_eq!(evidence.len(), 3);
    assert_eq!((evidence[0].start_line, evidence[0].end_line), (1, 2));
    assert_eq!((evidence[1].start_line, evidence[1].end_line), (6, 8));
    assert_eq!((evidence[2].start_line, evidence[2].end_line), (10, 12));
    let excerpts = evidence
        .iter()
        .map(|item| item.excerpt.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!excerpts.contains("line 3\n"));
    assert!(excerpts.contains("     6 | line 6"));
    assert!(excerpts.contains("    12 | line 12"));
}

#[test]
fn test_review_validation_discards_every_ungrounded_item_and_hop() {
    let (evidence, _) = evidence_from_document("src/lib.rs", "pub fn grounded() {}\n", "hash", 40);
    let response = json!({
        "claims": [
            {"text": "Grounded claim", "evidence_ids": ["E1"]},
            {"text": "Unknown citation", "evidence_ids": ["E404"]},
            {"text": "No citation", "evidence_ids": []}
        ],
        "recommendations": [
            {
                "title": "Grounded recommendation",
                "rationale": "The exact source supports it",
                "evidence_ids": ["E1"],
                "hops": [
                    {
                        "order": 2,
                        "action": "Verify",
                        "target": "src/lib.rs",
                        "evidence_ids": ["E1"],
                        "verification": "Run the focused test"
                    },
                    {
                        "order": 1,
                        "action": "Inspect",
                        "target": "src/lib.rs",
                        "evidence_ids": ["E1"],
                        "verification": "Read the cited source"
                    },
                    {
                        "order": 3,
                        "action": "Invalid citation",
                        "target": "src/lib.rs",
                        "evidence_ids": ["E404"],
                        "verification": "Must be removed"
                    }
                ]
            },
            {
                "title": "Ungrounded recommendation",
                "rationale": "No known source",
                "evidence_ids": ["E404"],
                "hops": []
            }
        ]
    });
    let fenced = format!("```json\n{}\n```", response);

    let (claims, recommendations, rejected) = validate_review_json(&fenced, &evidence).unwrap();

    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].text, "Grounded claim");
    assert_eq!(recommendations.len(), 1);
    assert_eq!(recommendations[0].title, "Grounded recommendation");
    assert_eq!(recommendations[0].hops.len(), 2);
    assert_eq!(recommendations[0].hops[0].order, 1);
    assert_eq!(recommendations[0].hops[1].order, 2);
    assert_eq!(rejected, 4);
}

#[test]
fn test_review_validation_rejects_non_contiguous_or_single_hop_recommendations() {
    let (evidence, _) = evidence_from_document("src/lib.rs", "pub fn grounded() {}\n", "hash", 40);
    let response = json!({
        "recommendations": [
            {
                "title": "Single hop",
                "rationale": "Incomplete action chain",
                "evidence_ids": ["E1"],
                "hops": [{
                    "order": 1,
                    "action": "Inspect",
                    "target": "src/lib.rs",
                    "evidence_ids": ["E1"],
                    "verification": "Read it"
                }]
            },
            {
                "title": "Gap",
                "rationale": "Orders skip a step",
                "evidence_ids": ["E1"],
                "hops": [
                    {"order": 1, "action": "Inspect", "target": "src/lib.rs", "evidence_ids": ["E1"], "verification": "Read it"},
                    {"order": 3, "action": "Verify", "target": "src/lib.rs", "evidence_ids": ["E1"], "verification": "Test it"}
                ]
            }
        ]
    });

    let (_, recommendations, rejected) =
        validate_review_json(&response.to_string(), &evidence).unwrap();
    assert!(recommendations.is_empty());
    assert_eq!(rejected, 2);
}

#[test]
fn test_review_validation_rejects_non_json_model_output() {
    let (evidence, _) = evidence_from_document("src/lib.rs", "pub fn grounded() {}\n", "hash", 40);

    assert!(validate_review_json("No structured evidence is available.", &evidence).is_err());
}
