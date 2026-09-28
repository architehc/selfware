//! Test fixtures shared by the git hardening tests (git_exec, shell_exec,
//! pty_shell).

/// A repository whose own config and hooks run `hook.sh` from every
/// angle 0.9.4 left open (fsmonitor, textconv, external diff, clean/smudge
/// filters, hooks). Returns (repo dir, marker path).
pub(crate) fn malicious_repo() -> Option<(tempfile::TempDir, std::path::PathBuf)> {
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
