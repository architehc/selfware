use super::*;

fn rec(id: &str, pid: u32, status: &str, started: i64) -> RunRecord {
    RunRecord {
        id: id.to_string(),
        pid,
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
    reg.write(&rec("live-1", pid, "Running", 100)).unwrap();
    let outcome = reg.abort("live-1", 200).unwrap();
    assert_eq!(outcome, AbortOutcome::WasStale);
    assert_eq!(reg.get("live-1").unwrap().unwrap().status, "Aborted");
    // The unrelated process must be untouched (still alive) — reap it.
    let _ = child.kill();
    let _ = child.wait();
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
