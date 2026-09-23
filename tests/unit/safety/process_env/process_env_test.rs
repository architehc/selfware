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

// --- Secondary spawn sites (git/rg/docker/cargo helpers) --------------------
//
// A sentinel set on the Command BEFORE sanitizing stands in for an inherited
// host secret: `env_clear` drops explicit and inherited vars alike, so this
// exercises the same removal without mutating the process environment.

const SENTINEL: &str = "SELFWARE_TEST_SENTINEL_SECRET";

/// `std::process::Command` spawn sites (`sanitize_std_command_env`).
#[test]
#[cfg(not(windows))]
fn std_sanitize_drops_sentinel_secret() {
    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.env(SENTINEL, "leak-me");
    sanitize_std_command_env(&mut cmd);
    let out = cmd.args(["-c", "env"]).output().expect("spawn env");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains(SENTINEL), "sentinel leaked:\n{stdout}");
    assert!(stdout.contains("PATH="), "allowlist PATH must survive");
}

/// Builder-chain spawn sites (`Command::new(..).sanitized_env()...`), std.
#[test]
#[cfg(not(windows))]
fn std_chain_sanitized_env_drops_sentinel_and_keeps_later_env() {
    let out = std::process::Command::new("/bin/sh")
        .env(SENTINEL, "leak-me")
        .sanitized_env()
        .env("GIT_INDEX_FILE", "/tmp/idx")
        .args(["-c", "env"])
        .output()
        .expect("spawn env");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains(SENTINEL), "sentinel leaked:\n{stdout}");
    assert!(
        stdout.contains("GIT_INDEX_FILE=/tmp/idx"),
        "vars set after sanitizing must survive:\n{stdout}"
    );
}

/// Builder-chain spawn sites, tokio.
#[tokio::test]
#[cfg(not(windows))]
async fn tokio_chain_sanitized_env_drops_sentinel() {
    let out = tokio::process::Command::new("/bin/sh")
        .env(SENTINEL, "leak-me")
        .env("RIPGREP_CONFIG_PATH", "/attacker/rgrc")
        .sanitized_env()
        .args(["-c", "env"])
        .output()
        .await
        .expect("spawn env");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains(SENTINEL), "sentinel leaked:\n{stdout}");
    assert!(
        !stdout.contains("RIPGREP_CONFIG_PATH"),
        "rg config path must not be forwarded:\n{stdout}"
    );
}

/// Container runtime spawn sites keep the daemon-location vars docker/podman
/// need, and nothing credential-bearing.
#[tokio::test]
#[cfg(not(windows))]
async fn container_keep_list_forwards_daemon_location_not_secrets() {
    let mut cmd = tokio::process::Command::new("/bin/sh");
    sanitize_command_env_from(
        &mut cmd,
        CONTAINER_RUNTIME_ENV,
        parent_env_with(&[
            ("DOCKER_HOST", "unix:///run/user/1000/docker.sock"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("AWS_SECRET_ACCESS_KEY", "aws-secret-sentinel"),
            ("GITHUB_TOKEN", "ghp_sentinel"),
            (SENTINEL, "leak-me"),
        ]),
    );
    let stdout = child_env(cmd).await;
    assert!(stdout.contains("DOCKER_HOST=unix:///run/user/1000/docker.sock"));
    assert!(stdout.contains("XDG_RUNTIME_DIR=/run/user/1000"));
    for secret in ["aws-secret-sentinel", "ghp_sentinel", SENTINEL] {
        assert!(!stdout.contains(secret), "{secret} leaked:\n{stdout}");
    }
    for name in CONTAINER_RUNTIME_ENV {
        let lower = name.to_ascii_lowercase();
        assert!(
            !["token", "secret", "password", "key"]
                .iter()
                .any(|s| lower.contains(s)),
            "container keep-list must not name credential vars: {name}"
        );
    }
}

/// Rule 5 guard: every `Command::new(` in non-test `src/` code must be
/// sanitized (a `sanitize*`/`sanitized_env*`/`env_clear` call within the
/// following lines), except the files listed here — each with the number of
/// intentionally unsanitized spawns and why. Counts may only go DOWN.
#[test]
fn no_new_unsanitized_spawns_in_src() {
    const WINDOW: usize = 30;
    // (file, max unsanitized spawns, reason)
    const ALLOWED: &[(&str, usize, &str)] = &[
        (
            "src/agent/interactive/mod.rs",
            1,
            "user `!cmd` shell escape",
        ),
        ("src/agent/mod.rs", 1, "user `!cmd` shell passthrough"),
        (
            "src/bench_harness/long_running/project.rs",
            10,
            "operator bench fixture git",
        ),
        (
            "src/bench_harness/long_running/runner.rs",
            4,
            "operator bench: selfware self-exec needs API keys",
        ),
        (
            "src/bench_harness/swebench_pro/dataset.rs",
            1,
            "operator bench: dataset loader needs HF creds",
        ),
        (
            "src/bench_harness/swebench_pro/harness.rs",
            11,
            "operator bench: clones need credential helpers",
        ),
        (
            "src/bench_harness/swebench_pro/runner.rs",
            2,
            "operator bench python evaluator",
        ),
        (
            "src/boot/model.rs",
            2,
            "operator-launched local model server (GPU env)",
        ),
        (
            "src/config/unpack.rs",
            4,
            "operator model setup (OLLAMA_HOST/OLLAMA_MODELS)",
        ),
        (
            "src/doctor.rs",
            1,
            "`selfware doctor` probes the user's real toolchain env",
        ),
        (
            "src/evolve/apply.rs",
            1,
            "selfware self-exec needs LLM credentials",
        ),
        ("src/input/mod.rs", 1, "user's own $EDITOR"),
        (
            "src/util.rs",
            2,
            "clipboard helper needs the display session env",
        ),
    ];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut stack = vec![root.join("src")];
    let mut violations = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read source");
            let lines: Vec<&str> = text.lines().collect();
            let test_start = lines
                .windows(2)
                .position(|w| w[0].trim() == "#[cfg(test)]" && w[1].contains("mod "))
                .unwrap_or(lines.len());
            let mut unsanitized = 0usize;
            for (i, line) in lines.iter().enumerate().take(test_start) {
                if line.trim_start().starts_with("//") || !line.contains("Command::new(") {
                    continue;
                }
                let end = (i + WINDOW).min(lines.len());
                let sanitized = lines[i..end]
                    .iter()
                    .any(|l| l.contains("sanitize") || l.contains("env_clear"));
                if !sanitized {
                    unsanitized += 1;
                }
            }
            if unsanitized == 0 {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let allowed = ALLOWED
                .iter()
                .find(|(f, _, _)| *f == rel)
                .map_or(0, |(_, n, _)| *n);
            if unsanitized > allowed {
                violations.push(format!(
                    "{rel}: {unsanitized} unsanitized (allowed {allowed})"
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "spawns inheriting the full host env (route them through \
         crate::safety::process_env):\n{}",
        violations.join("\n")
    );
}
