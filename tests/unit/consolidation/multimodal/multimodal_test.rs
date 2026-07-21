use super::*;

#[test]
fn test_screenshot_ref() {
    let r = MultimodalRef::Screenshot {
        path: PathBuf::from("/tmp/ss.png"),
        timestamp: Utc::now(),
        description: "Search results page".into(),
        dimensions: (1920, 1080),
    };
    assert_eq!(r.type_name(), "screenshot");
    assert!(r.has_file_dependency());
    assert_eq!(r.file_path().unwrap().to_str().unwrap(), "/tmp/ss.png");
}

#[test]
fn test_interaction_trace_ref() {
    let r = MultimodalRef::InteractionTrace {
        trace_id: "trace-001".into(),
        action_count: 5,
        summary: "Navigated to search page".into(),
        duration_ms: 3000,
    };
    assert_eq!(r.type_name(), "interaction_trace");
    assert!(!r.has_file_dependency());
    assert_eq!(r.description(), "Navigated to search page");
}

#[test]
fn test_spatial_layout_ref() {
    let r = MultimodalRef::SpatialLayout {
        description: "Dashboard layout".into(),
        element_positions: vec![("header".into(), 50.0, 5.0), ("sidebar".into(), 10.0, 50.0)],
        hierarchy: vec![("main".into(), vec!["header".into(), "sidebar".into()])],
    };
    assert_eq!(r.type_name(), "spatial_layout");
    assert!(!r.has_file_dependency());
}

#[test]
fn test_serde_roundtrip() {
    let refs = vec![
        MultimodalRef::Screenshot {
            path: PathBuf::from("/tmp/ss.png"),
            timestamp: Utc::now(),
            description: "test".into(),
            dimensions: (800, 600),
        },
        MultimodalRef::EmbeddingRef {
            collection: "consolidated".into(),
            chunk_id: "chunk-42".into(),
            similarity: 0.95,
        },
        MultimodalRef::TemporalPattern {
            description: "command burst".into(),
            event_times: vec![Utc::now()],
            intervals_ms: vec![],
        },
    ];

    let json = serde_json::to_string(&refs).unwrap();
    let parsed: Vec<MultimodalRef> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].type_name(), "screenshot");
    assert_eq!(parsed[1].type_name(), "embedding_ref");
    assert_eq!(parsed[2].type_name(), "temporal_pattern");
}
