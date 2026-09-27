use super::*;
use crate::resources::context::{self, Owner};

fn session(id: &str) -> SessionRecord {
    SessionRecord {
        id: id.into(),
        pid: 1,
        start_time: None,
        started_at: Utc::now(),
    }
}

fn port(p: u16) -> NewResource {
    NewResource::new(ResourceKind::Port, ResourceHandle::Port { port: p }, "port")
}

#[test]
fn persists_atomically_and_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state").join("resources.json");
    let reg = ResourceRegistry::at_path_as(&path, session("a"));
    let id = reg.register_owned(port(8080), "t1".into(), Some("agent-x".into()));

    let text = std::fs::read_to_string(&path).unwrap();
    let file: RegistryFile = serde_json::from_str(&text).unwrap();
    assert_eq!(file.version, 1);
    assert_eq!(file.sessions[0].id, "a");
    // No temp file left behind.
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    let reloaded = ResourceRegistry::at_path_as(&path, session("b"));
    let r = reloaded.get(&id).unwrap();
    assert_eq!(r.owner_task, "t1");
    assert_eq!(r.owner_agent.as_deref(), Some("agent-x"));
    assert_eq!(r.state, ResourceState::Live);
}

#[test]
fn two_sessions_sharing_the_file_keep_each_others_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resources.json");
    let a = ResourceRegistry::at_path_as(&path, session("a"));
    let b = ResourceRegistry::at_path_as(&path, session("b"));
    let ia = a.register_owned(port(1), "ta".into(), None);
    let ib = b.register_owned(port(2), "tb".into(), None);
    // `a` writes again after `b`: must not drop `b`'s entry.
    assert!(a.release(&ia, "test: observed exit"));

    let reader = ResourceRegistry::at_path_as(&path, session("c"));
    assert!(reader.get(&ia).unwrap().state.is_released());
    assert_eq!(reader.get(&ib).unwrap().state, ResourceState::Live);
    let snapshot = reader.snapshot();
    let ids: Vec<_> = snapshot.sessions.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"a") && ids.contains(&"b"), "{ids:?}");
}

#[test]
fn old_released_entries_are_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resources.json");
    let reg = ResourceRegistry::at_path_as(&path, session("a"));
    let old = reg.register_owned(port(1), "t".into(), None);
    reg.update(&old, |r| r.state = ResourceState::Released);
    {
        let mut inner = reg.lock();
        let r = inner
            .file
            .resources
            .iter_mut()
            .find(|r| r.id == old)
            .unwrap();
        r.updated_at = Utc::now() - chrono::Duration::days(RELEASED_RETENTION_DAYS + 1);
    }
    let fresh = reg.register_owned(port(2), "t".into(), None);
    assert!(reg.get(&old).is_none());
    assert!(reg.get(&fresh).is_some());
}

#[test]
fn release_where_only_releases_matches() {
    let reg = ResourceRegistry::in_memory_as(session("a"));
    let a = reg.register_owned(port(1), "t".into(), None);
    let b = reg.register_owned(port(2), "t".into(), None);
    let n = reg.release_where("test: closed", |r| {
        matches!(r.handle, ResourceHandle::Port { port: 2 })
    });
    assert_eq!(n, 1);
    assert!(!reg.get(&a).unwrap().state.is_released());
    assert!(reg.get(&b).unwrap().state.is_released());
    assert_eq!(reg.owned_by("t").len(), 1);
}

#[tokio::test]
async fn register_uses_the_scoped_task_owner_else_the_session() {
    let reg = ResourceRegistry::in_memory_as(session("a"));
    let outside = reg.register(port(1));
    assert_eq!(
        reg.get(&outside).unwrap().owner_task,
        context::session_owner()
    );

    let owner = Owner::new().with_agent("reviewer");
    let inside = context::scope(owner.clone(), async {
        // Before the checkpoint exists: still the session.
        let early = reg.register(port(2));
        assert!(context::set_current_task("task-42"));
        (early, reg.register(port(3)))
    })
    .await;
    assert_eq!(
        reg.get(&inside.0).unwrap().owner_task,
        context::session_owner()
    );
    let late = reg.get(&inside.1).unwrap();
    assert_eq!(late.owner_task, "task-42");
    assert_eq!(late.owner_agent.as_deref(), Some("reviewer"));
    assert_eq!(owner.task().as_deref(), Some("task-42"));
    assert!(!context::set_current_task("no-scope"));
}

#[tokio::test]
async fn container_labels_name_the_current_owner() {
    let labels = context::scope(Owner::for_task("t-9"), async {
        context::container_label_args()
    })
    .await;
    assert!(labels.contains(&"selfware.task=t-9".to_string()));
    assert!(labels.contains(&format!("selfware.session={}", context::session_id())));
    // Image labels use distinct keys: containers inherit image labels.
    let image = context::image_label_args();
    assert!(image.iter().all(|l| !l.starts_with("selfware.task=")));
}

#[test]
fn unreadable_file_is_ignored_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resources.json");
    std::fs::write(&path, "{ not json").unwrap();
    let reg = ResourceRegistry::at_path_as(&path, session("a"));
    assert!(reg.unreleased().is_empty());
    let id = reg.register_owned(port(1), "t".into(), None);
    assert!(ResourceRegistry::at_path_as(&path, session("b"))
        .get(&id)
        .is_some());
}

/// A `resources.json` exactly as the first registry (before it shared the
/// lifecycle's state model) wrote it: `server_port` kind, every handle type,
/// orphaned / leaked / released states.
const FIRST_FORMAT_FILE: &str = r#"{
  "version": 1,
  "sessions": [
    { "id": "5f0c2a9d1b3e", "pid": 4242, "start_time": 1790000000, "started_at": "2026-09-20T10:00:00Z" }
  ],
  "resources": [
    {
      "id": "res-aaaaaaaaaaaa", "kind": "container", "owner_task": "task-1", "owner_agent": null,
      "session": "5f0c2a9d1b3e", "created_at": "2026-09-20T10:00:01Z", "updated_at": "2026-09-20T10:00:01Z",
      "state": "live",
      "handle": { "type": "container", "runtime": "docker", "id": "3f2a1b9c0d11", "task_label": "task-1" },
      "keep": false, "label": "nginx:latest", "note": null
    },
    {
      "id": "res-bbbbbbbbbbbb", "kind": "server_port", "owner_task": "session:5f0c2a9d1b3e",
      "session": "5f0c2a9d1b3e", "created_at": "2026-09-20T10:00:02Z", "updated_at": "2026-09-20T10:00:02Z",
      "state": "orphaned", "handle": { "type": "port", "port": 5173 }, "keep": true, "label": "evolve"
    },
    {
      "id": "res-cccccccccccc", "kind": "process", "owner_task": "task-1",
      "session": "5f0c2a9d1b3e", "created_at": "2026-09-20T10:00:03Z", "updated_at": "2026-09-20T10:00:03Z",
      "state": "leaked",
      "handle": { "type": "process", "pid": 777, "pgid": 777, "start_time": 1790000003, "managed_id": "proc-1" },
      "note": "still running after 10s polite stop + force"
    },
    {
      "id": "res-dddddddddddd", "kind": "pty", "owner_task": "task-1",
      "session": "5f0c2a9d1b3e", "created_at": "2026-09-20T10:00:04Z", "updated_at": "2026-09-20T10:00:04Z",
      "state": "draining",
      "handle": { "type": "pty", "session_id": "pty-1", "pid": 778, "pgid": null, "start_time": null }
    },
    {
      "id": "res-eeeeeeeeeeee", "kind": "worktree", "owner_task": "session:5f0c2a9d1b3e",
      "session": "5f0c2a9d1b3e", "created_at": "2026-09-20T10:00:05Z", "updated_at": "2099-09-20T10:00:05Z",
      "state": "released", "handle": { "type": "path", "path": "/tmp/wt" }, "keep": true
    },
    {
      "id": "res-ffffffffffff", "kind": "temp_dir", "owner_task": "task-1",
      "session": "5f0c2a9d1b3e", "created_at": "2026-09-20T10:00:06Z", "updated_at": "2026-09-20T10:00:06Z",
      "state": "requested",
      "handle": { "type": "compose", "runtime": "docker", "dir": "/tmp/app", "file": null }
    }
  ]
}"#;

#[test]
fn a_registry_file_in_the_first_format_still_loads() {
    let parsed: Result<RegistryFile, _> = serde_json::from_str(FIRST_FORMAT_FILE);
    assert!(parsed.is_ok(), "{:?}", parsed.err());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resources.json");
    std::fs::write(&path, FIRST_FORMAT_FILE).unwrap();
    let reg = ResourceRegistry::at_path_as(&path, session("now"));
    let snap = reg.snapshot();
    assert_eq!(snap.resources.len(), 6, "no entry dropped");
    assert_eq!(snap.sessions[0].id, "5f0c2a9d1b3e");
    let kinds: Vec<ResourceKind> = snap.resources.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ResourceKind::Container,
            ResourceKind::Port,
            ResourceKind::Process,
            ResourceKind::Pty,
            ResourceKind::Worktree,
            ResourceKind::TempDir,
        ]
    );
    let states: Vec<ResourceState> = snap.resources.iter().map(|r| r.state).collect();
    assert_eq!(
        states,
        vec![
            ResourceState::Live,
            ResourceState::Orphaned,
            ResourceState::Leaked,
            ResourceState::Draining,
            ResourceState::Released,
            ResourceState::Requested,
        ]
    );
    // The states the first format wrote keep their labels when written back.
    assert!(reg.release("res-cccccccccccc", "test: found gone"));
    let text = std::fs::read_to_string(&path).unwrap();
    for label in ["\"live\"", "\"orphaned\"", "\"draining\"", "\"requested\""] {
        assert!(text.contains(label), "{label} missing from {text}");
    }
}

fn logged_registry() -> (
    tempfile::TempDir,
    ResourceRegistry,
    crate::lifecycle::EventLog,
) {
    let dir = tempfile::tempdir().unwrap();
    let log = crate::lifecycle::EventLog::at(dir.path().join("events.jsonl"));
    let reg = ResourceRegistry::in_memory_as(session("a")).with_event_log(log.clone());
    (dir, reg, log)
}

#[test]
fn every_state_change_is_a_recorded_resource_transition() {
    let (_dir, reg, log) = logged_registry();
    let id = reg.register_owned(port(9000), "task-7".into(), None);
    reg.transition(&id, ResourceEvent::Drain, "teardown: draining", None)
        .unwrap();
    assert!(reg.release(&id, "test: confirmed gone"));

    let (records, skipped) = log.read_all();
    assert_eq!(skipped, 0);
    let steps: Vec<(Option<&str>, &str, Option<&str>, &str)> = records
        .iter()
        .map(|r| {
            (
                r.from.as_deref(),
                r.to.as_str(),
                r.event.as_deref(),
                r.cause.as_str(),
            )
        })
        .collect();
    assert_eq!(
        steps,
        vec![
            (None, "live", None, "registered: port port 9000 (port)"),
            (
                Some("live"),
                "draining",
                Some("drain"),
                "teardown: draining"
            ),
            (
                Some("draining"),
                "released",
                Some("stopped"),
                "test: confirmed gone"
            ),
        ]
    );
    for r in &records {
        assert_eq!(r.entity, crate::lifecycle::Entity::Resource);
        assert_eq!(r.id, id);
        assert_eq!(r.owner.as_deref(), Some("task-7"));
    }
}

#[test]
fn a_refused_transition_is_a_typed_error_that_changes_nothing() {
    let (_dir, reg, log) = logged_registry();
    let id = reg.register_owned(port(9001), "t".into(), None);
    let before = log.read_all().0.len();
    let err = reg
        .transition(&id, ResourceEvent::Ready, "bogus", Some("n".into()))
        .unwrap_err();
    assert!(matches!(
        &err,
        TransitionError::Invalid { source, .. } if source.from == "live" && source.event == "ready"
    ));
    let r = reg.get(&id).unwrap();
    assert_eq!(r.state, ResourceState::Live);
    assert_eq!(r.note, None);
    assert_eq!(log.read_all().0.len(), before, "a refusal records nothing");
    assert_eq!(
        reg.transition("res-missing", ResourceEvent::Drain, "x", None),
        Err(TransitionError::NotFound("res-missing".into()))
    );
    // Released is sticky: releasing twice is not an error for callers, and
    // nothing leaves it.
    assert!(reg.release(&id, "gone"));
    assert!(reg.release(&id, "gone again"));
    assert!(reg
        .transition(&id, ResourceEvent::Reap, "retry", None)
        .is_err());
}

#[test]
fn entering_leaked_raises_the_leak_alarm() {
    let (_dir, reg, _log) = logged_registry();
    let id = reg.register_owned(port(9002), "t".into(), None);
    assert!(reg
        .transition(&id, ResourceEvent::Drain, "d", None)
        .unwrap()
        .is_empty());
    let effects = reg
        .transition(
            &id,
            ResourceEvent::Abandon,
            "gave up",
            Some("foreign".into()),
        )
        .unwrap();
    assert_eq!(effects, vec![crate::lifecycle::Effect::LeakAlarm]);
    assert_eq!(reg.get(&id).unwrap().note.as_deref(), Some("foreign"));
}
