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
    NewResource::new(
        ResourceKind::ServerPort,
        ResourceHandle::Port { port: p },
        "port",
    )
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
    a.release(&ia);

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
    let n = reg.release_where(|r| matches!(r.handle, ResourceHandle::Port { port: 2 }));
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
