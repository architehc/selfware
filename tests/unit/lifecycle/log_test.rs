use super::*;

fn rec(id: &str, from: Option<&str>, to: &str) -> TransitionRecord {
    TransitionRecord::now(Entity::Task, id, from, to, Some("start"), "cause")
}

#[test]
fn round_trips_records_in_append_order() {
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("nested").join("events.jsonl"));
    log.append(&rec("a", None, "queued"), false);
    log.append(&rec("a", Some("queued"), "planning"), true);
    let (records, skipped) = log.read_all();
    assert_eq!(skipped, 0);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].to, "queued");
    assert_eq!(records[0].from, None);
    assert_eq!(records[1].from.as_deref(), Some("queued"));
    // Wire format: the required fields are always present, `from` as null.
    let text = std::fs::read_to_string(log.path().unwrap()).unwrap();
    let first: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    for key in ["ts", "entity", "id", "from", "to", "cause"] {
        assert!(first.get(key).is_some(), "missing {key}: {first}");
    }
    assert!(first["from"].is_null());
    assert_eq!(first["entity"], "task");
}

#[cfg(unix)]
#[test]
fn log_file_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("events.jsonl"));
    log.append(&rec("a", None, "queued"), false);
    let mode = std::fs::metadata(log.path().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "mode {mode:o}");
}

#[test]
fn disabled_log_records_and_reads_nothing() {
    let log = EventLog::disabled();
    log.append(&rec("a", None, "queued"), true);
    assert!(log.path().is_none());
    assert_eq!(log.read_all(), (Vec::new(), 0));
}

#[test]
fn unwritable_location_never_fails_the_caller() {
    let dir = tempfile::tempdir().unwrap();
    // A regular file where the log's parent directory should be.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let log = EventLog::at(blocker.join("events.jsonl"));
    log.append(&rec("a", None, "queued"), true); // must not panic
    assert_eq!(log.read_all().0.len(), 0);
}

#[test]
fn malformed_lines_are_skipped_and_counted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let log = EventLog::at(&path);
    log.append(&rec("a", None, "queued"), false);
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("{not json\n\n");
    std::fs::write(&path, text).unwrap();
    log.append(&rec("a", Some("queued"), "planning"), false);
    let (records, skipped) = log.read_all();
    assert_eq!(records.len(), 2);
    assert_eq!(skipped, 1);
}

#[test]
fn rotates_once_past_the_size_cap_and_reads_both_generations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let log = EventLog::at(&path);
    log.append(&rec("old", None, "queued"), false);
    // Pad the file past the cap (the NUL padding reads back as one
    // unparseable, skipped line).
    let f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.set_len(MAX_LOG_BYTES + 10).unwrap();
    drop(f);
    log.append(&rec("new", None, "queued"), false);
    assert!(dir.path().join("events.jsonl.1").exists());
    assert!(std::fs::metadata(&path).unwrap().len() < 1024);
    let (records, skipped) = log.read_all();
    assert_eq!(skipped, 1);
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["old", "new"], "rotated generation is read first");
}

#[test]
fn cause_is_scrubbed_single_line_and_capped() {
    let r = TransitionRecord::now(
        Entity::Task,
        "a",
        None,
        "failed",
        Some("fail"),
        "API error: key sk-abc12345defghijk rejected\nsecond line",
    );
    assert!(!r.cause.contains("sk-abc12345defghijk"), "{}", r.cause);
    assert!(!r.cause.contains("second line"));
    let long = "x".repeat(1000);
    let r = TransitionRecord::now(Entity::Task, "a", None, "failed", None, &long);
    assert_eq!(r.cause.chars().count(), MAX_CAUSE_CHARS);
    assert!(r.cause.ends_with('…'));
}

#[test]
fn test_builds_default_to_a_temp_dir_not_the_home_log() {
    if std::env::var_os(EVENT_LOG_ENV).is_some() {
        return; // an explicit override wins; nothing to check here
    }
    let log = EventLog::default_location();
    let path = log.path().expect("enabled by default");
    assert!(path.starts_with(std::env::temp_dir()), "{}", path.display());
    assert!(path.ends_with("events.jsonl"));
}
