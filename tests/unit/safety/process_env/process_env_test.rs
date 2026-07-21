use super::*;

/// A command sanitized then given a specific var exposes only the allowlist
/// plus that var — not an inherited secret.
#[tokio::test]
async fn sanitized_env_drops_inherited_secrets() {
    // SAFETY: single-threaded test setting a process-local env var.
    std::env::set_var("SELFWARE_TEST_SECRET_ENVCLEAR", "leak-me");

    let mut cmd = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "/bin/sh" });
    sanitize_command_env(&mut cmd);
    cmd.env("ALLOWED_VAR", "ok");

    // Print the environment the child would actually see.
    #[cfg(not(windows))]
    {
        cmd.arg("-c").arg("env");
        let out = cmd.output().await.expect("spawn env");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.contains("SELFWARE_TEST_SECRET_ENVCLEAR"),
            "inherited secret must not reach the child; saw:\n{stdout}"
        );
        assert!(
            stdout.contains("ALLOWED_VAR=ok"),
            "explicitly-set var must survive the clear; saw:\n{stdout}"
        );
    }

    std::env::remove_var("SELFWARE_TEST_SECRET_ENVCLEAR");
}
