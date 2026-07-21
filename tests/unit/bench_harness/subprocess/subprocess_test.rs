use super::*;

#[test]
fn env_allowed_passes_selfware_and_toolchain_but_not_random_secrets() {
    assert!(env_allowed("SELFWARE_ENDPOINT"));
    assert!(env_allowed("LC_ALL"));
    assert!(env_allowed("PATH"));
    assert!(env_allowed("CARGO_HOME"));
    assert!(env_allowed("OPENROUTER_API_KEY"));
    assert!(!env_allowed("AWS_SECRET_ACCESS_KEY"));
    assert!(!env_allowed("GITHUB_TOKEN"));
    assert!(!env_allowed("HOME_ADDRESS"));
}

#[cfg(unix)]
#[test]
fn run_captures_stdout_and_exit_code() {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg("printf hello; exit 3");
    let out = run_with_group_timeout(cmd, Duration::from_secs(10)).unwrap();
    assert_eq!(out.stdout, "hello");
    assert_eq!(out.exit_code, 3);
    assert!(!out.timed_out);
}

#[cfg(unix)]
#[test]
fn run_times_out_and_kills_the_group() {
    // A child that spawns a grandchild and both sleep; on timeout the whole
    // group must die. We assert timed_out and that the call returns promptly
    // (≈ timeout + 500ms grace), not after the 30s sleep.
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg("sleep 30 & sleep 30");
    let started = Instant::now();
    let out = run_with_group_timeout(cmd, Duration::from_millis(300)).unwrap();
    assert!(out.timed_out, "must report timeout");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "must return shortly after timeout, not wait for the 30s sleep (took {:?})",
        started.elapsed()
    );
}
