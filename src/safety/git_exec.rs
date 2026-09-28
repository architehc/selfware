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

/// Neutral `diff.external` / `diff.<driver>.command`: a plain unified diff
/// of the two blobs git hands an external driver (`path old-file old-hex
/// old-mode new-file new-hex new-mode`), so `git diff` keeps working
/// instead of failing on an empty program (git runs an empty value as a
/// program named "" — checked with git 2.50). `diff` exits 1 on a
/// difference, which git would read as a crash.
#[cfg(not(windows))]
pub const NEUTRAL_EXTERNAL_DIFF: &str =
    r#"f() { diff -u -L "a/$1" -L "b/$1" -- "$2" "$5"; [ $? -le 1 ]; }; f"#;
#[cfg(windows)]
pub const NEUTRAL_EXTERNAL_DIFF: &str = "";

/// Neutral textconv / clean / smudge program: the identity (`cat`). An
/// empty value makes git fail with "cannot run" instead of skipping it.
#[cfg(not(windows))]
const NEUTRAL_FILTER: &str = "cat";
#[cfg(windows)]
const NEUTRAL_FILTER: &str = "";

/// The inert value for a config key that can make git run a program, or
/// `None` when the key cannot. `key` is the full dotted name as git prints
/// it (section and variable lowercased, subsection verbatim). Where git
/// would fail on an empty program, the value is a working stand-in that
/// runs nothing the repository chose ([`NEUTRAL_EXTERNAL_DIFF`], `cat`).
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
        ("diff", "external") => Some(NEUTRAL_EXTERNAL_DIFF),
        ("diff", "command") if has_subsection => Some(NEUTRAL_EXTERNAL_DIFF),
        ("diff", "textconv") if has_subsection => Some(NEUTRAL_FILTER),
        ("filter", "clean" | "smudge") if has_subsection => Some(NEUTRAL_FILTER),
        ("filter", "process") if has_subsection => Some(""),
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
    let key = config_cache_key(dir);
    if let Some((root, contents)) = &key {
        let cache = CONFIG_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached, exec)) = cache.get(root) {
            if cached == contents {
                return exec.clone();
            }
        }
    }
    let exec = repo_exec_config_uncached(dir);
    if let Some((root, contents)) = key {
        let mut cache = CONFIG_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= CONFIG_CACHE_MAX {
            cache.clear();
        }
        cache.insert(root, (contents, exec.clone()));
    }
    exec
}

const CONFIG_CACHE_MAX: usize = 64;

type ConfigCache =
    std::collections::HashMap<PathBuf, ([Option<Vec<u8>>; 2], Vec<(String, String)>)>;

/// Repository root → (the bytes of its `.git/config` and
/// `.git/config.worktree` when last listed, the exec keys found then).
static CONFIG_CACHE: std::sync::LazyLock<std::sync::Mutex<ConfigCache>> =
    std::sync::LazyLock::new(Default::default);

/// Cache key for [`repo_exec_config`]: the repository root and the exact
/// bytes of the only files the `local`/`worktree` scopes read. `None` (no
/// caching) when the repository is not a plain `.git` directory (worktree,
/// submodule, bare) or its config uses `include`/`includeIf` — then other
/// files contribute and the bytes here would not decide the answer.
fn config_cache_key(dir: &Path) -> Option<(PathBuf, [Option<Vec<u8>>; 2])> {
    let root = repo_root(dir)?;
    let git_dir = root.join(".git");
    if !git_dir.is_dir() {
        return None;
    }
    let read = |name: &str| std::fs::read(git_dir.join(name)).ok();
    let contents = [read("config"), read("config.worktree")];
    let includes = contents.iter().flatten().any(|bytes| {
        String::from_utf8_lossy(bytes)
            .to_ascii_lowercase()
            .contains("include")
    });
    (!includes).then_some((root, contents))
}

fn repo_exec_config_uncached(dir: &Path) -> Vec<(String, String)> {
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

/// Environment variables that pass git config the command-line way
/// (`GIT_CONFIG_COUNT` / `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>`, and
/// the older `GIT_CONFIG_PARAMETERS`). A caller-supplied one would replace
/// the neutralisation [`shell_git_env`] sets.
pub fn is_git_config_env(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper == "GIT_CONFIG_COUNT"
        || upper == "GIT_CONFIG_PARAMETERS"
        || upper.starts_with("GIT_CONFIG_KEY_")
        || upper.starts_with("GIT_CONFIG_VALUE_")
}

/// Environment for a shell the agent spawns for the model (`shell_exec`,
/// `pty_shell`, `process_start`, workflow shell steps) whose working
/// directory is `dir`: every `git` the shell runs — directly, from a
/// script, from `make` — gets the same neutralisation as a hardened
/// [`GitScope::UserOperation`] call, passed as `GIT_CONFIG_COUNT` /
/// `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>`. Environment config has
/// command-line precedence: it beats the repository's `.git/config` (each
/// key checked with git 2.50; git 2.31+ reads these variables).
///
/// - trusted repository ([`repo_is_trusted`]): nothing (it keeps its
///   fsmonitor, filters, hooks, …);
/// - otherwise, always `core.fsmonitor=false`, `core.hooksPath` = the null
///   device, `protocol.ext.allow=never`, `core.pager=cat`, plus every
///   exec-capable key the repository's own config sets (driver names are
///   only known from the config), with [`neutral_value`]'s working
///   stand-ins so `git diff` / `git log -p` still print diffs.
///
/// Limits: per-driver keys are enumerated for the repository at `dir` when
/// the shell starts; a `cd` into a DIFFERENT untrusted repository keeps
/// only the fixed keys above. A command can still undo this on purpose
/// (`GIT_CONFIG_COUNT=0 git …`): this protects a model from the
/// repository, not the repository from the model.
pub fn shell_git_env(dir: &Path) -> Vec<(String, String)> {
    if repo_is_trusted(dir) {
        return Vec::new();
    }
    let mut overrides = always_on();
    overrides.push(("core.pager".to_string(), "cat".to_string()));
    for (key, _) in repo_exec_config(dir) {
        if let Some(v) = neutral_value(&key) {
            if !overrides.iter().any(|(k, _)| k.eq_ignore_ascii_case(&key)) {
                overrides.push((key, v.to_string()));
            }
        }
    }
    let mut env = vec![("GIT_CONFIG_COUNT".to_string(), overrides.len().to_string())];
    for (n, (k, v)) in overrides.into_iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{n}"), k));
        env.push((format!("GIT_CONFIG_VALUE_{n}"), v));
    }
    env
}

/// Apply [`shell_git_env`] for `dir` to a model shell's command. Call after
/// the environment clear and after any caller-supplied variables.
pub fn apply_shell_git_env(cmd: &mut tokio::process::Command, dir: &Path) {
    for (k, v) in shell_git_env(dir) {
        cmd.env(k, v);
    }
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

#[cfg(all(test, unix))]
#[path = "../../tests/unit/safety/git_exec/support.rs"]
pub(crate) mod test_support;
