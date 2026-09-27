"""Build a selfware binary at a given commit, without touching the source.

The source (a worktree such as the integration worktree) is only READ: a
shared clone at <results>/build-src borrows its objects (`git clone
--shared`), fetches the wanted commit from it and checks it out there, so
the source's working tree, index and refs are never modified. Every build
reuses one CARGO_TARGET_DIR and the same checkout path, so rebuilds are
incremental. The binary is copied to <results>/bin/selfware-<sha12> so a
running scenario keeps its binary while the next one builds.
"""

import os
import shutil
import subprocess
from pathlib import Path

from harness import Binary, git

PROFILE = "release-fast"


class BuildError(Exception):
    pass


def resolve_rev(source, rev):
    try:
        return git("rev-parse", "--verify", f"{rev}^{{commit}}", cwd=source)
    except subprocess.CalledProcessError as exc:
        raise BuildError(f"cannot resolve {rev} in {source}: {exc.stderr.strip()[:200]}")


def build_checkout(results_dir, source, sha):
    """The shared clone at <results>/build-src, checked out at `sha`."""
    dest = Path(results_dir) / "build-src"
    if not (dest / ".git").exists():
        subprocess.run(
            ["git", "clone", "-q", "--shared", "--no-checkout", str(source), str(dest)],
            check=True, capture_output=True, text=True,
        )
    if subprocess.run(
        ["git", "cat-file", "-e", f"{sha}^{{commit}}"], cwd=dest, capture_output=True
    ).returncode != 0:
        # The shared clone normally sees new commits through its alternates;
        # fetch the source's branches only when that is not enough.
        subprocess.run(
            ["git", "fetch", "-q", str(source), "+refs/heads/*:refs/remotes/source/*"],
            cwd=dest, check=True, capture_output=True, text=True,
        )
    subprocess.run(
        ["git", "checkout", "-q", "--force", "--detach", sha], cwd=dest, check=True,
        capture_output=True, text=True,
    )
    subprocess.run(["git", "clean", "-qfdx"], cwd=dest, check=True, capture_output=True)
    return dest


def build(results_dir, source, rev, target_dir, log=print, keep_bins=3, protect=()):
    """Build `rev` of `source`; returns a Binary. Raises BuildError."""
    sha = resolve_rev(source, rev)
    bin_dir = Path(results_dir) / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    dest_bin = bin_dir / f"selfware-{sha[:12]}"
    try:
        checkout = build_checkout(results_dir, source, sha)
    except subprocess.CalledProcessError as exc:
        raise BuildError(f"checkout of {sha[:12]} failed: {(exc.stderr or '').strip()[:300]}")
    if dest_bin.exists():
        return Binary(dest_bin, sha, checkout)
    log(f"[build] {sha[:12]} from {source} (profile {PROFILE}, target {target_dir})")
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir), CARGO_TERM_COLOR="never")
    log_path = Path(results_dir) / "build.log"
    with open(log_path, "w") as fh:
        res = subprocess.run(
            ["cargo", "build", "--profile", PROFILE, "--bin", "selfware"], cwd=checkout, env=env,
            stdout=fh, stderr=subprocess.STDOUT,
        )
    if res.returncode != 0:
        tail = log_path.read_text(errors="replace")[-600:]
        raise BuildError(f"cargo build of {sha[:12]} failed (exit {res.returncode}): {tail}")
    built = Path(target_dir) / PROFILE / "selfware"
    tmp = dest_bin.with_suffix(".tmp")
    shutil.copy2(built, tmp)
    os.replace(tmp, dest_bin)
    prune_bins(bin_dir, keep_bins, protect=set(protect) | {str(dest_bin)})
    return Binary(dest_bin, sha, checkout)


def prune_bins(bin_dir, keep, protect=()):
    bins = sorted(
        (p for p in Path(bin_dir).glob("selfware-*") if p.suffix != ".tmp"),
        key=lambda p: p.stat().st_mtime,
    )
    while len(bins) > keep:
        victim = bins.pop(0)
        if str(victim) in protect:
            continue
        victim.unlink(missing_ok=True)
