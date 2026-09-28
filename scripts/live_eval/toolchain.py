"""The harness-owned toolchain an agent under test uses.

An agent runs in yolo mode, so whatever toolchain its shell reaches by
default it can also write to: `pip install --user`, `cargo install`,
`rustup update`. None of that may land on the user's machine, so the agent
never gets the user's Python user site, ~/.cargo or ~/.rustup. Instead,
once per harness install, under `<work root>/.toolchain`:

- `py-venv`: a venv (`python3 -m venv`, no system site packages, so no user
  site either) with the pinned `toolchain-requirements.txt` (pytest and
  what the fixtures import), installed from PyPI or, offline, from
  `$LIVE_EVAL_WHEELHOUSE`. It is made read-only after creation;
- `cargo-home`: config.toml copied from the user's (plus `build.target-dir`
  = `<results>/child-target`), `bin` holding links to the rustup proxies,
  and the registry/git caches as copy-on-write CLONES of the user's
  (`cp -c`, macOS) or empty (cargo downloads them);
- `rustup-home`: a copy-on-write clone of ~/.rustup (`cp -c`); where clones
  are not available the user's RUSTUP_HOME is used and the run records
  `rustup: shared` in `toolchain_isolation` instead of pretending.

selfware's tool spawns keep PATH, HOME, CARGO_HOME and RUSTUP_HOME
(`safety::process_env::DEFAULT_KEEP`), so these reach the agent's shell:
PATH starts with the venv's bin and the harness cargo bin, and the user's
~/.cargo/bin and Python user-base bin are removed from it.

Every run fingerprints the venv and cargo bin before and after; a run that
changed them is marked contaminated and the venv is rebuilt.
"""

import fcntl
import hashlib
import json
import os
import platform
import shutil
import site
import stat
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REQUIREMENTS = HERE / "toolchain-requirements.txt"


class ToolchainError(Exception):
    """The toolchain could not be prepared (recorded as a setup failure)."""


def _real_home():
    return Path(os.path.expanduser("~")).resolve()


def _clone_tree(src, dest):
    """Copy-on-write clone of `src` (macOS `cp -c`); False where unsupported."""
    if platform.system() != "Darwin" or not Path(src).is_dir():
        return False
    tmp = Path(str(dest) + ".tmp")
    shutil.rmtree(tmp, ignore_errors=True)
    res = subprocess.run(["cp", "-cR", str(src), str(tmp)], capture_output=True)
    if res.returncode != 0:
        shutil.rmtree(tmp, ignore_errors=True)
        return False
    os.replace(tmp, dest)
    return True


def _set_writable(path, writable):
    """chmod a tree u+w / a-w (symlinks untouched)."""
    for root, dirs, files in os.walk(path):
        for name in dirs + files:
            p = os.path.join(root, name)
            if os.path.islink(p):
                continue
            mode = os.lstat(p).st_mode
            os.chmod(p, (mode | stat.S_IWUSR) if writable else (mode & ~0o222))
    mode = os.lstat(path).st_mode
    os.chmod(path, (mode | stat.S_IWUSR) if writable else (mode & ~0o222))


def _remove(path):
    if Path(path).exists():
        _set_writable(path, True)
        shutil.rmtree(path, ignore_errors=True)


def fingerprint(paths):
    """Hash of every file's relative path, size and mtime under `paths`."""
    h = hashlib.sha256()
    for base in paths:
        base = Path(base)
        if not base.exists():
            h.update(f"missing:{base.name}".encode())
            continue
        for root, dirs, files in os.walk(base):
            dirs.sort()
            for name in sorted(files):
                p = Path(root, name)
                try:
                    st = p.lstat()
                except OSError:
                    continue
                h.update(f"{p.relative_to(base)}:{st.st_size}:{int(st.st_mtime)}\n".encode())
    return h.hexdigest()


class Toolchain:
    def __init__(self, root, results_dir):
        self.root = Path(root)
        self.results_dir = Path(results_dir)
        self.venv = self.root / "py-venv"
        self.cargo_home = self.root / "cargo-home"
        self.rustup_home = self.root / "rustup-home"
        self.isolation = {}

    # -- python ---------------------------------------------------------------

    @property
    def python(self):
        return self.venv / "bin" / "python3"

    def _requirements_sha(self):
        return hashlib.sha256(REQUIREMENTS.read_bytes()).hexdigest()

    def _stamp(self):
        try:
            return json.loads((self.root / "py-venv.stamp").read_text())
        except (OSError, ValueError):
            return {}

    def watched(self):
        return [self.venv, self.cargo_home / "bin"]

    def ensure_venv(self):
        stamp = self._stamp()
        if (
            self.python.exists()
            and stamp.get("requirements") == self._requirements_sha()
            and stamp.get("fingerprint") == fingerprint([self.venv])
        ):
            return
        _remove(self.venv)
        res = subprocess.run(
            [sys.executable, "-m", "venv", str(self.venv)], capture_output=True, text=True
        )
        if res.returncode != 0:
            raise ToolchainError(f"python3 -m venv failed: {res.stderr.strip()[:300]}")
        cmd = [str(self.python), "-m", "pip", "install", "-q", "--disable-pip-version-check",
               "-r", str(REQUIREMENTS)]
        wheelhouse = os.environ.get("LIVE_EVAL_WHEELHOUSE")
        if wheelhouse:
            cmd += ["--no-index", "--find-links", wheelhouse]
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=900)
        if res.returncode != 0:
            _remove(self.venv)
            raise ToolchainError(f"pip install of the toolchain failed: {res.stderr.strip()[-400:]}")
        _set_writable(self.venv, False)
        (self.root / "py-venv.stamp").write_text(json.dumps({
            "requirements": self._requirements_sha(),
            "fingerprint": fingerprint([self.venv]),
        }))

    # -- rust -----------------------------------------------------------------

    def ensure_cargo(self):
        real_cargo = Path(os.environ.get("CARGO_HOME") or _real_home() / ".cargo")
        real_rustup = Path(os.environ.get("RUSTUP_HOME") or _real_home() / ".rustup")
        self.cargo_home.mkdir(parents=True, exist_ok=True)
        config = ""
        for name in ("config.toml", "config"):
            if (real_cargo / name).is_file():
                config = (real_cargo / name).read_text()
                break
        target = str(self.results_dir / "child-target")
        if "target-dir" not in config:
            line = f'target-dir = "{target}"\n'
            if "[build]" in config:
                config = config.replace("[build]", "[build]\n" + line, 1)
            else:
                config = config.rstrip() + ("\n\n" if config.strip() else "") + "[build]\n" + line
        (self.cargo_home / "config.toml").write_text(config)
        # The rustup proxies (cargo, rustc, ...) as links: rustup dispatches
        # on the name. `cargo install` writes HERE, not into ~/.cargo/bin.
        bin_dir = self.cargo_home / "bin"
        if not bin_dir.exists():
            bin_dir.mkdir()
            rustup_bin = real_cargo / "bin" / "rustup"
            if rustup_bin.exists():
                for proxy in ("cargo", "rustc", "rustup", "rustdoc", "rustfmt", "cargo-fmt",
                              "cargo-clippy", "clippy-driver", "rust-analyzer"):
                    if (real_cargo / "bin" / proxy).exists():
                        (bin_dir / proxy).symlink_to(rustup_bin.resolve())
        for cache in ("registry", "git"):
            dest = self.cargo_home / cache
            if not dest.exists():
                cloned = _clone_tree(real_cargo / cache, dest)
                self.isolation[f"cargo_{cache}"] = "clone" if cloned else "empty (downloads)"
            else:
                self.isolation.setdefault(f"cargo_{cache}", "harness-owned")
        if not self.rustup_home.exists():
            if not _clone_tree(real_rustup, self.rustup_home):
                self.isolation["rustup"] = "shared (read-write: no copy-on-write clone here)"
                return
        self.isolation["rustup"] = "clone"

    # -- the environment -----------------------------------------------------

    def rustup_home_for_env(self):
        if self.rustup_home.exists():
            return str(self.rustup_home)
        return os.environ.get("RUSTUP_HOME") or str(_real_home() / ".rustup")

    def path(self, base_path):
        """PATH for the agent: venv bin, harness cargo bin, then `base_path`
        without the user's ~/.cargo/bin and Python user-base bin."""
        real_home = _real_home()
        drop = {
            str(Path(os.environ.get("CARGO_HOME") or real_home / ".cargo") / "bin"),
            str(Path(site.getuserbase()) / "bin"),
        }
        keep = [p for p in (base_path or "").split(os.pathsep)
                if p and os.path.normpath(p) not in {os.path.normpath(d) for d in drop}]
        return os.pathsep.join([str(self.venv / "bin"), str(self.cargo_home / "bin")] + keep)


def ensure(work_root, results_dir):
    """The toolchain under `<work_root>/.toolchain`, prepared once (locked)."""
    root = Path(work_root) / ".toolchain"
    root.mkdir(parents=True, exist_ok=True)
    tc = Toolchain(root, results_dir)
    with open(root / ".lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            tc.ensure_venv()
            tc.ensure_cargo()
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)
    return tc
