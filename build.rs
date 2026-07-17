//! Embed the git commit SHA so two builds never report the same version
//! string (HARNESS_REVIEW P2-2: a stale installed binary silently lacking
//! security fixes must be distinguishable from a fresh one).
//!
//! Emits:
//! - `cargo:rustc-env=SELFWARE_GIT_SHA=<short sha>` — "unknown" outside a git
//!   checkout (packaged .crate / crates.io-style installs), so
//!   `env!("SELFWARE_GIT_SHA")` always compiles.
//! - When the SHA is known, `cargo:rustc-env=CARGO_PKG_VERSION=<version>+g<sha>`.
//!   The suffix is semver build metadata: it does not affect precedence, and
//!   `cargo metadata` still reports the plain manifest version. clap's derived
//!   `--version` (src/cli/args.rs uses bare `#[command(version)]`) and the
//!   MCP/LSP version fields all read `env!("CARGO_PKG_VERSION")`, so they pick
//!   up the suffix with no source changes.
//!
//! SHA resolution order: `SELFWARE_GIT_SHA` env var (CI/Docker builds without
//! `.git`) → `git rev-parse --short HEAD` → "unknown".

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=SELFWARE_GIT_SHA");

    let sha = env::var("SELFWARE_GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(git_sha)
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=SELFWARE_GIT_SHA={sha}");

    if sha != "unknown" {
        let version = env::var("CARGO_PKG_VERSION").expect("cargo always sets CARGO_PKG_VERSION");
        println!("cargo:rustc-env=CARGO_PKG_VERSION={version}+g{sha}");
    }
}

/// Short SHA of HEAD, wiring change-tracking first so a commit, branch
/// switch, or ref update re-runs this script. None when not in a git checkout.
fn git_sha() -> Option<String> {
    let git_dir = Path::new(".git");
    if !git_dir.is_dir() {
        // Packaged crate, or a worktree where `.git` is a file: nothing
        // reliable to watch — resolve the SHA but skip rerun-if-changed.
        return rev_parse_short_head();
    }

    let head = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());
    if let Ok(contents) = std::fs::read_to_string(&head) {
        if let Some(reference) = contents.strip_prefix("ref:") {
            // Branch checkout: also watch the ref HEAD points at. It may only
            // exist inside packed-refs; watch that as the fallback.
            let ref_file = git_dir.join(reference.trim());
            if ref_file.exists() {
                println!("cargo:rerun-if-changed={}", ref_file.display());
            } else {
                let packed = git_dir.join("packed-refs");
                if packed.exists() {
                    println!("cargo:rerun-if-changed={}", packed.display());
                }
            }
        }
        // Detached HEAD: HEAD itself is rewritten on every commit, so
        // watching it above already covers new commits.
    }

    rev_parse_short_head()
}

fn rev_parse_short_head() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}
