"""Run one live-eval scenario against a real endpoint and record it.

One call of `run_scenario` = one JSONL record: set up a throwaway
workspace, copy the run's config next to it (outside the workspace, so a
review never reads it), run `selfware -c <cfg> --output-format stream-json`
with an isolated HOME, score the outcome, cap and compress the artifacts,
delete the workspace, append the record.

Honesty rules (AGENTS.md rule 3): an unreachable endpoint, a missing
fixture, a harness timeout or a missing result object are FAIL records with
a reason, never skips; a run the harness itself stopped (loop shutdown) is
`abandoned`, which the report counts but never shows as a pass.
"""

import fcntl
import gzip
import hashlib
import json
import os
import platform
import random
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import traceback
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import scorers  # noqa: E402
import toolchain  # noqa: E402

SCENARIO_DIR = HERE / "scenarios"
REPO_ROOT = HERE.parents[1]
TRACKED_CONFIG = "selfware-llm-selfware-design.toml"
SCHEMA_VERSION = 2  # 2: 0.9.5 scorer (review fixes), toolchain HOME, work root

# Artifact caps (bytes): keep the head and tail of long streams.
CAP_HEAD = 512 * 1024
CAP_TAIL = 512 * 1024
PATCH_CAP = 256 * 1024


class SetupError(Exception):
    """A scenario could not be set up; recorded as a FAIL with this reason."""


def utc_now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _inside(path, root):
    try:
        Path(path).resolve().relative_to(Path(root).resolve())
        return True
    except ValueError:
        return False


def default_results_dir(explicit=None):
    """`explicit`, else $LIVE_EVAL_RESULTS_DIR, else $TMPDIR/selfware-live-eval.

    Never inside the repository tree, whichever way it was given.
    """
    raw = explicit or os.environ.get("LIVE_EVAL_RESULTS_DIR") or os.path.join(
        os.environ.get("TMPDIR") or "/tmp", "selfware-live-eval"
    )
    path = Path(raw).expanduser().resolve()
    if _inside(path, REPO_ROOT):
        raise SystemExit(f"results dir {path} is inside the repository tree; choose another")
    return path


def work_root(results_dir):
    """Where run workspaces live: $LIVE_EVAL_WORK_ROOT, else a directory in
    the system temp dir — never inside the results dir or the harness repo.

    A workspace inside the results dir let a yolo shell walk `../../..` to
    the answer key (in build-src) and to earlier runs' results; keeping the
    workspace out of their ancestry removes that route, and
    `contamination_hits` flags any run that reaches them anyway.
    """
    raw = os.environ.get("LIVE_EVAL_WORK_ROOT") or os.path.join(
        tempfile.gettempdir(), "selfware-live-eval-work"
    )
    root = Path(raw).expanduser().resolve()
    for other in (results_dir, REPO_ROOT, HERE):
        if _inside(root, other) or _inside(other, root):
            raise SystemExit(
                f"work root {root} overlaps {other}: the agent could reach harness state"
            )
    root.mkdir(parents=True, exist_ok=True)
    return root


# --------------------------------------------------------------------------
# scenarios
# --------------------------------------------------------------------------


def load_scenarios():
    out = {}
    for spec_path in sorted(SCENARIO_DIR.glob("*/scenario.json")):
        spec = json.loads(spec_path.read_text())
        spec["_dir"] = str(spec_path.parent)
        out[spec["name"]] = spec
    return out


def expand(value):
    """Expand ${VAR}; an unset variable is a setup error naming it."""

    def sub(m):
        name = m.group(1)
        if not os.environ.get(name):
            raise SetupError(f"{name} is not set (needed for {value})")
        return os.environ[name]

    return re.sub(r"\$\{([A-Z0-9_]+)\}", sub, value)


def resolve_source(entry):
    """A path from {"env": VAR, "default": "..."}: $VAR wins, else default."""
    if entry.get("env") and os.environ.get(entry["env"]):
        return os.environ[entry["env"]]
    if "default" not in entry:
        raise SetupError(f"{entry.get('env')} is not set")
    return expand(entry["default"])


def scenario_prompt(spec):
    if "prompt" in spec:
        return spec["prompt"]
    pf = spec["prompt_file"]
    path = resolve_source(pf) if isinstance(pf, dict) else os.path.join(spec["_dir"], pf)
    if not os.path.isfile(path):
        raise SetupError(f"prompt file missing: {path}")
    return Path(path).read_text().strip()


# --------------------------------------------------------------------------
# binaries
# --------------------------------------------------------------------------


class Binary:
    """A built selfware binary and the source tree it was built from."""

    def __init__(self, path, commit, source_tree, version=None):
        self.path = str(path)
        self.commit = commit
        self.source_tree = str(source_tree)
        self.version = version or binary_version(path)
        built = version_sha(self.version)
        if built and not commit.startswith(built):
            raise ValueError(
                f"{path} reports {self.version!r} but is labelled {commit[:12]}: "
                "refusing to record its runs under the wrong commit"
            )

    def as_dict(self):
        return {
            "path": self.path,
            "commit": self.commit,
            "version": self.version,
            "source_tree": self.source_tree,
        }


def version_sha(version):
    """The `+g<sha>` a dev build embeds in `--version`, or None (release build)."""
    m = re.search(r"\+g([0-9a-f]{7,40})\b", version or "")
    return m.group(1) if m else None


def binary_version(path):
    try:
        out = subprocess.run(
            [str(path), "--version"], capture_output=True, text=True, timeout=30
        ).stdout
        return out.strip()
    except (OSError, subprocess.SubprocessError) as exc:
        return f"unknown ({exc})"


def git(*args, cwd=None, check=True, capture=True):
    return subprocess.run(
        ["git", *args], cwd=cwd, check=check, capture_output=capture, text=True
    ).stdout.strip()


# --------------------------------------------------------------------------
# environment
# --------------------------------------------------------------------------


def child_env(home, results_dir, tc):
    """Environment for the agent: isolated HOME, no inherited selfware/API keys,
    and the harness-owned toolchain `tc` (see toolchain.py) — never the
    user's Python user site, ~/.cargo or ~/.rustup, which a yolo agent could
    write to.

    HOME is per run so no response cache, checkpoint, trust entry or global
    config (~/.config/selfware) leaks between runs.
    """
    env = {
        k: v
        for k, v in os.environ.items()
        if not k.startswith("SELFWARE_")
        and not k.endswith("_API_KEY")
        and not k.startswith("GIT_")
        and not k.startswith("LIVE_EVAL_")
        and not k.startswith("PYTHON")
        and not k.startswith("VIRTUAL_ENV")
        and k not in ("CARGO_TARGET_DIR",)
    }
    env["HOME"] = str(home)
    env["NO_COLOR"] = "1"
    env["PATH"] = tc.path(os.environ.get("PATH", ""))
    env["CARGO_HOME"] = str(tc.cargo_home)
    env["RUSTUP_HOME"] = tc.rustup_home_for_env()
    # Not forwarded by selfware's tool spawns, but it covers the harness's
    # own subprocesses (trust, post-checks).
    env["PYTHONNOUSERSITE"] = "1"
    return env


def endpoint_from_config(text):
    m = re.search(r'^\s*endpoint\s*=\s*"([^"]+)"', text, re.M)
    return m.group(1) if m else None


def model_from_config(text):
    m = re.search(r'^\s*model\s*=\s*"([^"]+)"', text, re.M)
    return m.group(1) if m else None


def preflight(endpoint, timeout=20):
    """(ok, detail): GET <endpoint>/models answered with a model list."""
    url = endpoint.rstrip("/") + "/models"
    try:
        with urllib.request.urlopen(url, timeout=timeout) as resp:
            body = resp.read(65536)
        data = json.loads(body)
        ids = [m.get("id") for m in data.get("data", []) if isinstance(m, dict)]
        return True, ",".join(i for i in ids if i)
    except Exception as exc:  # noqa: BLE001 - any failure is an outage
        return False, f"{type(exc).__name__}: {exc}"[:300]


# --------------------------------------------------------------------------
# workspace setup
# --------------------------------------------------------------------------


def _git_init_commit(work):
    git("init", "-q", "-b", "main", cwd=work)
    git("add", "-A", cwd=work)
    git(
        "-c", "user.email=eval@local", "-c", "user.name=live-eval",
        "commit", "-q", "--allow-empty", "-m", "fixture", cwd=work,
    )


def setup_workspace(spec, work, binary):
    setup = spec["setup"]
    kind = setup["kind"]
    if kind == "empty":
        work.mkdir(parents=True)
        _git_init_commit(work)
    elif kind == "fixture":
        src = Path(spec["_dir"]) / setup["dir"]
        shutil.copytree(src, work, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        _git_init_commit(work)
    elif kind == "git_clone":
        src = resolve_source(setup)
        if not os.path.isdir(src):
            raise SetupError(f"fixture repo missing: {src}")
        # A clone checks out HEAD: the source's uncommitted edits never leak in.
        try:
            git("clone", "-q", "--no-hardlinks", src, str(work))
        except subprocess.CalledProcessError as exc:
            raise SetupError(f"git clone {src} failed: {exc.stderr.strip()[:200]}")
    elif kind == "source_snapshot":
        work.mkdir(parents=True)
        archive = subprocess.Popen(
            ["git", "-C", binary.source_tree, "archive", binary.commit], stdout=subprocess.PIPE
        )
        tar = subprocess.run(["tar", "-x", "-C", str(work)], stdin=archive.stdout)
        archive.stdout.close()
        if archive.wait() != 0 or tar.returncode != 0:
            raise SetupError(f"git archive {binary.commit} from {binary.source_tree} failed")
        _git_init_commit(work)
    else:
        raise SetupError(f"unknown setup kind {kind}")


def prepare_config(spec, binary, run_dir, endpoint_override=None):
    cfg = spec.get("config", {"kind": "tracked"})
    text = None
    if cfg["kind"] == "tracked":
        # The config AT THE BINARY'S COMMIT (the build checkout may already
        # have moved on to the next commit while this binary still runs).
        try:
            text = git("show", f"{binary.commit}:{TRACKED_CONFIG}", cwd=binary.source_tree) + "\n"
            src = f"{binary.commit[:12]}:{TRACKED_CONFIG}"
        except (subprocess.CalledProcessError, OSError):
            src = REPO_ROOT / TRACKED_CONFIG
    else:
        src = Path(resolve_source(cfg))
    if text is None:
        if not Path(src).is_file():
            raise SetupError(f"config missing: {src}")
        text = Path(src).read_text()
    if endpoint_override:
        text = re.sub(
            r'^(\s*endpoint\s*=\s*)"[^"]*"', rf'\g<1>"{endpoint_override}"', text, count=1,
            flags=re.M,
        )
    dest = Path(run_dir) / "cfg.toml"
    dest.write_text(text)
    return dest, text, str(src)


# --------------------------------------------------------------------------
# process control
# --------------------------------------------------------------------------


def _child_preexec():
    # Default dispositions: a backgrounded loop may have SIGINT ignored, and
    # an ignored disposition would be inherited (the interrupt scenario
    # would then test nothing).
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    signal.signal(signal.SIGTERM, signal.SIG_DFL)


def _signal_group(proc, sig):
    try:
        os.killpg(proc.pid, sig)
    except (ProcessLookupError, PermissionError):
        pass


def _stop_child(proc, grace=60):
    """SIGTERM the child's process group, SIGKILL after `grace` seconds."""
    _signal_group(proc, signal.SIGTERM)
    try:
        proc.wait(timeout=grace)
    except subprocess.TimeoutExpired:
        _signal_group(proc, signal.SIGKILL)
        proc.wait()


def run_child(cmd, cwd, env, out_path, err_path, hard_timeout, abandon=None, interrupt=None):
    """Run the agent, watching for a harness timeout, abandon or interrupt.

    Returns a dict: exit_code, wall_s, timed_out, abandoned, and for an
    interrupt scenario sigint_at / exit_after_sigint / streaming_before_sigint.
    """
    info = {"timed_out": False, "abandoned": False}
    t0 = time.time()
    with open(out_path, "wb") as out, open(err_path, "wb") as err:
        proc = subprocess.Popen(
            cmd, cwd=cwd, env=env, stdout=out, stderr=err, stdin=subprocess.DEVNULL,
            preexec_fn=_child_preexec, start_new_session=True,
        )
        first_stream = None
        sigint_at = None
        read_pos = 0
        buf = b""
        while proc.poll() is None:
            time.sleep(0.5)
            now = time.time()
            if interrupt is not None and sigint_at is None:
                with open(out_path, "rb") as fh:
                    fh.seek(read_pos)
                    chunk = fh.read()
                read_pos += len(chunk)
                buf += chunk
                if first_stream is None and b'"text_delta"' in buf:
                    first_stream = now
                buf = buf[-4096:]
                due = (
                    first_stream is not None
                    and now - first_stream >= interrupt.get("after_stream_secs", 4)
                ) or now - t0 >= interrupt.get("max_wait_secs", 240)
                if due:
                    # Like a terminal Ctrl-C: the whole foreground group.
                    _signal_group(proc, signal.SIGINT)
                    sigint_at = now
                    info["sigint_at"] = round(now - t0, 2)
                    info["streaming_before_sigint"] = first_stream is not None
            if abandon is not None and abandon.is_set():
                info["abandoned"] = True
                _stop_child(proc)
                break
            if now - t0 > hard_timeout:
                info["timed_out"] = True
                _stop_child(proc)
                break
        code = proc.wait()
    end = time.time()
    info["exit_code"] = code
    info["wall_s"] = round(end - t0, 2)
    if sigint_at is not None:
        info["exit_after_sigint"] = round(end - sigint_at, 2)
    return info


# --------------------------------------------------------------------------
# post-run checks
# --------------------------------------------------------------------------

PUB_FN_RE = re.compile(r"^\s*pub (?:async )?fn\s+\w+")


def pub_fn_stats(text):
    """(count, undocumented): `pub fn`s and those without a `///` above."""
    lines = text.splitlines()
    count = undocumented = 0
    for i, line in enumerate(lines):
        if not PUB_FN_RE.match(line):
            continue
        count += 1
        j = i - 1
        while j >= 0 and lines[j].strip().startswith("#["):
            j -= 1
        if j < 0 or not lines[j].strip().startswith("///"):
            undocumented += 1
    return count, undocumented


C24_FILES = ("src/agent/context.rs", "src/agent/compression.rs", "src/agent/context_management.rs")


def pre_c24(work):
    counts = {}
    for rel in C24_FILES:
        p = Path(work) / rel
        counts[rel] = pub_fn_stats(p.read_text()) if p.is_file() else (None, None)
    return {
        "expected_bullets": sum(c for c, _ in counts.values() if c is not None),
        "undocumented_before": counts[C24_FILES[0]][1],
    }


def post_c24(work, pre):
    extra = dict(pre)
    notes = Path(work) / "docs/CONTEXT_NOTES.md"
    extra["notes_bullets"] = scorers.count_bullets(notes.read_text()) if notes.is_file() else None
    diff = git("diff", "--", C24_FILES[0], cwd=work, check=False)
    ok, added = scorers.comments_only_diff(diff)
    extra["comments_only"] = ok
    extra["doc_comments_added"] = added
    ctx = Path(work) / C24_FILES[0]
    extra["undocumented_after"] = pub_fn_stats(ctx.read_text())[1] if ctx.is_file() else None
    return extra


def post_edit_tests(work, env, python):
    extra = {}
    try:
        res = subprocess.run(
            [python, "-m", "pytest", "-q", "--color=no", "-p", "no:cacheprovider"],
            cwd=work, env=env, capture_output=True, text=True, timeout=900,
        )
        extra["pytest_exit"] = res.returncode
        tail = [ln for ln in res.stdout.strip().splitlines() if ln.strip()]
        extra["pytest_summary"] = tail[-1][:200] if tail else ""
    except (OSError, subprocess.SubprocessError) as exc:
        extra["pytest_exit"] = None
        extra["pytest_summary"] = f"pytest did not run: {exc}"[:200]
    probe = (
        "from slugify import slugify\n"
        "r = slugify('one two three four', max_words=2)\n"
        "assert r == 'one-two', r\n"
        "assert slugify('one two three') == 'one-two-three'\n"
    )
    res = subprocess.run(
        [python, "-c", probe], cwd=work, env=env, capture_output=True, text=True,
        timeout=120,
    )
    extra["behavior_probe_ok"] = res.returncode == 0
    extra["test_mentions_max_words"] = any(
        "max_words" in p.read_text(errors="replace") for p in Path(work, "tests").glob("*.py")
    )
    return extra


def leaked_resources(binary, env, cwd):
    """Entries `selfware resources --json` still lists for this run's HOME."""
    try:
        res = subprocess.run(
            [binary.path, "resources", "--json"], cwd=cwd, env=env, capture_output=True,
            text=True, timeout=120,
        )
        data = json.loads(res.stdout)
        return len(data.get("entries") or [])
    except (OSError, subprocess.SubprocessError, ValueError):
        return None


def reap_resources(binary, env, cwd):
    """`selfware resources reap` with the run's HOME: tear down whatever the
    run registered and left behind (its own process groups are outside the
    child's session, so the harness's killpg does not reach them)."""
    try:
        res = subprocess.run(
            [binary.path, "resources", "reap", "--json"], cwd=cwd, env=env,
            capture_output=True, text=True, timeout=180,
        )
        return res.returncode
    except (OSError, subprocess.SubprocessError):
        return None


# --------------------------------------------------------------------------
# artifacts
# --------------------------------------------------------------------------


def cap_bytes(data, head=CAP_HEAD, tail=CAP_TAIL):
    if len(data) <= head + tail:
        return data
    cut = len(data) - head - tail
    return data[:head] + f"\n... [{cut} bytes cut by live_eval] ...\n".encode() + data[-tail:]


def compress_capped(src, dest):
    data = Path(src).read_bytes() if Path(src).exists() else b""
    with gzip.open(dest, "wb") as fh:
        fh.write(cap_bytes(data))
    Path(src).unlink(missing_ok=True)


CHECKPOINT_CAP = 20 * 1024 * 1024


def keep_checkpoint(home, run_dir):
    """Keep the run's checkpoints (the conversation as the model saw it:
    messages with tool results, tool calls) gzipped next to the record, one
    file per checkpoint; returns the kept paths.

    Turn artifacts are off by default and enabling them would change the
    tracked config under test, so the checkpoint is the evidence of what the
    model read. Kept whole (a capped JSON is unreadable) up to 20 MB.
    """
    kept = []
    for path in sorted(Path(home, ".selfware", "checkpoints").glob("*.json")):
        if path.stat().st_size > CHECKPOINT_CAP:
            with open(run_dir / "checkpoint.skipped", "a") as fh:
                fh.write(f"{path.name}: {path.stat().st_size} bytes\n")
            continue
        dest = run_dir / f"checkpoint-{path.stem}.json.gz"
        with open(path, "rb") as src, gzip.open(dest, "wb") as dst:
            shutil.copyfileobj(src, dst)
        kept.append(dest)
    return kept


def append_record(results_dir, record):
    path = Path(results_dir) / "results.jsonl"
    line = json.dumps(record, sort_keys=True) + "\n"
    with open(path, "a", encoding="utf-8") as fh:
        fcntl.flock(fh, fcntl.LOCK_EX)
        fh.write(line)
        fh.flush()
        fcntl.flock(fh, fcntl.LOCK_UN)
    return path


def dir_size(path):
    total = 0
    for root, _dirs, files in os.walk(path):
        for name in files:
            try:
                total += os.lstat(os.path.join(root, name)).st_size
            except OSError:
                pass
    return total


def rotate_runs(results_dir, keep=300, max_bytes=2 * 1024**3):
    """Delete the oldest run artifact dirs beyond `keep` or `max_bytes`.

    results.jsonl is never rotated: it is the append-only history.
    """
    runs = Path(results_dir) / "runs"
    if not runs.is_dir():
        return 0
    dirs = sorted((d for d in runs.iterdir() if d.is_dir()), key=lambda d: d.name)
    sizes = {d: dir_size(d) for d in dirs}
    total = sum(sizes.values())
    removed = 0
    while dirs and (len(dirs) > keep or total > max_bytes):
        victim = dirs.pop(0)
        total -= sizes[victim]
        shutil.rmtree(victim, ignore_errors=True)
        removed += 1
    return removed


# --------------------------------------------------------------------------
# the run
# --------------------------------------------------------------------------


def _record_base(spec, binary, run_id, started, endpoint, model):
    return {
        "schema": SCHEMA_VERSION,
        "run_id": run_id,
        "scenario": spec["name"],
        "tier": spec.get("tier", "quick"),
        "hermetic_fixture": bool(spec.get("hermetic_fixture")),
        "started_at": started,
        "commit": binary.commit,
        "binary_version": binary.version,
        "endpoint": endpoint,
        "model": model,
        "host": platform.node(),
        "harness": "scripts/live_eval",
    }


def _finish(record, status, reason, metrics, criteria, results_dir):
    record["status"] = status
    record["reason"] = reason
    record["metrics"] = metrics
    record["criteria"] = criteria
    record["failed_criteria"] = sorted(k for k, v in criteria.items() if not v)
    record["finished_at"] = utc_now()
    append_record(results_dir, record)
    return record


def run_scenario(spec, binary, results_dir, abandon=None, endpoint_override=None, log=print):
    """Run one scenario once; ALWAYS appends and returns exactly one record.

    Any exception inside the run (a post-check timeout, `git init` failing,
    a scorer bug) becomes a FAIL record with reason `harness_error: ...`
    instead of killing the caller's worker thread and silently dropping the
    rest of its queue; the run's scratch dir is removed either way.
    """
    results_dir = Path(results_dir)
    started = utc_now()
    run_id = f"{started.replace(':', '').replace('-', '')}-{spec['name']}-{random.randrange(16**4):04x}"
    run_dir = results_dir / "runs" / run_id
    run_dir.mkdir(parents=True)
    scratch = work_root(results_dir) / run_id
    state = {"record": _record_base(spec, binary, run_id, started, endpoint_override, None),
             "appended": False}
    try:
        return _run_scenario(spec, binary, results_dir, run_id, run_dir, scratch, state,
                             abandon, endpoint_override, log)
    except Exception as exc:  # noqa: BLE001 - recorded, never swallowed
        tb = traceback.format_exc(limit=6)
        (run_dir / "harness_error.txt").write_text(tb)
        log(f"[{spec['name']}] HARNESS ERROR {type(exc).__name__}: {exc}")
        if state["appended"]:
            return state["record"]
        return _finish(
            state["record"], "fail", f"harness_error: {type(exc).__name__}: {str(exc)[:300]}",
            {}, {"harness_ok": False}, results_dir,
        )
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def finish_setup_failure(state, results_dir, reason):
    rec = _finish(state["record"], "fail", f"setup_failed: {reason}", {}, {"setup": False},
                  results_dir)
    state["appended"] = True
    return rec


def _run_scenario(spec, binary, results_dir, run_id, run_dir, scratch, state, abandon,
                  endpoint_override, log):
    work = scratch / "ws"
    home = scratch / "home"
    home.mkdir(parents=True)
    try:
        tc = toolchain.ensure(scratch.parent, results_dir)
    except toolchain.ToolchainError as exc:
        return finish_setup_failure(state, results_dir, f"toolchain: {exc}")
    state["record"]["toolchain_isolation"] = dict(tc.isolation)
    toolchain_before = toolchain.fingerprint(tc.watched())
    env = child_env(home, results_dir, tc)

    def finish(*args):
        rec = _finish(state["record"], *args, results_dir)
        state["appended"] = True
        return rec

    try:
        cfg_path, cfg_text, cfg_src = prepare_config(spec, binary, run_dir, endpoint_override)
    except SetupError as exc:
        return finish("fail", f"setup_failed: {exc}", {}, {"setup": False})
    endpoint = endpoint_from_config(cfg_text)
    model = model_from_config(cfg_text)
    record = state["record"] = _record_base(
        spec, binary, run_id, state["record"]["started_at"], endpoint, model
    )
    record["config_source"] = cfg_src
    record["config_sha256"] = hashlib.sha256(cfg_text.encode()).hexdigest()
    record["artifacts"] = str(run_dir)

    def fail(reason, criteria, metrics=None):
        log(f"[{spec['name']}] FAIL {reason}")
        return finish("fail", reason, metrics or {}, criteria)

    ok, detail = preflight(endpoint or "")
    record["endpoint_models"] = detail if ok else None
    if not ok:
        return fail(f"endpoint_unreachable: {detail}", {"endpoint_reachable": False})

    try:
        prompt = scenario_prompt(spec)
        setup_workspace(spec, work, binary)
    except (SetupError, OSError, subprocess.CalledProcessError) as exc:
        return fail(f"setup_failed: {exc}", {"setup": False})

    extra = {}
    if spec.get("post_check") == "c24":
        extra.update(pre_c24(work))

    subprocess.run(
        [binary.path, "trust", str(cfg_path), "-q"], cwd=work, env=env, capture_output=True,
        timeout=60,
    )
    wall = int(spec.get("wall_secs", 1800))
    base_cmd = [binary.path, "-c", str(cfg_path), "--mode", spec.get("mode", "yolo"), "--no-color"]
    cmd = base_cmd + ["--output-format", "stream-json", "--max-wall-secs", str(wall), "-p", prompt]
    record["command"] = ["selfware"] + cmd[1:-1] + ["<prompt>"]
    log(f"[{spec['name']}] start {run_id} ({binary.commit[:10]})")
    out_path, err_path = run_dir / "events.jsonl", run_dir / "transcript.txt"
    info = run_child(
        cmd, work, env, out_path, err_path, hard_timeout=wall + 300, abandon=abandon,
        interrupt=spec.get("interrupt"),
    )
    stdout = out_path.read_text(errors="replace")
    stderr = err_path.read_text(errors="replace")

    if spec.get("scorer") == "qa_greeting" and not info["abandoned"]:
        # Its own HOME: the twin must not see the first run's checkpoint,
        # trust entry or response cache.
        twin_ws, twin_home = scratch / "twin", scratch / "twin-home"
        twin_ws.mkdir()
        twin_home.mkdir()
        twin_env = child_env(twin_home, results_dir, tc)
        _git_init_commit(twin_ws)
        subprocess.run(
            [binary.path, "trust", str(cfg_path), "-q"], cwd=twin_ws, env=twin_env,
            capture_output=True, timeout=60,
        )
        twin_cmd = base_cmd + ["--max-wall-secs", str(wall), "-p", prompt]
        twin_info = run_child(
            twin_cmd, twin_ws, twin_env, run_dir / "text_twin.txt", run_dir / "text_twin.err",
            hard_timeout=wall + 60, abandon=abandon,
        )
        extra["text_twin"] = (run_dir / "text_twin.txt").read_text(errors="replace") + (
            run_dir / "text_twin.err"
        ).read_text(errors="replace")
        extra["text_twin_exit"] = twin_info["exit_code"]
        compress_capped(run_dir / "text_twin.txt", run_dir / "text_twin.txt.gz")
        compress_capped(run_dir / "text_twin.err", run_dir / "text_twin.err.gz")
        reap_resources(binary, twin_env, twin_ws)

    if spec.get("post_check") == "c24":
        extra.update(post_c24(work, extra))
    elif spec.get("post_check") == "edit_tests":
        extra.update(post_edit_tests(work, env, str(tc.python)))
    prefixes = [str(work) + "/", os.path.realpath(work) + "/"]
    extra["prefixes"] = prefixes
    markers = []
    if spec["scorer"] == "review_planted":
        key = json.loads((Path(spec["_dir"]) / "answer_key.json").read_text())
        fixture = Path(spec["_dir"]) / spec["setup"]["dir"]
        scorers.resolve_anchor_lines(key, lambda rel: (fixture / rel).read_text())
        extra["answer_key"] = key
        extra["known_files"] = [
            str(p.relative_to(fixture)) for p in fixture.rglob("*") if p.is_file()
        ]
        # Citations are checked against the pristine fixture, not the
        # workspace the agent could have edited.
        extra["read_file"] = lambda rel: _read_or_none(fixture / rel)
        markers.append("answer_key")
    elif spec["scorer"] == "review_slugify":
        extra["read_file"] = lambda rel: _read_or_none(work / rel)
    extra["leaked_listing"] = leaked_resources(binary, env, work)
    for k in ("sigint_at", "exit_after_sigint", "streaming_before_sigint"):
        if k in info:
            extra[k] = info[k]

    inp = scorers.ScoreInput(info["exit_code"], stdout, stderr, extra)
    metrics, criteria = scorers.SCORERS[spec["scorer"]](inp)
    metrics["wall_s"] = info["wall_s"]
    metrics["harness_timed_out"] = info["timed_out"]
    metrics["leaked_listing"] = extra["leaked_listing"]
    criteria["no_harness_timeout"] = not info["timed_out"]

    kept = keep_checkpoint(home, run_dir)
    texts = [str(ev.get("args") or "") for ev in inp.stream["events"]
             if ev.get("event") == "tool_call_started"]
    texts += checkpoint_tool_texts(kept)
    hits = scorers.contamination_hits(
        texts,
        forbidden_paths=[str(results_dir), str(HERE), str(REPO_ROOT)],
        extra_markers=markers,
    )
    # A run that wrote into the shared toolchain (pip install into the venv,
    # cargo install) changed what later runs get: contaminated.
    if toolchain.fingerprint(tc.watched()) != toolchain_before:
        hits.append("the run modified the harness toolchain (venv or cargo bin)")
    metrics["contamination_hits"] = hits[:10]
    criteria["not_contaminated"] = not hits

    if inp.result is not None:
        (run_dir / "result.json").write_text(json.dumps(inp.result, indent=1)[:PATCH_CAP])
    try:
        git("add", "-A", cwd=work, check=False)
        patch = git("diff", "--cached", "HEAD", cwd=work, check=False)
        if patch:
            (run_dir / "patch.diff").write_text(patch[:PATCH_CAP])
    except (OSError, subprocess.SubprocessError):
        pass
    compress_capped(out_path, run_dir / "events.jsonl.gz")
    compress_capped(err_path, run_dir / "transcript.txt.gz")
    metrics["reap_exit"] = reap_resources(binary, env, work)

    if info["abandoned"]:
        status, reason = "abandoned", "harness stopped (loop shutdown) before the run ended"
    elif info["timed_out"]:
        status, reason = "fail", f"harness_timeout after {wall + 300}s"
    elif inp.result is None:
        status, reason = "fail", "no JSON result object on stdout"
    else:
        status = "pass" if all(criteria.values()) else "fail"
        reason = None if status == "pass" else "criteria: " + ", ".join(
            sorted(k for k, v in criteria.items() if not v)
        )
    # stderr only: stdout carries the model's own text, which may quote such
    # errors (a review of HTTP code) without any outage having happened.
    if status == "fail" and _looks_like_outage(stderr):
        reason = f"endpoint_error_mid_run; {reason}"
    record["wall_s"] = info["wall_s"]
    log(f"[{spec['name']}] {status.upper()} {reason or ''} wall={info['wall_s']}s")
    return finish(status, reason, metrics, criteria)


def _read_or_none(path):
    try:
        return Path(path).read_text(errors="replace")
    except OSError:
        return None


def checkpoint_tool_texts(paths):
    """The assistant turns (tool calls with full arguments) of kept checkpoints."""
    texts = []
    for path in paths:
        try:
            data = json.load(gzip.open(path))
        except (OSError, ValueError):
            continue
        for msg in (data.get("payload") or {}).get("messages") or []:
            if msg.get("role") == "assistant":
                content = msg.get("content")
                texts.append(content if isinstance(content, str) else json.dumps(content))
            for call in msg.get("tool_calls") or []:
                texts.append(json.dumps(call))
    return texts


OUTAGE_RE = re.compile(
    r"(connection refused|error sending request|dns error|502 Bad Gateway|503 Service|"
    r"504 Gateway|endpoint unreachable|timed out connecting)",
    re.I,
)


def _looks_like_outage(text):
    return bool(OUTAGE_RE.search(text[-20000:]))
