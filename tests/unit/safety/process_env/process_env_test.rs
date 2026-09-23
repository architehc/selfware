use super::*;

// Hermeticity: these tests NEVER mutate the process environment. They feed a
// synthetic parent env to `sanitize_command_env_from` (the same keep-list
// code the production wrappers run against the live env). Setting
// `HTTP_PROXY`, `TMPDIR`, `CARGO_HOME`, … in the process env during a
// parallel lib run leaked a fake proxy ("dns error" in every concurrently
// built reqwest client) and a nonexistent temp dir (`tempdir()` failures
// under /scratch/tmp) into unrelated tests. (On Windows these tests never
// asserted anything; they are now unix-only instead of mutating the env.)

/// Minimal synthetic parent env: a PATH so `sh -c env` can find `env`.
#[cfg(not(windows))]
fn parent_env_with(extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut env = vec![("PATH".to_string(), "/usr/bin:/bin".to_string())];
    env.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    env
}

/// Run `env` in the sanitized child and return its stdout.
#[cfg(not(windows))]
async fn child_env(mut cmd: tokio::process::Command) -> String {
    cmd.arg("-c").arg("env");
    let out = cmd.output().await.expect("spawn env");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A command sanitized then given a specific var exposes only the allowlist
/// plus that var — not an inherited secret.
#[tokio::test]
#[cfg(not(windows))]
async fn sanitized_env_drops_inherited_secrets() {
    let mut cmd = tokio::process::Command::new("/bin/sh");
    sanitize_command_env_from(
        &mut cmd,
        &[],
        parent_env_with(&[("SELFWARE_TEST_SECRET_ENVCLEAR", "leak-me")]),
    );
    cmd.env("ALLOWED_VAR", "ok");

    let stdout = child_env(cmd).await;
    assert!(
        !stdout.contains("SELFWARE_TEST_SECRET_ENVCLEAR"),
        "inherited secret must not reach the child; saw:\n{stdout}"
    );
    assert!(
        stdout.contains("ALLOWED_VAR=ok"),
        "explicitly-set var must survive the clear; saw:\n{stdout}"
    );
}

/// The production wrapper (live process env) clears too: a variable cargo
/// sets for the test process (`CARGO_PKG_NAME`, not on the keep-list) must
/// not reach the child, while PATH does. Reads the env; never mutates it.
#[tokio::test]
#[cfg(not(windows))]
async fn sanitize_command_env_live_wrapper_clears_non_kept_vars() {
    let mut cmd = tokio::process::Command::new("/bin/sh");
    sanitize_command_env(&mut cmd);
    let stdout = child_env(cmd).await;
    if std::env::var_os("CARGO_PKG_NAME").is_some() {
        assert!(
            !stdout.contains("CARGO_PKG_NAME="),
            "non-kept parent var must not reach the child; saw:\n{stdout}"
        );
    }
    if std::env::var_os("PATH").is_some() {
        assert!(
            stdout.contains("PATH="),
            "the base allowlist must still reach the child; saw:\n{stdout}"
        );
    }
}

/// `sanitize_command_env_preserve` semantics: named session vars are re-added
/// from the parent env while inherited secrets are still dropped.
#[tokio::test]
#[cfg(not(windows))]
async fn sanitized_env_preserve_keeps_named_vars() {
    let mut cmd = tokio::process::Command::new("/bin/sh");
    sanitize_command_env_from(
        &mut cmd,
        &["SELFWARE_TEST_PRESERVE_DISPLAY"],
        parent_env_with(&[
            ("SELFWARE_TEST_SECRET_ENVCLEAR", "leak-me"),
            ("SELFWARE_TEST_PRESERVE_DISPLAY", ":99"),
        ]),
    );

    let stdout = child_env(cmd).await;
    assert!(
        !stdout.contains("SELFWARE_TEST_SECRET_ENVCLEAR"),
        "inherited secret must not reach the child; saw:\n{stdout}"
    );
    assert!(
        stdout.contains("SELFWARE_TEST_PRESERVE_DISPLAY=:99"),
        "preserved var must reach the child; saw:\n{stdout}"
    );
}

/// The expanded keep-list — git identity, proxy/TLS trust, temp locations,
/// terminal type, Rust toolchain homes — passes through to the child when
/// present in the parent env, alongside the base PATH/HOME/LANG.
#[tokio::test]
#[cfg(not(windows))]
async fn sanitized_env_keeps_toolchain_and_identity_vars() {
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
    let mut extra = keep_vars.to_vec();
    extra.push(("SELFWARE_TEST_SECRET_ENVCLEAR", "leak-me"));

    let mut cmd = tokio::process::Command::new("/bin/sh");
    sanitize_command_env_from(&mut cmd, &[], parent_env_with(&extra));

    let stdout = child_env(cmd).await;
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

/// `SELFWARE_API_KEY` and other credential-bearing names (`AWS_*`,
/// `GITHUB_TOKEN`) must still be stripped even though the keep-list grew.
#[tokio::test]
#[cfg(not(windows))]
async fn sanitized_env_still_strips_credentials() {
    let secrets = [
        ("SELFWARE_API_KEY", "sk-selfware-topsecret"),
        ("AWS_ACCESS_KEY_ID", "AKIATESTTESTTEST"),
        ("AWS_SECRET_ACCESS_KEY", "sup3r/s3cret=="),
        ("GITHUB_TOKEN", "ghp_topsecret123"),
    ];

    let mut cmd = tokio::process::Command::new("/bin/sh");
    sanitize_command_env_from(&mut cmd, &[], parent_env_with(&secrets));

    let stdout = child_env(cmd).await;
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

/// Proxy URL `user:password@` userinfo must NOT reach a sanitized child: only
/// the host:port form is forwarded by default.
#[tokio::test]
#[cfg(not(windows))]
async fn sanitized_env_strips_proxy_userinfo_by_default() {
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

    let mut cmd = tokio::process::Command::new("/bin/sh");
    // No SELFWARE_FORWARD_PROXY_CREDENTIALS in the parent env: default.
    sanitize_command_env_from(&mut cmd, &[], parent_env_with(&proxy_vars));

    let stdout = child_env(cmd).await;
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

/// With `SELFWARE_FORWARD_PROXY_CREDENTIALS=1`, proxy URL userinfo is
/// forwarded verbatim (deployments whose proxy requires authentication).
#[tokio::test]
#[cfg(not(windows))]
async fn sanitized_env_forwards_proxy_credentials_when_opted_in() {
    let mut cmd = tokio::process::Command::new("/bin/sh");
    sanitize_command_env_from(
        &mut cmd,
        &[],
        parent_env_with(&[
            (
                "HTTP_PROXY",
                "http://proxyuser-abc123:proxypass-xyz789@proxy.example.test:3128",
            ),
            ("SELFWARE_FORWARD_PROXY_CREDENTIALS", "1"),
        ]),
    );

    let stdout = child_env(cmd).await;
    assert!(
        stdout.contains(
            "HTTP_PROXY=http://proxyuser-abc123:proxypass-xyz789@proxy.example.test:3128"
        ),
        "opt-in must forward proxy credentials verbatim; saw:\n{stdout}"
    );
}
