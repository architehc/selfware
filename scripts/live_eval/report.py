"""Aggregate live-eval JSONL records into per-scenario statistics.

Groups records by (scenario, commit), orders commits by git ancestry (when a
repository is given; else by first record), and compares the candidate with
its nearest ancestor that has records, using thresholds.json. Pass rates use
a one-sided Fisher exact test, continuous metrics a one-sided Mann-Whitney U
test: a change is a REGRESSION only when it is both beyond its threshold and
significant with enough runs on both sides, otherwise WATCH.
"""

import json
import math
import subprocess
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


def mann_whitney_greater(xs, ys):
    """One-sided p that `xs` tends to be LARGER than `ys` (Mann-Whitney U).

    Exact null distribution by dynamic programming for small samples (ties
    count 1/2, compared against the no-ties distribution, which is slightly
    conservative), normal approximation with continuity correction beyond.
    """
    m, n = len(xs), len(ys)
    if m == 0 or n == 0:
        return 1.0
    u = sum(1.0 if x > y else 0.5 if x == y else 0.0 for x in xs for y in ys)
    if m * n <= 400:
        counts = _u_distribution(m, n)
        total = sum(counts)
        need = math.ceil(u - 1e-9)
        return sum(counts[need:]) / total
    mu = m * n / 2
    sigma = math.sqrt(m * n * (m + n + 1) / 12)
    z = (u - 0.5 - mu) / sigma
    return 0.5 * math.erfc(z / math.sqrt(2))


def _u_distribution(m, n):
    """Number of arrangements giving each U = 0..m*n (no ties)."""
    # f(m, n, u) = f(m-1, n, u-n) + f(m, n-1, u)
    table = {}

    def f(a, b):
        if (a, b) in table:
            return table[(a, b)]
        if a == 0 or b == 0:
            res = [1] + [0] * (a * b)
        else:
            left, right = f(a - 1, b), f(a, b - 1)
            res = [0] * (a * b + 1)
            for u, c in enumerate(left):
                res[u + b] += c
            for u, c in enumerate(right):
                res[u] += c
        table[(a, b)] = res
        return res

    return f(m, n)


def is_outage(rec):
    reason = rec.get("reason") or ""
    return reason.startswith("endpoint_unreachable") or reason.startswith("endpoint_error_mid_run")


def is_harness_failure(rec):
    reason = rec.get("reason") or ""
    return reason.startswith("setup_failed") or reason.startswith("harness_error")


# Per-run metrics compared between commits:
# (name, record metric or None for a derived one, threshold key, direction, kind)
# direction +1: higher is worse. kind "ratio": relative change of the median;
# "abs": absolute change of the mean.
CONTINUOUS = (
    ("wall", "wall_s", "wall_p50_increase_ratio", +1, "ratio"),
    ("tokens", "total_tokens", "tokens_p50_increase_ratio", +1, "ratio"),
    ("recall", "planted_recall", "recall_drop", -1, "abs"),
    ("false_findings", "false_findings", "false_findings_increase", +1, "abs"),
    ("wrong_citation_rate", None, "citations_wrong_rate_increase", +1, "abs"),
    ("coverage", "coverage_percent", "coverage_drop_points", -1, "abs"),
    ("intervention_rate", "intervention_rate", "intervention_rate_increase", +1, "abs"),
)


def summarize(records):
    """Statistics for one group of records (one scenario at one commit)."""
    graded = [r for r in records if r.get("status") in ("pass", "fail")]
    abandoned = sum(1 for r in records if r.get("status") == "abandoned")
    outages = sum(1 for r in graded if is_outage(r))
    harness_failures = sum(1 for r in graded if is_harness_failure(r))
    # Runs that measured the commit: not an outage, not a harness failure.
    reachable = [r for r in graded if not is_outage(r) and not is_harness_failure(r)]
    passes = sum(1 for r in graded if r["status"] == "pass")
    passes_reachable = sum(1 for r in reachable if r["status"] == "pass")
    ran = [r for r in reachable if r.get("metrics")]

    def metric(name):
        return [r["metrics"].get(name) for r in ran if r["metrics"].get(name) is not None]

    # Wrong citations only mean something where the run produced checkable
    # citations: a run that wrote nothing has no wrong citations to count.
    # They are compared as a per-run RATE (wrong / checked), over citing
    # runs, and the share of runs that cite at all is compared on its own —
    # a candidate that stops citing must not pass as "no wrong citations".
    citing = [r for r in ran if (r["metrics"].get("citations_total") or 0) > 0]
    wrong_rates = [
        (r["metrics"].get("citations_wrong") or 0) / r["metrics"]["citations_total"]
        for r in citing
    ]
    values = {name: metric(key) for name, key, _t, _d, _k in CONTINUOUS if key}
    values["wrong_citation_rate"] = wrong_rates

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
        "setup_failures": harness_failures,
        "abandoned": abandoned,
        "wall_p50": percentile(metric("wall_s"), 50),
        "wall_p90": percentile(metric("wall_s"), 90),
        "tokens_p50": percentile(metric("total_tokens"), 50),
        "decode_tok_s_p50": percentile(metric("decode_tok_s"), 50),
        "turns_p50": percentile(metric("num_turns"), 50),
        "coverage_mean": mean(metric("coverage_percent")),
        "recall_mean": mean(metric("planted_recall")),
        "false_findings_mean": mean(metric("false_findings")),
        "runs_with_citations": len(citing),
        "citing_fraction": round(len(citing) / len(ran), 4) if ran else None,
        "wrong_citation_rate_mean": mean(wrong_rates),
        "citations_wrong_mean": mean([r["metrics"].get("citations_wrong") for r in citing]),
        "citations_verified_mean": mean(metric("citations_verified")),
        "fixture_citations_wrong_mean": mean(metric("fixture_citations_wrong")),
        "intervention_rate_mean": mean(metric("intervention_rate")),
        "interventions_mean": mean(metric("interventions")),
        "done_checks_mean": mean(metric("done_checks")),
        "files_per_hour_mean": mean(metric("files_per_hour")),
        "contaminated": sum(1 for r in ran if r["metrics"].get("contamination_hits")),
        "failed_criteria": failed,
        "values": values,
        "n_ran": len(ran),
    }


def _center(values, kind):
    if not values:
        return None
    return percentile(values, 50) if kind == "ratio" else sum(values) / len(values)


def compare(base, cand, th):
    """Flags for candidate summary `cand` against baseline summary `base`.

    Returns (level, kind, message) triples; level REGRESSION or WATCH, kind
    `pass_rate`, `citing_fraction` or `metric`. Pass rates and the citing
    fraction: a drop >= pass_rate_drop is REGRESSION when the one-sided
    Fisher p < alpha and both sides have >= min_samples runs. Continuous
    metrics: a change beyond its threshold is REGRESSION when the one-sided
    Mann-Whitney p <= alpha_continuous with >= min_samples runs per side
    (per CITING run for the wrong-citation rate).
    """
    flags = []
    min_n = th["min_samples"]

    def rate_check(kind, pb, nb, pc, nc, label):
        if not nb or not nc:
            return
        rb, rc = pb / nb, pc / nc
        if rb - rc >= th["pass_rate_drop"]:
            p = fisher_one_sided(pb, nb, pc, nc)
            level = "REGRESSION" if (p < th["alpha"] and nb >= min_n and nc >= min_n) else "WATCH"
            flags.append((level, kind, f"{label} {rb:.2f} -> {rc:.2f} (n {nb}->{nc}, Fisher p={p:.3f})"))

    rate_check("pass_rate", base["passes_reachable"], base["n_reachable"],
               cand["passes_reachable"], cand["n_reachable"], "pass rate")
    rate_check("citing_fraction", base["runs_with_citations"], base["n_ran"],
               cand["runs_with_citations"], cand["n_ran"], "runs with citations")
    alpha_c = th.get("alpha_continuous", th["alpha"])
    for name, _key, tkey, direction, kind in CONTINUOUS:
        bv, cv = base["values"].get(name) or [], cand["values"].get(name) or []
        b, c = _center(bv, kind), _center(cv, kind)
        if b is None or c is None:
            continue
        limit = th[tkey]
        if kind == "ratio":
            if b <= 0:
                continue
            change = (c - b) / b * direction
            msg = f"{name} median {b:g} -> {c:g} ({(c - b) / b * 100:+.0f}%)"
        else:
            change = (c - b) * direction
            msg = f"{name} mean {b:.3g} -> {c:.3g}"
        if change < limit:
            continue
        worse, better = (cv, bv) if direction > 0 else ([-x for x in cv], [-x for x in bv])
        p = mann_whitney_greater(worse, better)
        enough = len(bv) >= min_n and len(cv) >= min_n
        level = "REGRESSION" if (enough and p <= alpha_c) else "WATCH"
        flags.append((level, "metric", f"{msg} (n {len(bv)}->{len(cv)}, Mann-Whitney p={p:.3f})"))
    return flags


def floor_checks(summaries, th):
    """(failures, few_runs) for the absolute pass-rate floors.

    A floor FAILS whenever the scenario's pass rate is below it, whatever the
    number of runs. The 95% Wilson interval and a note when there are fewer
    than min_samples runs (`few_runs`) are reported alongside; they never
    change the outcome.
    """
    failures, few_runs = [], []
    for scenario, floor in (th.get("floor_pass_rate") or {}).items():
        s = summaries.get(scenario)
        if not s or not s["n"]:
            continue
        lo, hi = s["pass_ci95"]
        if s["n"] < th["min_samples"]:
            few_runs.append(f"{scenario}: floor judged on {s['n']} run(s)")
        if s["pass_rate"] < floor:
            failures.append(
                f"{scenario}: pass rate {s['pass_rate']:.2f} below floor {floor} "
                f"(n={s['n']}, 95% CI {lo:.2f}-{hi:.2f})"
            )
    return failures, few_runs


def floor_failures(summaries, th):
    return floor_checks(summaries, th)[0]


class Ancestry:
    """`git merge-base --is-ancestor` answers for a repository, cached.

    None (unknown commit, or no repository) makes callers fall back to the
    order records were first seen, and the report says so.
    """

    def __init__(self, repo=None):
        self.repo = repo
        self.cache = {}

    def is_ancestor(self, a, b):
        if not self.repo or not a or not b:
            return None
        if (a, b) not in self.cache:
            try:
                res = subprocess.run(
                    ["git", "merge-base", "--is-ancestor", a, b], cwd=self.repo,
                    capture_output=True, timeout=30,
                )
                self.cache[(a, b)] = {0: True, 1: False}.get(res.returncode)
            except (OSError, subprocess.SubprocessError):
                self.cache[(a, b)] = None
        return self.cache[(a, b)]


def first_seen_order(records):
    seen = {}
    for r in sorted(records, key=lambda r: r.get("started_at") or ""):
        seen.setdefault(r.get("commit"), r.get("started_at"))
    return [c for c, _ in sorted(seen.items(), key=lambda kv: kv[1] or "")]


def commit_order(records, ancestry=None):
    """Commits oldest to newest: by git ancestry when known, else first seen.

    Sort key: how many of the other commits are its ancestors (a linear
    history sorts exactly), then first-seen order. Commits the repository
    does not know count no ancestors. (Ordering by first record alone made
    a v0.9.3 replay the "candidate" against a newer baseline.)
    """
    order = first_seen_order(records)
    if ancestry is None or not ancestry.repo:
        return order
    rank = {
        c: (sum(1 for d in order if d != c and ancestry.is_ancestor(d, c)), i)
        for i, c in enumerate(order)
    }
    return sorted(order, key=lambda c: rank[c])


def nearest_ancestor(cand, commits, ancestry):
    """The newest of `commits` that is an ancestor of `cand` (None if none)."""
    ancestors = [c for c in commits if c != cand and ancestry.is_ancestor(c, cand)]
    for c in ancestors:
        if not any(d != c and ancestry.is_ancestor(c, d) for d in ancestors):
            return c
    return None


def group(records):
    out = {}
    for r in records:
        out.setdefault(r.get("scenario"), {}).setdefault(r.get("commit"), []).append(r)
    return out


class OrderError(ValueError):
    """An explicit baseline that is newer than the candidate."""


def build_report(records, th, candidate=None, baseline=None, scenarios=None, repo=None,
                 expect_runs=None):
    """Structured report: per scenario, the candidate/baseline stats and flags.

    `candidate` defaults to the newest commit with records (by ancestry when
    `repo` is given) and `baseline` to the candidate's nearest ancestor with
    records (without a repo: the commit first seen before it). An explicit
    baseline that is a DESCENDANT of the candidate raises OrderError.
    `scenarios` requested with no candidate records, or fewer than
    `expect_runs` graded runs, are listed under `missing`.
    """
    ancestry = Ancestry(repo)
    if candidate and baseline and ancestry.is_ancestor(candidate, baseline) and candidate != baseline:
        raise OrderError(
            f"baseline {baseline[:12]} is newer than candidate {candidate[:12]} "
            "(it descends from it); swap them"
        )
    grouped = group(records)
    out = {"scenarios": {}, "regressions": [], "watch": [], "missing": [],
           "ordering": "git ancestry" if repo else "first seen (no repository given)"}
    names = sorted(set(grouped) | set(scenarios or []))
    for scenario in names:
        if scenarios and scenario not in scenarios:
            continue
        by_commit = grouped.get(scenario, {})
        order = [c for c in commit_order(records, ancestry) if c in by_commit]
        if candidate is not None:
            cand = next((c for c in by_commit if c == candidate or c.startswith(candidate)), None)
        else:
            cand = order[-1] if order else None
        graded_n = summarize(by_commit[cand])["n"] if cand else 0
        if cand is None or (expect_runs and graded_n < expect_runs):
            out["missing"].append(
                f"{scenario}: {graded_n} graded run(s) at "
                f"{(candidate or cand or 'any commit')[:12]}"
                + (f", expected {expect_runs}" if expect_runs else "")
            )
            if cand is None:
                continue
        if baseline is not None:
            base = next((c for c in by_commit if c == baseline or c.startswith(baseline)), None)
        elif repo:
            # Only an ANCESTOR is a baseline: a sibling branch is not "before".
            base = nearest_ancestor(cand, list(by_commit), ancestry)
        else:
            earlier = order[: order.index(cand)]
            base = earlier[-1] if earlier else None
        cs = summarize(by_commit[cand])
        entry = {"candidate": cand, "candidate_stats": cs, "baseline": base, "flags": []}
        if base is not None:
            bs = summarize(by_commit[base])
            entry["baseline_stats"] = bs
            entry["flags"] = compare(bs, cs, th)
            for level, kind, msg in entry["flags"]:
                target = out["regressions"] if level == "REGRESSION" else out["watch"]
                target.append((kind, f"{scenario}: {msg}"))
        out["scenarios"][scenario] = entry
    cand_summaries = {s: e["candidate_stats"] for s, e in out["scenarios"].items()}
    out["floor_failures"], out["few_runs"] = floor_checks(cand_summaries, th)
    # Runs that measured nothing about the commit, which a gate cannot pass.
    out["uncertified"] = [
        f"{s}: {cs['outages']} outage(s), {cs['setup_failures']} setup/harness failure(s)"
        for s, cs in cand_summaries.items()
        if cs["outages"] or cs["setup_failures"]
    ]
    out["contaminated"] = [
        f"{s}: {cs['contaminated']} run(s) touched harness state outside the workspace"
        for s, cs in cand_summaries.items() if cs["contaminated"]
    ]
    return out


def _fmt(v, digits=2):
    if v is None:
        return "-"
    if isinstance(v, float):
        return f"{v:.{digits}f}"
    return str(v)


def render(report):
    lines = [f"(baseline = nearest earlier commit by {report.get('ordering', '?')})"]
    for scenario, e in report["scenarios"].items():
        cs = e["candidate_stats"]
        lo, hi = cs["pass_ci95"]
        lines.append(f"== {scenario}  @ {str(e['candidate'])[:10]}")
        lines.append(
            f"   pass {cs['passes']}/{cs['n']} = {_fmt(cs['pass_rate'])} "
            f"(95% CI {_fmt(lo)}-{_fmt(hi)})  outages {cs['outages']}  "
            f"harness/setup failures {cs['setup_failures']}  abandoned {cs['abandoned']}"
        )
        # Nearest-rank p90 of fewer than 10 runs is just the maximum.
        tail = "p90" if cs["n_ran"] >= 10 else "max"
        lines.append(
            f"   wall p50/{tail} {_fmt(cs['wall_p50'])}/{_fmt(cs['wall_p90'])} s  "
            f"tokens p50 {_fmt(cs['tokens_p50'])}  turns p50 {_fmt(cs['turns_p50'])}  "
            f"endpoint tok/s p50 {_fmt(cs['decode_tok_s_p50'])}"
        )
        lines.append(
            f"   coverage {_fmt(cs['coverage_mean'])}%  recall {_fmt(cs['recall_mean'])}  "
            f"false findings {_fmt(cs['false_findings_mean'])}  "
            f"citing runs {cs['runs_with_citations']}/{cs['n_ran']}  "
            f"wrong-citation rate {_fmt(cs['wrong_citation_rate_mean'], 3)} "
            f"(independent wrong {_fmt(cs['fixture_citations_wrong_mean'])})  "
            f"interventions/turn {_fmt(cs['intervention_rate_mean'], 3)}"
            + (
                f"  done-checks {_fmt(cs['done_checks_mean'])}"
                if cs["done_checks_mean"] is not None
                else ""
            )
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
            for level, _kind, msg in e["flags"]:
                lines.append(f"   {level}: {msg}")
        else:
            lines.append("   no baseline commit")
    lines.append("")
    for key, label in (("missing", "MISSING"), ("floor_failures", "BELOW FLOOR"),
                       ("few_runs", "NOTE"), ("uncertified", "UNCERTIFIED"),
                       ("contaminated", "CONTAMINATED")):
        for f in report.get(key, []):
            lines.append(f"{label}: {f}")
    lines.append(
        f"regressions: {len(report['regressions'])}  watch: {len(report['watch'])}  "
        f"below floor: {len(report['floor_failures'])}  missing: {len(report['missing'])}"
    )
    return "\n".join(lines)


def blocking_problems(report, strict_watch_kinds=("pass_rate", "citing_fraction")):
    """What fails a gate / `report --fail-on-regression`: regressions, a
    pass-rate or citing-fraction WATCH (a release is not certified on a drop
    the samples were too few to test), floors, missing runs, outages/harness
    failures, contamination. (`few_runs` is a note, never a failure.)"""
    out = [m for _k, m in report["regressions"]]
    out += [m for k, m in report["watch"] if k in strict_watch_kinds]
    for key in ("missing", "floor_failures", "uncertified", "contaminated"):
        out += report.get(key, [])
    return out
