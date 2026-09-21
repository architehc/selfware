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

/// `sanitize_command_env_preserve` re-adds the named session vars from the
/// parent env while still dropping inherited secrets.
#[tokio::test]
async fn sanitized_env_preserve_keeps_named_vars() {
    // SAFETY: single-threaded test setting process-local env vars.
    std::env::set_var("SELFWARE_TEST_SECRET_ENVCLEAR", "leak-me");
    std::env::set_var("SELFWARE_TEST_PRESERVE_DISPLAY", ":99");

    let mut cmd = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "/bin/sh" });
    sanitize_command_env_preserve(&mut cmd, &["SELFWARE_TEST_PRESERVE_DISPLAY"]);

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
            stdout.contains("SELFWARE_TEST_PRESERVE_DISPLAY=:99"),
            "preserved var must reach the child; saw:\n{stdout}"
        );
    }

    std::env::remove_var("SELFWARE_TEST_SECRET_ENVCLEAR");
    std::env::remove_var("SELFWARE_TEST_PRESERVE_DISPLAY");
}

/// The expanded keep-list — git identity, proxy/TLS trust, temp locations,
/// terminal type, Rust toolchain homes — passes through to the child when
/// present in the parent env, alongside the base PATH/HOME/LANG.
#[tokio::test]
async fn sanitized_env_keeps_toolchain_and_identity_vars() {
    // SAFETY: single-threaded test setting process-local env vars. The real
    // keep-list names are used on purpose (the keep-list resolves values from
    // the parent env); each key is restored to its prior value before the
    // child is awaited so concurrently-running tests never observe them.
    let keep_vars = [
        ("GIT_AUTHOR_NAME", "Ada Lovelace"),
        ("GIT_AUTHOR_EMAIL", "ada@example.test"),
        ("GIT_COMMITTER_NAME", "Ada Lovelace"),
        ("GIT_COMMITTER_EMAIL", "ada@example.test"),
        ("HTTP_PROXY", "http://proxy.example.test:3128"),
        ("HTTPS_PROXY", "http://proxy.example.test:3128"),
        ("NO_PROXY", "internal.example.test,localhost"),
        ("SSL_CERT_FILE", "/etc/ssl/certs/ca-bundle.crt"),
        ("SSL_CERT_DIR", "/etc/ssl/certs"),
        ("TMPDIR", "/scratch/tmp"),
        ("TEMP", "/scratch/tmp"),
        ("TMP", "/scratch/tmp"),
        ("TERM", "xterm-256color"),
        ("CARGO_HOME", "/opt/cargo"),
        ("RUSTUP_HOME", "/opt/rustup"),
    ];
    let previous: Vec<(_, _)> = keep_vars
        .iter()
        .map(|(key, _)| (*key, std::env::var_os(*key)))
        .collect();
    std::env::set_var("SELFWARE_TEST_SECRET_ENVCLEAR", "leak-me");
    for (key, value) in keep_vars {
        std::env::set_var(key, value);
    }

    let mut cmd = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "/bin/sh" });
    sanitize_command_env(&mut cmd);

    // Values are now captured in the Command; restore the parent env before
    // the (non-awaiting) child spawn so sibling tests see their own env.
    for (key, old) in previous {
        match old {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    std::env::remove_var("SELFWARE_TEST_SECRET_ENVCLEAR");

    #[cfg(not(windows))]
    {
        cmd.arg("-c").arg("env");
        let out = cmd.output().await.expect("spawn env");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.contains("SELFWARE_TEST_SECRET_ENVCLEAR"),
            "inherited secret must not reach the child; saw:\n{stdout}"
        );
        for (key, value) in keep_vars {
            assert!(
                stdout.contains(&format!("{key}={value}")),
                "keep-listed var {key} must reach the child; saw:\n{stdout}"
            );
        }
    }
}

/// `SELFWARE_API_KEY` and other credential-bearing names (`AWS_*`,
/// `GITHUB_TOKEN`) must still be stripped even though the keep-list grew.
#[tokio::test]
async fn sanitized_env_still_strips_credentials() {
    // SAFETY: single-threaded test setting process-local env vars; each key
    // is restored to its prior value before the child is awaited.
    let secrets = [
        ("SELFWARE_API_KEY", "sk-selfware-topsecret"),
        ("AWS_ACCESS_KEY_ID", "AKIATESTTESTTEST"),
        ("AWS_SECRET_ACCESS_KEY", "sup3r/s3cret=="),
        ("GITHUB_TOKEN", "ghp_topsecret123"),
    ];
    let previous: Vec<(_, _)> = secrets
        .iter()
        .map(|(key, _)| (*key, std::env::var_os(*key)))
        .collect();
    for (key, value) in secrets {
        std::env::set_var(key, value);
    }

    let mut cmd = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "/bin/sh" });
    sanitize_command_env(&mut cmd);

    for (key, old) in previous {
        match old {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[cfg(not(windows))]
    {
        cmd.arg("-c").arg("env");
        let out = cmd.output().await.expect("spawn env");
        let stdout = String::from_utf8_lossy(&out.stdout);
        for (key, value) in secrets {
            assert!(
                !stdout.contains(value),
                "credential var {key} leaked to the child; saw:\n{stdout}"
            );
        }
        assert!(
            stdout.contains("PATH="),
            "the base allowlist must still reach the child; saw:\n{stdout}"
        );
    }
}

/// Proxy URL `user:password@` userinfo must NOT reach a sanitized child: only
/// the host:port form is forwarded by default.
#[tokio::test]
async fn sanitized_env_strips_proxy_userinfo_by_default() {
    // SAFETY: single-threaded test setting process-local env vars; each key
    // is restored to its prior value before the child is awaited.
    let proxy_vars = [
        (
            "HTTP_PROXY",
            "http://proxyuser-abc123:proxypass-xyz789@proxy.example.test:3128",
        ),
        (
            "HTTPS_PROXY",
            "https://proxyuser-abc123:proxypass-xyz789@proxy.example.test:3128",
        ),
    ];
    let previous: Vec<(_, _)> = proxy_vars
        .iter()
        .map(|(key, _)| (*key, std::env::var_os(*key)))
        .collect();
    let previous_opt_in = std::env::var_os("SELFWARE_FORWARD_PROXY_CREDENTIALS");
    std::env::remove_var("SELFWARE_FORWARD_PROXY_CREDENTIALS");
    for (key, value) in proxy_vars {
        std::env::set_var(key, value);
    }

    let mut cmd = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "/bin/sh" });
    sanitize_command_env(&mut cmd);

    for (key, old) in previous {
        match old {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    match previous_opt_in {
        Some(v) => std::env::set_var("SELFWARE_FORWARD_PROXY_CREDENTIALS", v),
        None => std::env::remove_var("SELFWARE_FORWARD_PROXY_CREDENTIALS"),
    }

    #[cfg(not(windows))]
    {
        cmd.arg("-c").arg("env");
        let out = cmd.output().await.expect("spawn env");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.contains("proxypass-xyz789"),
            "proxy password must not reach the child; saw:\n{stdout}"
        );
        assert!(
            !stdout.contains("proxyuser-abc123@"),
            "proxy userinfo must not reach the child; saw:\n{stdout}"
        );
        assert!(
            stdout.contains("HTTP_PROXY=http://proxy.example.test:3128"),
            "proxy host:port must still reach the child; saw:\n{stdout}"
        );
        assert!(
            stdout.contains("HTTPS_PROXY=https://proxy.example.test:3128"),
            "proxy host:port must still reach the child; saw:\n{stdout}"
        );
    }
}

/// With `SELFWARE_FORWARD_PROXY_CREDENTIALS=1`, proxy URL userinfo is
/// forwarded verbatim (deployments whose proxy requires authentication).
#[tokio::test]
async fn sanitized_env_forwards_proxy_credentials_when_opted_in() {
    // SAFETY: single-threaded test setting process-local env vars; each key
    // is restored to its prior value before the child is awaited.
    let previous_http = std::env::var_os("HTTP_PROXY");
    let previous_opt_in = std::env::var_os("SELFWARE_FORWARD_PROXY_CREDENTIALS");
    std::env::set_var(
        "HTTP_PROXY",
        "http://proxyuser-abc123:proxypass-xyz789@proxy.example.test:3128",
    );
    std::env::set_var("SELFWARE_FORWARD_PROXY_CREDENTIALS", "1");

    let mut cmd = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "/bin/sh" });
    sanitize_command_env(&mut cmd);

    match previous_http {
        Some(v) => std::env::set_var("HTTP_PROXY", v),
        None => std::env::remove_var("HTTP_PROXY"),
    }
    match previous_opt_in {
        Some(v) => std::env::set_var("SELFWARE_FORWARD_PROXY_CREDENTIALS", v),
        None => std::env::remove_var("SELFWARE_FORWARD_PROXY_CREDENTIALS"),
    }

    #[cfg(not(windows))]
    {
        cmd.arg("-c").arg("env");
        let out = cmd.output().await.expect("spawn env");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(
                "HTTP_PROXY=http://proxyuser-abc123:proxypass-xyz789@proxy.example.test:3128"
            ),
            "opt-in must forward proxy credentials verbatim; saw:\n{stdout}"
        );
    }
}
