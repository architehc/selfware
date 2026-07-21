use super::*;
use tempfile::tempdir;

#[test]
fn test_dream_trigger_default() {
    let trigger = DreamTrigger::default();
    assert_eq!(trigger.min_hours_since_last, 24);
    assert_eq!(trigger.min_sessions_since_last, 5);
}

#[test]
fn test_dream_trigger_builder() {
    let trigger = DreamTrigger::new().with_min_hours(48).with_min_sessions(10);
    assert_eq!(trigger.min_hours_since_last, 48);
    assert_eq!(trigger.min_sessions_since_last, 10);
}

#[test]
fn test_dream_state_gate_checks() {
    let trigger = DreamTrigger::default();

    // New state should not pass gates (0 hours, 0 sessions)
    let state = DreamState::new();
    assert!(!state.should_run_dream_check_gates(&trigger));

    // State with enough hours but not enough sessions
    let mut state = DreamState::new();
    state.last_dream_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - (25 * 3600); // 25 hours ago
    assert!(!state.should_run_dream_check_gates(&trigger));

    // State with enough sessions but not enough hours
    let mut state = DreamState::new();
    state.sessions_since_last_dream = 10;
    assert!(!state.should_run_dream_check_gates(&trigger));

    // State with both requirements met
    let mut state = DreamState::new();
    state.last_dream_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - (25 * 3600);
    state.sessions_since_last_dream = 5;
    assert!(state.should_run_dream_check_gates(&trigger));
}

#[test]
fn test_should_run_dream_with_lock() {
    let trigger = DreamTrigger::default();
    let mut state = DreamState::new();

    // Set up state to pass gates 1 and 2
    state.last_dream_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - (25 * 3600);
    state.sessions_since_last_dream = 5;

    // Should acquire lock and return true
    assert!(should_run_dream(&mut state, &trigger));
    assert!(state.consolidation_lock);

    // Should fail now because lock is held
    assert!(!should_run_dream(&mut state, &trigger));

    // Release lock
    state.release_consolidation_lock();
    assert!(!state.consolidation_lock);

    // Should succeed again
    assert!(should_run_dream(&mut state, &trigger));
}

#[test]
fn test_dream_state_persistence() {
    let dir = tempdir().unwrap();
    let mut state = DreamState::new();
    state.last_dream_timestamp = 12345;
    state.sessions_since_last_dream = 3;
    state.dream_count = 5;

    // Save state
    state.save(dir.path()).unwrap();

    // Load state
    let loaded = DreamState::load(dir.path()).unwrap();
    assert_eq!(loaded.last_dream_timestamp, 12345);
    assert_eq!(loaded.sessions_since_last_dream, 3);
    assert_eq!(loaded.dream_count, 5);
    assert!(!loaded.consolidation_lock);
}

#[test]
fn test_memory_entry_parse() {
    let entry =
        MemoryEntry::parse("- [2026-03-31] Project uses tokio", &MemorySection::Facts).unwrap();
    assert_eq!(entry.date, Some("2026-03-31".to_string()));
    assert_eq!(entry.content, "Project uses tokio");

    let entry = MemoryEntry::parse(
        "- Always use 4-space indentation",
        &MemorySection::Preferences,
    )
    .unwrap();
    assert_eq!(entry.date, None);
    assert_eq!(entry.content, "Always use 4-space indentation");

    assert!(MemoryEntry::parse("", &MemorySection::Facts).is_none());
    assert!(MemoryEntry::parse("## Header", &MemorySection::Facts).is_none());
}

#[test]
fn test_memory_store_parse_and_format() {
    let content = r#"# Project Memory

## Facts (consolidated)
- [2026-03-31] Project uses tokio for async runtime
- [2026-03-30] Prefer anyhow::Result over custom errors

## Preferences (user-defined)
- Always use 4-space indentation
- Never commit without tests

## Architecture Decisions
- Chose HNSW over flat vector search for performance
"#;

    let store = MemoryStore::parse(content);
    assert_eq!(store.entries.len(), 5); // 2 facts + 2 preferences + 1 architecture
    assert_eq!(
        store.sections.get(&MemorySection::Facts).map(|v| v.len()),
        Some(2)
    );
    assert_eq!(
        store
            .sections
            .get(&MemorySection::Preferences)
            .map(|v| v.len()),
        Some(2)
    );
    assert_eq!(
        store
            .sections
            .get(&MemorySection::ArchitectureDecisions)
            .map(|v| v.len()),
        Some(1)
    );

    // Format and re-parse should give same structure
    let reformatted = store.format();
    let reparsed = MemoryStore::parse(&reformatted);
    assert_eq!(reparsed.entries.len(), store.entries.len());
}

#[test]
fn test_memory_entry_is_stale() {
    // Old entry (100 days ago)
    let old_date = (chrono::Local::now() - chrono::Duration::days(100))
        .format("%Y-%m-%d")
        .to_string();
    let old_entry = MemoryEntry {
        date: Some(old_date.clone()),
        content: "Old fact".to_string(),
        section: MemorySection::Facts,
        raw_line: format!("- [{}] Old fact", old_date),
    };
    assert!(old_entry.is_stale(90));

    // Recent entry (10 days ago)
    let recent_date = (chrono::Local::now() - chrono::Duration::days(10))
        .format("%Y-%m-%d")
        .to_string();
    let recent_entry = MemoryEntry {
        date: Some(recent_date.clone()),
        content: "Recent fact".to_string(),
        section: MemorySection::Facts,
        raw_line: format!("- [{}] Recent fact", recent_date),
    };
    assert!(!recent_entry.is_stale(90));

    // Undated entry is never stale
    let undated_entry = MemoryEntry {
        date: None,
        content: "Preference".to_string(),
        section: MemorySection::Preferences,
        raw_line: "- Preference".to_string(),
    };
    assert!(!undated_entry.is_stale(90));
}

#[test]
fn test_memory_store_prune_stale() {
    // Dates are computed relative to today so the test is not a time-bomb:
    // a hardcoded "recent" date eventually ages past the 90-day threshold and
    // the test starts failing on wall-clock alone (observed 2026-07: a
    // 2026-03-31 fixture became 96 days old and was pruned unexpectedly).
    let old_date = (chrono::Local::now() - chrono::Duration::days(200))
        .format("%Y-%m-%d")
        .to_string();
    let recent_date = (chrono::Local::now() - chrono::Duration::days(10))
        .format("%Y-%m-%d")
        .to_string();
    let content = format!(
            "# Project Memory\n\n## Facts (consolidated)\n- [{}] Very old fact\n- [{}] Recent fact\n\n## Preferences (user-defined)\n- Always use spaces\n",
            old_date, recent_date
        );

    let mut store = MemoryStore::parse(&content);
    let removed = store.prune_stale(90);
    assert_eq!(removed, 1);
    assert_eq!(store.entries.len(), 2);
}

#[test]
fn test_generate_consolidation_prompt() {
    let memories = vec![
        MemoryEntry {
            date: Some("2026-03-31".to_string()),
            content: "Project uses tokio".to_string(),
            section: MemorySection::Facts,
            raw_line: "- [2026-03-31] Project uses tokio".to_string(),
        },
        MemoryEntry {
            date: None,
            content: "Use 4-space indent".to_string(),
            section: MemorySection::Preferences,
            raw_line: "- Use 4-space indent".to_string(),
        },
    ];

    let prompt = generate_consolidation_prompt(&memories);
    assert!(prompt.contains("Project uses tokio"));
    assert!(prompt.contains("memory consolidation"));
    assert!(prompt.contains("Facts (consolidated)"));
}

#[test]
fn test_dream_config_default() {
    let config = DreamConfig::default();
    assert_eq!(config.max_memory_lines, 200);
    assert_eq!(config.max_memory_size, 25 * 1024);
    assert_eq!(config.stale_memory_days, 90);
}

#[test]
fn test_dream_phase_display() {
    assert_eq!(DreamPhase::Orient.to_string(), "Orient");
    assert_eq!(DreamPhase::Consolidate.to_string(), "Consolidate");
}
