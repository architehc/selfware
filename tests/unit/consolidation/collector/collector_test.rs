use super::*;

fn make_episode(id: &str, hours_ago: i64, importance: u8) -> EpisodeData {
    EpisodeData {
        id: id.into(),
        content: format!("Episode {id}"),
        timestamp: Utc::now() - chrono::Duration::hours(hours_ago),
        importance,
        tags: vec!["test".into()],
        context: HashMap::new(),
        related_ids: Vec::new(),
        session_id: "session-1".into(),
    }
}

#[test]
fn test_collect_episodes_filters_by_age() {
    let collector = ShortTermCollector::new(86400, 1); // 24 hours, min importance 1
    let episodes = vec![make_episode("recent", 1, 2), make_episode("old", 48, 2)];

    let collected = collector.collect_episodes(&episodes);
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].source_id, "recent");
}

#[test]
fn test_collect_episodes_filters_by_importance() {
    let collector = ShortTermCollector::new(86400 * 7, 3); // 7 days, min High
    let episodes = vec![
        make_episode("low", 1, 1),
        make_episode("high", 1, 3),
        make_episode("critical", 1, 4),
    ];

    let collected = collector.collect_episodes(&episodes);
    assert_eq!(collected.len(), 2);
}

#[test]
fn test_assemble_batch() {
    let collector = ShortTermCollector::new(86400, 1);
    let items = vec![
        CollectedItem {
            source_id: "ep-1".into(),
            source_type: SourceType::Episode,
            content: "test".into(),
            timestamp: Utc::now(),
            importance: 2,
            tags: vec![],
            metadata: HashMap::new(),
            related_ids: vec![],
            session_id: None,
            file_refs: vec![],
        },
        CollectedItem {
            source_id: "mem-1".into(),
            source_type: SourceType::MemoryEntry,
            content: "test".into(),
            timestamp: Utc::now(),
            importance: 2,
            tags: vec![],
            metadata: HashMap::new(),
            related_ids: vec![],
            session_id: None,
            file_refs: vec![],
        },
    ];

    let batch = collector.assemble_batch(items);
    assert_eq!(batch.items.len(), 2);
    assert_eq!(batch.source_counts[&SourceType::Episode], 1);
    assert_eq!(batch.source_counts[&SourceType::MemoryEntry], 1);
}
