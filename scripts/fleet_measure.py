#!/usr/bin/env python3
"""Fleet measurement snapshot: per-endpoint tok/s (1m/5m/15m/1h), corpus
size, and gate-violation trend. Appends one TSV row to
/home/rig/selfdev/measure_log.tsv and prints a compact summary.

Usage: python3 scripts/fleet_measure.py [--gate]
  --gate also runs the full redteam gate and records the violation count
  (takes ~3 min; intended for hourly snapshots).
"""
import json, subprocess, sys, time
from pathlib import Path

USAGE = Path("/home/rig/selfdev/redteam_usage.jsonl")
CORPUS = Path("/home/rig/selfware/tests/redteam/corpus")
LOG = Path("/home/rig/selfdev/measure_log.tsv")

now = time.time()
buckets = {60: {}, 300: {}, 900: {}, 3600: {}}
if USAGE.exists():
    for line in USAGE.read_text().splitlines():
        try:
            r = json.loads(line)
        except json.JSONDecodeError:
            continue
        ts = r.get("ts", 0)
        ep = r.get("ep") or r.get("endpoint_label") or r.get("endpoint", "?")
        tin = r.get("prompt_tokens") or r.get("tokens_in", 0)
        tout = r.get("completion_tokens") or r.get("tokens_out", 0)
        for w, agg in buckets.items():
            if now - ts <= w:
                a = agg.setdefault(ep, [0, 0])
                a[0] += tin
                a[1] += tout

corpus_n = 0
for f in CORPUS.glob("*.jsonl"):
    corpus_n += sum(1 for _ in f.open())

def rate(w):
    parts = []
    for ep in sorted(buckets[w]):
        a = buckets[w][ep]
        parts.append(f"{ep} {a[0]//w}/{a[1]//w}")
    return "; ".join(parts) if parts else "-"

GATE_ARTIFACTS = Path("/home/rig/selfdev/gate_artifacts")

def run_gate():
    """Run the redteam gate once. Returns (status, violations, exit_code, artifact).

    status: pass | violations | compile-failure | infra-failure | timeout.
    violations is None unless the tests actually ran to completion —
    unknown must remain unknown, never reported as 0 (external review
    finding 13: a mocked cargo exit 101 + compile error printed
    gate_violations=0). Raw cargo output is kept in the artifact file.
    """
    GATE_ARTIFACTS.mkdir(parents=True, exist_ok=True)
    artifact = GATE_ARTIFACTS / f"gate_{time.strftime('%Y%m%dT%H%M%S')}.log"
    try:
        p = subprocess.run(
            ["cargo", "test", "--test", "redteam_gate_test"],
            cwd="/home/rig/selfware", capture_output=True, text=True, timeout=600)
    except FileNotFoundError:
        return "infra-failure", None, None, None
    except subprocess.TimeoutExpired as e:
        out = e.stdout if isinstance(e.stdout, str) else ""
        err = e.stderr if isinstance(e.stderr, str) else ""
        artifact.write_text(f"TIMEOUT after 600s\nSTDOUT:\n{out}\nSTDERR:\n{err}\n")
        return "timeout", None, None, artifact
    artifact.write_text(
        f"exit_code={p.returncode}\nSTDOUT:\n{p.stdout}\nSTDERR:\n{p.stderr}\n")
    out = p.stdout + p.stderr
    if "error[" in out or "could not compile" in out:
        return "compile-failure", None, p.returncode, artifact
    if "test result:" not in p.stdout:
        # cargo exited before running any test binary
        return "infra-failure", None, p.returncode, artifact
    n = out.count("attack was ALLOWED")
    status = "pass" if p.returncode == 0 and n == 0 else "violations"
    return status, n, p.returncode, artifact

gate_viol = ""
gate_status = ""
gate_artifact = None
if "--gate" in sys.argv:
    gate_status, n, code, gate_artifact = run_gate()
    gate_viol = "" if n is None else str(n)
    if code is not None:
        gate_status += f"(exit={code})"

row = [time.strftime("%Y-%m-%dT%H:%M:%S"), str(corpus_n), gate_viol,
       rate(60), rate(300), rate(900), rate(3600), gate_status]
new = not LOG.exists()
with LOG.open("a") as fh:
    if new:
        fh.write("ts\tcorpus_cases\tgate_violations\ttok_in/out_1m\ttok_in/out_5m\ttok_in/out_15m\ttok_in/out_1h\tgate_status\n")
    fh.write("\t".join(row) + "\n")

if "--gate" in sys.argv:
    print(f"corpus={corpus_n} gate_violations={gate_viol or 'unknown'} gate_status={gate_status}")
    if gate_artifact and not gate_status.startswith("pass"):
        print(f"  gate artifact: {gate_artifact}")
else:
    print(f"corpus={corpus_n} gate_violations=n/a")
print(f" 1m: {rate(60)}")
print(f" 5m: {rate(300)}")
print(f"15m: {rate(900)}")
print(f" 1h: {rate(3600)}")
