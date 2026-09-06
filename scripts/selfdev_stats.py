#!/usr/bin/env python3
"""Selfdev observability — live token/request/cost stats across every
selfware consumer, built from the artifacts the runs already produce.

Sources (no new infra, no endpoints needed):
- Harbor trials  jobs/*/agent/selfware.txt  (llm_request_sent/response events,
  outcome lines with tokens+cost) + jobs/*/result.json (rewards)
- Vero runs      vero/agent_runs/*/source/agent_events.jsonl + eval reports
- Red-team       tests/redteam/corpus/*.jsonl growth

Endpoint attribution comes from each job's harbor config filename:
  selfware-harbor.toml       -> glm/openrouter (paid)
  selfware-harbor-local.toml -> ablit/localhost:31000
  selfware-harbor-lan.toml   -> unc/192.168.137.1:8000
  selfware-harbor-flash1m.toml -> flash/llm.selfware.design

Usage:
  python3 scripts/selfdev_stats.py            # one-shot report
  python3 scripts/selfdev_stats.py --watch 30 # refresh every 30s
  python3 scripts/selfdev_stats.py --json out.json
"""

import argparse
import json
import os
import re
import subprocess
import time
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path

HARBOR_JOBS = Path("/home/rig/harbor-agents/jobs")
VERO_RUNS = Path("/home/rig/vero/agent_runs")
CORPUS = Path("/home/rig/selfware/tests/redteam/corpus")

# OpenRouter price per token (prompt, completion) for cost-equivalence.
# Measured from the OpenRouter /models API 2026-09-04. Local models priced at
# their closest hosted equivalent: 27B locals -> qwen3.8-27b ($0.42/$3.0 per M),
# flash-next (flagship class) -> qwen3.8-max ($2.0/$6.0 per M).
SHADOW_RATES = {
    "ablit/31000": (0.42e-6, 3.0e-6),
    "unc/lan8000": (0.42e-6, 3.0e-6),
    "flash/design": (2.0e-6, 6.0e-6),
    "glm/openrouter": (1.4e-6, 4.4e-6),
    "unknown": (1.4e-6, 4.4e-6),
}

CONFIG_ENDPOINT = {
    "selfware-harbor.toml": "glm/openrouter",
    "selfware-harbor-local.toml": "ablit/31000",
    "selfware-harbor-lan.toml": "unc/lan8000",
    "selfware-harbor-flash1m.toml": "flash/design",
}

REQ_RE = re.compile(r"\[(\d\d:\d\d:\d\d)\] kind=llm_request_sent prompt_tokens=(\d+)")
RESP_RE = re.compile(r"\[(\d\d:\d\d:\d\d)\] kind=llm_response_received .*completion_tokens=(\d+)")
TOKENS_RE = re.compile(r"tokens: (\d+) total(?:, cost \$([\d.]+))?")
REWARD_RE = re.compile(r"'([01]\.0)': \[(.*?)\]")


def endpoint_of(job_dir: Path) -> str:
    """Endpoint label from the trial config uploaded into the job."""
    # Containers rename the uploaded config to a generic path, so filename
    # matching is useless — attribute by the endpoint URL in the trial log.
    cfg = os.environ.get("SELFWARE_HARBOR_CONFIG_HINT", "")
    for name, label in CONFIG_ENDPOINT.items():
        if name == "selfware-harbor.toml":
            continue
        if name in cfg:
            return label
    url_labels = [
        ("openrouter.ai", "glm/openrouter"),
        ("192.168.137.1:8000", "unc/lan8000"),
        ("llm.selfware.design", "flash/design"),
        ("172.17.0.1:31000", "ablit/31000"),
        ("localhost:31000", "ablit/31000"),
    ]
    # Model-name fallback: the endpoint URL only appears in the log when the
    # key is MISSING (the config warning prints it) — a properly keyed run
    # never logs the URL and used to fall through to "unknown", splitting
    # GLM spend across two buckets (36 trials, $406 undercounted as of
    # 2026-09-05). `kind=step_started ... model=<id>` lines name the model.
    model_labels = [
        ("z-ai/glm", "glm/openrouter"),
        ("google/gemini", "gemini/openrouter"),
        ("qwen38-flash-next", "flash/design"),
        ("qwen38-unc", "unc/lan8000"),
        ("qwen38-next", "ablit/31000"),
    ]
    try:
        for trial in sorted(job_dir.glob("*__*/agent/selfware.txt"))[:2]:
            head = trial.read_text(errors="replace")[:20000]
            for marker, label in url_labels:
                if marker in head:
                    return label
            for marker, label in model_labels:
                if f"model={marker}" in head:
                    return label
    except OSError:
        pass
    return "unknown"


def scan_trials(job_dir: Path):
    """Per-trial token/cost/request events + rewards."""
    model_cache = None
    for trial in sorted(job_dir.glob("*__*/")):
        log = trial / "agent" / "selfware.txt"
        if not log.exists():
            continue
        if model_cache is None:
            model_cache = endpoint_of(job_dir)
        endpoint = model_cache
        mtime = log.stat().st_mtime
        requests = responses = 0
        prompt_toks = completion_toks = 0
        total_tokens = 0
        cost = 0.0
        first_ts = last_ts = None
        try:
            with log.open(errors="replace") as f:
                for line in f:
                    m = REQ_RE.search(line)
                    if m:
                        requests += 1
                        prompt_toks += int(m.group(2))
                        last_ts = mtime
                        if first_ts is None:
                            first_ts = mtime
                    m = RESP_RE.search(line)
                    if m:
                        responses += 1
                        completion_toks += int(m.group(2))
                    m = TOKENS_RE.search(line)
                    if m:
                        total_tokens = max(total_tokens, int(m.group(1)))
                        if m.group(2):
                            cost = max(cost, float(m.group(2)))
        except OSError:
            continue
        yield {
            "endpoint": endpoint,
            "trial": trial.name,
            "requests": requests,
            "responses": responses,
            "prompt_tokens": prompt_toks,
            "completion_tokens": completion_toks,
            "total_tokens": total_tokens,
            "cost": cost,
            "mtime": mtime,
        }


def job_rewards(job_dir: Path):
    try:
        data = json.loads((job_dir / "result.json").read_text())
    except (OSError, json.JSONDecodeError):
        return {}
    stats = data.get("stats", {})
    evals = stats.get("evals", {})
    for v in evals.values():
        return v.get("reward_stats", {}).get("reward", {})
    return {}


RATE_WINDOWS = [("1m", 60), ("5m", 300), ("15m", 900), ("1h", 3600)]


def scan_rates(now: float):
    """Per-endpoint tok/s (in/out) over rolling windows, from recent trial logs.

    Anchors each log's event timestamps to the file's mtime (container clocks
    differ; rates only need within-file consistency).
    """
    rates = defaultdict(lambda: {w: [0, 0] for w, _ in RATE_WINDOWS})
    # Red-team generator telemetry (all endpoints, incl. LAN + design which
    # run outside harbor jobs and otherwise never appear in live rates).
    usage_log = Path("/home/rig/selfdev/redteam_usage.jsonl")
    if usage_log.exists():
        try:
            with usage_log.open(errors="replace") as f:
                for line in f:
                    try:
                        rec = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    ts = rec.get("ts", 0)
                    ep = rec.get("ep", "other/?")
                    for w, secs in RATE_WINDOWS:
                        if now - ts <= secs:
                            rates[ep][w][0] += int(rec.get("prompt_tokens", 0))
                            rates[ep][w][1] += int(rec.get("completion_tokens", 0))
        except OSError:
            pass
    if not HARBOR_JOBS.exists():
        return rates
    for log in HARBOR_JOBS.glob("2026-*/*__*/agent/selfware.txt"):
        try:
            mtime = log.stat().st_mtime
        except OSError:
            continue
        if now - mtime > 3600:
            continue  # only files active in the last hour can contribute
        try:
            job_dir = log.parent.parent.parent
            ep = endpoint_of(job_dir)
            with log.open(errors="replace") as f:
                for line in f:
                    m = REQ_RE.search(line)
                    if m:
                        h, mi, s = map(int, m.group(1).split(":"))
                        ts = mtime - ((mtime % 86400) - (h * 3600 + mi * 60 + s))
                        if ts > mtime:
                            ts -= 86400
                        for w, secs in RATE_WINDOWS:
                            if now - ts <= secs:
                                rates[ep][w][0] += int(m.group(2))
                        continue
                    m = RESP_RE.search(line)
                    if m:
                        h, mi, s = map(int, m.group(1).split(":"))
                        ts = mtime - ((mtime % 86400) - (h * 3600 + mi * 60 + s))
                        if ts > mtime:
                            ts -= 86400
                        for w, secs in RATE_WINDOWS:
                            if now - ts <= secs:
                                rates[ep][w][1] += int(m.group(2))
        except OSError:
            continue
    return rates


def bucket(t: float, now: float) -> str:
    age = now - t
    if age <= 3600:
        return "1h"
    if age <= 86400:
        return "24h"
    if age <= 7 * 86400:
        return "7d"
    return "older"


def report(now: float) -> str:
    per_ep = defaultdict(lambda: defaultdict(float))
    rewards = defaultdict(int)
    trials_seen = 0
    for job_dir in sorted(HARBOR_JOBS.glob("2026-*/"), reverse=True):
        if not job_dir.is_dir():
            continue
        jrewards = job_rewards(job_dir)
        job_ep = endpoint_of(job_dir)
        for reward, trials in jrewards.items():
            rewards[(job_ep, reward)] += len(trials)
        for r in scan_trials(job_dir):
            trials_seen += 1
            b = bucket(r["mtime"], now)
            if b == "older":
                continue
            ep = r["endpoint"]
            for b2 in {b, "24h" if b == "1h" else None, "7d"} - {None}:
                d = per_ep[(ep, b2)]
                d["trials"] += 1
                d["requests"] += r["requests"]
                d["prompt_tokens"] += r["prompt_tokens"]
                d["completion_tokens"] += r["completion_tokens"]
                d["total_tokens"] += r["total_tokens"]
                d["cost"] += r["cost"]

    # Vero runs
    vero = defaultdict(int)
    for run in sorted(VERO_RUNS.glob("*-selfware-*"), reverse=True)[:50]:
        events = run / "source" / "agent_events.jsonl"
        if not events.exists():
            continue
        age = now - events.stat().st_mtime
        if age > 7 * 86400:
            continue
        try:
            last = events.read_text(errors="replace").strip().splitlines()[-1]
            d = json.loads(last)
        except (OSError, json.JSONDecodeError, IndexError):
            continue
        name = run.name.split("-selfware-")[0]
        if d.get("kind") == "run_end":
            key = "finished_ok" if d.get("ok") else "finished_fail"
            vero[key] += 1
            rep = run / "eval" / "default" / "report.md"
            if rep.exists():
                m = re.search(r"Specs passed\*\*: (\d+) / (\d+)", rep.read_text())
                if m and int(m.group(1)) > 0:
                    vero["specs_passed_runs"] += 1
        else:
            vero["running"] += 1
        vero.setdefault("by_benchmark:" + name, 0)
        vero["by_benchmark:" + name] += 1

    # Corpus growth
    corpus_lines = 0
    for f in CORPUS.glob("*.jsonl"):
        if f.name.startswith("probe_"):
            continue
        corpus_lines += sum(1 for _ in f.open(errors="replace"))

    shadow = defaultdict(float)
    shadow_paid = defaultdict(float)
    out = []
    out.append(f"selfdev stats — {datetime.now().strftime('%F %T')}  ({trials_seen} trials scanned)")
    out.append("")
    hdr = f"{'endpoint':<16} {'window':>5} {'trials':>6} {'reqs':>6} {'prompt_tok':>12} {'compl_tok':>11} {'total_tok':>13} {'cost $':>9}"
    out.append(hdr)
    out.append("-" * len(hdr))
    for (ep, b), d in sorted(per_ep.items(), key=lambda x: (x[0][0], ["1h", "24h", "7d"].index(x[0][1]))):
        out.append(
            f"{ep:<16} {b:>5} {int(d['trials']):>6} {int(d['requests']):>6} "
            f"{int(d['prompt_tokens']):>12,} {int(d['completion_tokens']):>11,} "
            f"{int(d['total_tokens']):>13,} {d['cost']:>9.2f}"
        )
        pr, cr = SHADOW_RATES.get(ep, (0.0, 0.0))
        if ep in ("glm/openrouter", "unknown"):
            shadow_paid[b] += d["prompt_tokens"] * pr + d["completion_tokens"] * cr
        else:
            shadow[b] += d["prompt_tokens"] * pr + d["completion_tokens"] * cr
    out.append("")
    if shadow or shadow_paid:
        out.append("cost at OpenRouter rates:")
        for b in ["1h", "24h", "7d"]:
            saved = shadow.get(b, 0.0)
            paid = shadow_paid.get(b, 0.0)
            if saved or paid:
                out.append(f"  {b:>4}: saved by local fleet ${saved:,.2f}   (glm-equivalent spend ${paid:,.2f})")
        out.append("")
    if rewards:
        out.append("rewards by endpoint: " + ", ".join(
            f"{ep}={rw}×{n}" for (ep, rw), n in sorted(rewards.items())))
        out.append("")
    rates = scan_rates(now)
    if rates:
        out.append("LIVE RATES (tok/s in/out by endpoint)")
        hdr2 = f"{'endpoint':<16}" + "".join(f"  {w:>11}" for w, _ in RATE_WINDOWS)
        out.append(hdr2)
        out.append("-" * len(hdr2))
        for ep in sorted(rates):
            cells = []
            for w, secs in RATE_WINDOWS:
                i, o = rates[ep][w]
                cells.append(f"{i//secs:>5}/{o//secs:<5}")
                # total tok/s into 24h calc below
            out.append(f"{ep:<16}" + "  ".join(f"{c:>11}" for c in cells))
        # fleet totals
        fleet_bits = []
        for ep in sorted(rates):
            i, o = rates[ep]["1h"]
            fleet_bits.append(f"{ep} {(i + o) // 3600} tok/s")
        out.append("fleet total/h: " + ", ".join(fleet_bits))
        out.append("")

    out.append(f"vero: {dict((k, v) for k, v in vero.items() if not k.startswith('by_benchmark'))}")
    out.append(f"red-team corpus: {corpus_lines} attack cases")
    out.extend(redteam_wave_ledger())
    return "\n".join(out)


def redteam_wave_ledger() -> list:
    """The red-team wave arc from git history: each hardening/triage commit,
    with a running count of zero-hole waves. Proves the loop works, not just
    that it runs (AGENTS.md rule 3)."""
    try:
        log = subprocess.run(
            ["git", "log", "--oneline", "-80"], capture_output=True, text=True,
            cwd=Path(__file__).resolve().parent.parent, check=True,
        ).stdout
    except Exception:
        return []
    entries = []
    zero_streak = 0
    for line in log.splitlines():
        if "wave" not in line.lower():
            continue
        if "zero-hole" in line or "0 gate holes" in line:
            zero_streak += 1
        elif not line.startswith(" "):
            # first non-zero wave commit ends the current streak display
            pass
        entries.append(line)
    if not entries:
        return []
    out = ["", f"RED-TEAM WAVES ({len(entries)} commits; {zero_streak} zero-hole)"]
    for e in entries[:18]:
        out.append(f"  {e[:118]}")
    if len(entries) > 18:
        out.append(f"  … {len(entries) - 18} earlier")
    return out


HTML_TMPL = """<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="refresh" content="30">
<title>selfdev stats</title><style>
body{background:#0d1117;color:#c9d1d9;font:14px/1.45 ui-monospace,Menlo,Consolas,monospace;margin:2em}
h1{color:#58a6ff;font-size:1.2em} table{border-collapse:collapse;margin:1em 0}
td,th{padding:3px 14px 3px 0;text-align:right} td:first-child,th:first-child{text-align:left}
tr.hdr{color:#8b949e;border-bottom:1px solid #30363d} .sec{color:#8b949e;margin-top:1.2em}
.good{color:#3fb950}.bad{color:#f85149} pre{white-space:pre-wrap}
</style></head><body><h1>selfdev stats — __TS__</h1><pre>__BODY__</pre></body></html>"""


def render_html(text: str) -> str:
    import html as _html
    return (HTML_TMPL
            .replace("__TS__", datetime.now().strftime("%F %T"))
            .replace("__BODY__", _html.escape(text)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--watch", type=int, default=0, metavar="SECS")
    ap.add_argument("--json", metavar="OUT")
    ap.add_argument("--html", metavar="OUT", help="write an auto-refreshing HTML dashboard")
    ap.add_argument("--html-loop", action="store_true", help="keep regenerating the HTML (use with --html)")
    args = ap.parse_args()
    while True:
        now = time.time()
        text = report(now)
        print("\033[2J\033[H" + text if args.watch else text)
        if args.json:
            Path(args.json).write_text(text + "\n")
        if args.html:
            Path(args.html).write_text(render_html(text))
        if not args.watch and not args.html_loop:
            break
        time.sleep(args.watch or 30)


if __name__ == "__main__":
    main()
