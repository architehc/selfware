#!/usr/bin/env python3
"""
Selfware SWE-bench Pro Benchmark Aggregator

Scans runs10_* and runs50_* output directories, reads evaluation summaries,
and produces a ranked leaderboard with cost and speed estimates.

Usage:
    python benchmark.py [--output-dir .] [--snapshot snapshot.json]
"""

import argparse
import json
import re
from datetime import datetime, timezone
from pathlib import Path


def parse_summary(report_path: Path):
    """Read the structured evaluation report JSON.

    Falls back to the Markdown summary only when the JSON report is missing.
    """
    if report_path.exists():
        try:
            data = json.loads(report_path.read_text(encoding="utf-8"))
            return {
                "total": data.get("total_instances", 0),
                "completed": data.get("completed_instances", 0),
                "errored": data.get("errored_instances", 0),
                "passed": data.get("overall_passed_instances", 0),
                "pass_rate": 0.0,
                "fail_to_pass": (
                    data.get("fail_to_pass_passed", 0),
                    data.get("fail_to_pass_total", 0),
                ),
                "pass_to_pass": (
                    data.get("pass_to_pass_passed", 0),
                    data.get("pass_to_pass_total", 0),
                ),
            }
        except Exception:
            pass
    # Markdown fallback for legacy summaries.
    summary_path = report_path.with_name("evaluation_summary.md")
    if not summary_path.exists():
        return {
            "total": 0,
            "completed": 0,
            "errored": 0,
            "passed": 0,
            "pass_rate": 0.0,
            "fail_to_pass": (0, 0),
            "pass_to_pass": (0, 0),
        }
    text = summary_path.read_text(encoding="utf-8")
    result = {
        "total": 0,
        "completed": 0,
        "errored": 0,
        "passed": 0,
        "pass_rate": 0.0,
        "fail_to_pass": (0, 0),
        "pass_to_pass": (0, 0),
    }
    # The Markdown summary lists the total-instance headline first and the
    # "completed only" alternative afterwards; only capture the first match so
    # the conservative denominator is preserved.
    got_passed = False
    got_f2p = False
    got_p2p = False
    for line in text.splitlines():
        m = re.search(r"Total instances:\s*\*\*(\d+)\*\*", line)
        if m:
            result["total"] = int(m.group(1))
        m = re.search(r"Completed:\s*\*\*(\d+)\*\*", line)
        if m:
            result["completed"] = int(m.group(1))
        m = re.search(r"Errored:\s*\*\*(\d+)\*\*", line)
        if m:
            result["errored"] = int(m.group(1))
        if not got_passed:
            m = re.search(
                r"Overall passed instances[^*]*\*\*(\d+)/(\d+)\*\*", line
            )
            if m:
                result["passed"] = int(m.group(1))
                got_passed = True
        if not got_f2p:
            m = re.search(r"Fail-to-pass[^*]*\*\*(\d+)/(\d+)\*\*", line)
            if m:
                result["fail_to_pass"] = (int(m.group(1)), int(m.group(2)))
                got_f2p = True
        if not got_p2p:
            m = re.search(r"Pass-to-pass[^*]*\*\*(\d+)/(\d+)\*\*", line)
            if m:
                result["pass_to_pass"] = (int(m.group(1)), int(m.group(2)))
                got_p2p = True
    return result


def compute_conservative_pass_rate(result: dict) -> float:
    """Compute pass rate over total instances, counting errors as failed."""
    total = result.get("total", 0)
    if total == 0:
        return 0.0
    return result.get("passed", 0) / total


def load_pricing(registry_path: Path):
    """Load pricing from the central registry keyed by profile name."""
    pricing = {}
    if not registry_path.exists():
        return pricing
    try:
        import tomllib
        data = tomllib.loads(registry_path.read_text(encoding="utf-8"))
    except Exception:
        return pricing
    for category, profiles in data.items():
        if not isinstance(profiles, dict):
            continue
        for key, profile in profiles.items():
            if not isinstance(profile, dict):
                continue
            pricing[key] = (
                float(profile.get("cost_input_per_1m", 0.0)),
                float(profile.get("cost_output_per_1m", 0.0)),
            )
    return pricing


def estimate_cost(profile_name: str, predictions: int, pricing: dict):
    """Estimate cost from registry pricing."""
    if profile_name not in pricing:
        return None
    in_price, out_price = pricing[profile_name]
    # Rough heuristic: 15k input tokens and 8k output tokens per instance.
    cost = predictions * (15_000 / 1_000_000 * in_price + 8_000 / 1_000_000 * out_price)
    return round(cost, 4)


def estimate_speed(log_path: Path):
    """Estimate elapsed hours from log timestamps."""
    if not log_path.exists():
        return None
    text = log_path.read_text(encoding="utf-8", errors="ignore")
    lines = [ln for ln in text.splitlines() if "Starting model=" in ln or "Evaluation done" in ln or "Predictions done" in ln]
    if len(lines) < 2:
        return None
    try:
        ts_start = lines[0].split("]", 1)[0][1:]
        ts_end = lines[-1].split("]", 1)[0][1:]
        start = datetime.fromisoformat(ts_start)
        end = datetime.fromisoformat(ts_end)
        hours = max((end - start).total_seconds() / 3600, 0.01)
        return round(hours, 2)
    except Exception:
        return None


def collect_results(base_dir: Path, pricing: dict):
    rows = []
    for run_dir in sorted(base_dir.glob("runs*_*")):
        if not run_dir.is_dir():
            continue
        if run_dir.name.endswith(".agent.log"):
            continue
        m = re.match(r"runs(\d+)_(.+)", run_dir.name)
        if not m:
            continue
        sample_size = int(m.group(1))
        model = m.group(2)
        report = run_dir / "eval" / "evaluation_report.json"
        preds_file = run_dir / "out" / "predictions.jsonl"
        preds_count = sum(1 for _ in preds_file.open(encoding="utf-8") if _.strip()) if preds_file.exists() else 0
        log_file = run_dir / "agent.log"
        elapsed = estimate_speed(log_file)
        row = {
            "model": model,
            "sample_size": sample_size,
            "predictions": preds_count,
            "completed": None,
            "passed": None,
            "pass_rate": None,
            "fail_to_pass": None,
            "pass_to_pass": None,
            "estimated_cost_usd": estimate_cost(model, preds_count, pricing),
            "elapsed_hours": elapsed,
            "instances_per_hour": round(sample_size / elapsed, 2) if elapsed and elapsed > 0 else None,
        }
        if report.exists() or (run_dir / "eval" / "evaluation_summary.md").exists():
            s = parse_summary(report)
            s["pass_rate"] = compute_conservative_pass_rate(s)
            row.update({
                "completed": s["completed"],
                "passed": s["passed"],
                "pass_rate": s["pass_rate"],
                "fail_to_pass": s["fail_to_pass"],
                "pass_to_pass": s["pass_to_pass"],
            })
            row["estimated_cost_usd"] = estimate_cost(model, s["total"], pricing)
        rows.append(row)
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", default=".", type=Path)
    parser.add_argument("--snapshot", default=None, type=Path)
    args = parser.parse_args()

    base_dir = args.output_dir.resolve()
    # Try a few likely registry locations relative to the base directory.
    candidates = [
        base_dir.parent / "projecte2e" / "config" / "openrouter_models.toml",
        Path(__file__).resolve().parent.parent / "projecte2e" / "config" / "openrouter_models.toml",
        Path("system_tests/projecte2e/config/openrouter_models.toml").resolve(),
    ]
    registry_path = next((p for p in candidates if p.exists()), candidates[0])

    pricing = load_pricing(registry_path)
    rows = collect_results(base_dir, pricing)
    rows.sort(key=lambda r: (r["pass_rate"] if r["pass_rate"] is not None else -1, r["predictions"]), reverse=True)

    timestamp = datetime.now(timezone.utc).isoformat()
    snapshot = {
        "generated_at": timestamp,
        "runs": rows,
    }

    snapshot_path = args.snapshot or base_dir / "benchmark_snapshot.json"
    snapshot_path.write_text(json.dumps(snapshot, indent=2), encoding="utf-8")

    md_path = base_dir / "benchmark_leaderboard.md"
    lines = ["# Selfware SWE-bench Pro Benchmark Leaderboard\n\n"]
    lines.append(f"*Generated: {timestamp}*\n\n")
    lines.append("| Rank | Model | Sample | Pass | Completed | Pass Rate (total) | $/instance | Speed (inst/hr) |\n")
    lines.append("|------|-------|--------|------|-----------|-------------------|------------|------------------|\n")
    for i, r in enumerate(rows, 1):
        rate = f"{r['pass_rate']:.1%}" if r["pass_rate"] is not None else "N/A"
        comp = r["completed"] if r["completed"] is not None else r["predictions"]
        passed = r["passed"] if r["passed"] is not None else "N/A"
        cost = f"${r['estimated_cost_usd']:.4f}" if r.get("estimated_cost_usd") is not None else "N/A"
        speed = f"{r['instances_per_hour']:.2f}" if r.get("instances_per_hour") is not None else "N/A"
        lines.append(f"| {i} | {r['model']} | {r['sample_size']} | {passed} | {comp} | {rate} | {cost} | {speed} |\n")

    md_path.write_text("".join(lines), encoding="utf-8")
    print(f"Wrote {snapshot_path} ({len(rows)} runs)")
    print(f"Wrote {md_path}")


if __name__ == "__main__":
    main()
