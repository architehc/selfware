//! Defensive regression tests for the arm quarantine. Everything lives under
//! a `TempDir` the test creates: the "operator home" an arm must not reach
//! is a temp directory passed as the would-be host HOME, the "secret" is a
//! dummy canary variable on a synthetic parent environment, the malicious
//! git repository is the shared `git_exec` fixture (its hook only appends to
//! a marker file in its own temp dir), and each candidate is a minimal
//! generated Cargo project without dependencies. The only real host path
//! used is the Rust toolchain sysroot, to run the compiler.

use super::*;
use std::process::Command as StdCommand;

const CANARY_KEY: &str = "SELFWARE_TEST_CANARY";
const CANARY: &str = "canary-not-a-secret";
const TOKEN_CANARY: &str = "canary-token-not-a-secret";

struct Fixture {
    tmp: tempfile::TempDir,
    host: HostToolchain,
    toolchain: StagedToolchain,
}

impl Fixture {
    fn fake_home(&self) -> PathBuf {
        self.tmp.path().join("host-home")
    }
    fn arms(&self) -> PathBuf {
        self.tmp.path().join("arms")
    }
}

fn test_sysroot() -> PathBuf {
    let out = StdCommand::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("rustc on PATH while running cargo test");
    PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let fake_home = tmp.path().join("host-home");
    let fake_cargo = fake_home.join(".cargo");
    std::fs::create_dir_all(&fake_cargo).unwrap();
    std::fs::create_dir_all(tmp.path().join("arms")).unwrap();
    let sysroot = test_sysroot();
    let parent_env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), std::env::var_os("PATH").unwrap_or_default()),
        ("HOME".into(), fake_home.clone().into()),
        ("CARGO_HOME".into(), fake_cargo.clone().into()),
        ("LANG".into(), "C".into()),
        (CANARY_KEY.into(), CANARY.into()),
        ("GITHUB_TOKEN".into(), TOKEN_CANARY.into()),
    ];
    Fixture {
        host: HostToolchain {
            home: fake_home,
            cargo_home: fake_cargo,
            sysroot: sysroot.clone(),
            parent_env,
        },
        toolchain: StagedToolchain {
            bin: sysroot.join("bin"),
            sysroot,
            isolation: "test: host sysroot, not cloned".into(),
        },
        tmp,
    }
}

fn git_in(dir: &Path, args: &[&str]) {
    let ok = StdCommand::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

/// A committed repository holding a minimal crate `cand` with `build_rs`
/// (if any) and `lib_rs`.
fn candidate_repo(fx: &Fixture, build_rs: Option<&str>, lib_rs: &str) -> PathBuf {
    let repo = fx.tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname = \"cand\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(repo.join("src/lib.rs"), lib_rs).unwrap();
    if let Some(b) = build_rs {
        std::fs::write(repo.join("build.rs"), b).unwrap();
    }
    git_in(&repo, &["init", "-q"]);
    git_in(&repo, &["add", "-A"]);
    git_in(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "base",
        ],
    );
    repo
}

fn prepare(fx: &Fixture, repo: &Path, name: &str, sandbox: bool) -> ArmQuarantine {
    ArmQuarantine::prepare(
        &fx.arms().join(name),
        repo,
        "HEAD",
        &fx.host,
        &fx.toolchain,
        &QuarantineOptions {
            registry: RegistryMode::Empty,
            sandbox,
        },
    )
    .expect("prepare arm")
}

const LIB_OK: &str =
    "pub fn one() -> u8 { 1 }\n#[test]\nfn one_is_one() { assert_eq!(one(), 1); }\n";

async fn cargo(arm: &ArmQuarantine, args: &[&str]) -> QuarantinedOutput {
    arm.run("cargo", args, Duration::from_secs(300), 1 << 20)
        .await
        .expect("spawn cargo in arm")
}

fn assert_ok(out: &QuarantinedOutput) {
    assert!(
        out.success,
        "{} {:?} failed:\n{}",
        out.program,
        out.args,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn arm_environment_replaces_every_host_location_and_drops_secrets() {
    let fx = fixture();
    let repo = candidate_repo(&fx, None, LIB_OK);
    let arm = prepare(&fx, &repo, "arm-env", false);
    let get = |k: &str| {
        arm.env()
            .iter()
            .find(|(ek, _)| ek == k)
            .map(|(_, v)| PathBuf::from(v))
    };
    assert_eq!(get("HOME"), Some(arm.home.clone()));
    assert_eq!(get("CARGO_HOME"), Some(arm.cargo_home.clone()));
    assert_eq!(get("RUSTUP_HOME"), Some(arm.rustup_home.clone()));
    assert_eq!(get("CARGO_TARGET_DIR"), Some(arm.target_dir.clone()));
    assert_eq!(get("TMPDIR"), Some(arm.tmp.clone()));
    assert_eq!(get("GIT_CEILING_DIRECTORIES"), Some(arm.root.clone()));
    assert_eq!(get("GIT_CONFIG_GLOBAL"), Some(PathBuf::from("/dev/null")));
    assert!(get("GIT_CONFIG_COUNT").is_some());
    assert!(
        get("GIT_ATTR_SOURCE").is_some(),
        "no .gitattributes drivers"
    );
    let path = get("PATH").unwrap();
    let dirs: Vec<PathBuf> = std::env::split_paths(path.as_os_str()).collect();
    assert_eq!(dirs[0], fx.toolchain.bin);
    assert_eq!(
        dirs[1..].to_vec(),
        SYSTEM_PATH.iter().map(PathBuf::from).collect::<Vec<_>>()
    );
    for (k, v) in arm.env() {
        let v = v.to_string_lossy();
        assert!(
            !v.contains(CANARY) && !v.contains(TOKEN_CANARY),
            "{k:?} leaks a canary"
        );
        assert!(
            !v.contains(fx.fake_home().to_str().unwrap()),
            "{k:?} points into the host home: {v}"
        );
    }
    assert!(arm.worktree.join("Cargo.toml").is_file());
    assert!(
        !arm.worktree.join(".git").exists(),
        "snapshot, not a worktree"
    );
    assert!(arm.isolation.filesystem.contains("NOT sandboxed"));
}

#[test]
fn cargo_config_is_copied_without_credentials_or_host_wrappers() {
    let fx = fixture();
    std::fs::write(
        fx.host.cargo_home.join("config.toml"),
        "[registry]\ntoken = \"dummy-token\"\n\n[registries.corp]\nindex = \"https://example.invalid/index\"\ntoken = \"dummy-corp-token\"\n\n[build]\nrustc-wrapper = \"sccache\"\njobs = 2\n\n[env]\nDUMMY = \"x\"\n\n[net]\nretry = 1\n",
    )
    .unwrap();
    std::fs::write(
        fx.host.cargo_home.join("credentials.toml"),
        "[registry]\ntoken = \"dummy-token\"\n",
    )
    .unwrap();
    let repo = candidate_repo(&fx, None, LIB_OK);
    let arm = prepare(&fx, &repo, "arm-config", false);
    let copied = std::fs::read_to_string(arm.cargo_home.join("config.toml")).unwrap();
    assert!(!copied.contains("dummy"), "{copied}");
    assert!(
        !copied.contains("sccache") && !copied.contains("[env]"),
        "{copied}"
    );
    assert!(
        copied.contains("jobs = 2") && copied.contains("retry = 1"),
        "{copied}"
    );
    assert!(!arm.cargo_home.join("credentials.toml").exists());
    for dropped in [
        "registry.token",
        "build.rustc-wrapper",
        "env",
        "registries.corp.token",
    ] {
        assert!(arm.isolation.cargo_config.contains(dropped), "{dropped}");
    }
}

#[test]
fn arm_directory_inside_the_repository_is_refused() {
    let fx = fixture();
    let repo = candidate_repo(&fx, None, LIB_OK);
    let err = ArmQuarantine::prepare(
        &repo.join("arms").join("a"),
        &repo,
        "HEAD",
        &fx.host,
        &fx.toolchain,
        &QuarantineOptions::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("inside the repository"), "{err}");
}

#[cfg(unix)]
#[tokio::test]
async fn a_benign_candidate_builds_and_its_tests_pass() {
    let fx = fixture();
    let repo = candidate_repo(&fx, Some("fn main() {}\n"), LIB_OK);
    let arm = prepare(&fx, &repo, "arm-benign", false);
    let out = cargo(&arm, &["test"]).await;
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("test one_is_one ... ok"), "{stdout}");
    assert!(
        arm.target_dir.join("debug").is_dir(),
        "target stays in the arm"
    );
    assert!(!repo.join("target").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn build_script_writes_to_home_land_in_the_arm_not_the_host_home() {
    let fx = fixture();
    let build = r#"fn main() {
    for var in ["HOME", "CARGO_HOME"] {
        let dir = std::env::var(var).expect(var);
        std::fs::write(std::path::Path::new(&dir).join("ESCAPED"), "x").unwrap();
    }
}
"#;
    let repo = candidate_repo(&fx, Some(build), LIB_OK);
    let arm = prepare(&fx, &repo, "arm-home", false);
    assert_ok(&cargo(&arm, &["build"]).await);
    // The build script ran (its markers exist in the arm's own homes)…
    assert!(arm.home.join("ESCAPED").is_file());
    assert!(arm.cargo_home.join("ESCAPED").is_file());
    // …and nothing reached the would-be host home.
    assert!(!fx.fake_home().join("ESCAPED").exists());
    assert!(!fx.host.cargo_home.join("ESCAPED").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn build_script_cannot_read_a_parent_env_canary() {
    let fx = fixture();
    let build = r#"fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    let seen: String = std::env::vars().map(|(k, v)| format!("{k}={v}\n")).collect();
    std::fs::write(std::path::Path::new(&out).join("env-seen.txt"), seen).unwrap();
}
"#;
    let repo = candidate_repo(&fx, Some(build), LIB_OK);
    let arm = prepare(&fx, &repo, "arm-env-read", false);
    assert_ok(&cargo(&arm, &["build"]).await);
    let seen_file = walkdir::WalkDir::new(&arm.target_dir)
        .into_iter()
        .flatten()
        .find(|e| e.file_name() == "env-seen.txt")
        .expect("the build script ran and wrote what it saw")
        .into_path();
    let seen = std::fs::read_to_string(seen_file).unwrap();
    assert!(
        seen.contains(&format!("HOME={}", arm.home.display())),
        "{seen}"
    );
    assert!(
        !seen.contains(CANARY_KEY) && !seen.contains(CANARY),
        "{seen}"
    );
    assert!(!seen.contains(TOKEN_CANARY), "{seen}");
}

#[cfg(unix)]
#[tokio::test]
async fn git_run_by_a_build_script_does_not_execute_a_repository_fsmonitor() {
    let Some((evil, marker)) = crate::safety::git_exec::test_support::malicious_repo() else {
        eprintln!("SKIP: git unavailable for the malicious-repo fixture");
        return;
    };
    let fx = fixture();
    let evil_path = evil.path().display().to_string();
    let build = format!(
        r#"fn main() {{
    let mut ran = String::new();
    for sub in ["status", "diff"] {{
        let st = std::process::Command::new("git")
            .args(["-C", {evil_path:?}, sub])
            .output()
            .map(|o| o.status.success());
        ran.push_str(&format!("{{sub}}={{st:?}}\n"));
    }}
    let home = std::env::var("HOME").unwrap();
    std::fs::write(std::path::Path::new(&home).join("git-ran"), ran).unwrap();
}}
"#
    );
    let repo = candidate_repo(&fx, Some(&build), LIB_OK);
    let arm = prepare(&fx, &repo, "arm-git", false);
    if !arm.git().attr_source_honoured() {
        // git < 2.40 everywhere (no GIT_ATTR_SOURCE): the fixture's
        // `.gitattributes`-selected clean filter WOULD run, and the arm's
        // report must say so instead of claiming the protection (Rule 3).
        assert!(
            arm.isolation.git.contains("NOT neutralised"),
            "{}",
            arm.isolation.git
        );
        eprintln!(
            "SKIP: no git 2.40+ on this host, attribute drivers are not neutralised \
             (reported): {}",
            arm.isolation.git
        );
        return;
    }
    assert_ok(&cargo(&arm, &["build"]).await);
    let ran = std::fs::read_to_string(arm.home.join("git-ran")).unwrap();
    assert!(ran.contains("status=Ok(true)"), "git really ran: {ran}");
    assert!(
        !marker.exists(),
        "a repository-configured program ran inside the arm: {:?}",
        std::fs::read_to_string(&marker)
    );

    // Control: the same `git status` without the quarantine environment runs
    // the fixture's fsmonitor, so the absence above is the quarantine's doing.
    let _ = StdCommand::new("git")
        .args(["-C", &evil_path, "status"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output();
    assert!(
        marker.exists(),
        "fixture hook is live for an unhardened git"
    );
}

/// Documents the limit stated in the module docs and every
/// `IsolationReport`: without the sandbox, a write to an ABSOLUTE path
/// outside the arm succeeds. With the macOS sandbox it does not.
#[cfg(unix)]
#[tokio::test]
async fn absolute_path_writes_are_contained_only_by_the_sandbox() {
    let fx = fixture();
    let target = fx.fake_home().join("ESCAPED");
    let build = format!(
        "fn main() {{ let _ = std::fs::write({:?}, \"x\"); }}\n",
        target.display().to_string()
    );
    let repo = candidate_repo(&fx, Some(&build), LIB_OK);

    let open_arm = prepare(&fx, &repo, "arm-open", false);
    assert_ok(&cargo(&open_arm, &["build"]).await);
    assert!(
        target.exists(),
        "no filesystem sandbox by default (documented)"
    );
    std::fs::remove_file(&target).unwrap();

    if !cfg!(target_os = "macos") {
        return;
    }
    let nested_ok = StdCommand::new("/usr/bin/sandbox-exec")
        .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
        .status()
        .is_ok_and(|s| s.success());
    if !nested_ok {
        eprintln!("SKIP sandbox half: sandbox-exec cannot apply a profile here (nested sandbox)");
        return;
    }
    let boxed = prepare(&fx, &repo, "arm-sandboxed", true);
    assert!(boxed.offline());
    assert!(boxed.isolation.filesystem.contains("sandbox-exec"));
    let out = cargo(&boxed, &["test"]).await;
    assert_ok(&out);
    assert!(!target.exists(), "the sandbox confined the write");
}

#[cfg(target_os = "linux")]
#[test]
fn sandbox_request_on_linux_is_an_error_not_a_silent_no_op() {
    let fx = fixture();
    let repo = candidate_repo(&fx, None, LIB_OK);
    let err = ArmQuarantine::prepare(
        &fx.arms().join("a"),
        &repo,
        "HEAD",
        &fx.host,
        &fx.toolchain,
        &QuarantineOptions {
            registry: RegistryMode::Empty,
            sandbox: true,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("sandbox-exec"), "{err}");
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_kills_the_whole_process_group() {
    let fx = fixture();
    let repo = candidate_repo(&fx, None, LIB_OK);
    let arm = prepare(&fx, &repo, "arm-timeout", false);
    let out = arm
        .run(
            "sh",
            &["-c", "sleep 30 & echo $! > \"$HOME/pid\"; wait"],
            Duration::from_secs(1),
            4096,
        )
        .await
        .unwrap();
    assert!(out.timed_out && !out.success);
    let pid: i32 = std::fs::read_to_string(arm.home.join("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let pid = nix::unistd::Pid::from_raw(pid);
    let mut gone = false;
    for _ in 0..40 {
        if nix::sys::signal::kill(pid, None).is_err() {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(gone, "the grandchild `sleep` outlived the timeout");
}

#[test]
fn arm_removal_deletes_everything_it_owns() {
    let fx = fixture();
    let repo = candidate_repo(&fx, None, LIB_OK);
    let arm = prepare(&fx, &repo, "arm-rm", false);
    let root = arm.root.clone();
    arm.remove().unwrap();
    assert!(!root.exists());
}

#[test]
fn toolchain_is_cloned_per_run_or_the_shared_host_sysroot_is_recorded() {
    // A stand-in sysroot in the temp dir (not the real toolchain).
    let fx = fixture();
    let fake_sysroot = fx.tmp.path().join("fake-sysroot");
    std::fs::create_dir_all(fake_sysroot.join("bin")).unwrap();
    std::fs::write(fake_sysroot.join("bin").join(exe("cargo")), "stand-in").unwrap();
    let host = HostToolchain {
        sysroot: fake_sysroot.clone(),
        ..fx.host.clone()
    };
    let run_root = fx.tmp.path().join("run");
    let staged = stage_toolchain(&host, &run_root).unwrap();
    if staged.sysroot == fake_sysroot {
        assert!(
            staged.isolation.contains("could modify it"),
            "{}",
            staged.isolation
        );
    } else {
        assert!(staged.sysroot.starts_with(&run_root));
        assert!(staged.isolation.contains("clone"));
        assert!(staged.bin.join(exe("cargo")).is_file());
        // Writes to the clone do not reach the source.
        std::fs::write(staged.bin.join(exe("cargo")), "changed").unwrap();
        assert_eq!(
            std::fs::read_to_string(fake_sysroot.join("bin").join(exe("cargo"))).unwrap(),
            "stand-in"
        );
    }
}

#[test]
fn parse_git_version_reads_apple_homebrew_and_windows_builds() {
    assert_eq!(
        parse_git_version("git version 2.39.3 (Apple Git-146)\n"),
        Some((2, 39))
    );
    assert_eq!(parse_git_version("git version 2.55.0\n"), Some((2, 55)));
    assert_eq!(
        parse_git_version("git version 2.45.2.windows.1"),
        Some((2, 45))
    );
    assert_eq!(parse_git_version("hub version 2.14.2"), None);
    assert_eq!(parse_git_version(""), None);
}

/// macos-14 runners: `/usr/bin/git` (Xcode 15.4) is Apple Git 2.39, which
/// ignores `GIT_ATTR_SOURCE`, while Homebrew's git 2.55 is on the
/// operator's PATH. The arm must run the new one; a git inside the
/// operator's home is never chosen; with no 2.40+ anywhere the report
/// names the gap.
#[cfg(unix)]
#[test]
fn arm_git_replaces_a_system_git_that_ignores_attr_source() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let mk = |dir: &str| {
        let d = tmp.path().join(dir);
        std::fs::create_dir_all(&d).unwrap();
        let g = d.join("git");
        std::fs::write(&g, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&g, std::fs::Permissions::from_mode(0o755)).unwrap();
        d
    };
    let system = mk("usr-bin");
    let brew = mk("opt-homebrew-bin");
    let home = tmp.path().join("home");
    let home_bin = {
        let d = mk("home/.local/bin");
        assert!(d.starts_with(&home));
        d
    };
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let versions = |system_v: Option<(u32, u32)>| {
        let system = system.clone();
        let home_bin = home_bin.clone();
        move |p: &Path| {
            if p.starts_with(&system) {
                system_v
            } else if p.starts_with(&home_bin) {
                Some((2, 99))
            } else {
                Some((2, 55))
            }
        }
    };

    // New enough system git: kept, nothing replaced.
    let g = select_arm_git(
        std::slice::from_ref(&system),
        std::slice::from_ref(&brew),
        &home,
        versions(Some((2, 40))),
    );
    assert!(
        matches!(
            &g,
            ArmGit::System {
                version: (2, 40),
                ..
            }
        ),
        "{g:?}"
    );
    assert!(g.attr_source_honoured());

    // Apple Git 2.39: the operator's Homebrew git replaces it; the home one
    // (listed first, newer) is skipped.
    let g = select_arm_git(
        std::slice::from_ref(&system),
        &[home_bin.clone(), brew.clone()],
        &home,
        versions(Some((2, 39))),
    );
    match &g {
        ArmGit::Replaced {
            path,
            version,
            system_version,
            ..
        } => {
            assert_eq!(path, &brew.join("git"));
            assert_eq!(*version, (2, 55));
            assert_eq!(*system_version, Some((2, 39)));
        }
        other => panic!("expected Replaced, got {other:?}"),
    }
    assert!(g.attr_source_honoured());
    assert!(
        g.describe().contains("ignores GIT_ATTR_SOURCE"),
        "{}",
        g.describe()
    );

    // An unknown system version is treated as too old.
    let g = select_arm_git(
        std::slice::from_ref(&system),
        std::slice::from_ref(&brew),
        &home,
        versions(None),
    );
    assert!(matches!(g, ArmGit::Replaced { .. }), "{g:?}");

    // No 2.40+ outside the operator's home: the gap is reported.
    let g = select_arm_git(
        std::slice::from_ref(&system),
        &[home_bin.clone(), empty.clone()],
        &home,
        versions(Some((2, 39))),
    );
    assert!(matches!(g, ArmGit::Old { .. }), "{g:?}");
    assert!(!g.attr_source_honoured());
    assert!(g.describe().contains("NOT neutralised"), "{}", g.describe());

    // No system git at all: nothing is added to the arm.
    let g = select_arm_git(
        std::slice::from_ref(&empty),
        std::slice::from_ref(&brew),
        &home,
        versions(Some((2, 39))),
    );
    assert_eq!(g, ArmGit::Absent);
}

/// The wrapper the arm runs as `git` execs its target with every argument.
#[cfg(unix)]
#[test]
fn arm_git_wrapper_execs_its_target_with_all_arguments() {
    let tmp = tempfile::tempdir().unwrap();
    let target_dir = tmp.path().join("it's here");
    std::fs::create_dir_all(&target_dir).unwrap();
    let target = target_dir.join("git");
    std::fs::write(&target, "#!/bin/sh\nprintf '%s|' \"$@\"\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let bin = tmp.path().join("bin");
    write_git_wrapper(&bin, &target).unwrap();
    let out = StdCommand::new(bin.join("git"))
        .args(["-C", "a b", "status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "-C|a b|status|");
}
