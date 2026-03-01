use codegen_task_runner::{Priority, Status, TaskManager};

#[test]
fn test_new_empty() {
    let tm = TaskManager::new();
    assert!(tm.get(1).is_none());
    assert!(tm.sorted_by_priority().is_empty());
}

#[test]
fn test_add_task() {
    let mut tm = TaskManager::new();
    let id1 = tm.add("first", Priority::Low);
    let id2 = tm.add("second", Priority::High);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(tm.get(1).unwrap().name, "first");
    assert_eq!(tm.get(2).unwrap().priority, Priority::High);
    assert_eq!(tm.get(1).unwrap().status, Status::Pending);
    assert!(tm.get(1).unwrap().tags.is_empty());
}

#[test]
fn test_get_task() {
    let mut tm = TaskManager::new();
    tm.add("alpha", Priority::Medium);
    let task = tm.get(1).unwrap();
    assert_eq!(task.id, 1);
    assert_eq!(task.name, "alpha");
    assert_eq!(task.priority, Priority::Medium);
}

#[test]
fn test_remove_task() {
    let mut tm = TaskManager::new();
    tm.add("removable", Priority::Low);
    let removed = tm.remove(1);
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().name, "removable");
    assert!(tm.get(1).is_none());
    // Removing again should return None
    assert!(tm.remove(1).is_none());
}

#[test]
fn test_update_status() {
    let mut tm = TaskManager::new();
    tm.add("status_test", Priority::High);
    assert!(tm.update_status(1, Status::Running));
    assert_eq!(tm.get(1).unwrap().status, Status::Running);
    assert!(tm.update_status(1, Status::Failed("timeout".into())));
    assert_eq!(
        tm.get(1).unwrap().status,
        Status::Failed("timeout".into())
    );
    // Non-existent task returns false
    assert!(!tm.update_status(999, Status::Completed));
}

#[test]
fn test_add_tag() {
    let mut tm = TaskManager::new();
    tm.add("tagged", Priority::Medium);
    assert!(tm.add_tag(1, "urgent"));
    assert!(tm.add_tag(1, "backend"));
    let tags = &tm.get(1).unwrap().tags;
    assert_eq!(tags.len(), 2);
    assert!(tags.contains(&"urgent".to_string()));
    assert!(tags.contains(&"backend".to_string()));
    // Non-existent task returns false
    assert!(!tm.add_tag(999, "nope"));
}

#[test]
fn test_filter_by_status() {
    let mut tm = TaskManager::new();
    tm.add("a", Priority::Low);
    tm.add("b", Priority::Medium);
    tm.add("c", Priority::High);
    tm.update_status(1, Status::Running);
    tm.update_status(2, Status::Running);
    let running = tm.by_status(&Status::Running);
    assert_eq!(running.len(), 2);
    let pending = tm.by_status(&Status::Pending);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].name, "c");
}

#[test]
fn test_filter_by_priority() {
    let mut tm = TaskManager::new();
    tm.add("lo1", Priority::Low);
    tm.add("hi1", Priority::High);
    tm.add("lo2", Priority::Low);
    let lows = tm.by_priority(&Priority::Low);
    assert_eq!(lows.len(), 2);
    let highs = tm.by_priority(&Priority::High);
    assert_eq!(highs.len(), 1);
    assert_eq!(highs[0].name, "hi1");
}

#[test]
fn test_filter_by_tag() {
    let mut tm = TaskManager::new();
    tm.add("x", Priority::Low);
    tm.add("y", Priority::Medium);
    tm.add("z", Priority::High);
    tm.add_tag(1, "frontend");
    tm.add_tag(2, "frontend");
    tm.add_tag(3, "backend");
    let frontend = tm.by_tag("frontend");
    assert_eq!(frontend.len(), 2);
    let backend = tm.by_tag("backend");
    assert_eq!(backend.len(), 1);
    assert_eq!(backend[0].name, "z");
    let none = tm.by_tag("nonexistent");
    assert!(none.is_empty());
}

#[test]
fn test_sorted_by_priority() {
    let mut tm = TaskManager::new();
    tm.add("low_task", Priority::Low);
    tm.add("critical_task", Priority::Critical);
    tm.add("medium_task", Priority::Medium);
    tm.add("high_task", Priority::High);
    let sorted = tm.sorted_by_priority();
    assert_eq!(sorted.len(), 4);
    assert_eq!(sorted[0].priority, Priority::Critical);
    assert_eq!(sorted[1].priority, Priority::High);
    assert_eq!(sorted[2].priority, Priority::Medium);
    assert_eq!(sorted[3].priority, Priority::Low);
}

#[test]
fn test_json_roundtrip() {
    let mut tm = TaskManager::new();
    tm.add("json_test", Priority::High);
    tm.add_tag(1, "serializable");
    tm.update_status(1, Status::Running);
    tm.add("second_json", Priority::Low);

    let json = tm.to_json();
    let restored = TaskManager::from_json(&json).expect("should parse");
    assert_eq!(restored.get(1).unwrap().name, "json_test");
    assert_eq!(restored.get(1).unwrap().status, Status::Running);
    assert!(restored.get(1).unwrap().tags.contains(&"serializable".to_string()));
    assert_eq!(restored.get(2).unwrap().name, "second_json");

    // New IDs should continue from where we left off
    let mut restored = TaskManager::from_json(&json).expect("should parse");
    let id3 = restored.add("third", Priority::Medium);
    assert_eq!(id3, 3);
}

#[test]
fn test_cleanup_completed() {
    let mut tm = TaskManager::new();
    let id1 = tm.add("old_done", Priority::Low);
    let id2 = tm.add("new_done", Priority::Medium);
    let id3 = tm.add("still_running", Priority::High);
    tm.update_status(id1, Status::Completed);
    tm.update_status(id2, Status::Completed);
    tm.update_status(id3, Status::Running);

    // Cleanup tasks completed with created_at < threshold
    // Since add() sets created_at, we use a threshold larger than all timestamps
    let threshold = u64::MAX;
    let removed = tm.cleanup_completed(threshold);
    assert_eq!(removed, 2);
    assert!(tm.get(id1).is_none());
    assert!(tm.get(id2).is_none());
    assert!(tm.get(id3).is_some()); // Running task should remain
}

#[test]
fn test_multiple_operations() {
    let mut tm = TaskManager::new();

    // Add several tasks
    let id1 = tm.add("deploy", Priority::Critical);
    let id2 = tm.add("write docs", Priority::Low);
    let id3 = tm.add("fix bug", Priority::High);
    let id4 = tm.add("review PR", Priority::Medium);

    // Tag them
    tm.add_tag(id1, "ops");
    tm.add_tag(id3, "ops");
    tm.add_tag(id2, "docs");
    tm.add_tag(id4, "review");

    // Update statuses
    tm.update_status(id1, Status::Running);
    tm.update_status(id2, Status::Completed);

    // Verify filters work together
    let ops = tm.by_tag("ops");
    assert_eq!(ops.len(), 2);

    let running = tm.by_status(&Status::Running);
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].name, "deploy");

    // Remove a task and verify
    tm.remove(id4);
    assert_eq!(tm.sorted_by_priority().len(), 3);

    // Priority ordering with remaining tasks
    let sorted = tm.sorted_by_priority();
    assert_eq!(sorted[0].name, "deploy");   // Critical
    assert_eq!(sorted[1].name, "fix bug");  // High
    assert_eq!(sorted[2].name, "write docs"); // Low
}

#[test]
fn test_nonexistent_task() {
    let mut tm = TaskManager::new();
    assert!(tm.get(42).is_none());
    assert!(tm.remove(42).is_none());
    assert!(!tm.update_status(42, Status::Running));
    assert!(!tm.add_tag(42, "nope"));
}
