//! One way to spawn git: repository configuration never runs a program.
//!
//! A repository's own `.git/config` (and the files it `include`s) can make
//! almost any git command execute a program of the repository's choosing:
//! `core.fsmonitor` runs on `status`/`diff`/`ls-files`, `diff.external` and
//! `diff.<driver>.textconv` on `diff`/`log -p`/`show`, `filter.<x>.clean` on
//! `status`/`diff`/`stash`, `filter.<x>.smudge` and every hook in
//! `.git/hooks` / `core.hooksPath` on `checkout`/`worktree add`/`commit`,
//! plus `core.sshCommand`, `credential.helper`, `gpg.program`,
//! `core.editor`, `core.pager`, `remote.<x>.uploadpack`, … A reviewed
//! repository's `core.fsmonitor=./hook.sh` ran during an inventory `git
//! ls-files` (0.9.5 review, G1; confirmed with git 2.50.1 under `env -i`).
//!
//! [`git_command`] / [`git_command_async`] build a `git` command whose
//! repo-controlled execution is neutralised with command-line config, which
//! outranks every config file:
//!
//! - always: `--no-pager`, `core.fsmonitor=false`, `core.hooksPath` pointed
//!   at nothing (the null device), `protocol.ext.allow=never`, and
//!   `GIT_TERMINAL_PROMPT=0`;
//! - every exec-capable key found in the repository's `local`/`worktree`
//!   scope (includes followed) is overridden with an inert value
//!   ([`neutral_value`]) — driver names are only known from the config
//!   itself, so they are enumerated with `git config --list --show-scope`
//!   (which runs no program);
//! - [`GitScope::Internal`] (selfware's own read/bookkeeping calls) also sets
//!   `GIT_CONFIG_NOSYSTEM=1`.
//!
//! [`GitScope::UserOperation`] is for git operations the user or model asked
//! for through a tool (`git_commit` running the user's hooks, `checkout`
//! running an LFS smudge filter, `push` using the system credential
//! helper): the repository keeps its hooks, filters and helpers ONLY when
//! it is trusted ([`repo_is_trusted`] — `selfware trust` on the repo's
//! `selfware.toml`); otherwise it is neutralised exactly like an internal
//! call. Global and system configuration (the operator's own) are never
//! overridden except for the always-on settings above.
//!
//! [`GitScope::CommitGate`] is the evolution daemon's promotion commit in
//! the repository the operator pointed `selfware evolve` at: its commit
//! hooks run (the pre-commit gate is part of the promotion contract), all
//! other repository-configured programs are neutralised.
//!
//! Operator-global drivers stay intact: only keys the REPOSITORY sets are
//! overridden, so a global `filter.lfs.*` keeps working unless the
//! repository redefines it.

use std::path::{Path, PathBuf};

/// Whose git operation this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitScope {
    /// selfware's own calls (inventory, verification snapshots, diffs,
    /// worktree bookkeeping): repository execution is always neutralised.
    Internal,
    /// A git operation requested through a tool: repository hooks, filters
    /// and helpers run only in a trusted repository.
    UserOperation,
    /// The evolution daemon's promotion commit in the repository the
    /// operator pointed `selfware evolve` at: the repository's commit hooks
    /// run (its pre-commit gate is part of the promotion contract), every
    /// other repository-configured program is neutralised.
    CommitGate,
}

#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";

/// Settings applied to every hardened call, whatever the repository says.
fn always_on() -> Vec<(String, String)> {
    vec![
        ("core.fsmonitor".to_string(), "false".to_string()),
        ("core.hooksPath".to_string(), NULL_DEVICE.to_string()),
        ("protocol.ext.allow".to_string(), "never".to_string()),
    ]
}

/// The inert value for a config key that can make git run a program, or
/// `None` when the key cannot. `key` is the full dotted name as git prints
/// it (section and variable lowercased, subsection verbatim).
pub fn neutral_value(key: &str) -> Option<&'static str> {
    let lower = key.to_ascii_lowercase();
    let (section, rest) = lower.split_once('.')?;
    let var = rest.rsplit('.').next().unwrap_or(rest);
    let has_subsection = rest.contains('.');
    match (section, var) {
        ("core", "fsmonitor") => Some("false"),
        ("core", "hookspath") => Some(NULL_DEVICE),
        ("core", "pager") | ("pager", _) => Some("cat"),
        ("core", "editor") | ("sequence", "editor") => Some(":"),
        ("core", "sshcommand" | "gitproxy" | "askpass" | "alternaterefscommand") => Some(""),
        ("diff", "external") => Some(""),
        ("diff", "textconv" | "command") if has_subsection => Some(""),
        ("filter", "clean" | "smudge" | "process") if has_subsection => Some(""),
        ("filter", "required") if has_subsection => Some("false"),
        ("merge", "driver") if has_subsection => Some(""),
        ("credential", "helper") => Some(""),
        ("gpg", "program") => Some(if has_subsection {
            if rest.starts_with("ssh.") {
                "ssh-keygen"
            } else {
                "gpgsm"
            }
        } else {
            "gpg"
        }),
        ("remote", "uploadpack") => Some("git-upload-pack"),
        ("remote", "receivepack") => Some("git-receive-pack"),
        ("remote", "vcs") => Some(""),
        ("submodule", "update") if has_subsection => Some("checkout"),
        ("interactive", "difffilter") => Some(""),
        ("difftool" | "mergetool" | "man" | "browser" | "web", "cmd" | "path" | "browser") => {
            Some("")
        }
        ("sendemail", "smtpserver" | "sendmailcmd" | "tocmd" | "cccmd") => Some(""),
        ("uploadpack", "packobjectshook") => Some(""),
        _ => None,
    }
}

/// Exec-capable keys set in the repository's own configuration (`local`
/// and `worktree` scope, includes followed) of the repository containing
/// `dir`, as `(key, value)` pairs. Empty when `dir` is not in a repository
/// or git is unavailable. Lists configuration only — `git config` runs no
/// hook, filter or monitor.
pub fn repo_exec_config(dir: &Path) -> Vec<(String, String)> {
    let mut cmd = std::process::Command::new("git");
    crate::safety::process_env::sanitize_std_command_env(&mut cmd);
    cmd.args(["config", "--list", "--show-scope", "--includes", "-z"])
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let Ok(out) = cmd.output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_scoped_config(&out.stdout)
        .into_iter()
        .filter(|(scope, key, _)| {
            matches!(scope.as_str(), "local" | "worktree") && neutral_value(key).is_some()
        })
        .map(|(_, k, v)| (k, v))
        .collect()
}

/// Parse `git config --list --show-scope -z` output: records are
/// `scope\0key\nvalue\0` (a key without a value has no `\n`).
fn parse_scoped_config(raw: &[u8]) -> Vec<(String, String, String)> {
    let text = String::from_utf8_lossy(raw);
    let mut fields = text.split('\0');
    let mut out = Vec::new();
    while let (Some(scope), Some(entry)) = (fields.next(), fields.next()) {
        if scope.is_empty() {
            break;
        }
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        out.push((scope.to_string(), key.to_string(), value.to_string()));
    }
    out
}

/// The top level of the work tree containing `dir`, found by walking up to
/// a `.git` entry (no git spawn).
fn repo_root(dir: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    start
        .ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Whether the repository containing `dir` is trusted: its `selfware.toml`
/// was trusted with `selfware trust` (the same trust the config loader uses
/// for checkout-local settings).
pub fn repo_is_trusted(dir: &Path) -> bool {
    repo_root(dir)
        .map(|root| crate::config::trust::is_config_trusted(&root.join("selfware.toml")))
        .unwrap_or(false)
}

/// Whether running a read-type git command in `dir` executes nothing the
/// repository chose: the repository is trusted, or its own configuration
/// sets no exec-capable key that a read can reach. Hook locations
/// (`core.hooksPath`) and editors are not consulted — no read command runs
/// a hook or opens an editor.
pub fn repo_git_is_inert(dir: &Path) -> bool {
    repo_is_trusted(dir)
        || repo_exec_config(dir).iter().all(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "core.hookspath" | "core.editor" | "sequence.editor"
            )
        })
}

/// The global arguments (`--no-pager -c key=value …`) that neutralise
/// repository-controlled execution for a git call in `dir`, and whether
/// the repository keeps its own execution ([`GitScope::UserOperation`] in a
/// trusted repository).
pub fn hardening_args(dir: &Path, scope: GitScope) -> Vec<String> {
    if scope == GitScope::UserOperation && repo_is_trusted(dir) {
        return vec!["-c".to_string(), "protocol.ext.allow=never".to_string()];
    }
    let keep_hooks = scope == GitScope::CommitGate;
    let mut overrides = always_on();
    if keep_hooks {
        overrides.retain(|(k, _)| k != "core.hooksPath");
    }
    for (key, _) in repo_exec_config(dir) {
        if keep_hooks && key.eq_ignore_ascii_case("core.hookspath") {
            continue;
        }
        if let Some(v) = neutral_value(&key) {
            if !overrides.iter().any(|(k, _)| k.eq_ignore_ascii_case(&key)) {
                overrides.push((key, v.to_string()));
            }
        }
    }
    let mut args = vec!["--no-pager".to_string()];
    for (k, v) in overrides {
        args.push("-c".to_string());
        args.push(format!("{k}={v}"));
    }
    args
}

fn hardening_env(scope: GitScope) -> Vec<(&'static str, &'static str)> {
    let mut env = vec![("GIT_TERMINAL_PROMPT", "0")];
    if scope == GitScope::Internal {
        env.push(("GIT_CONFIG_NOSYSTEM", "1"));
    }
    env
}

/// A hardened `git` command for `dir` (also its working directory), with
/// the inherited environment cleared to the shared non-sensitive allowlist
/// (`crate::safety::process_env`). Append the subcommand and its arguments
/// with `.args(…)` as usual; do NOT clear the environment again (that would
/// drop `GIT_TERMINAL_PROMPT` / `GIT_CONFIG_NOSYSTEM` — use
/// [`git_command_preserve`] to keep extra variables).
pub fn git_command(dir: impl AsRef<Path>, scope: GitScope) -> std::process::Command {
    git_command_preserve(dir, scope, &[])
}

/// [`git_command`] that also keeps the named parent variables (e.g.
/// `SSH_AUTH_SOCK` for a push). Never pass credential-bearing names.
pub fn git_command_preserve(
    dir: impl AsRef<Path>,
    scope: GitScope,
    preserve: &[&str],
) -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    cmd.sanitized_git_preserve(dir, scope, preserve);
    cmd
}

/// [`git_command`] for async call sites.
pub fn git_command_async(dir: impl AsRef<Path>, scope: GitScope) -> tokio::process::Command {
    git_command_async_preserve(dir, scope, &[])
}

/// [`git_command_preserve`] for async call sites.
pub fn git_command_async_preserve(
    dir: impl AsRef<Path>,
    scope: GitScope,
    preserve: &[&str],
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("git");
    cmd.sanitized_git_preserve(dir, scope, preserve);
    cmd
}

/// Re-apply the hardening environment after an environment clear.
pub trait HardenedGitEnv {
    /// Set `GIT_TERMINAL_PROMPT=0` (and `GIT_CONFIG_NOSYSTEM=1` for
    /// [`GitScope::Internal`]).
    fn git_hardening_env(&mut self, scope: GitScope) -> &mut Self;
}

impl HardenedGitEnv for std::process::Command {
    fn git_hardening_env(&mut self, scope: GitScope) -> &mut Self {
        for (k, v) in hardening_env(scope) {
            self.env(k, v);
        }
        self
    }
}

impl HardenedGitEnv for tokio::process::Command {
    fn git_hardening_env(&mut self, scope: GitScope) -> &mut Self {
        for (k, v) in hardening_env(scope) {
            self.env(k, v);
        }
        self
    }
}

/// Builder form of [`git_command`] for call sites written as one chain:
///
/// ```ignore
/// Command::new("git")
///     .sanitized_git(&root, GitScope::Internal)
///     .args(["ls-files", "-z"])
///     .output()
/// ```
///
/// Must come FIRST, right after `Command::new("git")`: it clears the
/// environment to the shared allowlist, adds the global hardening options
/// (which must precede the subcommand), sets the hardening environment and
/// the working directory.
pub trait SanitizedGitExt {
    /// See [`git_command`].
    fn sanitized_git(&mut self, dir: impl AsRef<Path>, scope: GitScope) -> &mut Self;
    /// See [`git_command_preserve`].
    fn sanitized_git_preserve(
        &mut self,
        dir: impl AsRef<Path>,
        scope: GitScope,
        preserve: &[&str],
    ) -> &mut Self;
}

impl SanitizedGitExt for std::process::Command {
    fn sanitized_git(&mut self, dir: impl AsRef<Path>, scope: GitScope) -> &mut Self {
        self.sanitized_git_preserve(dir, scope, &[])
    }
    fn sanitized_git_preserve(
        &mut self,
        dir: impl AsRef<Path>,
        scope: GitScope,
        preserve: &[&str],
    ) -> &mut Self {
        let dir = dir.as_ref();
        crate::safety::process_env::sanitize_std_command_env_preserve(self, preserve);
        self.args(hardening_args(dir, scope)).current_dir(dir);
        self.git_hardening_env(scope)
    }
}

impl SanitizedGitExt for tokio::process::Command {
    fn sanitized_git(&mut self, dir: impl AsRef<Path>, scope: GitScope) -> &mut Self {
        self.sanitized_git_preserve(dir, scope, &[])
    }
    fn sanitized_git_preserve(
        &mut self,
        dir: impl AsRef<Path>,
        scope: GitScope,
        preserve: &[&str],
    ) -> &mut Self {
        let dir = dir.as_ref();
        crate::safety::process_env::sanitize_command_env_preserve(self, preserve);
        self.args(hardening_args(dir, scope)).current_dir(dir);
        self.git_hardening_env(scope)
    }
}

/// Diff-family flags that keep external diff drivers and textconv filters
/// from running even when a driver is configured outside the repository
/// (`git diff`, `git log -p`, `git show`).
pub const NO_EXTERNAL_DIFF: [&str; 2] = ["--no-ext-diff", "--no-textconv"];

#[cfg(test)]
#[path = "../../tests/unit/safety/git_exec/git_exec_test.rs"]
mod tests;
