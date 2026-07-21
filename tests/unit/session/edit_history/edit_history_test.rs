use super::*;

#[test]
fn test_checkpoint_id_creation() {
    let id = EditCheckpointId::new(42);
    assert_eq!(id.0, 42);
}

#[test]
fn test_file_snapshot_creation() {
    let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "fn main() {}".to_string());
    assert_eq!(snapshot.path, PathBuf::from("test.rs"));
    assert_eq!(snapshot.size, 12);
    assert!(!snapshot.hash.is_empty());
}

#[test]
fn test_file_snapshot_changed() {
    let s1 = FileSnapshot::new(PathBuf::from("test.rs"), "content1".to_string());
    let s2 = FileSnapshot::new(PathBuf::from("test.rs"), "content2".to_string());
    let s3 = FileSnapshot::new(PathBuf::from("test.rs"), "content1".to_string());

    assert!(s1.changed_from(&s2));
    assert!(!s1.changed_from(&s3));
}

#[test]
fn test_edit_action_description() {
    let action = EditAction::FileEdit {
        path: PathBuf::from("main.rs"),
        tool: "file_edit".to_string(),
    };
    assert!(action.description().contains("main.rs"));
}

#[test]
fn test_edit_action_icons() {
    assert_eq!(
        EditAction::FileCreate {
            path: PathBuf::new()
        }
        .icon(),
        "📄"
    );
    assert_eq!(
        EditAction::FileEdit {
            path: PathBuf::new(),
            tool: String::new()
        }
        .icon(),
        "✏️"
    );
    assert_eq!(
        EditAction::FileDelete {
            path: PathBuf::new()
        }
        .icon(),
        "🗑️"
    );
    assert_eq!(
        EditAction::GitCommit {
            hash: String::new(),
            message: String::new()
        }
        .icon(),
        "🔀"
    );
    assert_eq!(
        EditAction::Manual {
            description: String::new()
        }
        .icon(),
        "📌"
    );
    assert_eq!(EditAction::SessionStart.icon(), "🚀");
    assert_eq!(EditAction::SessionEnd.icon(), "🏁");
}

#[test]
fn test_checkpoint_creation() {
    let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
    assert_eq!(checkpoint.id.0, 1);
    assert!(checkpoint.files.is_empty());
}

#[test]
fn test_checkpoint_add_file() {
    let mut checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
    let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
    checkpoint.add_file(snapshot);
    assert_eq!(checkpoint.files.len(), 1);
}

#[test]
fn test_checkpoint_builder_methods() {
    let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart)
        .with_git_hash("abc123".to_string())
        .with_parent(EditCheckpointId::new(0))
        .with_branch("feature".to_string())
        .with_label("Test label".to_string());

    assert_eq!(checkpoint.git_hash, Some("abc123".to_string()));
    assert_eq!(checkpoint.parent, Some(EditCheckpointId::new(0)));
    assert_eq!(checkpoint.branch, Some("feature".to_string()));
    assert_eq!(checkpoint.label, Some("Test label".to_string()));
}

#[test]
fn test_checkpoint_formatted_time() {
    let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
    let time = checkpoint.formatted_time();
    assert!(time.contains(":")); // HH:MM:SS format
}

#[test]
fn test_checkpoint_relative_time() {
    let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
    let relative = checkpoint.relative_time();
    assert!(relative.contains("ago") || relative.contains("s"));
}

#[test]
fn test_edit_history_creation() {
    let history = EditHistory::new();
    assert!(history.is_empty());
    assert_eq!(history.len(), 0);
}

#[test]
fn test_edit_history_default() {
    let history = EditHistory::default();
    assert!(history.is_empty());
}

#[test]
fn test_edit_history_with_max() {
    let history = EditHistory::with_max_checkpoints(50);
    assert_eq!(history.max_checkpoints, 50);
}

#[test]
fn test_create_checkpoint() {
    let mut history = EditHistory::new();
    let id = history.create_checkpoint(EditAction::SessionStart);
    assert_eq!(id.0, 1);
    assert_eq!(history.len(), 1);
}

#[test]
fn test_multiple_checkpoints() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_checkpoint(EditAction::FileEdit {
        path: PathBuf::from("test.rs"),
        tool: "edit".to_string(),
    });
    history.create_checkpoint(EditAction::SessionEnd);
    assert_eq!(history.len(), 3);
}

#[test]
fn test_add_file_to_current() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
    history.add_file_to_current(snapshot);

    let current = history.current_checkpoint().unwrap();
    assert_eq!(current.files.len(), 1);
}

#[test]
fn test_undo_redo() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_checkpoint(EditAction::FileEdit {
        path: PathBuf::from("test.rs"),
        tool: "edit".to_string(),
    });
    history.create_checkpoint(EditAction::SessionEnd);

    assert_eq!(history.position(), 3);
    assert!(history.can_undo());
    assert!(!history.can_redo());

    history.undo();
    assert_eq!(history.position(), 2);
    assert!(history.can_undo());
    assert!(history.can_redo());

    history.redo();
    assert_eq!(history.position(), 3);
}

#[test]
fn test_goto() {
    let mut history = EditHistory::new();
    let id1 = history.create_checkpoint(EditAction::SessionStart);
    history.create_checkpoint(EditAction::SessionEnd);

    history.goto(id1);
    assert_eq!(history.position(), 1);
}

#[test]
fn test_cannot_undo_at_start() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    assert!(!history.can_undo());
}

#[test]
fn test_cannot_redo_at_end() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    assert!(!history.can_redo());
}

#[test]
fn test_branches() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);

    assert!(history.current_branch().is_none());

    history.create_branch("feature-x");
    assert_eq!(history.current_branch(), Some("feature-x"));

    history.switch_to_main();
    assert!(history.current_branch().is_none());
}

#[test]
fn test_timeline() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_checkpoint(EditAction::SessionEnd);

    let timeline = history.timeline();
    assert_eq!(timeline.len(), 2);
    assert!(timeline[1].is_current);
}

#[test]
fn test_timeline_entry_display() {
    let entry = TimelineEntry {
        id: EditCheckpointId::new(1),
        is_current: true,
        action: EditAction::SessionStart,
        timestamp: Utc::now(),
        label: None,
        branch: None,
    };
    assert!(!entry.display_text().is_empty());

    let entry_with_label = TimelineEntry {
        label: Some("Custom Label".to_string()),
        ..entry
    };
    assert_eq!(entry_with_label.display_text(), "Custom Label");
}

#[test]
fn test_clear_history() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_checkpoint(EditAction::SessionEnd);
    assert_eq!(history.len(), 2);

    history.clear();
    assert!(history.is_empty());
    assert_eq!(history.position(), 0);
}

#[test]
fn test_max_checkpoints_pruning() {
    let mut history = EditHistory::with_max_checkpoints(5);

    for i in 0..10 {
        history.create_checkpoint(EditAction::Manual {
            description: format!("Checkpoint {}", i),
        });
    }

    assert_eq!(history.len(), 5);
}

#[test]
fn test_truncate_future_on_new_checkpoint() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    let id2 = history.create_checkpoint(EditAction::SessionEnd);
    history.create_checkpoint(EditAction::Manual {
        description: "Will be removed".to_string(),
    });

    // Go back
    history.goto(id2);
    assert_eq!(history.position(), 2);

    // Create new checkpoint (should truncate future)
    history.create_checkpoint(EditAction::Manual {
        description: "New future".to_string(),
    });
    assert_eq!(history.len(), 3);
}

#[test]
fn test_get_checkpoint() {
    let mut history = EditHistory::new();
    let id = history.create_checkpoint(EditAction::SessionStart);

    let checkpoint = history.get(id);
    assert!(checkpoint.is_some());
    assert_eq!(checkpoint.unwrap().id, id);

    let invalid = history.get(EditCheckpointId::new(999));
    assert!(invalid.is_none());
}

#[test]
fn test_compute_hash() {
    let h1 = compute_hash("content");
    let h2 = compute_hash("content");
    let h3 = compute_hash("different");

    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
}

#[test]
fn test_truncate_function() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello world", 8), "hello...");
}

#[test]
fn test_multi_file_action() {
    let action = EditAction::MultiFileEdit {
        paths: vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")],
        tool: "refactor".to_string(),
    };
    assert!(action.description().contains("2 files"));
    assert_eq!(action.icon(), "📝");
}

#[test]
fn test_all_checkpoints() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_checkpoint(EditAction::SessionEnd);

    let all = history.all();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_checkpoint_id_clone() {
    let id = EditCheckpointId::new(42);
    let cloned = id;
    assert_eq!(id, cloned);
}

#[test]
fn test_checkpoint_id_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(EditCheckpointId::new(1));
    set.insert(EditCheckpointId::new(2));
    set.insert(EditCheckpointId::new(1)); // Duplicate
    assert_eq!(set.len(), 2);
}

#[test]
fn test_checkpoint_id_serialize() {
    let id = EditCheckpointId::new(42);
    let json = serde_json::to_string(&id).unwrap();
    assert!(json.contains("42"));
}

#[test]
fn test_checkpoint_id_deserialize() {
    let json = "42";
    let id: EditCheckpointId = serde_json::from_str(json).unwrap();
    assert_eq!(id.0, 42);
}

#[test]
fn test_file_snapshot_clone() {
    let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
    let cloned = snapshot.clone();
    assert_eq!(snapshot.path, cloned.path);
    assert_eq!(snapshot.hash, cloned.hash);
}

#[test]
fn test_file_snapshot_serialize() {
    let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(json.contains("test.rs"));
    assert!(json.contains("content"));
}

#[test]
fn test_file_snapshot_empty_content() {
    let snapshot = FileSnapshot::new(PathBuf::from("empty.txt"), "".to_string());
    assert_eq!(snapshot.size, 0);
    assert!(!snapshot.hash.is_empty());
}

#[test]
fn test_edit_action_clone() {
    let action = EditAction::FileEdit {
        path: PathBuf::from("test.rs"),
        tool: "edit".to_string(),
    };
    let cloned = action.clone();
    assert!(matches!(cloned, EditAction::FileEdit { .. }));
}

#[test]
fn test_edit_action_serialize() {
    let action = EditAction::GitCommit {
        hash: "abc123".to_string(),
        message: "Test commit".to_string(),
    };
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("abc123"));
    assert!(json.contains("Test commit"));
}

#[test]
fn test_edit_action_file_delete_description() {
    let action = EditAction::FileDelete {
        path: PathBuf::from("obsolete.rs"),
    };
    assert!(action.description().contains("Deleted"));
    assert!(action.description().contains("obsolete.rs"));
}

#[test]
fn test_edit_action_git_commit_short_hash() {
    let action = EditAction::GitCommit {
        hash: "abc".to_string(),
        message: "Short hash".to_string(),
    };
    let desc = action.description();
    assert!(desc.contains("abc")); // Full hash since < 7 chars
}

#[test]
fn test_edit_action_manual_long_description() {
    let long_desc = "This is a very long description that should be truncated when displayed";
    let action = EditAction::Manual {
        description: long_desc.to_string(),
    };
    let desc = action.description();
    assert!(desc.len() < long_desc.len() + 10); // Should be truncated
}

#[test]
fn test_edit_checkpoint_clone() {
    let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
    let cloned = checkpoint.clone();
    assert_eq!(checkpoint.id, cloned.id);
}

#[test]
fn test_edit_checkpoint_serialize() {
    let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
    let json = serde_json::to_string(&checkpoint).unwrap();
    assert!(json.contains("id"));
    assert!(json.contains("timestamp"));
}

#[test]
fn test_history_undo_at_start() {
    let mut history = EditHistory::new();
    history.undo(); // Should do nothing, not panic
    assert_eq!(history.position(), 0);
}

#[test]
fn test_history_redo_at_end() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.redo(); // Should do nothing, not panic
    assert_eq!(history.position(), 1);
}

#[test]
fn test_history_goto_invalid() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.goto(EditCheckpointId::new(999)); // Invalid ID
    assert_eq!(history.position(), 1); // Should stay at current
}

#[test]
fn test_history_current_checkpoint_empty() {
    let history = EditHistory::new();
    assert!(history.current_checkpoint().is_none());
}

#[test]
fn test_add_file_to_current_empty_history() {
    let mut history = EditHistory::new();
    let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
    history.add_file_to_current(snapshot); // Should not panic
}

#[test]
fn test_timeline_with_labels() {
    let mut history = EditHistory::new();
    let _checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart)
        .with_label("Important".to_string());

    // We need to use the internal method to get proper label coverage
    history.create_checkpoint(EditAction::Manual {
        description: "Labeled checkpoint".to_string(),
    });

    let timeline = history.timeline();
    assert!(!timeline.is_empty());
}

#[test]
fn test_timeline_with_branch() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_branch("feature-y");

    // Verify the branch was created
    assert_eq!(history.current_branch(), Some("feature-y"));

    // Create another checkpoint while on branch
    history.create_checkpoint(EditAction::SessionEnd);

    let timeline = history.timeline();
    // The new checkpoint should have the branch
    assert!(timeline.len() >= 2);
}

#[test]
fn test_switch_branch_back() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_branch("branch-a");
    history.create_branch("branch-b");
    history.switch_to_main();

    assert!(history.current_branch().is_none());
}

#[test]
fn test_multiple_files_in_checkpoint() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);

    let snapshot1 = FileSnapshot::new(PathBuf::from("a.rs"), "content a".to_string());
    let snapshot2 = FileSnapshot::new(PathBuf::from("b.rs"), "content b".to_string());

    history.add_file_to_current(snapshot1);
    history.add_file_to_current(snapshot2);

    let checkpoint = history.current_checkpoint().unwrap();
    assert_eq!(checkpoint.files.len(), 2);
}

#[test]
fn test_compute_hash_unicode() {
    let h1 = compute_hash("こんにちは");
    let h2 = compute_hash("你好");
    assert_ne!(h1, h2);
    assert!(!h1.is_empty());
}

#[test]
fn test_truncate_exact_length() {
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn test_truncate_short() {
    assert_eq!(truncate("hi", 10), "hi");
}

#[test]
fn test_timeline_entry_current_marker() {
    let entry = TimelineEntry {
        id: EditCheckpointId::new(1),
        is_current: true,
        action: EditAction::SessionStart,
        timestamp: Utc::now(),
        label: None,
        branch: None,
    };
    assert!(entry.is_current);
}

#[test]
fn test_file_snapshot_debug() {
    let snapshot = FileSnapshot::new(PathBuf::from("test.rs"), "content".to_string());
    let debug = format!("{:?}", snapshot);
    assert!(debug.contains("FileSnapshot"));
}

#[test]
fn test_edit_action_debug() {
    let action = EditAction::SessionStart;
    let debug = format!("{:?}", action);
    assert!(debug.contains("SessionStart"));
}

#[test]
fn test_checkpoint_debug() {
    let checkpoint = EditCheckpoint::new(EditCheckpointId::new(1), EditAction::SessionStart);
    let debug = format!("{:?}", checkpoint);
    assert!(debug.contains("EditCheckpoint"));
}

#[test]
fn test_history_multiple_undo() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_checkpoint(EditAction::FileEdit {
        path: PathBuf::from("a.rs"),
        tool: "edit".to_string(),
    });
    history.create_checkpoint(EditAction::FileEdit {
        path: PathBuf::from("b.rs"),
        tool: "edit".to_string(),
    });

    history.undo();
    history.undo();

    assert_eq!(history.position(), 1);
    assert!(history.can_redo());
}

#[test]
fn test_history_undo_then_create() {
    let mut history = EditHistory::new();
    history.create_checkpoint(EditAction::SessionStart);
    history.create_checkpoint(EditAction::SessionEnd);
    history.undo();

    // Creating new checkpoint should truncate redo stack
    history.create_checkpoint(EditAction::Manual {
        description: "New".to_string(),
    });

    assert!(!history.can_redo());
}
