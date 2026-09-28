use super::*;

#[test]
fn neutral_values_cover_the_exec_capable_keys() {
    for (key, want) in [
        ("core.fsmonitor", "false"),
        ("core.hookspath", NULL_DEVICE),
        ("core.pager", "cat"),
        ("pager.log", "cat"),
        ("core.sshcommand", ""),
        ("diff.external", NEUTRAL_EXTERNAL_DIFF),
        ("diff.evil.textconv", NEUTRAL_FILTER),
        ("diff.evil.command", NEUTRAL_EXTERNAL_DIFF),
        ("filter.evil.clean", NEUTRAL_FILTER),
        ("filter.evil.smudge", NEUTRAL_FILTER),
        ("filter.evil.process", ""),
        ("filter.evil.required", "false"),
        ("merge.evil.driver", ""),
        ("credential.helper", ""),
        ("credential.https://x.helper", ""),
        ("gpg.program", "gpg"),
        ("gpg.ssh.program", "ssh-keygen"),
        ("core.editor", ":"),
        ("sequence.editor", ":"),
        ("remote.origin.uploadpack", "git-upload-pack"),
        ("submodule.x.update", "checkout"),
    ] {
        assert_eq!(neutral_value(key), Some(want), "{key}");
    }
    for key in [
        "user.name",
        "core.autocrlf",
        "diff.renames",
        "filter.x",
        "remote.origin.url",
    ] {
        assert_eq!(neutral_value(key), None, "{key}");
    }
}

#[test]
fn scoped_config_output_is_parsed() {
    let raw = b"global\0user.name\nA B\0local\0core.fsmonitor\n./hook.sh\0local\0core.bare\0";
    let parsed = parse_scoped_config(raw);
    assert_eq!(parsed.len(), 3);
    assert_eq!(
        parsed[1],
        (
            "local".to_string(),
            "core.fsmonitor".to_string(),
            "./hook.sh".to_string()
        )
    );
    assert_eq!(parsed[2].2, "");
}

#[cfg(unix)]
use super::test_support::malicious_repo;

#[cfg(unix)]
fn run(mut cmd: std::process::Command) {
    let _ = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(unix)]
#[test]
fn hardened_git_runs_nothing_the_repository_configured() {
    let Some((tmp, marker)) = malicious_repo() else {
        eprintln!("git unavailable; skipping");
        return;
    };
    let dir = tmp.path();

    // Sanity: the plain spawn DOES run the repository's program, so the
    // assertions below are not vacuous.
    let mut plain = std::process::Command::new("git");
    plain.arg("status").current_dir(dir);
    run(plain);
    assert!(marker.exists(), "fixture must trigger on an unhardened git");
    std::fs::remove_file(&marker).unwrap();

    assert!(!repo_git_is_inert(dir));
    assert!(repo_exec_config(dir)
        .iter()
        .any(|(k, _)| k == "core.fsmonitor"));

    let calls: &[&[&str]] = &[
        &["status", "--porcelain"],
        &["ls-files"],
        &["diff"],
        &["diff", "HEAD"],
        &["log", "-p", "-1"],
        &["show", "HEAD"],
        &["stash"],
        &["stash", "pop"],
        &["checkout", "-q", "-b", "side"],
        &["commit", "-qam", "x"],
        &["checkout", "-q", "-"],
    ];
    for scope in [GitScope::Internal, GitScope::UserOperation] {
        for args in calls {
            let mut cmd = git_command(dir, scope);
            cmd.args(*args);
            run(cmd);
            assert!(
                !marker.exists(),
                "{scope:?} `git {}` ran a repository-configured program: {}",
                args.join(" "),
                std::fs::read_to_string(&marker).unwrap_or_default()
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn clean_repository_is_inert() {
    let tmp = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(tmp.path())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return;
    }
    assert!(repo_exec_config(tmp.path()).is_empty());
    assert!(repo_git_is_inert(tmp.path()));
    let args = hardening_args(tmp.path(), GitScope::Internal);
    assert_eq!(args[0], "--no-pager");
    assert!(args.contains(&"core.fsmonitor=false".to_string()));
    assert!(args.iter().any(|a| a.starts_with("core.hooksPath=")));
}

/// Run `script` with `sh -c` in `dir` the way a model shell runs it: the
/// sanitized environment plus [`shell_git_env`]. Returns stdout.
#[cfg(unix)]
fn shell_run(dir: &std::path::Path, script: &str) -> String {
    let mut cmd = std::process::Command::new("sh");
    crate::safety::process_env::sanitize_std_command_env(&mut cmd);
    cmd.args(["-c", script])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for (k, v) in shell_git_env(dir) {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("sh runs");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A model shell in an untrusted repository: git run directly, from a
/// script, or nested in a subshell executes nothing the repository
/// configured, and `git diff` / `git log -p` still print real diffs.
#[cfg(unix)]
#[test]
fn shell_git_env_neutralises_repository_programs_for_nested_git() {
    use std::os::unix::fs::PermissionsExt;
    let Some((tmp, marker)) = malicious_repo() else {
        eprintln!("git unavailable; skipping");
        return;
    };
    let dir = tmp.path();
    // Not vacuous: the same shell without the environment triggers.
    let mut plain = std::process::Command::new("sh");
    plain.args(["-c", "git status"]).current_dir(dir);
    run(plain);
    assert!(marker.exists(), "fixture must trigger on an unhardened git");
    std::fs::remove_file(&marker).unwrap();

    let env = shell_git_env(dir);
    let get = |key: &str| {
        let n = env
            .iter()
            .find(|(k, v)| k.starts_with("GIT_CONFIG_KEY_") && v.eq_ignore_ascii_case(key))
            .map(|(k, _)| k.trim_start_matches("GIT_CONFIG_KEY_").to_string())?;
        env.iter()
            .find(|(k, _)| *k == format!("GIT_CONFIG_VALUE_{n}"))
            .map(|(_, v)| v.clone())
    };
    assert_eq!(get("core.fsmonitor").as_deref(), Some("false"));
    assert_eq!(get("core.pager").as_deref(), Some("cat"));
    assert!(get("diff.evil.textconv").is_some(), "{env:?}");
    assert!(get("filter.evil.clean").is_some(), "{env:?}");

    let script = dir.join("gitscript.sh");
    std::fs::write(&script, "#!/bin/sh\ngit diff\ngit status --short\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    for cmd in [
        "git status",
        "git status --porcelain",
        "git diff",
        "git diff HEAD",
        "git log -p -1",
        "git show HEAD",
        "git -p diff",
        "./gitscript.sh",
        "sh -c 'cd . && git diff'",
        "git add -A && git commit -qm x",
    ] {
        let out = shell_run(dir, cmd);
        assert!(
            !marker.exists(),
            "`{cmd}` ran a repository-configured program: {}",
            std::fs::read_to_string(&marker).unwrap_or_default()
        );
        if cmd == "git diff" || cmd == "./gitscript.sh" {
            assert!(
                out.contains("+b"),
                "`{cmd}` must still print the diff: {out}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn trusted_or_clean_repositories_get_only_what_they_need() {
    let tmp = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(tmp.path())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return;
    }
    // A clean untrusted repository still gets the fixed keys (a later
    // `git clone` + `cd` is covered for fsmonitor/hooks/pager).
    let env = shell_git_env(tmp.path());
    assert_eq!(env[0], ("GIT_CONFIG_COUNT".to_string(), "4".to_string()));
    assert!(is_git_config_env("GIT_CONFIG_COUNT"));
    assert!(is_git_config_env("git_config_key_0"));
    assert!(is_git_config_env("GIT_CONFIG_PARAMETERS"));
    assert!(!is_git_config_env("GIT_CONFIG_NOSYSTEM"));
    assert!(!is_git_config_env("GIT_AUTHOR_NAME"));
}

/// The config listing is cached by the bytes of `.git/config`: a changed
/// config is re-listed, and a config with includes is never cached.
#[cfg(unix)]
#[test]
fn exec_config_cache_follows_the_config_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if !git(&["init", "-q"]) {
        return;
    }
    assert!(repo_exec_config(dir).is_empty());
    assert!(repo_exec_config(dir).is_empty());
    assert!(git(&["config", "core.fsmonitor", "./x.sh"]));
    assert_eq!(
        repo_exec_config(dir),
        vec![("core.fsmonitor".to_string(), "./x.sh".to_string())]
    );
    // An included file changes without `.git/config` changing.
    let inc = dir.join("inc.cfg");
    std::fs::write(&inc, "").unwrap();
    assert!(git(&["config", "include.path", inc.to_str().unwrap()]));
    assert_eq!(repo_exec_config(dir).len(), 1);
    std::fs::write(&inc, "[diff]\n\texternal = ./y.sh\n").unwrap();
    assert_eq!(
        repo_exec_config(dir).len(),
        2,
        "{:?}",
        repo_exec_config(dir)
    );
}
