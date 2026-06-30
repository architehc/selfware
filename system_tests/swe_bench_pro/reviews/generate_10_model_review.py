"""Generate a consolidated review report from existing SWE-bench Pro eval reports."""

import json
import os
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REVIEW_DIR = Path(__file__).resolve().parent

# 10 smaller-parameter models selected from runs10_* directories that already
# have evaluation reports on disk.
MODELS = [
    "runs10_lfm-2.5-1.2b-thinking-free-sweap-v2",
    "runs10_llama-3.1-8b",
    "runs10_qwen2.5-7b",
    "runs10_mistral-nemo",
    "runs10_nova-lite",
    "runs10_granite-4.1-8b",
    "runs10_gemma-3-12b",
    "runs10_ling-2.6-flash",
    "runs10_deepseek-v3.2",
    "runs10_xiaomi-mimo-v2.5",
]


def load_report(model_dir: Path) -> dict | None:
    path = model_dir / "eval" / "evaluation_report.json"
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def classify_error(instance: dict) -> str:
    if instance.get("error"):
        return instance["error"]
    if instance.get("overall_pass"):
        return "passed"
    return "test failures"


def repo_from_instance_id(iid: str) -> str:
    """Extract a repo slug from an instance_id like instance_org__repo-...."""
    if iid.startswith("instance_"):
        rest = iid[len("instance_") :]
        # Repo slug ends at the first hyphen (the one before the commit hash).
        if "-" in rest:
            return rest.split("-", 1)[0]
    return "unknown"


def main() -> None:
    rows = []
    global_error_counter: Counter = Counter()
    repo_failure_counter: Counter = Counter()
    test_failure_counter: Counter = Counter()
    model_results: list[dict] = []
    failure_examples: dict[str, list[tuple[str, str]]] = defaultdict(list)

    for model in MODELS:
        model_name = model.replace("runs10_", "")
        report = load_report(ROOT / model)
        if report is None:
            rows.append(f"| {model_name} | — | — | — | — | — | (no eval report) |")
            continue

        total = report.get("total_instances", 0)
        completed = report.get("completed_instances", 0)
        errored = report.get("errored_instances", total - completed)
        overall_pass = report.get("overall_passed_instances", 0)
        # Use the total-instance (errors-counted-as-failed) rates as the headline
        # so shrinking denominators do not inflate the reported pass rate.
        overall_rate_total = report.get("overall_pass_rate_total", 0.0)
        ftp_rate_total = report.get("fail_to_pass_rate_total", 0.0)
        ptp_rate_total = report.get("pass_to_pass_rate_total", 0.0)

        rows.append(
            f"| {model_name} | {total} | {completed} | {errored} | {overall_pass}/{total} "
            f"({overall_rate_total:.1%}) | {ftp_rate_total:.1%} | {ptp_rate_total:.1%} |"
        )

        model_results.append(
            {
                "model": model_name,
                "overall_rate": overall_rate_total,
                "ftp_rate": ftp_rate_total,
                "ptp_rate": ptp_rate_total,
            }
        )

        for inst in report.get("per_instance", []):
            err = classify_error(inst)
            global_error_counter[err] += 1
            if err == "test failures":
                repo = repo_from_instance_id(inst["instance_id"])
                repo_failure_counter[repo] += 1
                for detail in inst.get("fail_to_pass_details", []):
                    if not detail.get("passed"):
                        test_name = detail.get("test", "unknown")
                        test_failure_counter[test_name] += 1
                        if len(failure_examples[test_name]) < 3:
                            failure_examples[test_name].append(
                                (model_name, inst["instance_id"])
                            )

    avg_overall = sum(r["overall_rate"] for r in model_results) / len(model_results)
    avg_ftp = sum(r["ftp_rate"] for r in model_results) / len(model_results)
    avg_ptp = sum(r["ptp_rate"] for r in model_results) / len(model_results)

    lines = [
        "# SWE-bench Pro 10-Model Review (Smaller Models)",
        "",
        "This report aggregates the existing evaluation reports for 10 smaller-parameter models.",
        "",
        "## Summary",
        "",
        f"- Models reviewed: **{len(MODELS)}**",
        f"- Average overall pass rate: **{avg_overall:.1%}**",
        f"- Average fail-to-pass rate: **{avg_ftp:.1%}**",
        f"- Average pass-to-pass rate: **{avg_ptp:.1%}**",
        "",
        "## Per-Model Results",
        "",
        "| Model | Total | Completed | Errored | Overall Pass (total) | Fail-to-Pass (total) | Pass-to-Pass (total) |",
        "|-------|-------|-----------|---------|----------------------|----------------------|----------------------|",
    ]
    lines.extend(rows)
    lines.append("")

    lines.extend(
        [
            "## Failure-Mode Breakdown",
            "",
            "| Failure Mode | Instance Count |",
            "|--------------|----------------|",
        ]
    )
    for err, count in global_error_counter.most_common():
        lines.append(f"| {err} | {count} |")
    lines.append("")

    lines.extend(
        [
            "## Top 3 Failure Modes with Examples",
            "",
        ]
    )
    # Build three concrete categories: NodeBB test failures, qutebrowser test failures, empty patches.
    top3 = [
        ("NodeBB test failures", repo_failure_counter.get("NodeBB__NodeBB", 0)),
        ("qutebrowser test failures", repo_failure_counter.get("qutebrowser__qutebrowser", 0)),
        ("empty patch", global_error_counter.get("empty patch", 0)),
    ]
    for idx, (err, count) in enumerate(top3, start=1):
        lines.append(f"### {idx}. {err} ({count} instances)")
        lines.append("")
        if "test failures" in err:
            repo_key = "NodeBB__NodeBB" if "NodeBB" in err else "qutebrowser__qutebrowser"
            lines.append("Most frequently failing tests in this repo:")
            lines.append("")
            lines.append("| Failing Test | Count | Example Model / Instance |")
            lines.append("|--------------|-------|--------------------------|")
            repo_tests = Counter()
            repo_examples: dict[str, list[tuple[str, str]]] = defaultdict(list)
            for model in MODELS:
                model_name = model.replace("runs10_", "")
                report = load_report(ROOT / model) or {}
                for inst in report.get("per_instance", []):
                    if repo_from_instance_id(inst["instance_id"]) != repo_key:
                        continue
                    if classify_error(inst) != "test failures":
                        continue
                    for detail in inst.get("fail_to_pass_details", []):
                        if not detail.get("passed"):
                            tname = detail.get("test", "unknown")
                            repo_tests[tname] += 1
                            if len(repo_examples[tname]) < 3:
                                repo_examples[tname].append((model_name, inst["instance_id"]))
            for tname, tcnt in repo_tests.most_common(5):
                examples = repo_examples.get(tname, [])
                example_str = "; ".join(f"{m}: {iid}" for m, iid in examples[:2])
                lines.append(f"| `{tname}` | {tcnt} | {example_str} |")
            lines.append("")
        else:
            examples = [
                (model_name, inst["instance_id"])
                for model in MODELS
                for inst in (
                    load_report(ROOT / model) or {}
                ).get("per_instance", [])
                if classify_error(inst) == "empty patch"
            ][:3]
            lines.append("Example occurrences:")
            lines.append("")
            for model_name, iid in examples:
                lines.append(f"- {model_name}: `{iid}`")
            lines.append("")

    lines.extend(
        [
            "## Recommended Next Fixes",
            "",
            "1. **Patch quality**: the high share of empty / apply-failed / no-op patches suggests the edit extraction and agent loop still need tightening; see `patch_utils.py` and `run_selfware.py` recovery paths.",
            "2. **Fail-to-pass focus**: when tests do run, fail-to-pass tests almost never pass. The focused test oracle and post-edit test command should help, but the model may need stronger hints about the exact assertion to satisfy.",
            "3. **Per-language test parsing**: the most frequent failing tests cluster in NodeBB and qutebrowser; verify that the test command formatter and parser correctly map the official test names in `small_model_adapter.py` / `evaluate_predictions.py`.",
            "",
        ]
    )

    out_path = REVIEW_DIR / "10_model_review.md"
    out_path.write_text("\n".join(lines), encoding="utf-8")
    print(f"Wrote review to {out_path}")


if __name__ == "__main__":
    main()
