//! Quarantine for building and testing untrusted (model-generated) code.
//!
//! An evolution arm runs `cargo check` / `cargo test` / a benchmark on
//! source a model wrote. A `build.rs`, a proc macro or a test is arbitrary
//! code execution as the operator's user. [`ArmQuarantine`] runs every such
//! process with (approach of `scripts/live_eval/toolchain.py`):
//!
//! - a **source snapshot** of the base commit (`git archive`, no `.git`),
//!   not a linked `git worktree`: a linked worktree shares `.git/config`,
//!   hooks and refs with the operator's repository, so a `git config
//!   core.fsmonitor …` run by the arm would persist into the operator's own
//!   repository and execute on their next `git status`;
//! - its **own `HOME`** (and `XDG_*` dirs, `TMPDIR`) inside the arm;
//! - its **own `CARGO_HOME`**: `config.toml` copied from the operator's
//!   with credential-bearing and host-process keys dropped (`token`s,
//!   `credential-provider`, `[env]`, `build.rustc-wrapper` — an sccache
//!   server would compile outside the quarantine), never
//!   `credentials.toml`; `registry/` and `git/` are copy-on-write clones of
//!   the operator's (`cp -c` on APFS, `cp --reflink=always` on btrfs/xfs) or
//!   empty (cargo downloads) — recorded in [`IsolationReport`];
//! - its **own `RUSTUP_HOME`** (empty): the toolchain is called directly from
//!   a sysroot `bin/` on `PATH` (a copy-on-write clone per run where
//!   available), so no `rustup` proxy runs and nothing can install into or
//!   update the operator's `~/.rustup`;
//! - `CARGO_TARGET_DIR` inside the arm;
//! - `PATH` = the toolchain `bin` + `/usr/bin:/bin:/usr/sbin:/sbin` only
//!   (plus the arm's own `bin/` holding just `git`, see below);
//! - the shared sanitized environment ([`crate::safety::process_env`]: no
//!   API keys or tokens);
//! - git neutralised for every `git` the arm runs: `GIT_CONFIG_*`
//!   overrides as in [`crate::safety::git_exec::quarantine_git_env`]
//!   (fsmonitor off, hooks at the null device, …, regardless of trust),
//!   `GIT_CONFIG_NOSYSTEM`, `GIT_CONFIG_GLOBAL` = the null device, and
//!   `GIT_CEILING_DIRECTORIES` at the arm root so repository discovery never
//!   climbs out of the arm into an enclosing repository, and
//!   `GIT_ATTR_SOURCE` = the empty tree so no `.gitattributes`-selected
//!   filter/textconv/diff driver runs (limits in `quarantine_git_env`).
//!   git older than 2.40 ignores `GIT_ATTR_SOURCE`, so when the system
//!   `git` is that old (macOS with Xcode ≤ 16: Apple Git 2.39) the arm gets
//!   a private `bin/git` running the first git 2.40+ on the operator's
//!   `PATH` ([`select_arm_git`]); when there is none, the
//!   [`IsolationReport`] says those drivers are NOT neutralised;
//! - its own process group, SIGKILLed as a group on timeout, cancel (future
//!   dropped) or leftover descendants
//!   ([`crate::tools::process_guard::run_command_bounded`]);
//! - the snapshot as working directory.
//!
//! **Not isolated** (stated in every [`IsolationReport`]): without the
//! sandbox there is no filesystem or network sandbox — the arm runs as the
//! operator's user, so code that writes to an absolute path (`/Users/<me>/…`,
//! found via `getpwuid` or `/etc/passwd`) or opens a socket still can; a
//! process that calls `setsid()` leaves the process group; arms of one run
//! can write into each other's directories. The quarantine keeps the
//! *default* locations (HOME, cargo/rustup homes, git config, env secrets)
//! away from the arm; it is not a security boundary against code written to
//! escape it.
//!
//! **Opt-in sandbox** ([`QuarantineOptions::sandbox`], macOS only):
//! every process runs under `sandbox-exec` with a profile that denies
//! writes outside the arm root (plus `/dev` nodes), denies reads of the
//! operator's home directory, and denies network access (cargo runs
//! offline: dependencies must be in the cloned registry). Linux has no equivalent here
//! yet (`bwrap`/`unshare` would be the route) — requesting it there is an
//! error, never a silent no-op.

use crate::safety::process_env::sanitized_env_from;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The only host directories on an arm's `PATH` besides the toolchain.
pub const SYSTEM_PATH: &[&str] = &["/usr/bin", "/bin", "/usr/sbin", "/sbin"];

#[cfg(windows)]
const NULL_DEVICE: &str = "NUL";
#[cfg(not(windows))]
const NULL_DEVICE: &str = "/dev/null";

/// The operator's toolchain and environment an arm is quarantined FROM.
#[derive(Debug, Clone)]
pub struct HostToolchain {
    /// The operator's home directory (never given to an arm).
    pub home: PathBuf,
    /// The operator's cargo home: source of `config.toml` and the registry
    /// clone. Never given to an arm.
    pub cargo_home: PathBuf,
    /// Sysroot of the toolchain the project builds with; `cargo`/`rustc`
    /// are run from its `bin/`.
    pub sysroot: PathBuf,
    /// The environment the sanitizer filters (the live process env in
    /// production; tests pass a synthetic one).
    pub parent_env: Vec<(OsString, OsString)>,
}

impl HostToolchain {
    /// The operator's toolchain for `repo`: the sysroot `rustc --print
    /// sysroot` reports there (honouring `rust-toolchain.toml`).
    pub fn detect(repo: &Path) -> Result<Self> {
        let parent_env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        let home = dirs::home_dir().context("no home directory")?;
        let cargo_home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cargo"));
        let mut cmd = std::process::Command::new("rustc");
        crate::safety::process_env::sanitize_std_command_env(&mut cmd);
        let out = cmd
            .args(["--print", "sysroot"])
            .current_dir(repo)
            .stdin(std::process::Stdio::null())
            .output()
            .context("running `rustc --print sysroot`")?;
        if !out.status.success() {
            bail!(
                "`rustc --print sysroot` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let sysroot = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        if !sysroot.join("bin").join(exe("cargo")).is_file() {
            bail!(
                "toolchain sysroot {} has no bin/cargo (install the cargo component)",
                sysroot.display()
            );
        }
        Ok(Self {
            home,
            cargo_home,
            sysroot,
            parent_env,
        })
    }
}

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The oldest git that honours `GIT_ATTR_SOURCE` (2.40). An older git
/// ignores it, so a `.gitattributes`-selected filter/textconv driver that a
/// repository's own config defines runs in that repository — observed on
/// the macos-14 runners, whose `/usr/bin/git` (Xcode 15.4) is Apple Git
/// 2.39: a build script's `git -C <evil repo> status` ran the repository's
/// `filter.<x>.clean`.
pub const GIT_ATTR_SOURCE_MIN: (u32, u32) = (2, 40);

/// `(major, minor)` of the git at `git` (`git version 2.39.3 (Apple
/// Git-146)` → `(2, 39)`), `None` when it does not run or says something
/// else. `env` is the whole environment it runs with.
pub fn git_version(git: &Path, env: &[(OsString, OsString)]) -> Option<(u32, u32)> {
    let out = std::process::Command::new(git)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    parse_git_version(&String::from_utf8_lossy(&out.stdout))
}

/// `(major, minor)` from `git --version` output.
pub fn parse_git_version(text: &str) -> Option<(u32, u32)> {
    let rest = text.trim().strip_prefix("git version ")?;
    let mut nums = rest
        .split(|c: char| !c.is_ascii_digit())
        .map(str::parse::<u32>);
    Some((nums.next()?.ok()?, nums.next()?.ok()?))
}

/// Which `git` an arm's processes run, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmGit {
    /// No `git` on the system part of the arm `PATH`: nested git calls
    /// fail, nothing to harden. None is added.
    Absent,
    /// The first `git` on the system `PATH` honours `GIT_ATTR_SOURCE`.
    System { path: PathBuf, version: (u32, u32) },
    /// The system `git` is older than 2.40 (or its version is unknown); the
    /// arm gets a private `bin/git` that runs `path` (the first git 2.40+
    /// on the operator's `PATH` outside their home) instead.
    Replaced {
        path: PathBuf,
        version: (u32, u32),
        system: PathBuf,
        system_version: Option<(u32, u32)>,
    },
    /// The system `git` is older than 2.40 and no git 2.40+ was found:
    /// `.gitattributes`-selected drivers of repositories other than the
    /// snapshot are NOT neutralised (stated in the [`IsolationReport`]).
    Old {
        path: PathBuf,
        version: Option<(u32, u32)>,
    },
}

/// Choose the arm's `git`: the first one in `system_dirs` (what the arm
/// `PATH` resolves) when it honours `GIT_ATTR_SOURCE`, otherwise the first
/// git 2.40+ in `operator_dirs` that is not inside `operator_home` (the arm
/// never runs a program from the operator's home: under the sandbox it
/// cannot read it, and a home shim reads the operator's own state).
/// `version_of` runs `git --version` (injected for tests).
pub fn select_arm_git(
    system_dirs: &[PathBuf],
    operator_dirs: &[PathBuf],
    operator_home: &Path,
    version_of: impl Fn(&Path) -> Option<(u32, u32)>,
) -> ArmGit {
    let git = exe("git");
    let Some(system) = system_dirs
        .iter()
        .map(|d| d.join(&git))
        .find(|p| p.is_file())
    else {
        return ArmGit::Absent;
    };
    let system_version = version_of(&system);
    if let Some(version) = system_version.filter(|v| *v >= GIT_ATTR_SOURCE_MIN) {
        return ArmGit::System {
            path: system,
            version,
        };
    }
    for dir in operator_dirs {
        if !dir.is_absolute() || is_within(dir, operator_home) {
            continue;
        }
        let candidate = dir.join(&git);
        if !candidate.is_file() {
            continue;
        }
        if let Some(version) = version_of(&candidate).filter(|v| *v >= GIT_ATTR_SOURCE_MIN) {
            return ArmGit::Replaced {
                path: candidate,
                version,
                system,
                system_version,
            };
        }
    }
    ArmGit::Old {
        path: system,
        version: system_version,
    }
}

impl ArmGit {
    /// The [`IsolationReport`] sentence about which git the arm runs.
    pub fn describe(&self) -> String {
        let v = |v: &Option<(u32, u32)>| match v {
            Some((a, b)) => format!("git {a}.{b}"),
            None => "unknown version".to_string(),
        };
        match self {
            ArmGit::Absent => "no git on the arm PATH".to_string(),
            ArmGit::System { path, version } => format!(
                "arm git {} (git {}.{}, honours GIT_ATTR_SOURCE)",
                path.display(),
                version.0,
                version.1
            ),
            ArmGit::Replaced {
                path,
                version,
                system,
                system_version,
            } => format!(
                "arm git {} (git {}.{}) via the arm's own bin/, ahead of {} ({}, which \
                 ignores GIT_ATTR_SOURCE); a program calling {} by absolute path is \
                 not covered",
                path.display(),
                version.0,
                version.1,
                system.display(),
                v(system_version),
                system.display()
            ),
            ArmGit::Old { path, version } => format!(
                "arm git {} ({}) ignores GIT_ATTR_SOURCE and no git 2.40+ was found: \
                 .gitattributes-selected filter/textconv drivers of repositories other \
                 than the snapshot are NOT neutralised",
                path.display(),
                v(version)
            ),
        }
    }

    /// Whether `.gitattributes`-selected drivers are neutralised for every
    /// repository the arm's `git` (by `PATH`) touches.
    pub fn attr_source_honoured(&self) -> bool {
        !matches!(self, ArmGit::Old { .. })
    }
}

/// How an arm gets cargo's package caches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryMode {
    /// Copy-on-write clone of the operator's `registry/` and `git/` where the
    /// filesystem supports it, otherwise empty (downloads).
    CloneOrEmpty,
    /// Always empty: cargo downloads what it needs (network required).
    Empty,
}

/// Knobs for [`ArmQuarantine::prepare`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineOptions {
    pub registry: RegistryMode,
    /// Run every arm process under the macOS `sandbox-exec` profile
    /// (filesystem writes confined to the arm, no reads of the operator's
    /// home, no network). Errors on other platforms.
    pub sandbox: bool,
}

impl Default for QuarantineOptions {
    fn default() -> Self {
        Self {
            registry: RegistryMode::CloneOrEmpty,
            sandbox: false,
        }
    }
}

/// What an arm is and is not isolated from, recorded with its results
/// (Rule 3: a report never implies isolation that was not set up).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolationReport {
    pub source: String,
    pub environment: String,
    pub home: String,
    pub cargo_home: String,
    pub cargo_config: String,
    pub registry: String,
    pub toolchain: String,
    pub git: String,
    pub process: String,
    pub filesystem: String,
    pub network: String,
}

/// The per-run toolchain every arm of the run calls (prepared once).
#[derive(Debug, Clone)]
pub struct StagedToolchain {
    /// Directory holding `cargo`, `rustc`, … for the arms' `PATH`.
    pub bin: PathBuf,
    /// Sysroot the arms read (the clone, or the host's).
    pub sysroot: PathBuf,
    /// `clone` or why the host sysroot is used.
    pub isolation: String,
}

/// Stage the toolchain for a run under `run_root/toolchain`: a
/// copy-on-write clone of the host sysroot where supported, otherwise the
/// host sysroot itself (recorded — it is then writable by arm code).
pub fn stage_toolchain(host: &HostToolchain, run_root: &Path) -> Result<StagedToolchain> {
    let dest = run_root.join("toolchain").join("sysroot");
    if dest.join("bin").join(exe("cargo")).is_file() {
        return Ok(StagedToolchain {
            bin: dest.join("bin"),
            sysroot: dest,
            isolation: "copy-on-write clone of the host sysroot (per run)".into(),
        });
    }
    std::fs::create_dir_all(dest.parent().expect("has parent"))?;
    if clone_tree(&host.sysroot, &dest, &host.parent_env) {
        return Ok(StagedToolchain {
            bin: dest.join("bin"),
            sysroot: dest,
            isolation: "copy-on-write clone of the host sysroot (per run)".into(),
        });
    }
    Ok(StagedToolchain {
        bin: host.sysroot.join("bin"),
        sysroot: host.sysroot.clone(),
        isolation: format!(
            "host sysroot {} (no copy-on-write clone on this filesystem: arm code \
             running as this user could modify it)",
            host.sysroot.display()
        ),
    })
}

/// Copy-on-write clone of directory `src` to `dest` (`cp -cR` on macOS,
/// `cp -R --reflink=always` elsewhere). `false` (and nothing left at
/// `dest`) when the filesystem cannot clone — never a full byte copy of a
/// multi-gigabyte tree.
pub fn clone_tree(src: &Path, dest: &Path, parent_env: &[(OsString, OsString)]) -> bool {
    if !src.is_dir() || dest.exists() {
        return false;
    }
    let tmp = dest.with_extension("cloning");
    let _ = std::fs::remove_dir_all(&tmp);
    let mut cmd = std::process::Command::new("cp");
    cmd.env_clear();
    for (k, v) in parent_env {
        if k == "PATH" {
            cmd.env(k, v);
        }
    }
    if cfg!(target_os = "macos") {
        cmd.arg("-cR");
    } else {
        cmd.args(["-R", "--reflink=always"]);
    }
    let ok = cmd
        .arg(src)
        .arg(&tmp)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok || std::fs::rename(&tmp, dest).is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
        return false;
    }
    true
}

/// Output of one quarantined process.
#[derive(Debug, Clone)]
pub struct QuarantinedOutput {
    pub program: String,
    pub args: Vec<String>,
    /// Exit code 0 and no descendant had to be killed.
    pub success: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// Descendants outlived the direct child and were killed with the group.
    pub killed_descendants: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Measured wall time of the process (spawn to exit).
    pub wall: Duration,
}

/// One arm's quarantine: a source snapshot plus private homes, run
/// environment and (optionally) a sandbox profile. See the module docs.
#[derive(Debug, Clone)]
pub struct ArmQuarantine {
    /// `…/sw-evolve-arm-<id>`: everything the arm owns lives under it.
    pub root: PathBuf,
    /// The source snapshot (working directory of every arm process).
    pub worktree: PathBuf,
    pub home: PathBuf,
    pub cargo_home: PathBuf,
    pub rustup_home: PathBuf,
    pub target_dir: PathBuf,
    pub tmp: PathBuf,
    env: Vec<(OsString, OsString)>,
    sandbox_profile: Option<String>,
    offline: bool,
    git: ArmGit,
    pub isolation: IsolationReport,
}

/// `path` with its longest existing ancestor canonicalized (symlinks such
/// as macOS `/var` -> `/private/var` resolved) and the rest appended.
fn canonical_lossy(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        if let Ok(c) = std::fs::canonicalize(&cur) {
            return rest
                .iter()
                .rev()
                .fold(c, |acc: PathBuf, part| acc.join(part));
        }
        match (cur.file_name().map(|n| n.to_os_string()), cur.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                cur = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Whether `child` is `parent` or inside it, comparing resolved paths even
/// when `child` does not exist yet.
fn is_within(child: &Path, parent: &Path) -> bool {
    canonical_lossy(child).starts_with(canonical_lossy(parent))
}

impl ArmQuarantine {
    /// Prepare the arm directory `root` (must not exist yet; must not be
    /// inside `repo` or the operator's cargo home): export `rev` of `repo`
    /// into `root/worktree`, create the private homes, copy the cargo
    /// config, clone the registry, and build the run environment.
    pub fn prepare(
        root: &Path,
        repo: &Path,
        rev: &str,
        host: &HostToolchain,
        toolchain: &StagedToolchain,
        options: &QuarantineOptions,
    ) -> Result<Self> {
        if options.sandbox && !cfg!(target_os = "macos") {
            bail!(
                "the arm sandbox is implemented with macOS sandbox-exec only; \
                 on this platform arms run with environment/home isolation but no \
                 filesystem sandbox (run without --arm-sandbox to accept that)"
            );
        }
        if root.exists() {
            bail!("arm directory {} already exists", root.display());
        }
        if let Some(parent) = root.parent() {
            if is_within(parent, repo) {
                bail!(
                    "arm directory {} is inside the repository {}: cargo would treat \
                     the arm as part of the project's workspace and git discovery \
                     would reach the operator's repository",
                    root.display(),
                    repo.display()
                );
            }
            if is_within(parent, &host.cargo_home) || is_within(parent, &host.home.join(".rustup"))
            {
                bail!(
                    "arm directory {} is inside the operator's cargo/rustup home",
                    root.display()
                );
            }
        }
        std::fs::create_dir_all(root)?;
        let root = std::fs::canonicalize(root)?;
        let worktree = root.join("worktree");
        let home = root.join("home");
        let cargo_home = root.join("cargo-home");
        let rustup_home = root.join("rustup-home");
        let target_dir = root.join("target");
        let tmp = root.join("tmp");
        for dir in [
            &worktree,
            &home,
            &home.join(".config"),
            &home.join(".cache"),
            &home.join(".local/share"),
            &cargo_home,
            &rustup_home,
            &tmp,
        ] {
            std::fs::create_dir_all(dir)?;
        }

        export_snapshot(repo, rev, &root, &worktree, &host.parent_env)?;

        let cargo_config = copy_cargo_config(&host.cargo_home, &cargo_home)?;
        let registry = match options.registry {
            RegistryMode::Empty => "empty (cargo downloads dependencies)".to_string(),
            RegistryMode::CloneOrEmpty => {
                let mut cloned = Vec::new();
                for cache in ["registry", "git"] {
                    let src = host.cargo_home.join(cache);
                    if src.is_dir() && clone_tree(&src, &cargo_home.join(cache), &host.parent_env) {
                        cloned.push(cache);
                    }
                }
                if cloned.is_empty() {
                    "empty (no copy-on-write clone here; cargo downloads dependencies)".to_string()
                } else {
                    format!(
                        "copy-on-write clone of the operator's cargo {} (writes stay in the arm)",
                        cloned.join(" + ")
                    )
                }
            }
        };
        let offline = options.sandbox;

        let system_dirs: Vec<PathBuf> = SYSTEM_PATH.iter().map(PathBuf::from).collect();
        let operator_dirs: Vec<PathBuf> = host
            .parent_env
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| std::env::split_paths(v).collect())
            .unwrap_or_default();
        let probe_env: Vec<(OsString, OsString)> = vec![
            (
                "PATH".into(),
                std::env::join_paths(&system_dirs).context("building the probe PATH")?,
            ),
            ("HOME".into(), home.clone().into()),
        ];
        let arm_git = select_arm_git(&system_dirs, &operator_dirs, &host.home, |g| {
            git_version(g, &probe_env)
        });
        let mut path_dirs = vec![toolchain.bin.clone()];
        if let ArmGit::Replaced { path, .. } = &arm_git {
            let bin = root.join("bin");
            write_git_wrapper(&bin, path)?;
            path_dirs.insert(0, bin);
        }
        path_dirs.extend(system_dirs);
        let path = std::env::join_paths(&path_dirs).context("building the arm PATH")?;

        // Sanitized base (the shared allowlist, from the explicit parent env),
        // then every location the allowlist would have inherited from the
        // operator replaced with the arm's own.
        let mut env = sanitized_env_from(&[], host.parent_env.iter().cloned());
        let null = OsString::from(NULL_DEVICE);
        let overrides: Vec<(&str, OsString)> = vec![
            ("PATH", path),
            ("HOME", home.clone().into()),
            ("XDG_CONFIG_HOME", home.join(".config").into()),
            ("XDG_CACHE_HOME", home.join(".cache").into()),
            ("XDG_DATA_HOME", home.join(".local/share").into()),
            ("CARGO_HOME", cargo_home.clone().into()),
            ("RUSTUP_HOME", rustup_home.clone().into()),
            ("CARGO_TARGET_DIR", target_dir.clone().into()),
            ("TMPDIR", tmp.clone().into()),
            ("TMP", tmp.clone().into()),
            ("TEMP", tmp.clone().into()),
            ("CARGO_TERM_COLOR", "never".into()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("GIT_CONFIG_GLOBAL", null),
            ("GIT_TERMINAL_PROMPT", "0".into()),
            ("GIT_CEILING_DIRECTORIES", root.clone().into()),
        ];
        for (k, v) in overrides {
            env.retain(|(ek, _)| ek != k);
            env.push((k.into(), v));
        }
        for (k, v) in crate::safety::git_exec::quarantine_git_env(&worktree) {
            env.retain(|(ek, _)| *ek != *k);
            env.push((k.into(), v.into()));
        }
        if offline {
            env.push(("CARGO_NET_OFFLINE".into(), "true".into()));
        }

        let sandbox_profile = options
            .sandbox
            .then(|| sandbox_profile(&root, &host.home, &toolchain.sysroot));

        let isolation = IsolationReport {
            source: format!(
                "snapshot of `{rev}` exported with git archive (no .git; not linked to \
                 the operator's repository)"
            ),
            environment: "sanitized allowlist (no API keys/tokens); PATH = toolchain bin + \
                          /usr/bin:/bin:/usr/sbin:/sbin"
                .into(),
            home: "private HOME, XDG_* and TMPDIR inside the arm".into(),
            cargo_home: "private CARGO_HOME inside the arm (no credentials.toml)".into(),
            cargo_config,
            registry,
            toolchain: format!(
                "{}; private empty RUSTUP_HOME (no rustup proxy runs)",
                toolchain.isolation
            ),
            git: format!(
                "GIT_CONFIG_* neutralisation (fsmonitor/hooks/pager/external diff off), \
                 no .gitattributes drivers (GIT_ATTR_SOURCE, git 2.40+), system and global \
                 config off, discovery ceiling at the arm root; {}",
                arm_git.describe()
            ),
            process: "own process group; the group is SIGKILLed on timeout, cancel or \
                      leftover descendants (a setsid() escapes it)"
                .into(),
            filesystem: if options.sandbox {
                "sandbox-exec: writes only inside the arm (plus /dev nodes), no reads \
                 of the operator's home"
                    .into()
            } else {
                "NOT sandboxed: code writing to an absolute path outside the arm runs \
                 with the operator's permissions"
                    .into()
            },
            network: if options.sandbox {
                "denied (sandbox-exec); cargo runs offline".into()
            } else {
                "NOT restricted".into()
            },
        };

        Ok(Self {
            root,
            worktree,
            home,
            cargo_home,
            rustup_home,
            target_dir,
            tmp,
            env,
            sandbox_profile,
            offline,
            git: arm_git,
            isolation,
        })
    }

    /// Which `git` the arm's processes run (see [`select_arm_git`]).
    pub fn git(&self) -> &ArmGit {
        &self.git
    }

    /// Whether cargo runs offline in this arm (sandboxed arms).
    pub fn offline(&self) -> bool {
        self.offline
    }

    /// The environment every arm process gets (for inspection and tests).
    pub fn env(&self) -> &[(OsString, OsString)] {
        &self.env
    }

    /// A command for `program` in the arm: quarantine environment, the
    /// snapshot as working directory, wrapped in `sandbox-exec` when the
    /// sandbox is on. Run it with [`ArmQuarantine::run`] (process group).
    pub fn command(&self, program: &str, args: &[&str]) -> tokio::process::Command {
        let mut cmd = match &self.sandbox_profile {
            Some(profile) => {
                let mut c = tokio::process::Command::new("/usr/bin/sandbox-exec");
                c.arg("-p").arg(profile).arg(program);
                c
            }
            None => tokio::process::Command::new(program),
        };
        cmd.env_clear();
        cmd.envs(self.env.iter().map(|(k, v)| (k, v)));
        cmd.args(args)
            .current_dir(&self.worktree)
            .stdin(std::process::Stdio::null());
        cmd
    }

    /// Run `program args` in the arm with a wall-clock `timeout`: own process
    /// group, group SIGKILL on timeout / drop, output capped at
    /// `max_output_bytes` per stream.
    pub async fn run(
        &self,
        program: &str,
        args: &[&str],
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Result<QuarantinedOutput> {
        use crate::tools::process_guard::{run_command_bounded, CommandRunError};
        let cmd = self.command(program, args);
        let started = Instant::now();
        let result = run_command_bounded(cmd, timeout, max_output_bytes).await;
        let wall = started.elapsed();
        let base = |success, exit_code, timed_out, killed, stdout, stderr| QuarantinedOutput {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            success,
            exit_code,
            timed_out,
            killed_descendants: killed,
            stdout,
            stderr,
            wall,
        };
        match result {
            Ok(out) => Ok(base(
                out.success(),
                out.status.code(),
                false,
                out.killed_descendants,
                out.stdout,
                out.stderr,
            )),
            Err(CommandRunError::Timeout(_)) => {
                Ok(base(false, None, true, true, Vec::new(), Vec::new()))
            }
            Err(e) => Err(anyhow::anyhow!(
                "{program} in arm {}: {e}",
                self.root.display()
            )),
        }
    }

    /// Remove the arm directory (snapshot, homes, target).
    pub fn remove(self) -> Result<()> {
        make_writable(&self.root);
        std::fs::remove_dir_all(&self.root)
            .with_context(|| format!("removing arm {}", self.root.display()))
    }
}

/// Owner-writable again, so a tree whose permissions the arm changed can be
/// removed.
/// `bin/git` in the arm: a shell script that execs `target` (a wrapper, not
/// a symlink, so git finds its own exec-path from its real location).
#[cfg(unix)]
fn write_git_wrapper(bin: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(bin)?;
    let quoted = target.to_string_lossy().replace('\'', r"'\''");
    let git = bin.join("git");
    std::fs::write(&git, format!("#!/bin/sh\nexec '{quoted}' \"$@\"\n"))?;
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_git_wrapper(_bin: &Path, _target: &Path) -> Result<()> {
    bail!("the arm git wrapper is a POSIX shell script (unix only)")
}

fn make_writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for entry in walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_symlink() {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                let mut perms = meta.permissions();
                perms.set_mode(perms.mode() | 0o200);
                let _ = std::fs::set_permissions(entry.path(), perms);
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Export `rev` of `repo` into `dest` (`git archive` through the hardened
/// git spawn, unpacked with `tar`). Runs before any arm code exists.
fn export_snapshot(
    repo: &Path,
    rev: &str,
    root: &Path,
    dest: &Path,
    parent_env: &[(OsString, OsString)],
) -> Result<()> {
    if rev.starts_with('-') {
        bail!("revision `{rev}` looks like an option");
    }
    let archive = root.join("base.tar");
    let out =
        crate::safety::git_exec::git_command(repo, crate::safety::git_exec::GitScope::Internal)
            .args(["archive", "--format=tar", "-o"])
            .arg(&archive)
            .arg(rev)
            .stdin(std::process::Stdio::null())
            .output()
            .context("running git archive")?;
    if !out.status.success() {
        bail!(
            "git archive {rev} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let mut tar = std::process::Command::new("tar");
    tar.env_clear();
    for (k, v) in parent_env {
        if k == "PATH" {
            tar.env(k, v);
        }
    }
    let status = tar
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(dest)
        .stdin(std::process::Stdio::null())
        .status()
        .context("running tar")?;
    let _ = std::fs::remove_file(&archive);
    if !status.success() {
        bail!("unpacking the snapshot of {rev} failed");
    }
    Ok(())
}

/// Cargo config keys never copied into an arm: credentials, and settings
/// that run or reach host processes outside the quarantine.
const DROPPED_CARGO_KEYS: &[&str] = &[
    "registry.token",
    "registry.credential-provider",
    "registry.global-credential-providers",
    "build.rustc-wrapper",
    "build.rustc-workspace-wrapper",
    "build.target-dir",
    "env",
];

/// Copy the operator's `config.toml` (or legacy `config`) into the arm's
/// cargo home minus [`DROPPED_CARGO_KEYS`] and every `registries.*.token` /
/// `credential-provider`; returns what was done, for the isolation report.
fn copy_cargo_config(host_cargo_home: &Path, arm_cargo_home: &Path) -> Result<String> {
    let Some(src) = ["config.toml", "config"]
        .iter()
        .map(|n| host_cargo_home.join(n))
        .find(|p| p.is_file())
    else {
        return Ok("no operator cargo config to copy".into());
    };
    let text =
        std::fs::read_to_string(&src).with_context(|| format!("reading {}", src.display()))?;
    let mut table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(_) => {
            return Ok(format!(
                "operator cargo config {} is not valid TOML: not copied",
                src.display()
            ))
        }
    };
    let mut dropped = Vec::new();
    for key in DROPPED_CARGO_KEYS {
        let mut parts = key.split('.');
        let first = parts.next().unwrap_or_default();
        match parts.next() {
            None => {
                if table.remove(first).is_some() {
                    dropped.push(key.to_string());
                }
            }
            Some(second) => {
                if let Some(toml::Value::Table(t)) = table.get_mut(first) {
                    if t.remove(second).is_some() {
                        dropped.push(key.to_string());
                    }
                }
            }
        }
    }
    if let Some(toml::Value::Table(regs)) = table.get_mut("registries") {
        for (name, reg) in regs.iter_mut() {
            if let toml::Value::Table(r) = reg {
                for k in ["token", "credential-provider"] {
                    if r.remove(k).is_some() {
                        dropped.push(format!("registries.{name}.{k}"));
                    }
                }
            }
        }
    }
    let rendered = toml::to_string(&table).context("re-serializing the cargo config")?;
    std::fs::write(arm_cargo_home.join("config.toml"), rendered)?;
    Ok(if dropped.is_empty() {
        "operator config.toml copied (no credential or host-process keys found)".into()
    } else {
        format!(
            "operator config.toml copied without: {}",
            dropped.join(", ")
        )
    })
}

/// Escape a path for an SBPL string literal.
fn sbpl_str(path: &Path) -> String {
    let s = path.to_string_lossy();
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The macOS sandbox profile for an arm rooted at `root` (canonical path).
/// Later rules win in SBPL, so each deny is followed by its exceptions.
pub fn sandbox_profile(root: &Path, operator_home: &Path, sysroot: &Path) -> String {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let root = sbpl_str(&canon(root));
    let home = sbpl_str(&canon(operator_home));
    let sysroot = sbpl_str(&canon(sysroot));
    format!(
        "(version 1)\n\
         (allow default)\n\
         (deny network*)\n\
         (deny file-write*)\n\
         (allow file-write* (subpath {root}) \
         (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/tty\") \
         (literal \"/dev/dtracehelper\") (regex #\"^/dev/fd/\"))\n\
         (deny file-read* (subpath {home}))\n\
         (allow file-read* (subpath {root}) (subpath {sysroot}))\n"
    )
}

#[cfg(test)]
#[path = "../../tests/unit/safety/quarantine/quarantine_test.rs"]
mod tests;
