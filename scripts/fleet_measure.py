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

gate_viol = ""
if "--gate" in sys.argv:
    p = subprocess.run(
        ["cargo", "test", "--test", "redteam_gate_test"],
        cwd="/home/rig/selfware", capture_output=True, text=True, timeout=600)
    gate_viol = str(p.stdout.count("attack was ALLOWED"))

row = [time.strftime("%Y-%m-%dT%H:%M:%S"), str(corpus_n), gate_viol,
       rate(60), rate(300), rate(900), rate(3600)]
new = not LOG.exists()
with LOG.open("a") as fh:
    if new:
        fh.write("ts\tcorpus_cases\tgate_violations\ttok_in/out_1m\ttok_in/out_5m\ttok_in/out_15m\ttok_in/out_1h\n")
    fh.write("\t".join(row) + "\n")

print(f"corpus={corpus_n} gate_violations={gate_viol or 'n/a'}")
print(f" 1m: {rate(60)}")
print(f" 5m: {rate(300)}")
print(f"15m: {rate(900)}")
print(f" 1h: {rate(3600)}")
