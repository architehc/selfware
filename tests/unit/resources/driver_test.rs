use super::*;

#[test]
fn parse_labelled_keeps_only_fully_labelled_containers() {
    let text = "\
abc123|task-1|sess-1|reviewer|running|web|nginx:latest
def456|task-2|sess-2||exited|db|postgres:16
0000aa|||||user-box|ubuntu
0000bb|task-x||||half|alpine
garbage line
";
    let found = parse_labelled("docker", text);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].id, "abc123");
    assert_eq!(found[0].agent.as_deref(), Some("reviewer"));
    assert!(found[0].running);
    assert_eq!(found[1].agent, None);
    assert!(!found[1].running);
}

#[test]
fn probe_pid_treats_absent_and_reused_pids_as_gone_and_unknown_start_as_unknown() {
    const DEAD_PID: u32 = 2_147_483_646;
    assert_eq!(probe_pid(DEAD_PID, Some(1)), Probe::Gone);
    let me = std::process::id();
    let start = process_start_time(me).expect("own start time");
    assert_eq!(probe_pid(me, Some(start)), Probe::Running);
    // Same pid, different start time: the recorded process is gone and the
    // current holder of the pid is not ours.
    assert_eq!(probe_pid(me, Some(start.wrapping_add(7))), Probe::Gone);
    assert!(matches!(probe_pid(me, None), Probe::Unknown(_)));
}

#[test]
fn session_alive_uses_start_time() {
    let driver = SystemDriver::default();
    let me = std::process::id();
    let live = SessionRecord {
        id: "someone-else".into(),
        pid: me,
        start_time: process_start_time(me),
        started_at: chrono::Utc::now(),
    };
    assert!(driver.session_alive(&live));
    let reused = SessionRecord {
        start_time: live.start_time.map(|s| s + 3),
        ..live.clone()
    };
    assert!(!driver.session_alive(&reused));
}

/// Real-process tests: only our own freshly spawned `sleep` children.
#[cfg(unix)]
mod unix {
    use super::*;
    use crate::resources::registry::{NewResource, ResourceRegistry};
    use crate::resources::{ResourceKind, ResourceState, TeardownPolicy};

    fn spawn_sleeper() -> tokio::process::Child {
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("30").process_group(0).kill_on_drop(true);
        cmd.spawn().expect("spawn sleep")
    }

    fn sleeper_resource(child: &tokio::process::Child, start_time: Option<u64>) -> NewResource {
        let pid = child.id().unwrap();
        NewResource::new(
            ResourceKind::Process,
            ResourceHandle::Process {
                pid,
                pgid: Some(pid),
                start_time,
                managed_id: None,
            },
            "sleep 30",
        )
    }

    /// A real child in its own process group is drained politely and confirmed.
    #[tokio::test]
    async fn system_driver_drains_a_verified_child_process() {
        let mut child = spawn_sleeper();
        let pid = child.id().unwrap();
        let reg = ResourceRegistry::in_memory();
        let id = reg.register_owned(
            sleeper_resource(&child, process_start_time(pid)),
            "t".into(),
            None,
        );
        let policy = crate::resources::TeardownPolicy::with_deadline(Duration::from_secs(5));
        let report =
            crate::resources::teardown::teardown_task(&reg, &SystemDriver::default(), "t", policy)
                .await;
        assert_eq!(report.released.len(), 1, "{report:?}");
        assert_eq!(reg.get(&id).unwrap().state, ResourceState::Released);
        let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(!status.success(), "terminated by signal");
    }

    /// A pid whose start time does not match is never signalled, and a pid
    /// without a recorded start time is never signalled either.
    #[tokio::test]
    async fn system_driver_never_signals_an_unverified_pid() {
        let mut child = spawn_sleeper();
        let pid = child.id().unwrap();
        let start = process_start_time(pid).unwrap();
        let driver = SystemDriver::default();
        let reg = ResourceRegistry::in_memory();

        let mismatched = reg.register_owned(
            sleeper_resource(&child, Some(start + 100)),
            "a".into(),
            None,
        );
        let unrecorded = reg.register_owned(sleeper_resource(&child, None), "b".into(), None);
        let policy = TeardownPolicy {
            deadline: Duration::from_millis(50),
            force_grace: Duration::from_millis(50),
            poll: Duration::from_millis(10),
        };
        let a = crate::resources::teardown::teardown_task(&reg, &driver, "a", policy).await;
        let b = crate::resources::teardown::teardown_task(&reg, &driver, "b", policy).await;

        // Mismatch reads as "ours is gone" (released) without a signal; no
        // start time reads as unknown (leaked), also without a signal.
        assert_eq!(a.released.len(), 1);
        assert_eq!(b.leaked.len(), 1);
        assert_eq!(reg.get(&unrecorded).unwrap().state, ResourceState::Leaked);
        assert!(reg.get(&mismatched).unwrap().state.is_released());
        assert!(
            child.try_wait().unwrap().is_none(),
            "the foreign-looking process was not touched"
        );
        child.kill().await.unwrap();
    }
}
