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
(`safety::process_env::DEFAULT_KEEP`), so these reach the agent's shell.
PATH is the venv's bin, the harness cargo bin, `tools-bin` (single-file
links to LINKED_TOOLS, e.g. rg) and the system dirs (SYSTEM_PATH) — nothing
else of the host PATH: ~/.local/bin, nvm, homebrew, ~/.cargo/bin and the
Python user base are unreachable by name.

Every run fingerprints the venv and cargo bin before and after; a run that
changed them is marked contaminated and the venv is rebuilt.
"""

import fcntl
import hashlib
import json
import os
import platform
import shutil
import stat
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REQUIREMENTS = HERE / "toolchain-requirements.txt"

# The only directories of the host PATH the agent gets.
SYSTEM_PATH = ("/usr/bin", "/bin", "/usr/sbin", "/sbin")
# User-installed tools a scenario provably needs, linked one file at a time
# (never their directory), with the reason recorded per run.
LINKED_TOOLS = {
    # selfware's grep_search uses ripgrep when `rg` is on PATH and falls back
    # to its built-in walker otherwise (src/tools/grep_search/mod.rs
    # rg_available): without it every review scenario would measure a
    # different search backend than users (and all earlier records) get.
    "rg": "grep_search backend (tools/grep_search rg_available)",
    # Every workspace is a git repo and selfware spawns git throughout; it is
    # linked only where it is not already in a system dir (/usr/bin/git on
    # macOS and CI runners).
    "git": "workspace VCS; selfware spawns git",
}


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


def _tool_version(path):
    try:
        out = subprocess.run([path, "--version"], capture_output=True, text=True, timeout=20)
        return (out.stdout or out.stderr).strip().splitlines()[0][:80]
    except (OSError, subprocess.SubprocessError, IndexError):
        return "version unknown"


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

    def ensure_linked_tools(self, base_path):
        """`tools-bin`: single-file links to the few user-installed tools a
        scenario provably needs (LINKED_TOOLS), so their whole directories
        (~/.local/bin, nvm, homebrew, ...) stay off the agent's PATH.

        Records each link's target (or `absent`) in `isolation["linked_tools"]`.
        """
        tools_bin = self.root / "tools-bin"
        tools_bin.mkdir(parents=True, exist_ok=True)
        search = os.pathsep.join(
            p for p in (base_path or "").split(os.pathsep)
            if p and not p.startswith(str(self.root))
        )
        linked = {}
        for name, why in LINKED_TOOLS.items():
            link = tools_bin / name
            target = shutil.which(name, path=search)
            if target and os.path.dirname(target) in SYSTEM_PATH:
                linked[name] = f"{target} (system dir, not linked)"
                continue
            if target:
                # The same binary the host PATH resolves first (what earlier
                # records and the user's own shell used); its version is
                # recorded because a different rg is a different backend.
                target = os.path.realpath(target)
                if not link.is_symlink() or os.readlink(link) != target:
                    link.unlink(missing_ok=True)
                    link.symlink_to(target)
                linked[name] = f"{target} [{_tool_version(target)}] ({why})"
            else:
                link.unlink(missing_ok=True)
                linked[name] = f"absent ({why})"
        self.isolation["linked_tools"] = linked
        self.isolation["path"] = "harness venv + cargo + tools-bin, then " + ":".join(SYSTEM_PATH)

    def path(self, base_path=None):
        """PATH for the agent: the harness venv, harness cargo bin, the linked
        tools, then the system directories only — never the user's PATH
        (~/.local/bin, nvm, homebrew, ~/.cargo/bin, the Python user base)."""
        dirs = [str(self.venv / "bin"), str(self.cargo_home / "bin"), str(self.root / "tools-bin")]
        return os.pathsep.join(dirs + list(SYSTEM_PATH))


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
            tc.ensure_linked_tools(os.environ.get("PATH", ""))
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)
    return tc
