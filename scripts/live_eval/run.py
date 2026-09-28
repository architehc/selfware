#!/usr/bin/env python3
"""Live evaluation harness for selfware against a real endpoint.

Subcommands:
  run     run scenarios once (or --samples N times) with a given binary
  loop    run the suite continuously, rebuilding when the source HEAD moves
  report  per-scenario statistics and the comparison vs the previous commit
  gate    build a revision, sample the quick scenarios, exit 1 on regression
  status  print the loop heartbeat
  stop    stop a running loop (SIGTERM: abandon the current runs honestly;
          --finish: start nothing new and exit when the current runs end)
  list    list the scenarios

Results go to $LIVE_EVAL_RESULTS_DIR (default $TMPDIR/selfware-live-eval,
never inside the repository): results.jsonl (append-only, one record per
scenario run), runs/<run_id>/ (capped, gzipped artifacts), heartbeat.json,
loop.log. See docs/live-eval.md.
"""

import argparse
import calendar
import fcntl
import json
import os
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import builder  # noqa: E402
import harness  # noqa: E402
import report as report_mod  # noqa: E402

MAX_CONCURRENCY = 2
DEFAULT_TARGET_DIR = os.environ.get("LIVE_EVAL_TARGET_DIR") or os.environ.get("CARGO_TARGET_DIR")


def log_to(path):
    lock = threading.Lock()

    def log(msg):
        line = f"{harness.utc_now()} {msg}"
        with lock:
            print(line, flush=True)
            if path:
                with open(path, "a") as fh:
                    fh.write(line + "\n")

    return log


def pick_scenarios(names, tier=None, hermetic_only=False, gate_only=False):
    specs = harness.load_scenarios()
    if names:
        unknown = [n for n in names if n not in specs]
        if unknown:
            raise SystemExit(f"unknown scenario(s): {', '.join(unknown)}; see `run.py list`")
        return [specs[n] for n in names]
    out = [s for s in specs.values() if tier is None or s.get("tier") == tier]
    if gate_only:
        # A config-switch variant (`"gate": false`) is measured by the loop
        # and `run`, never a release floor.
        out = [s for s in out if s.get("gate", True)]
    if hermetic_only:
        out = [s for s in out if s.get("hermetic_fixture")]
    return out


def binary_from_args(args, results_dir, log):
    if args.binary:
        tree = Path(args.source).resolve()
        version = harness.binary_version(args.binary)
        built = harness.version_sha(version)
        if args.commit:
            commit = harness.git("rev-parse", "--verify", f"{args.commit}^{{commit}}", cwd=tree)
        elif built:
            # The commit the binary says it was built from, never HEAD.
            commit = harness.git("rev-parse", "--verify", f"{built}^{{commit}}", cwd=tree)
        else:
            raise SystemExit(
                f"{args.binary} reports {version!r} without a +g<sha>: pass --commit"
            )
        try:
            return harness.Binary(Path(args.binary).resolve(), commit, tree, version=version)
        except ValueError as exc:
            raise SystemExit(str(exc))
    target = args.target_dir or DEFAULT_TARGET_DIR
    if not target:
        raise SystemExit("--target-dir (or LIVE_EVAL_TARGET_DIR) is required to build")
    return builder.build(results_dir, Path(args.source).resolve(), args.rev, target, log=log)


def run_batch(specs, binary, results_dir, samples, concurrency, log, endpoint=None, abandon=None):
    """Run each spec `samples` times, at most `concurrency` at once."""
    queue = [s for _ in range(samples) for s in specs]
    records = []
    lock = threading.Lock()

    def worker():
        while True:
            with lock:
                if not queue or (abandon is not None and abandon.is_set()):
                    return
                spec = queue.pop(0)
            try:
                rec = harness.run_scenario(
                    spec, binary, results_dir, abandon=abandon, endpoint_override=endpoint,
                    log=log,
                )
            except Exception as exc:  # noqa: BLE001 - run_scenario records its own errors;
                # this is the last line of defence (e.g. the run dir could not be made).
                log(f"[{spec['name']}] harness error outside the run: {exc}")
                rec = {"scenario": spec["name"], "status": "fail",
                       "reason": f"harness_error: {type(exc).__name__}: {exc}"}
            with lock:
                records.append(rec)

    threads = [threading.Thread(target=worker) for _ in range(min(concurrency, MAX_CONCURRENCY))]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    harness.rotate_runs(results_dir)
    return records


def cmd_list(_args):
    for name, spec in harness.load_scenarios().items():
        print(
            f"{name:18} tier={spec.get('tier'):5} hermetic={bool(spec.get('hermetic_fixture'))!s:5}"
            f" wall={spec.get('wall_secs')}s scorer={spec['scorer']}"
        )
    return 0


def cmd_run(args):
    results_dir = harness.default_results_dir(args.results_dir)
    results_dir.mkdir(parents=True, exist_ok=True)
    log = log_to(results_dir / "run.log")
    specs = pick_scenarios(args.scenarios, tier=args.tier, hermetic_only=args.hermetic_only)
    sets = harness.parse_config_set(args.config_set)
    specs = [harness.with_config_set(s, sets) for s in specs]
    binary = binary_from_args(args, results_dir, log)
    records = run_batch(
        specs, binary, results_dir, args.samples, args.concurrency, log, endpoint=args.endpoint
    )
    for r in records:
        m = r.get("metrics") or {}
        log(
            f"{r['scenario']}: {r['status']} {r.get('reason') or ''} wall={m.get('wall_s')} "
            f"turns={m.get('num_turns')} tokens={m.get('total_tokens')}"
        )
    expected = args.samples * len(specs)
    if len(records) < expected:
        log(f"only {len(records)} of {expected} runs recorded")
        return 1
    return 0 if all(r["status"] == "pass" for r in records) else 1


def cmd_report(args):
    results_dir = harness.default_results_dir(args.results_dir)
    records = report_mod.load_records(args.results or results_dir / "results.jsonl")
    th = report_mod.load_thresholds(args.thresholds)
    try:
        rep = report_mod.build_report(
            records, th, candidate=args.candidate, baseline=args.baseline,
            scenarios=args.scenarios or None, repo=args.repo, expect_runs=args.expect_runs,
        )
    except report_mod.OrderError as exc:
        print(f"report refused: {exc}")
        return 2
    if args.fail_on_regression and not records:
        rep["missing"].append("no records at all")
    if args.json:
        print(json.dumps(rep, indent=1, default=str))
    else:
        print(f"{len(records)} records in {args.results or results_dir / 'results.jsonl'}")
        print(report_mod.render(rep))
    if args.out:
        Path(args.out).write_text(report_mod.render(rep) + "\n")
    bad = report_mod.blocking_problems(rep)
    if args.fail_on_regression and bad:
        for p in bad:
            print(f"FAIL: {p}")
        return 1
    return 0


def cmd_gate(args):
    results_dir = harness.default_results_dir(args.results_dir)
    results_dir.mkdir(parents=True, exist_ok=True)
    log = log_to(results_dir / "gate.log")
    specs = pick_scenarios(args.scenarios, tier="quick", hermetic_only=args.hermetic_only,
                           gate_only=True)
    try:
        binary = binary_from_args(args, results_dir, log)
    except builder.BuildError as exc:
        log(f"GATE FAILED: build: {exc}")
        return 1
    records = run_batch(
        specs, binary, results_dir, args.samples, args.concurrency, log, endpoint=args.endpoint
    )
    all_records = report_mod.load_records(results_dir / "results.jsonl")
    th = report_mod.load_thresholds(args.thresholds)
    own_ids = {r.get("run_id") for r in records}
    # The candidate is judged on THIS gate's runs only: an outage or a bad
    # run from an earlier loop at the same commit must not decide it (nor
    # make it permanently uncertified). Baselines come from the history.
    history = [
        r for r in all_records if r.get("commit") != binary.commit or r.get("run_id") in own_ids
    ]
    try:
        rep = report_mod.build_report(
            history, th, candidate=binary.commit, baseline=args.baseline,
            scenarios=[s["name"] for s in specs], repo=args.source,
            expect_runs=args.samples,
        )
    except report_mod.OrderError as exc:
        log(f"GATE FAILED: {exc}")
        return 1
    print(report_mod.render(rep))
    problems = report_mod.blocking_problems(rep)
    expected = args.samples * len(specs)
    if len(records) < expected:
        problems.append(f"only {len(records)} of {expected} gate runs were recorded")
    if args.out:
        Path(args.out).write_text(report_mod.render(rep) + "\n")
    if problems:
        for p in problems:
            log(f"GATE FAILED: {p}")
        return 1
    log(f"GATE PASSED: {binary.commit[:12]} ({len(records)} runs)")
    return 0


# --------------------------------------------------------------------------
# loop
# --------------------------------------------------------------------------


class Loop:
    def __init__(self, args):
        self.args = args
        self.results_dir = harness.default_results_dir(args.results_dir)
        self.results_dir.mkdir(parents=True, exist_ok=True)
        self.log = log_to(self.results_dir / "loop.log")
        self.source = Path(args.source).resolve()
        self.target = args.target_dir or DEFAULT_TARGET_DIR
        if not self.target:
            raise SystemExit("--target-dir (or LIVE_EVAL_TARGET_DIR) is required")
        self.quick = pick_scenarios(args.scenarios, tier="quick") if not args.scenarios else [
            s for s in pick_scenarios(args.scenarios) if s.get("tier") == "quick"
        ]
        longs = [s for s in harness.load_scenarios().values() if s.get("tier") == "long"]
        self.long = longs if not args.no_long else []
        self.concurrency = max(1, min(args.concurrency, MAX_CONCURRENCY))
        self.stopping = threading.Event()
        self.abandon = threading.Event()
        self.lock = threading.Lock()
        self.active = {}
        self.binary = None
        self.building = False
        self.source_head = None
        self.completed = 0
        self.last = []
        self.consecutive_outages = 0
        self.backoff_until = 0.0
        self.last_long_start = 0.0
        self.quick_index = 0
        self.started_at = harness.utc_now()
        self.state = "starting"
        self.last_build_error = None
        self.failed_head = None

    # -- signals / heartbeat ------------------------------------------------

    def install_signals(self):
        def on_term(signum, _frame):
            self.log(f"signal {signum}: stopping, abandoning current runs")
            self.state = "stopping"
            self.stopping.set()
            self.abandon.set()

        signal.signal(signal.SIGTERM, on_term)
        signal.signal(signal.SIGINT, on_term)

    def heartbeat(self):
        with self.lock:
            data = {
                "pid": os.getpid(),
                "updated_at": harness.utc_now(),
                "started_at": self.started_at,
                "state": self.state,
                "results_dir": str(self.results_dir),
                "source": str(self.source),
                "source_head": self.source_head,
                "binary": self.binary.as_dict() if self.binary else None,
                "building": self.building,
                "last_build_error": self.last_build_error,
                "active": list(self.active.values()),
                "completed_runs": self.completed,
                "last_results": self.last[-8:],
                "consecutive_outages": self.consecutive_outages,
                "backoff_until": time.strftime(
                    "%Y-%m-%dT%H:%M:%SZ", time.gmtime(self.backoff_until)
                ) if self.backoff_until > time.time() else None,
            }
        tmp = self.results_dir / "heartbeat.json.tmp"
        tmp.write_text(json.dumps(data, indent=1))
        os.replace(tmp, self.results_dir / "heartbeat.json")

    # -- building -------------------------------------------------------------

    def current_head(self):
        try:
            return harness.git("rev-parse", "HEAD", cwd=self.source)
        except (subprocess.CalledProcessError, OSError) as exc:
            self.log(f"[build] cannot read HEAD of {self.source}: {exc}")
            return self.source_head

    def build_head(self, sha):
        protect = [self.binary.path] if self.binary else []
        with self.lock:
            protect += [a["binary"] for a in self.active.values()]
        try:
            binary = builder.build(
                self.results_dir, self.source, sha, self.target, log=self.log, protect=protect
            )
            with self.lock:
                self.binary = binary
                self.source_head = sha
                self.last_build_error = None
                self.failed_head = None
            self.log(f"[build] now evaluating {binary.commit[:12]} ({binary.version})")
        except Exception as exc:  # noqa: BLE001 - BuildError, OSError, a bad label...
            # HEAD is NOT marked done: the same head is retried after
            # --build-retry-secs (a transient failure must not stick).
            with self.lock:
                self.last_build_error = f"{sha[:12]}: {type(exc).__name__}: {str(exc)[:400]}"
                self.failed_head = (sha, time.time())
            self.log(f"[build] FAILED {sha[:12]}: {str(exc)[:400]}; keeping the previous binary")
        finally:
            self.building = False

    def maybe_rebuild(self):
        head = self.current_head()
        if not head or head == self.source_head or self.building:
            return
        if self.failed_head and self.failed_head[0] == head:
            if time.time() - self.failed_head[1] < self.args.build_retry_secs:
                return
        self.building = True
        threading.Thread(target=self.build_head, args=(head,), daemon=True).start()

    # -- scheduling -----------------------------------------------------------

    def next_spec(self):
        long_busy = any(a["tier"] == "long" for a in self.active.values())
        every = self.args.long_every_hours * 3600
        if self.long and not long_busy and time.time() - self.last_long_start >= every:
            self.last_long_start = time.time()
            return self.long[0]
        if not self.quick:
            return None
        spec = self.quick[self.quick_index % len(self.quick)]
        self.quick_index += 1
        return spec

    def worker(self, spec, binary, slot):
        rec = None
        try:
            rec = harness.run_scenario(
                spec, binary, self.results_dir, abandon=self.abandon,
                endpoint_override=self.args.endpoint, log=self.log,
            )
        except Exception as exc:  # noqa: BLE001 - a harness bug must not kill the loop
            self.log(f"[{spec['name']}] harness error: {type(exc).__name__}: {exc}")
        finally:
            with self.lock:
                self.active.pop(slot, None)
                self.completed += 1
                if rec is not None:
                    self.last.append(
                        {
                            "scenario": rec["scenario"],
                            "status": rec["status"],
                            "reason": (rec.get("reason") or "")[:160],
                            "commit": (rec.get("commit") or "")[:12],
                            "finished_at": rec.get("finished_at"),
                        }
                    )
                    if report_mod.is_outage(rec):
                        self.consecutive_outages += 1
                        delay = min(60 * 2 ** (self.consecutive_outages - 1), 1800)
                        self.backoff_until = time.time() + delay
                        self.log(f"endpoint outage #{self.consecutive_outages}: backing off {delay}s")
                    elif rec.get("status") in ("pass", "fail"):
                        self.consecutive_outages = 0
            harness.rotate_runs(
                self.results_dir, keep=self.args.keep_runs, max_bytes=self.args.max_artifact_mb * 2**20
            )
            prune_child_target(self.results_dir, self.args.child_target_gb)

    def run(self):
        lock_fh = open(self.results_dir / "loop.pid", "a+")
        try:
            fcntl.flock(lock_fh, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit(f"another loop holds {self.results_dir / 'loop.pid'}")
        lock_fh.seek(0)
        lock_fh.truncate()
        lock_fh.write(str(os.getpid()))
        lock_fh.flush()
        self.install_signals()
        self.log(
            f"loop start pid={os.getpid()} source={self.source} concurrency={self.concurrency} "
            f"quick={[s['name'] for s in self.quick]} long={[s['name'] for s in self.long]}"
        )
        stop_file = self.results_dir / "STOP"
        stop_file.unlink(missing_ok=True)
        last_head_check = 0.0
        slot_seq = 0
        while True:
            now = time.time()
            if stop_file.exists() and not self.stopping.is_set():
                self.log("STOP file: finishing current runs, starting nothing new")
                self.state = "draining"
                self.stopping.set()
            if not self.stopping.is_set() and now - last_head_check >= self.args.poll_secs:
                last_head_check = now
                self.maybe_rebuild()
            with self.lock:
                n_active = len(self.active)
            if self.stopping.is_set():
                if n_active == 0:
                    break
            elif self.binary is None:
                self.state = "building" if self.building else "waiting for a binary"
            elif now < self.backoff_until:
                self.state = "backoff (endpoint outage)"
            else:
                self.state = "running"
                while n_active < self.concurrency and now >= self.backoff_until:
                    spec = self.next_spec()
                    if spec is None:
                        break
                    slot_seq += 1
                    binary = self.binary
                    with self.lock:
                        self.active[slot_seq] = {
                            "scenario": spec["name"],
                            "tier": spec.get("tier"),
                            "commit": binary.commit[:12],
                            "binary": binary.path,
                            "started_at": harness.utc_now(),
                        }
                        n_active = len(self.active)
                    threading.Thread(
                        target=self.worker, args=(spec, binary, slot_seq), daemon=True
                    ).start()
                    if self.args.stagger_secs:
                        time.sleep(self.args.stagger_secs)
            self.heartbeat()
            time.sleep(5)
        self.state = "stopped"
        self.heartbeat()
        self.log("loop stopped")
        return 0


def prune_child_target(results_dir, max_gb):
    """Drop the agents' shared cargo target dir when it outgrows `max_gb`."""
    path = Path(results_dir) / "child-target"
    if path.is_dir() and harness.dir_size(path) > max_gb * 2**30:
        shutil.rmtree(path, ignore_errors=True)


def cmd_loop(args):
    return Loop(args).run()


def cmd_status(args):
    results_dir = harness.default_results_dir(args.results_dir)
    hb = results_dir / "heartbeat.json"
    if not hb.exists():
        print(f"no heartbeat at {hb}")
        return 1
    data = json.loads(hb.read_text())
    age = time.time() - calendar.timegm(time.strptime(data["updated_at"], "%Y-%m-%dT%H:%M:%SZ"))
    alive = _pid_alive(data.get("pid"))
    print(json.dumps(data, indent=1))
    print(f"heartbeat age {age:.0f}s, pid {data.get('pid')} {'alive' if alive else 'NOT running'}")
    return 0 if alive and age < 120 else 1


def _pid_alive(pid):
    try:
        os.kill(int(pid), 0)
        return True
    except (OSError, TypeError, ValueError):
        return False


def cmd_stop(args):
    results_dir = harness.default_results_dir(args.results_dir)
    if args.finish:
        (results_dir / "STOP").write_text("finish current runs, then exit\n")
        print(f"wrote {results_dir / 'STOP'}: the loop exits after its current runs")
        return 0
    pid_file = results_dir / "loop.pid"
    try:
        pid = int(pid_file.read_text().strip())
    except (OSError, ValueError):
        print(f"no loop pid in {pid_file}")
        return 1
    if not _pid_alive(pid):
        print(f"loop pid {pid} is not running")
        return 1
    os.kill(pid, signal.SIGTERM)
    print(f"sent SIGTERM to loop pid {pid}: current runs are recorded as abandoned")
    return 0


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("--results-dir", help="default $LIVE_EVAL_RESULTS_DIR or $TMPDIR/selfware-live-eval")
    sub = p.add_subparsers(dest="cmd", required=True)

    def binary_opts(sp, rev_default="HEAD"):
        sp.add_argument("--binary", help="use this built binary instead of building")
        sp.add_argument("--commit", help="commit the --binary was built from (default: HEAD of --source)")
        sp.add_argument("--source", default=str(harness.REPO_ROOT), help="source repo/worktree (read only)")
        sp.add_argument("--rev", default=rev_default, help="revision of --source to build")
        sp.add_argument("--target-dir", help="CARGO_TARGET_DIR for builds (or LIVE_EVAL_TARGET_DIR)")
        sp.add_argument("--endpoint", help="override the config's endpoint")
        sp.add_argument("--concurrency", type=int, default=1, help=f"parallel runs (max {MAX_CONCURRENCY})")
        sp.add_argument("--scenarios", nargs="*", help="scenario names (default: all of the tier)")
        sp.add_argument("--hermetic-only", action="store_true",
                        help="only scenarios whose fixtures live in the repo (CI)")

    sp = sub.add_parser("list")
    sp.set_defaults(fn=cmd_list)

    sp = sub.add_parser("run")
    binary_opts(sp)
    sp.add_argument("--samples", type=int, default=1)
    sp.add_argument("--config-set", action="append", metavar="SECTION.KEY=VALUE",
                    help="set a config key for every scenario of this run (repeatable), e.g. "
                         "agent.done_check=true; records go under `<scenario>+key=value`")
    sp.add_argument("--tier", choices=["quick", "long"], default="quick",
                    help="tier to run when --scenarios is not given")
    sp.set_defaults(fn=cmd_run)

    sp = sub.add_parser("gate")
    binary_opts(sp)
    sp.add_argument("--samples", type=int, default=5,
                    help="runs per scenario (default 5: at 3, only a 3/3 -> 0/3 drop is testable)")
    sp.add_argument("--baseline",
                    help="baseline commit (default: the candidate's nearest ancestor with results)")
    sp.add_argument("--thresholds")
    sp.add_argument("--out", help="also write the rendered report here")
    sp.set_defaults(fn=cmd_gate)

    sp = sub.add_parser("report")
    sp.add_argument("--results", help="results.jsonl (default: in the results dir)")
    sp.add_argument("--thresholds")
    sp.add_argument("--candidate")
    sp.add_argument("--baseline")
    sp.add_argument("--scenarios", nargs="*")
    sp.add_argument("--json", action="store_true")
    sp.add_argument("--out")
    sp.add_argument("--repo", default=str(harness.REPO_ROOT),
                    help="git repository used to order commits by ancestry (default: this one)")
    sp.add_argument("--expect-runs", type=int,
                    help="each requested scenario must have at least this many graded runs "
                         "at the candidate")
    sp.add_argument("--fail-on-regression", action="store_true",
                    help="exit 1 on a regression, a pass-rate WATCH, a pass rate below its "
                         "floor, missing runs, or any outage/harness failure/contamination")
    sp.set_defaults(fn=cmd_report)

    sp = sub.add_parser("loop")
    sp.add_argument("--source", required=True, help="worktree to follow (read only)")
    sp.add_argument("--target-dir")
    sp.add_argument("--endpoint")
    sp.add_argument("--concurrency", type=int, default=2)
    sp.add_argument("--scenarios", nargs="*", help="quick scenarios to rotate (default: all quick)")
    sp.add_argument("--no-long", action="store_true", help="never run the long scenario")
    sp.add_argument("--long-every-hours", type=float, default=6.0,
                    help="start review-core-long at most this often (one at a time)")
    sp.add_argument("--poll-secs", type=int, default=300, help="how often to check the source HEAD")
    sp.add_argument("--build-retry-secs", type=int, default=1800,
                    help="retry a head whose build failed after this long")
    sp.add_argument("--stagger-secs", type=int, default=30)
    sp.add_argument("--keep-runs", type=int, default=300)
    sp.add_argument("--max-artifact-mb", type=int, default=2048)
    sp.add_argument("--child-target-gb", type=int, default=20)
    sp.set_defaults(fn=cmd_loop)

    sp = sub.add_parser("status")
    sp.set_defaults(fn=cmd_status)

    sp = sub.add_parser("stop")
    sp.add_argument("--finish", action="store_true", help="drain: finish current runs, then exit")
    sp.set_defaults(fn=cmd_stop)

    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
