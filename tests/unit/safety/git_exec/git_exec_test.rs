use super::*;

#[test]
fn neutral_values_cover_the_exec_capable_keys() {
    for (key, want) in [
        ("core.fsmonitor", "false"),
        ("core.hookspath", NULL_DEVICE),
        ("core.pager", "cat"),
        ("pager.log", "cat"),
        ("core.sshcommand", ""),
        ("diff.external", ""),
        ("diff.evil.textconv", ""),
        ("diff.evil.command", ""),
        ("filter.evil.clean", ""),
        ("filter.evil.smudge", ""),
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

/// A repository whose own config and hooks run `hook.sh` from every
/// angle 0.9.4 left open (fsmonitor, textconv, external diff, clean/smudge
/// filters, hooks). Returns (repo dir, marker path).
#[cfg(unix)]
fn malicious_repo() -> Option<(tempfile::TempDir, std::path::PathBuf)> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let tmp = tempfile::tempdir().ok()?;
    let dir = tmp.path();
    let marker = dir.join("MARKER");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
    };
    git(&["init", "-q"])?;
    git(&["config", "user.email", "t@example.com"])?;
    git(&["config", "user.name", "t"])?;
    git(&["config", "commit.gpgsign", "false"])?;
    let hook = dir.join("hook.sh");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\necho \"$0 $*\" >> '{}'\nexit 0\n",
            marker.display()
        ),
    )
    .ok()?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).ok()?;
    std::fs::write(dir.join("f.txt"), "a\n").ok()?;
    std::fs::write(dir.join(".gitattributes"), "f.txt diff=evil filter=evil\n").ok()?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "init"])?;
    std::fs::write(dir.join("f.txt"), "b\n").ok()?;
    let hook_s = hook.to_str()?;
    let hooks_dir = dir.join("hk");
    std::fs::create_dir(&hooks_dir).ok()?;
    for name in [
        "post-checkout",
        "pre-commit",
        "post-commit",
        "reference-transaction",
    ] {
        let h = hooks_dir.join(name);
        std::fs::copy(&hook, &h).ok()?;
    }
    for (k, v) in [
        ("core.fsmonitor", hook_s),
        ("diff.external", hook_s),
        ("diff.evil.textconv", hook_s),
        ("filter.evil.clean", hook_s),
        ("filter.evil.smudge", hook_s),
        ("core.hooksPath", hooks_dir.to_str()?),
    ] {
        git(&["config", k, v])?;
    }
    Some((tmp, marker))
}

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
