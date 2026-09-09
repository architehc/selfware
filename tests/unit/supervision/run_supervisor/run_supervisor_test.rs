use super::*;
use std::time::Duration;

/// Wait (with a timeout) for a run to reach the expected status.
///
/// Polls `status()` with short yields so the spawned task has a chance to
/// finish. Avoids fixed long sleeps.
async fn wait_for_status(
    sup: &RunSupervisor,
    id: &RunId,
    expected: RunStatus,
    timeout: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(s) = sup.status(id).await {
            if s == expected {
                return true;
            }
        } else {
            return false;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        // Yield to let the spawned task make progress.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

#[tokio::test]
async fn spawn_ok_completes() {
    let sup = RunSupervisor::new();
    let id = sup.spawn("trivial-ok".into(), async { Ok(()) }).await;

    assert!(
        wait_for_status(&sup, &id, RunStatus::Completed, Duration::from_secs(2)).await,
        "run should reach Completed"
    );

    // list() must include it.
    let listed = sup.list().await;
    assert!(listed
        .iter()
        .any(|(rid, st)| rid == &id && *st == RunStatus::Completed));
}

#[tokio::test]
async fn spawn_err_fails() {
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("trivial-err".into(), async { Err(anyhow::anyhow!("boom")) })
        .await;

    assert!(
        wait_for_status(&sup, &id, RunStatus::Failed, Duration::from_secs(2)).await,
        "run should reach Failed"
    );
}

#[tokio::test]
async fn spawn_running_then_abort() {
    let sup = RunSupervisor::new();
    // A genuinely long future — we abort before it completes, so no real
    // 30s wait occurs in the test.
    let id = sup
        .spawn("long-sleep".into(), async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(())
        })
        .await;

    // Immediately it should be Running.
    assert_eq!(sup.status(&id).await, Some(RunStatus::Running));

    // Abort it.
    let aborted = sup.abort(&id).await;
    assert!(aborted, "abort should return true for existing id");
    assert_eq!(sup.status(&id).await, Some(RunStatus::Aborted));
}

#[tokio::test]
async fn run_id_monotonic_unique() {
    let sup = RunSupervisor::new();
    let a = sup.spawn("a".into(), async { Ok(()) }).await;
    let b = sup.spawn("b".into(), async { Ok(()) }).await;
    let c = sup.spawn("c".into(), async { Ok(()) }).await;

    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
    // Monotonic: a < b < c.
    assert!(a < b);
    assert!(b < c);
}

#[tokio::test]
async fn abort_nonexistent_returns_false() {
    let sup = RunSupervisor::new();
    let bogus = RunId(999_999);
    assert!(!sup.abort(&bogus).await);
}

#[tokio::test]
async fn list_snapshot_reflects_state() {
    let sup = RunSupervisor::new();
    let ok = sup.spawn("ok".into(), async { Ok(()) }).await;
    let err = sup
        .spawn("err".into(), async { Err(anyhow::anyhow!("x")) })
        .await;
    let long = sup
        .spawn("long".into(), async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(())
        })
        .await;

    // Wait for ok + err to settle.
    assert!(wait_for_status(&sup, &ok, RunStatus::Completed, Duration::from_secs(2)).await);
    assert!(wait_for_status(&sup, &err, RunStatus::Failed, Duration::from_secs(2)).await);

    // Abort the long one.
    assert!(sup.abort(&long).await);

    let listed = sup.list().await;
    assert_eq!(listed.len(), 3);

    let by_id: HashMap<RunId, RunStatus> = listed.into_iter().collect();
    assert_eq!(by_id[&ok], RunStatus::Completed);
    assert_eq!(by_id[&err], RunStatus::Failed);
    assert_eq!(by_id[&long], RunStatus::Aborted);
}

// ── Increment 2: broadcast event channel ───────────────────────────

#[tokio::test]
async fn attach_nonexistent_returns_none() {
    let sup = RunSupervisor::new();
    let bogus = RunId(999_999);
    assert!(sup.attach(&bogus).await.is_none());
}

#[tokio::test]
async fn attach_returns_live_subscriber() {
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("trivial".into(), async {
            // Keep the run alive so its terminal event (broadcast when
            // the run settles) cannot race ahead of the emitted event
            // into the subscriber's queue.
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        })
        .await;

    let sub = sup.attach(&id).await;
    assert!(
        sub.is_some(),
        "attach should return Some for an existing run"
    );

    // The subscriber should be a broadcast::Receiver.
    let mut rx = sub.unwrap();

    // Emit an event and verify we receive it.
    let event = AgentEvent::Status {
        message: "hello".into(),
    };
    assert!(sup.emit_event(&id, event.clone()).await);

    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("recv should not time out")
        .expect("recv should succeed");

    assert_eq!(
        received,
        AgentEvent::Status {
            message: "hello".into()
        }
    );

    // Clean up.
    sup.abort(&id).await;
}

#[tokio::test]
async fn attach_fans_out_to_multiple_subscribers() {
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("fan-out-test".into(), async {
            // Keep the run alive long enough for the test to emit + recv.
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        })
        .await;

    // Two independent subscribers on the same run.
    let mut rx1 = sup.attach(&id).await.expect("attach rx1");
    let mut rx2 = sup.attach(&id).await.expect("attach rx2");

    // Emit a single event — BOTH subscribers must receive it.
    let event = AgentEvent::Started;
    assert!(sup.emit_event(&id, event.clone()).await);

    let r1 = tokio::time::timeout(Duration::from_secs(2), rx1.recv())
        .await
        .expect("rx1 should not time out")
        .expect("rx1 recv should succeed");
    let r2 = tokio::time::timeout(Duration::from_secs(2), rx2.recv())
        .await
        .expect("rx2 should not time out")
        .expect("rx2 recv should succeed");

    assert_eq!(r1, AgentEvent::Started);
    assert_eq!(r2, AgentEvent::Started);

    // Clean up.
    sup.abort(&id).await;
}

#[tokio::test]
async fn emit_event_nonexistent_returns_false() {
    let sup = RunSupervisor::new();
    let bogus = RunId(42);
    assert!(
        !sup.emit_event(&bogus, AgentEvent::Started).await,
        "emit_event on unknown run should return false"
    );
}

// ── P0-3 regression: wait_for_terminal ───────────────────────────
//
// The old CLI loop checked the status only before each blocking
// `recv().await`; the broadcast sender lives in the RunHandle and is
// never closed, so once the agent stopped emitting events the CLI hung
// in `recv()` forever, never printed the final status, and left the
// registry record at "Running". These tests pin the fixed behavior:
// the wait ALWAYS ends once the run settles, with the real terminal
// status — even when the run emitted no events at all.

#[tokio::test]
async fn wait_for_terminal_exits_when_silent_run_completes() {
    // A run that emits NO events and then completes must not leave the
    // waiter blocked in recv() forever (the exact P0-3 hang).
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("silent-ok".into(), async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(())
        })
        .await;
    let rx = sup.attach(&id).await.expect("attach");

    let final_status = tokio::time::timeout(
        Duration::from_secs(5),
        sup.wait_for_terminal(&id, rx, |_| {}),
    )
    .await
    .expect("wait_for_terminal must not hang after the run ends")
    .expect("run id must exist");

    assert_eq!(final_status, RunStatus::Completed);
}

#[tokio::test]
async fn wait_for_terminal_reports_failed_run() {
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("silent-err".into(), async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Err(anyhow::anyhow!("boom"))
        })
        .await;
    let rx = sup.attach(&id).await.expect("attach");

    let final_status = tokio::time::timeout(
        Duration::from_secs(5),
        sup.wait_for_terminal(&id, rx, |_| {}),
    )
    .await
    .expect("wait_for_terminal must not hang after the run ends")
    .expect("run id must exist");

    assert_eq!(final_status, RunStatus::Failed);
}

#[tokio::test]
async fn wait_for_terminal_exits_on_abort() {
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("long".into(), async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(())
        })
        .await;
    let rx = sup.attach(&id).await.expect("attach");

    assert!(sup.abort(&id).await);

    let final_status = tokio::time::timeout(
        Duration::from_secs(5),
        sup.wait_for_terminal(&id, rx, |_| {}),
    )
    .await
    .expect("wait_for_terminal must not hang after abort")
    .expect("run id must exist");

    assert_eq!(final_status, RunStatus::Aborted);
}

#[tokio::test]
async fn wait_for_terminal_drains_events_and_sees_terminal_event() {
    // The wait loop must forward run events to the callback AND observe
    // the supervisor's own terminal event before returning the final
    // status — this is what lets the CLI print the final status line.
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("chatty".into(), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        })
        .await;
    let rx = sup.attach(&id).await.expect("attach");
    assert!(sup.emit_event(&id, AgentEvent::Started).await);

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_cb = std::sync::Arc::clone(&seen);
    let final_status = tokio::time::timeout(
        Duration::from_secs(5),
        sup.wait_for_terminal(&id, rx, move |ev| seen_cb.lock().unwrap().push(ev)),
    )
    .await
    .expect("wait_for_terminal must not hang")
    .expect("run id must exist");

    assert_eq!(final_status, RunStatus::Completed);
    let seen = seen.lock().unwrap();
    assert!(
        seen.contains(&AgentEvent::Started),
        "run events must be forwarded to the callback: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|ev| matches!(ev, AgentEvent::Completed { .. })),
        "the supervisor's terminal event must reach waiters: {seen:?}"
    );
}

#[tokio::test]
async fn wait_for_terminal_unknown_run_returns_none() {
    let sup = RunSupervisor::new();
    // A live receiver from a real run, but we query a bogus id.
    let id = sup.spawn("x".into(), async { Ok(()) }).await;
    let rx = sup.attach(&id).await.expect("attach");
    let bogus = RunId(999_999);
    assert_eq!(sup.wait_for_terminal(&bogus, rx, |_| {}).await, None);
}

#[tokio::test]
async fn terminal_event_is_broadcast_after_final_status() {
    // Ordering guarantee: a subscriber that wakes on the terminal event
    // must already read the terminal status.
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("ordered".into(), async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(())
        })
        .await;
    let mut rx = sup.attach(&id).await.expect("attach");

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Ok(AgentEvent::Completed { .. }) => break,
                Ok(_) => continue,
                Err(e) => panic!("channel error before terminal event: {e}"),
            }
        }
    })
    .await
    .expect("terminal event must be broadcast");

    assert_eq!(sup.status(&id).await, Some(RunStatus::Completed));
}

#[tokio::test]
async fn abort_broadcasts_terminal_event_exactly_once() {
    let sup = RunSupervisor::new();
    let id = sup
        .spawn("abort-ev".into(), async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(())
        })
        .await;
    let mut rx = sup.attach(&id).await.expect("attach");

    assert!(sup.abort(&id).await);
    // A second abort must NOT emit another terminal event.
    assert!(sup.abort(&id).await);

    let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("terminal event expected")
        .expect("recv ok");
    assert!(
        matches!(ev, AgentEvent::Status { ref message } if message.contains("Aborted")),
        "abort terminal event: {ev:?}"
    );
    // No second terminal event is queued.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .is_err(),
        "abort must emit exactly one terminal event"
    );
}

#[tokio::test]
async fn abort_preserves_completed_and_failed_statuses() {
    let sup = RunSupervisor::new();
    for expected in [RunStatus::Completed, RunStatus::Failed] {
        let id = sup
            .spawn("settled".into(), async move {
                if expected == RunStatus::Failed {
                    anyhow::bail!("original failure");
                }
                Ok(())
            })
            .await;
        assert!(wait_for_status(&sup, &id, expected, Duration::from_secs(2)).await);
        let mut rx = sup.attach(&id).await.unwrap();
        assert!(sup.abort(&id).await);
        assert_eq!(sup.status(&id).await, Some(expected));
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }
}

#[tokio::test]
async fn agent_terminal_event_is_settled_once_with_original_message() {
    let sup = RunSupervisor::new();
    for failed in [false, true] {
        let emitter = Arc::new(SupervisedEmitter::new());
        let mut agent =
            crate::agent::Agent::new(crate::test_support::mock_agent_config("http://127.0.0.1:1"))
                .await
                .unwrap()
                .with_event_emitter(emitter.clone());
        let expected = if failed {
            AgentEvent::Error {
                message: "specific failure".into(),
            }
        } else {
            AgentEvent::Completed {
                message: "review findings".into(),
            }
        };
        let terminal = expected.clone();
        let (release, ready) = tokio::sync::oneshot::channel();
        let id = sup
            .spawn_with_events(
                "agent terminal".into(),
                emitter,
                Arc::new(AtomicBool::new(false)),
                false,
                async move {
                    ready.await?;
                    agent.emit_terminal_event_once(terminal);
                    if failed {
                        anyhow::bail!("generic fallback");
                    }
                    Ok(())
                },
            )
            .await;
        let mut rx = sup.attach(&id).await.unwrap();
        release.send(()).unwrap();
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event, expected);
        assert_eq!(
            sup.status(&id).await,
            Some(if failed {
                RunStatus::Failed
            } else {
                RunStatus::Completed
            })
        );
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }
}

#[tokio::test]
async fn wait_for_terminal_drains_events_already_queued_at_settlement() {
    let sup = RunSupervisor::new();
    let (release, ready) = tokio::sync::oneshot::channel();
    let id = sup
        .spawn("queued completion".into(), async {
            ready.await?;
            Ok(())
        })
        .await;
    let rx = sup.attach(&id).await.unwrap();
    release.send(()).unwrap();
    assert!(wait_for_status(&sup, &id, RunStatus::Completed, Duration::from_secs(2)).await);
    let mut events = Vec::new();
    assert_eq!(
        sup.wait_for_terminal(&id, rx, |event| events.push(event))
            .await,
        Some(RunStatus::Completed)
    );
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], AgentEvent::Completed { .. }));
}

#[tokio::test]
async fn start_abort_persists_checkpoint_before_terminal_event() {
    let workspace = tempfile::tempdir().unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(workspace.path());
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_latency(30_000)
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    config.continuous_work.enabled = true;
    config.continuous_work.checkpoint_interval_tools = 1000;
    config.continuous_work.checkpoint_interval_secs = 3600;
    let task = format!(
        "Inspect this project without changing files. Cancellation test {}",
        uuid::Uuid::new_v4()
    );
    let sup = RunSupervisor::new();
    let id = sup.start(task.clone(), config).await;
    let mut rx = sup.attach(&id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while server.captured_request_bodies().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("agent must reach the provider request");
    assert!(sup.abort(&id).await);
    assert_eq!(sup.status(&id).await, Some(RunStatus::Aborted));
    let manager = crate::checkpoint::CheckpointManager::default_path().unwrap();
    let saved = manager
        .list_tasks()
        .unwrap()
        .into_iter()
        .find(|checkpoint| checkpoint.task_description == task)
        .expect("abort must save this run's checkpoint");
    let checkpoint = manager.load(&saved.task_id).unwrap();
    manager.delete(&saved.task_id).unwrap();
    assert_eq!(checkpoint.task_description, task);
    assert!(
        !checkpoint.messages.is_empty(),
        "checkpoint must retain conversation state"
    );
    let mut terminal_count = 0;
    while let Ok(event) = rx.try_recv() {
        if matches!(&event, AgentEvent::Status { message } if message.contains("Aborted")) {
            terminal_count += 1;
        }
        assert!(
            !matches!(event, AgentEvent::Completed { .. }),
            "cancelled run cannot complete"
        );
    }
    assert_eq!(terminal_count, 1);
    server.stop().await;
}
