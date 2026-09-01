use super::*;

fn make_record() -> TemporalRecord {
    let now = Utc::now();
    TemporalRecord {
        id: "test-001".into(),
        created_at: now,
        source_timestamps: vec![
            now - chrono::Duration::hours(2),
            now - chrono::Duration::hours(1),
        ],
        sequence_order: 1,
        causal_parents: vec![],
        causal_children: vec![],
        decay_score: 1.0,
        access_count: 0,
        last_accessed: now,
        content: CompactedContent {
            summary: "Test summary".into(),
            key_facts: vec!["fact1".into()],
            entities: vec!["entity1".into()],
            actions: vec!["action1".into()],
            outcomes: vec!["outcome1".into()],
            insights: vec!["insight1".into()],
        },
        multimodal_refs: vec![],
        source_ids: vec!["ep-1".into(), "ep-2".into()],
        tags: vec!["test".into()],
        importance: RecordImportance::Normal,
        session_id: Some("session-1".into()),
        metadata: HashMap::new(),
    }
}

#[test]
fn test_decay_score_fresh() {
    let record = make_record();
    let score = record.decay_score_at(Utc::now(), 24.0);
    // Fresh record should have score close to 1.0
    assert!(score > 0.9, "Fresh score should be > 0.9, got {score}");
}

#[test]
fn test_decay_score_aged() {
    let mut record = make_record();
    record.created_at = Utc::now() - chrono::Duration::hours(48);
    record.last_accessed = record.created_at;
    let score = record.decay_score_at(Utc::now(), 24.0);
    // 48 hours = 2 half-lives, so ~0.25
    assert!(score < 0.3, "48h old score should be < 0.3, got {score}");
}

#[test]
fn test_decay_score_with_access() {
    let mut record = make_record();
    record.created_at = Utc::now() - chrono::Duration::hours(24);
    record.last_accessed = Utc::now();
    record.access_count = 5;
    let score = record.decay_score_at(Utc::now(), 24.0);
    // Should be higher than a non-accessed record due to access bonus
    let mut plain = make_record();
    plain.created_at = record.created_at;
    plain.last_accessed = plain.created_at;
    let plain_score = plain.decay_score_at(Utc::now(), 24.0);
    assert!(
        score > plain_score,
        "Accessed score {score} should be > plain {plain_score}"
    );
}

#[test]
fn test_importance_multiplier() {
    let mut low = make_record();
    low.importance = RecordImportance::Low;
    let mut critical = make_record();
    critical.importance = RecordImportance::Critical;

    let now = Utc::now();
    assert!(critical.decay_score_at(now, 24.0) > low.decay_score_at(now, 24.0));
}

#[test]
fn test_time_span() {
    let record = make_record();
    let (min, max) = record.time_span().unwrap();
    assert!(min < max);
}

#[test]
fn test_record_access() {
    let mut record = make_record();
    assert_eq!(record.access_count, 0);
    record.record_access();
    assert_eq!(record.access_count, 1);
    record.record_access();
    assert_eq!(record.access_count, 2);
}

#[test]
fn test_serde_roundtrip() {
    let record = make_record();
    let json = serde_json::to_string(&record).unwrap();
    let parsed: TemporalRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, record.id);
    assert_eq!(parsed.source_ids.len(), 2);
    assert_eq!(parsed.importance, RecordImportance::Normal);
}

#[test]
fn test_compacted_content_tokens() {
    let content = CompactedContent {
        summary: "A".repeat(400),
        key_facts: vec!["B".repeat(100)],
        entities: vec![],
        actions: vec![],
        outcomes: vec![],
        insights: vec![],
    };
    // Measured accounting (AGENTS.md rule 4): the value comes from
    // estimate_content_tokens, not the old len/4 heuristic (500/4 = 125).
    let expected = crate::token_count::estimate_content_tokens(&"A".repeat(400))
        + crate::token_count::estimate_content_tokens(&"B".repeat(100));
    assert_eq!(content.estimated_tokens(), expected);
    assert_ne!(
        content.estimated_tokens(),
        125,
        "must not be the old byte-fraction heuristic value"
    );
}
