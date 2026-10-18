use super::*;

fn rec(id: &str, pid: u32, status: &str, started: i64) -> RunRecord {
    RunRecord {
        id: id.to_string(),
        pid,
        process_start_time: None,
        task: format!("task {id}"),
        status: status.to_string(),
        started_unix: started,
        updated_unix: started,
    }
}

// A pid that is essentially guaranteed not to exist (positive, > pid_max).
const DEAD_PID: u32 = 2_147_483_646;

#[test]
fn write_then_get_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    let r = rec("1234-1", 1234, "Running", 100);
    reg.write(&r).unwrap();
    assert_eq!(reg.get("1234-1").unwrap(), Some(r));
    assert_eq!(reg.get("nope").unwrap(), None);
}

#[test]
fn list_is_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    reg.write(&rec("1-1", 1, "Completed", 100)).unwrap();
    reg.write(&rec("1-2", 1, "Completed", 300)).unwrap();
    reg.write(&rec("1-3", 1, "Completed", 200)).unwrap();
    let ids: Vec<String> = reg.list().unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(ids, vec!["1-2", "1-3", "1-1"]);
}

#[test]
fn update_status_changes_status() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    reg.write(&rec("1-1", 1, "Running", 100)).unwrap();
    reg.update_status("1-1", "Completed", 150).unwrap();
    let g = reg.get("1-1").unwrap().unwrap();
    assert_eq!(g.status, "Completed");
    assert_eq!(g.updated_unix, 150);
}

#[test]
fn effective_status_reports_stale_for_dead_pid() {
    let running_dead = rec("x", DEAD_PID, "Running", 100);
    assert_eq!(running_dead.effective_status(), "Stale");
    let completed_dead = rec("y", DEAD_PID, "Completed", 100);
    assert_eq!(completed_dead.effective_status(), "Completed");
}

#[test]
fn abort_unknown_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    assert_eq!(reg.abort("missing", 200).unwrap(), AbortOutcome::NotFound);
}

#[test]
fn abort_already_terminal_is_already_done() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    reg.write(&rec("1-1", 1, "Completed", 100)).unwrap();
    assert_eq!(reg.abort("1-1", 200).unwrap(), AbortOutcome::AlreadyDone);
}

#[test]
fn pid_alive_detects_running_and_dead() {
    // The cross-platform liveness probe (sysinfo) that replaced the old
    // "always true on non-Unix" stub: the test process itself is alive,
    // and a pid past pid_max is not. On the old non-Unix path both would
    // have (wrongly) reported alive.
    assert!(
        pid_alive(std::process::id()),
        "current process must be alive"
    );
    assert!(!pid_alive(DEAD_PID), "a pid past pid_max must be dead");
}

#[test]
fn abort_dead_pid_is_was_stale_and_marks_aborted() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    reg.write(&rec("1-1", DEAD_PID, "Running", 100)).unwrap();
    assert_eq!(reg.abort("1-1", 200).unwrap(), AbortOutcome::WasStale);
    assert_eq!(reg.get("1-1").unwrap().unwrap().status, "Aborted");
}

#[test]
fn abort_live_non_selfware_pid_is_not_signalled() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    // A live process whose PID matches the record but which is NOT a
    // selfware process (i.e. a reused PID). It must NOT be signalled;
    // the run is recorded Aborted regardless.
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let pid = child.id();
    // Fork/exec race: right after spawn, /proc/<pid>/exe can still point
    // at the test binary (whose path contains "selfware"), so
    // pid_is_selfware would misfire and the run would be Signalled.
    // Wait until the child has actually exec'd sleep.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while pid_is_selfware(pid) {
        assert!(
            std::time::Instant::now() < deadline,
            "child {pid} never exec'd into a non-selfware process"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let live_non_selfware = rec("live-1", pid, "Running", 100);
    assert_eq!(
        live_non_selfware.effective_status(),
        "Stale",
        "reused non-selfware PID must be reported Stale"
    );
    reg.write(&live_non_selfware).unwrap();
    let outcome = reg.abort("live-1", 200).unwrap();
    assert_eq!(outcome, AbortOutcome::WasStale);
    assert_eq!(reg.get("live-1").unwrap().unwrap().status, "Aborted");
    // The unrelated process must be untouched (still alive) — reap it.
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn mismatched_process_start_time_prevents_signalling_reused_selfware_pid() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    let self_pid = std::process::id();
    let actual_start = crate::resources::driver::process_start_time(self_pid)
        .expect("current process start time must be readable");

    let mut matching = rec("self-match", self_pid, "Running", 100);
    matching.process_start_time = Some(actual_start);
    assert_eq!(matching.effective_status(), "Running");

    // Simulate a stale RunRecord whose PID was reused by a later selfware process
    // with a different OS start time: must report Stale and must NOT signal self_pid!
    let mut reused = rec("self-reused", self_pid, "Running", 100);
    reused.process_start_time = Some(actual_start.saturating_sub(10).max(1));
    if reused.process_start_time == Some(actual_start) {
        reused.process_start_time = Some(actual_start + 10);
    }
    assert_eq!(reused.effective_status(), "Stale");

    reg.write(&reused).unwrap();
    let outcome = reg.abort("self-reused", 200).unwrap();
    assert_eq!(outcome, AbortOutcome::WasStale);
    assert_eq!(reg.get("self-reused").unwrap().unwrap().status, "Aborted");
}

#[test]
fn update_status_terminal_states_are_sticky() {
    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();
    // Running -> Completed is allowed.
    reg.write(&rec("r1", 1, "Running", 100)).unwrap();
    reg.update_status("r1", "Completed", 110).unwrap();
    assert_eq!(reg.get("r1").unwrap().unwrap().status, "Completed");
    // Aborted must NOT be overwritten by a late Completed (the P0).
    reg.write(&rec("r2", 1, "Aborted", 100)).unwrap();
    reg.update_status("r2", "Completed", 120).unwrap();
    assert_eq!(reg.get("r2").unwrap().unwrap().status, "Aborted");
    // Completed/Failed are likewise sticky.
    reg.write(&rec("r3", 1, "Completed", 100)).unwrap();
    reg.update_status("r3", "Running", 130).unwrap();
    assert_eq!(reg.get("r3").unwrap().unwrap().status, "Completed");
}

#[cfg(unix)]
#[test]
fn run_registry_rejects_symlinks_in_dir_get_list_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let target_dir = dir.path().join("real_runs");
    std::fs::create_dir_all(&target_dir).unwrap();
    let symlink_dir = dir.path().join("linked_runs");
    std::os::unix::fs::symlink(&target_dir, &symlink_dir).unwrap();

    assert!(
        RunRegistry::with_dir(symlink_dir).is_err(),
        "RunRegistry::with_dir must reject symlinked directory"
    );

    let reg = RunRegistry::with_dir(target_dir.clone()).unwrap();
    reg.write(&rec("good-1", 1, "Completed", 100)).unwrap();

    let outside = dir.path().join("outside.json");
    let fake_json = serde_json::to_string_pretty(&rec("evil-1", 1, "Running", 200)).unwrap();
    std::fs::write(&outside, fake_json).unwrap();
    let linked_rec = target_dir.join("evil-1.json");
    std::os::unix::fs::symlink(&outside, &linked_rec).unwrap();

    assert!(
        reg.get("evil-1").is_err(),
        "RunRegistry::get must reject symlinked record file"
    );
    let listed: Vec<String> = reg.list().unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(
        listed,
        vec!["good-1"],
        "RunRegistry::list must skip symlinked record files"
    );
    assert!(
        reg.remove("evil-1").is_err(),
        "RunRegistry::remove must refuse to unlink a symlinked record file"
    );
    assert!(outside.exists(), "outside target file must remain intact");
}

#[test]
fn rust_run_registry_matches_lean_formal_table() {
    let raw = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/formal/supervision_table.json"
    ));
    let rows: Vec<[String; 3]> =
        serde_json::from_str(raw).expect("valid formal/supervision_table.json");

    let dir = tempfile::tempdir().unwrap();
    let reg = RunRegistry::with_dir(dir.path().to_path_buf()).unwrap();

    let mut checked_updates = 0usize;
    let mut checked_aborts = 0usize;

    let self_pid = std::process::id();
    let self_start = crate::resources::driver::process_start_time(self_pid)
        .expect("current process start time must be readable");

    for [domain, event, expected] in &rows {
        if let Some(cur_status) = domain.strip_prefix("run_update:") {
            let id = format!("upd-{checked_updates}");
            reg.write(&rec(&id, 1, cur_status, 100)).unwrap();
            reg.update_status(&id, event, 110).unwrap();
            let after = reg.get(&id).unwrap().unwrap().status;
            assert_eq!(
                &after, expected,
                "Lean SV2 run_update mismatch for ({cur_status}, {event})"
            );
            checked_updates += 1;
        } else if let Some(scenario) = domain.strip_prefix("run_abort:") {
            // Only test `sig_ok` rows on real processes when they do NOT SIGTERM `self_pid`.
            if scenario == "running_live_owner" {
                // Verified structurally in `owner_pid_alive`: live self_pid + matching start time is true.
                assert!(owner_pid_alive(self_pid, Some(self_start)));
                checked_aborts += 1;
                continue;
            }
            let id = format!("abt-{checked_aborts}");
            let record = match scenario {
                "running_pid_reused_other_selfware" => {
                    let mut r = rec(&id, self_pid, "Running", 100);
                    r.process_start_time = Some(self_start + 999);
                    r
                }
                "running_pid_reused_non_selfware" | "running_pid_dead" => {
                    let mut r = rec(&id, 999_999_999, "Running", 100);
                    r.process_start_time = Some(1000);
                    r
                }
                "completed_already" => rec(&id, self_pid, "Completed", 100),
                "aborted_already" => rec(&id, self_pid, "Aborted", 100),
                other => panic!("unknown run_abort scenario in formal table: {other}"),
            };
            reg.write(&record).unwrap();
            let outcome = reg.abort(&id, 200).unwrap();
            let outcome_name = match outcome {
                AbortOutcome::AlreadyDone => "already_done",
                AbortOutcome::WasStale => "was_stale",
                AbortOutcome::Signalled(_) => "signalled",
                AbortOutcome::SignalFailed(_, _) => "signal_failed",
                AbortOutcome::SignalUnsupported(_) => "signal_unsupported",
                AbortOutcome::NotFound => "not_found",
            };
            let after_status = reg.get(&id).unwrap().unwrap().status;
            let actual = format!("{outcome_name}:{after_status}");
            assert_eq!(
                &actual, expected,
                "Lean SV2 run_abort mismatch for ({scenario}, {event})"
            );
            checked_aborts += 1;
        }
    }

    assert_eq!(
        checked_updates, 16,
        "all 16 SV2 run_update rows in formal/supervision_table.json must be verified"
    );
    assert_eq!(
        checked_aborts, 18,
        "all 18 SV2 run_abort rows in formal/supervision_table.json must be verified"
    );
}
