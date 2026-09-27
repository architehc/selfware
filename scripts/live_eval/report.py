"""Aggregate live-eval JSONL records into per-scenario statistics.

Groups records by (scenario, commit), orders commits by when they were
first seen, and compares the latest commit with the previous one using the
thresholds in thresholds.json. Pure functions over record dicts, so the
regression logic is unit-tested offline.
"""

import json
import math
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load_thresholds(path=None):
    data = json.loads(Path(path or HERE / "thresholds.json").read_text())
    return {k: v["value"] for k, v in data.items() if not k.startswith("_")}


def load_records(path):
    records = []
    p = Path(path)
    if not p.exists():
        return records
    for line in p.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return records


def percentile(values, q):
    """Nearest-rank percentile (q in 0..100) of a non-empty list; None if empty."""
    vals = sorted(v for v in values if v is not None)
    if not vals:
        return None
    rank = max(1, math.ceil(q / 100 * len(vals)))
    return vals[rank - 1]


def mean(values):
    vals = [v for v in values if v is not None]
    return round(sum(vals) / len(vals), 4) if vals else None


def wilson(passes, n, z=1.96):
    """95% Wilson score interval for a pass rate."""
    if n == 0:
        return (None, None)
    p = passes / n
    denom = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return (round(max(0.0, centre - half), 3), round(min(1.0, centre + half), 3))


def fisher_one_sided(pass_a, n_a, pass_b, n_b):
    """P(candidate B has this few passes or fewer | same rate as A).

    One-sided Fisher exact test on the 2x2 table, hypergeometric tail.
    """
    total_pass = pass_a + pass_b
    total = n_a + n_b
    if total == 0 or n_b == 0:
        return 1.0
    denom = math.comb(total, n_b)
    p = 0.0
    for k in range(0, pass_b + 1):
        if k > total_pass or n_b - k > total - total_pass:
            continue
        p += math.comb(total_pass, k) * math.comb(total - total_pass, n_b - k) / denom
    return min(1.0, p)


def is_outage(rec):
    reason = rec.get("reason") or ""
    return reason.startswith("endpoint_unreachable") or reason.startswith("endpoint_error_mid_run")


def summarize(records):
    """Statistics for one group of records (one scenario at one commit)."""
    graded = [r for r in records if r.get("status") in ("pass", "fail")]
    abandoned = sum(1 for r in records if r.get("status") == "abandoned")
    outages = sum(1 for r in graded if is_outage(r))
    reachable = [r for r in graded if not is_outage(r)]
    passes = sum(1 for r in graded if r["status"] == "pass")
    passes_reachable = sum(1 for r in reachable if r["status"] == "pass")
    ran = [r for r in reachable if r.get("metrics")]

    def metric(name):
        return [r["metrics"].get(name) for r in ran if r["metrics"].get(name) is not None]

    # Wrong citations only mean something where the run produced checkable
    # citations: a run that wrote nothing has 0 wrong and must not make a
    # later run that cites (and gets 3 of 44 wrong) look like a regression
    # (0.9.3 vs fix-094 c24 A/B, 2026-09-27).
    citing = [r for r in ran if (r["metrics"].get("citations_total") or 0) > 0]
    wrong_citing = [r["metrics"].get("citations_wrong") for r in citing]

    n = len(graded)
    failed = {}
    for r in graded:
        for crit in r.get("failed_criteria") or []:
            failed[crit] = failed.get(crit, 0) + 1
    return {
        "n": n,
        "passes": passes,
        "pass_rate": round(passes / n, 4) if n else None,
        "pass_ci95": wilson(passes, n),
        "n_reachable": len(reachable),
        "passes_reachable": passes_reachable,
        "pass_rate_reachable": round(passes_reachable / len(reachable), 4) if reachable else None,
        "outages": outages,
        "setup_failures": sum(
            1 for r in graded if (r.get("reason") or "").startswith("setup_failed")
        ),
        "abandoned": abandoned,
        "wall_p50": percentile(metric("wall_s"), 50),
        "wall_p90": percentile(metric("wall_s"), 90),
        "tokens_p50": percentile(metric("total_tokens"), 50),
        "decode_tok_s_p50": percentile(metric("decode_tok_s"), 50),
        "turns_p50": percentile(metric("num_turns"), 50),
        "coverage_mean": mean(metric("coverage_percent")),
        "recall_mean": mean(metric("planted_recall")),
        "false_findings_mean": mean(metric("false_findings")),
        "citations_wrong_mean": mean(wrong_citing),
        "runs_with_citations": len(citing),
        "citations_verified_mean": mean(metric("citations_verified")),
        "intervention_rate_mean": mean(metric("intervention_rate")),
        "interventions_mean": mean(metric("interventions")),
        "files_per_hour_mean": mean(metric("files_per_hour")),
        "failed_criteria": failed,
    }


def commit_order(records):
    """Commits in order of their first record."""
    seen = {}
    for r in sorted(records, key=lambda r: r.get("started_at") or ""):
        seen.setdefault(r.get("commit"), r.get("started_at"))
    return [c for c, _ in sorted(seen.items(), key=lambda kv: kv[1] or "")]


def group(records):
    out = {}
    for r in records:
        out.setdefault(r.get("scenario"), {}).setdefault(r.get("commit"), []).append(r)
    return out


# (metric key, threshold key, direction, kind) — direction +1 means "higher is worse".
CONTINUOUS_CHECKS = (
    ("wall_p50", "wall_p50_increase_ratio", +1, "ratio"),
    ("tokens_p50", "tokens_p50_increase_ratio", +1, "ratio"),
    ("recall_mean", "recall_drop", -1, "abs"),
    ("false_findings_mean", "false_findings_increase", +1, "abs"),
    ("citations_wrong_mean", "citations_wrong_increase", +1, "abs"),
    ("coverage_mean", "coverage_drop_points", -1, "abs"),
    ("intervention_rate_mean", "intervention_rate_increase", +1, "abs"),
)


def compare(base, cand, th):
    """Flags for candidate summary `cand` against baseline summary `base`.

    Pass rates are compared over endpoint-reachable runs (an outage is a
    FAIL in the pass rate, but says nothing about the commit).
    Returns a list of (level, message) with level REGRESSION or WATCH.
    """
    flags = []
    min_n = th["min_samples"]
    nb, nc = base["n_reachable"], cand["n_reachable"]
    if nb and nc:
        rb = base["passes_reachable"] / nb
        rc = cand["passes_reachable"] / nc
        drop = rb - rc
        if drop >= th["pass_rate_drop"]:
            p = fisher_one_sided(base["passes_reachable"], nb, cand["passes_reachable"], nc)
            msg = f"pass rate {rb:.2f} -> {rc:.2f} (n {nb}->{nc}, Fisher p={p:.3f})"
            level = "REGRESSION" if (p < th["alpha"] and nb >= min_n and nc >= min_n) else "WATCH"
            flags.append((level, msg))
    for key, tkey, direction, kind in CONTINUOUS_CHECKS:
        b, c = base.get(key), cand.get(key)
        if b is None or c is None:
            continue
        limit = th[tkey]
        if kind == "ratio":
            worse = b > 0 and (c - b) / b >= limit if direction > 0 else False
            msg = f"{key} {b} -> {c} (+{(c - b) / b * 100:.0f}%)" if b > 0 else ""
        else:
            delta = (c - b) * direction
            worse = delta >= limit
            msg = f"{key} {b} -> {c}"
        if worse:
            enough = nb >= min_n and nc >= min_n
            flags.append(("REGRESSION" if enough else "WATCH", msg))
    return flags


def floor_failures(summaries, th):
    """Scenarios whose pass rate is below the absolute floor."""
    out = []
    for scenario, floor in (th.get("floor_pass_rate") or {}).items():
        s = summaries.get(scenario)
        if s and s["n"] and s["pass_rate"] < floor:
            out.append(f"{scenario}: pass rate {s['pass_rate']:.2f} below floor {floor}")
    return out


def build_report(records, th, candidate=None, baseline=None, scenarios=None):
    """Structured report: per scenario, the candidate/baseline stats and flags.

    `candidate` defaults to the latest commit seen for the scenario and
    `baseline` to the commit seen before it.
    """
    grouped = group(records)
    out = {"scenarios": {}, "regressions": [], "watch": []}
    for scenario in sorted(grouped):
        if scenarios and scenario not in scenarios:
            continue
        by_commit = grouped[scenario]
        order = [c for c in commit_order(records) if c in by_commit]
        cand = candidate if candidate in by_commit else (order[-1] if candidate is None else None)
        if cand is None:
            continue
        if baseline is not None:
            base = baseline if baseline in by_commit else None
        else:
            earlier = order[: order.index(cand)] if cand in order else []
            base = earlier[-1] if earlier else None
        cs = summarize(by_commit[cand])
        entry = {"candidate": cand, "candidate_stats": cs, "baseline": base, "flags": []}
        if base is not None:
            bs = summarize(by_commit[base])
            entry["baseline_stats"] = bs
            entry["flags"] = compare(bs, cs, th)
            for level, msg in entry["flags"]:
                target = out["regressions"] if level == "REGRESSION" else out["watch"]
                target.append(f"{scenario}: {msg}")
        out["scenarios"][scenario] = entry
    cand_summaries = {s: e["candidate_stats"] for s, e in out["scenarios"].items()}
    out["floor_failures"] = floor_failures(cand_summaries, th)
    # Runs that measured nothing about the commit, which a gate cannot pass.
    out["uncertified"] = [
        f"{s}: {cs['outages']} outage(s), {cs['setup_failures']} setup failure(s)"
        for s, cs in cand_summaries.items()
        if cs["outages"] or cs["setup_failures"]
    ]
    return out


def _fmt(v, digits=2):
    if v is None:
        return "-"
    if isinstance(v, float):
        return f"{v:.{digits}f}"
    return str(v)


def render(report):
    lines = []
    for scenario, e in report["scenarios"].items():
        cs = e["candidate_stats"]
        lo, hi = cs["pass_ci95"]
        lines.append(f"== {scenario}  @ {str(e['candidate'])[:10]}")
        lines.append(
            f"   pass {cs['passes']}/{cs['n']} = {_fmt(cs['pass_rate'])} "
            f"(95% CI {_fmt(lo)}-{_fmt(hi)})  outages {cs['outages']}  "
            f"abandoned {cs['abandoned']}"
        )
        lines.append(
            f"   wall p50/p90 {_fmt(cs['wall_p50'])}/{_fmt(cs['wall_p90'])} s  "
            f"tokens p50 {_fmt(cs['tokens_p50'])}  turns p50 {_fmt(cs['turns_p50'])}  "
            f"endpoint tok/s p50 {_fmt(cs['decode_tok_s_p50'])}"
        )
        lines.append(
            f"   coverage {_fmt(cs['coverage_mean'])}%  recall {_fmt(cs['recall_mean'])}  "
            f"false findings {_fmt(cs['false_findings_mean'])}  "
            f"citations wrong {_fmt(cs['citations_wrong_mean'])} "
            f"verified {_fmt(cs['citations_verified_mean'])}  "
            f"interventions/turn {_fmt(cs['intervention_rate_mean'], 3)}"
            + (
                f"  files/hour {_fmt(cs['files_per_hour_mean'])}"
                if cs["files_per_hour_mean"] is not None
                else ""
            )
        )
        if cs["failed_criteria"]:
            crit = ", ".join(f"{k}x{v}" for k, v in sorted(cs["failed_criteria"].items()))
            lines.append(f"   failed criteria: {crit}")
        if e.get("baseline"):
            bs = e["baseline_stats"]
            lines.append(
                f"   vs {str(e['baseline'])[:10]}: pass {bs['passes']}/{bs['n']}, "
                f"wall p50 {_fmt(bs['wall_p50'])}, recall {_fmt(bs['recall_mean'])}, "
                f"coverage {_fmt(bs['coverage_mean'])}"
            )
            for level, msg in e["flags"]:
                lines.append(f"   {level}: {msg}")
        else:
            lines.append("   no baseline commit yet")
    lines.append("")
    for f in report["floor_failures"]:
        lines.append(f"BELOW FLOOR: {f}")
    for f in report.get("uncertified", []):
        lines.append(f"UNCERTIFIED: {f}")
    lines.append(
        f"regressions: {len(report['regressions'])}  watch: {len(report['watch'])}  "
        f"below floor: {len(report['floor_failures'])}"
    )
    return "\n".join(lines)
