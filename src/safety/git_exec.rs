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

use std::ffi::OsString;
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
        ("safe.bareRepository".to_string(), "explicit".to_string()),
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
        ("gpg", "defaultkeycommand") if has_subsection => Some(""),
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

/// The work-tree relocation key: git's setup reads it from the repository's
/// own config file before any command-line config, so it is neutralised
/// with `--work-tree` / `GIT_WORK_TREE` instead of a [`neutral_value`].
fn is_work_tree_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("core.worktree")
}

/// Resolve the git directory for a checkout at `root` (`.git` directory,
/// `.git` gitfile `gitdir: …`, or `root` itself for a bare layout).
fn resolve_git_dir(root: &Path) -> PathBuf {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return dot_git;
    }
    if let Ok(text) = crate::tools::file::read_to_string_capped(&dot_git, 65_536) {
        if let Some(rest) = text.lines().next().and_then(|l| l.strip_prefix("gitdir:")) {
            let rel = Path::new(rest.trim());
            return if rel.is_absolute() {
                rel.to_path_buf()
            } else {
                root.join(rel)
            };
        }
    }
    root.to_path_buf()
}

/// True when a `core.worktree` value set in `root`'s config resolves to
/// `root` itself (as in a standard git submodule whose `.git/modules/<sub>/config`
/// sets `worktree = ../../../<sub>`).
fn work_tree_is_checkout(root: &Path, configured_worktree: &str) -> bool {
    let Ok(canon_root) = std::fs::canonicalize(root) else {
        return false;
    };
    let git_dir = resolve_git_dir(&canon_root);
    let candidate = if Path::new(configured_worktree).is_absolute() {
        PathBuf::from(configured_worktree)
    } else {
        git_dir.join(configured_worktree)
    };
    std::fs::canonicalize(candidate).ok().as_deref() == Some(canon_root.as_path())
}

/// Exec-capable keys (plus `core.worktree` when it relocates the work tree
/// outside the checkout) set in the repository's own configuration (`local`
/// and `worktree` scope, includes followed) of the repository containing
/// `dir`, as `(key, value)` pairs. Empty when `dir` is not in a repository
/// or git is unavailable. Lists configuration only — `git config` runs no
/// hook, filter or monitor.
pub fn repo_exec_config(dir: &Path) -> Vec<(String, String)> {
    repo_exec_config_raw(dir)
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

/// Byte-exact counterpart of [`repo_exec_config`]: subsection names in
/// `.git/config` are arbitrary bytes (`[diff "…"]`), and `GIT_CONFIG_KEY_<n>`
/// must match those bytes verbatim for git to recognise the override.
fn repo_exec_config_raw(dir: &Path) -> Vec<(OsString, OsString)> {
    let key = config_cache_key(dir);
    if let Some((root, contents)) = &key {
        let cache = CONFIG_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached, exec)) = cache.get(root) {
            if cached == contents {
                return exec.clone();
            }
        }
    }
    let (exec, cacheable) = repo_exec_config_raw_uncached(dir);
    if cacheable {
        if let Some((root, contents)) = key {
            let mut cache = CONFIG_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() >= CONFIG_CACHE_MAX {
                cache.clear();
            }
            cache.insert(root, (contents, exec.clone()));
        }
    }
    exec
}

const CONFIG_CACHE_MAX: usize = 64;

/// `.git/config`, `.git/config.worktree`, `.git/index`.
type DecidingFiles = [Option<Vec<u8>>; 3];

type ConfigCache = std::collections::HashMap<PathBuf, (DecidingFiles, Vec<(OsString, OsString)>)>;

/// Repository root → (the bytes of its `.git/config`, `.git/config.worktree`
/// and `.git/index` when last listed, the exec keys found then).
static CONFIG_CACHE: std::sync::LazyLock<std::sync::Mutex<ConfigCache>> =
    std::sync::LazyLock::new(Default::default);

/// Cache key for [`repo_exec_config`]: the repository root and the exact
/// bytes of the files that decide its own config and its gitlink set. `None`
/// (no caching) when the repository is not a plain `.git` directory
/// (worktree, submodule, bare), its config uses `include`/`includeIf`, or
/// it has gitlinks (whose own `.git/config` files also contribute — handled
/// by `cacheable` in [`repo_exec_config_raw_uncached`]).
fn config_cache_key(dir: &Path) -> Option<(PathBuf, DecidingFiles)> {
    let root = repo_root(dir)?;
    let git_dir = root.join(".git");
    if !git_dir.is_dir() {
        return None;
    }
    let read = |name: &str| std::fs::read(git_dir.join(name)).ok();
    let contents = [read("config"), read("config.worktree"), read("index")];
    let includes = contents[..2].iter().flatten().any(|bytes| {
        String::from_utf8_lossy(bytes)
            .to_ascii_lowercase()
            .contains("include")
    });
    (!includes).then_some((root, contents))
}

/// Enumerate exec-capable keys for `dir` AND any gitlinked nested
/// repositories under `dir`'s root (a `git status` or `git diff` in the
/// parent descends into a dirty gitlink to inspect it, and command-line /
/// environment config is inherited by that child `git`). Second element is
/// whether the result is decided by `dir`'s own files alone (no gitlinks).
fn repo_exec_config_raw_uncached(dir: &Path) -> (Vec<(OsString, OsString)>, bool) {
    let mut out = single_repo_exec_config_raw(dir, true);
    let Some(root) = repo_root(dir) else {
        return (out, true);
    };
    let links = gitlinks(&root, &out);
    if links.is_empty() {
        return (out, true);
    }
    for sub in links {
        for (k, v) in single_repo_exec_config_raw(&root.join(sub), false) {
            let k_lossy = k.to_string_lossy();
            if !out
                .iter()
                .any(|(ek, _)| ek.to_string_lossy().eq_ignore_ascii_case(&k_lossy))
            {
                out.push((k, v));
            }
        }
    }
    (out, false)
}

fn single_repo_exec_config_raw(dir: &Path, include_work_tree: bool) -> Vec<(OsString, OsString)> {
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
    let root = repo_root(dir);
    parse_scoped_config_raw(&out.stdout)
        .into_iter()
        .filter(|(scope, key, value)| {
            if !matches!(scope.as_str(), "local" | "worktree") {
                return false;
            }
            let key_str = key.to_string_lossy();
            if neutral_value(&key_str).is_some() {
                return true;
            }
            if include_work_tree && is_work_tree_key(&key_str) {
                let val_str = value.to_string_lossy();
                return match &root {
                    Some(r) => !work_tree_is_checkout(r, &val_str),
                    None => true,
                };
            }
            false
        })
        .map(|(_, k, v)| (k, v))
        .collect()
}

/// Paths of gitlink entries (mode `160000`) in `root`'s index that are
/// themselves git repositories on disk. Reads the index only (`--stage`),
/// with `root`'s own exec keys already neutralised via environment config.
fn gitlinks(root: &Path, own: &[(OsString, OsString)]) -> Vec<PathBuf> {
    let mut cmd = std::process::Command::new("git");
    crate::safety::process_env::sanitize_std_command_env(&mut cmd);
    cmd.arg("--no-pager");
    for (k, v) in always_on() {
        cmd.arg("-c").arg(format!("{k}={v}"));
    }
    let mut env_overrides = Vec::new();
    for (key, _) in own {
        if let Some(v) = neutral_value(&key.to_string_lossy()) {
            env_overrides.push((key.clone(), OsString::from(v)));
        }
    }
    for (k, v) in config_env_raw(env_overrides) {
        cmd.env(k, v);
    }
    cmd.args(["ls-files", "-z", "--stage"])
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let Ok(out) = cmd.output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|e| e.starts_with("160000 "))
        .filter_map(|e| e.split_once('\t').map(|(_, p)| PathBuf::from(p)))
        .filter(|p| {
            !p.is_absolute()
                && !p
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                && root.join(p).join(".git").symlink_metadata().is_ok()
        })
        .collect()
}

/// Parse `git config --list --show-scope -z` output: records are
/// `scope\0key\nvalue\0` (a key without a value has no `\n`).
#[cfg(test)]
fn parse_scoped_config(raw: &[u8]) -> Vec<(String, String, String)> {
    parse_scoped_config_raw(raw)
        .into_iter()
        .map(|(s, k, v)| {
            (
                s,
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

/// Byte-exact parser for `git config --list --show-scope -z`.
fn parse_scoped_config_raw(raw: &[u8]) -> Vec<(String, OsString, OsString)> {
    let mut fields = raw.split(|&b| b == 0);
    let mut out = Vec::new();
    while let (Some(scope), Some(entry)) = (fields.next(), fields.next()) {
        if scope.is_empty() {
            break;
        }
        let scope = String::from_utf8_lossy(scope).into_owned();
        let (key, value) = match entry.iter().position(|&b| b == b'\n') {
            Some(i) => (&entry[..i], &entry[i + 1..]),
            None => (entry, &[][..]),
        };
        out.push((scope, bytes_to_os(key), bytes_to_os(value)));
    }
    out
}

#[cfg(unix)]
fn bytes_to_os(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes).to_os_string()
}

#[cfg(not(unix))]
fn bytes_to_os(bytes: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

/// The top level of the repository containing `dir`, found by walking up to
/// a `.git` entry or a bare-repository `HEAD` (no git spawn). Uses
/// `symlink_metadata` so a dangling `.git` or `HEAD` symlink is still
/// recognised as a repository boundary.
fn repo_root(dir: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    start
        .ancestors()
        .find(|d| {
            d.join(".git").symlink_metadata().is_ok() || d.join("HEAD").symlink_metadata().is_ok()
        })
        .map(Path::to_path_buf)
}

/// Where a hardened call pins git's work tree: the checkout (the directory
/// holding `.git`) when the repository's own configuration relocates the
/// work tree outside the checkout with `core.worktree`, `None` otherwise.
pub fn pinned_work_tree(dir: &Path) -> Option<PathBuf> {
    if !repo_exec_config(dir)
        .iter()
        .any(|(key, _)| is_work_tree_key(key))
    {
        return None;
    }
    repo_root(dir)
}

/// [`pinned_work_tree`] for libgit2 users: refuse a repository opened at
/// `dir` whose own configuration moved its work tree outside the checkout
/// (`core.worktree`) — libgit2 honours the key like git does, so every path
/// a status or diff reports would be outside `dir`.
pub fn ensure_work_tree_is_checkout(
    repo: &git2::Repository,
    dir: &Path,
    scope: GitScope,
) -> anyhow::Result<()> {
    if scope == GitScope::UserOperation && repo_is_trusted(dir) {
        return Ok(());
    }
    let checkout = repo_root(dir)
        .unwrap_or_else(|| std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()));
    let work_tree = repo.workdir().map(std::fs::canonicalize).transpose()?;
    if work_tree.as_deref() != Some(checkout.as_path()) {
        anyhow::bail!(
            "refusing repository at {}: its configuration (core.worktree) places the work tree \
             outside the checkout, at {}",
            checkout.display(),
            work_tree
                .map(|w| w.display().to_string())
                .unwrap_or_else(|| "<none>".to_string())
        );
    }
    Ok(())
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
/// (`core.hooksPath`), editors, and commit-signing key commands are not
/// consulted — no read command runs a hook, opens an editor, or signs a
/// commit.
pub fn repo_git_is_inert(dir: &Path) -> bool {
    repo_is_trusted(dir)
        || repo_exec_config(dir).iter().all(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "core.hookspath" | "core.editor" | "sequence.editor" | "gpg.ssh.defaultkeycommand"
            )
        })
}

/// The global arguments (`--no-pager [--work-tree=…] -c key=value …`) that
/// neutralise repository-controlled execution (and work-tree relocation)
/// for a git call in `dir`.
///
/// Repository-named keys containing `=` are never spliced into `-c
/// key=value` (git splits `-c` on the first `=`, so a subsection name
/// containing `=` would be misparsed as a different key). Those keys — and
/// all repository-named overrides — are passed via [`hardening_config_env`]
/// (`GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>`), where the key and value
/// are separate variables.
pub fn hardening_args(dir: &Path, scope: GitScope) -> Vec<String> {
    if scope == GitScope::UserOperation && repo_is_trusted(dir) {
        return vec![
            "-c".to_string(),
            "protocol.ext.allow=never".to_string(),
            "-c".to_string(),
            "safe.bareRepository=explicit".to_string(),
        ];
    }
    let keep_hooks = scope == GitScope::CommitGate;
    let mut overrides = always_on();
    if keep_hooks {
        overrides.retain(|(k, _)| k != "core.hooksPath");
    }
    let mut args = vec!["--no-pager".to_string()];
    if let Some(root) = pinned_work_tree(dir) {
        args.push(format!("--work-tree={}", root.display()));
    }
    for (k, v) in overrides {
        args.push("-c".to_string());
        args.push(format!("{k}={v}"));
    }
    args
}

/// The `GIT_CONFIG_COUNT` / `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>`
/// environment overrides that neutralise exec-capable keys set in `dir`'s
/// repository configuration. Because the key and value live in separate
/// variables, a subsection name containing `=` (e.g.
/// `[diff "a\n\tfsmonitor = ./pwn.sh\n[x"]`) cannot be split on `=` into a
/// different config key.
pub fn hardening_config_env(dir: &Path, scope: GitScope) -> Vec<(String, OsString)> {
    if scope == GitScope::UserOperation && repo_is_trusted(dir) {
        return Vec::new();
    }
    let keep_hooks = scope == GitScope::CommitGate;
    let mut overrides: Vec<(OsString, OsString)> = Vec::new();
    for (key, _) in repo_exec_config_raw(dir) {
        let key_lossy = key.to_string_lossy();
        if keep_hooks && key_lossy.eq_ignore_ascii_case("core.hookspath") {
            continue;
        }
        if let Some(v) = neutral_value(&key_lossy) {
            if !overrides
                .iter()
                .any(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case(&key_lossy))
            {
                overrides.push((key, OsString::from(v)));
            }
        }
    }
    config_env_raw(overrides)
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
/// directory is `dir`.
pub fn shell_git_env(dir: &Path) -> Vec<(String, String)> {
    if repo_is_trusted(dir) {
        return Vec::new();
    }
    let mut env = config_env(neutral_overrides(dir, &[]));
    if let Some(root) = pinned_work_tree(dir) {
        env.push((
            "GIT_WORK_TREE".to_string(),
            root.to_string_lossy().into_owned(),
        ));
    }
    env
}

/// The empty tree: an attribute source with no `.gitattributes`.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// [`shell_git_env`] WITHOUT the trust exemption, and stricter: the
/// neutralisation for processes running untrusted code (evolution arms
/// building model-written source, `crate::safety::quarantine`).
pub fn quarantine_git_env(dir: &Path) -> Vec<(String, String)> {
    let mut env = config_env(neutral_overrides(
        dir,
        &[("diff.external", NEUTRAL_EXTERNAL_DIFF)],
    ));
    env.push(("GIT_ATTR_SOURCE".to_string(), EMPTY_TREE.to_string()));
    env
}

/// The always-on overrides plus `extra`, plus every exec-capable key `dir`'s
/// repository sets, each with its inert value.
fn neutral_overrides(dir: &Path, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut overrides = always_on();
    overrides.push(("core.pager".to_string(), "cat".to_string()));
    for (k, v) in extra {
        overrides.push((k.to_string(), v.to_string()));
    }
    for (key, _) in repo_exec_config(dir) {
        if let Some(v) = neutral_value(&key) {
            if !overrides.iter().any(|(k, _)| k.eq_ignore_ascii_case(&key)) {
                overrides.push((key, v.to_string()));
            }
        }
    }
    overrides
}

/// `GIT_CONFIG_COUNT` / `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>` for
/// `overrides`.
fn config_env(overrides: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut env = vec![("GIT_CONFIG_COUNT".to_string(), overrides.len().to_string())];
    for (n, (k, v)) in overrides.into_iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{n}"), k));
        env.push((format!("GIT_CONFIG_VALUE_{n}"), v));
    }
    env
}

fn config_env_raw(overrides: Vec<(OsString, OsString)>) -> Vec<(String, OsString)> {
    if overrides.is_empty() {
        return Vec::new();
    }
    let mut env = vec![(
        "GIT_CONFIG_COUNT".to_string(),
        OsString::from(overrides.len().to_string()),
    )];
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
        for (k, v) in hardening_config_env(dir, scope) {
            self.env(k, v);
        }
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
        for (k, v) in hardening_config_env(dir, scope) {
            self.env(k, v);
        }
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
