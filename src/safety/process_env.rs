//! Environment sanitization for spawned child processes.
//!
//! The agent process holds secrets in its environment — `SELFWARE_API_KEY`,
//! and whatever the operator exported (`AWS_*`, `GITHUB_TOKEN`, …). Any child
//! process spawned with the inherited environment can read all of it, so a
//! single compromised subprocess (a hostile MCP server, an injected shell
//! command, the Playwright bridge loading attacker-controlled content) can
//! exfiltrate every credential on the box.
//!
//! `shell_exec` already clears the environment and re-adds a minimal base.
//! This helper centralizes that policy so every spawn site applies the same
//! allowlist and none silently drifts back to inheriting the full env.

/// Clear a command's inherited environment and re-populate only a minimal,
/// non-sensitive base (`PATH`, `HOME`, `LANG`).
///
/// Call this immediately after constructing the `Command` and BEFORE adding
/// any task-specific variables, so those additions survive the clear.
pub fn sanitize_command_env(cmd: &mut tokio::process::Command) {
    cmd.env_clear();
    for key in ["PATH", "HOME", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
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
}
