#!/usr/bin/env python3
"""Evaluate SWE-bench Pro predictions using the official harness.

This script takes a ``predictions.json`` or ``predictions.jsonl`` file produced
by ``run_selfware.py`` and runs the official per-instance ``run_script.sh`` +
``parser.py`` inside the SWE-bench Pro container for each prediction.  It
computes per-instance fail-to-pass and pass-to-pass results, an overall pass
rate, and writes both a JSON report and a Markdown summary.
"""

from __future__ import annotations

import argparse
import ast
import json
import logging
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

from run_selfware import (
    container_name,
    copy_into_container,
    podman,
    pull_image,
    start_container,
    stop_and_remove_container,
)


SWEBENCH_PRO_ROOT = Path("/tmp/SWE-bench_Pro-os")
CONTAINER_REPO_DIR = "/app"
PREDICTIONS_LOCK = threading.Lock()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Evaluate SWE-bench Pro predictions against the official harness."
    )
    parser.add_argument(
        "--predictions",
        required=True,
        help="Path to predictions.json or predictions.jsonl.",
    )
    parser.add_argument(
        "--sample-file",
        required=True,
        help="Path to a JSONL file with instance metadata (base_commit, dockerhub_tag, tests).",
    )
    parser.add_argument(
        "--output-dir",
        default="eval_results",
        help="Directory for evaluation outputs (default: eval_results).",
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=1,
        help="Number of instances to evaluate in parallel (default: 1).",
    )
    parser.add_argument(
        "--test-timeout",
        type=int,
        default=600,
        help="Timeout in seconds for each containerized test run (default: 600).",
    )
    return parser.parse_args()


def _load_list_field(value: Any) -> list[str]:
    """Normalize a HF dataset field that may be a JSON-encoded list or a real list."""
    if isinstance(value, list):
        return [str(x) for x in value]
    if isinstance(value, str):
        value = value.strip()
        if not value:
            return []
        try:
            parsed = ast.literal_eval(value)
            if isinstance(parsed, list):
                return [str(x) for x in parsed]
        except (SyntaxError, ValueError):
            pass
        return [value]
    return []


def load_predictions(path: Path) -> list[dict[str, Any]]:
    """Load predictions from a JSON array or JSONL file."""
    if not path.exists():
        raise FileNotFoundError(f"Predictions file not found: {path}")

    text = path.read_text(encoding="utf-8").strip()
    if text.startswith("["):
        return json.loads(text)

    records: list[dict[str, Any]] = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                records.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return records


def load_instances(path: Path) -> dict[str, dict[str, Any]]:
    """Load instance metadata keyed by instance_id."""
    if not path.exists():
        raise FileNotFoundError(f"Sample file not found: {path}")

    instances: dict[str, dict[str, Any]] = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
                instances[row["instance_id"]] = row
            except (json.JSONDecodeError, KeyError):
                continue
    return instances


def _build_entryscript(instance: dict[str, Any]) -> str:
    """Build the container entry script that applies the patch and runs tests.

    The script is strict: if the patch is empty or cannot be applied, it writes
    an output.json marking the instance as failed instead of silently running
    tests on the unpatched base commit.
    """
    selected = _load_list_field(instance.get("selected_test_files_to_run", []))
    test_arg = ",".join(selected)
    before_cmd = instance.get("before_repo_set_cmd", "") or ""
    return (
        "#!/bin/bash\n"
        "set -euo pipefail\n"
        "cd /app\n"
        f"git reset --hard {instance['base_commit']}\n"
        f"git checkout {instance['base_commit']}\n"
        f"{before_cmd}\n"
        "if [ ! -s /workspace/patch.diff ] || [ \"$(grep -v '^[[:space:]]*$' /workspace/patch.diff | wc -l)\" -eq 0 ]; then\n"
        "  echo '{\"tests\": []}' > /workspace/output.json\n"
        "  echo 'PATCH_EMPTY' > /workspace/patch_apply_status.txt\n"
        "  exit 0\n"
        "fi\n"
        "set +e\n"
        "git apply -v /workspace/patch.diff > /workspace/patch_apply.log 2>&1\n"
        "apply_status=$?\n"
        "if [ $apply_status -ne 0 ]; then\n"
        "  git apply --3way -v /workspace/patch.diff >> /workspace/patch_apply.log 2>&1\n"
        "  apply_status=$?\n"
        "fi\n"
        "if [ $apply_status -ne 0 ]; then\n"
        "  patch -p1 -i /workspace/patch.diff >> /workspace/patch_apply.log 2>&1\n"
        "  apply_status=$?\n"
        "fi\n"
        "set -e\n"
        "echo \"git apply exit code: $apply_status\" >> /workspace/patch_apply.log\n"
        "echo $apply_status > /workspace/patch_apply_status.txt\n"
        "if [ $apply_status -ne 0 ]; then\n"
        "  echo '{\"tests\": []}' > /workspace/output.json\n"
        "  exit 0\n"
        "fi\n"
        f"bash /workspace/run_script.sh {test_arg} > /workspace/stdout.log 2> /workspace/stderr.log\n"
        "python /workspace/parser.py /workspace/stdout.log /workspace/stderr.log /workspace/output.json\n"
    )


def _score_output(output: dict[str, Any], instance: dict[str, Any]) -> dict[str, Any]:
    """Compute fail-to-pass and pass-to-pass results from parsed test output."""
    fail_to_pass = _load_list_field(instance.get("fail_to_pass", []))
    pass_to_pass = _load_list_field(instance.get("pass_to_pass", []))

    status_map: dict[str, str] = {}
    for test in output.get("tests", []):
        name = test.get("name", "")
        status = test.get("status", "")
        if name:
            status_map[name] = status.upper()

    fail_details: list[dict[str, Any]] = []
    fail_passed = 0
    for t in fail_to_pass:
        status = status_map.get(t, "MISSING")
        passed = status == "PASSED"
        if passed:
            fail_passed += 1
        fail_details.append({"test": t, "status": status, "passed": passed})

    pass_details: list[dict[str, Any]] = []
    pass_passed = 0
    for t in pass_to_pass:
        status = status_map.get(t, "MISSING")
        passed = status in ("PASSED", "SKIPPED")
        if passed:
            pass_passed += 1
        pass_details.append({"test": t, "status": status, "passed": passed})

    overall_pass = (
        fail_passed == len(fail_to_pass)
        and pass_passed == len(pass_to_pass)
        and len(fail_to_pass) + len(pass_to_pass) > 0
    )

    return {
        "fail_to_pass_passed": fail_passed,
        "fail_to_pass_total": len(fail_to_pass),
        "pass_to_pass_passed": pass_passed,
        "pass_to_pass_total": len(pass_to_pass),
        "overall_pass": overall_pass,
        "fail_to_pass_details": fail_details,
        "pass_to_pass_details": pass_details,
        "status_map": status_map,
    }


def _copy_artifact_out(
    container: str,
    src: str,
    dst: Path,
    logger: logging.Logger,
) -> bool:
    """Copy a file from a container path to the host."""
    proc = podman("cp", f"{container}:{src}", str(dst), logger=logger)
    if proc.returncode != 0:
        logger.error(
            "Failed to copy %s:%s to %s: %s",
            container,
            src,
            dst,
            proc.stderr.strip(),
        )
        return False
    return True


def _is_retryable_container_error(stderr: str) -> bool:
    """Return True when a Podman stderr looks like a transient storage race."""
    retryable = (
        "container state improper",
        "no such container",
        "directory not empty",
        "failed to start container",
        "removing mount point",
        "resource temporarily unavailable",
    )
    lowered = stderr.lower()
    return any(needle in lowered for needle in retryable)


def evaluate_instance(
    instance: dict[str, Any],
    prediction: dict[str, Any],
    output_dir: Path,
    logger: logging.Logger,
    test_timeout: int,
) -> dict[str, Any]:
    """Evaluate a single prediction and return scoring metadata."""
    instance_id = instance["instance_id"]
    image = f"docker.io/jefzda/sweap-images:{instance['dockerhub_tag']}"
    # Include the run and output directory names in the container name so
    # parallel evaluations of the same instance (e.g., different model runs)
    # do not collide on the same rootless container.
    name = container_name(instance_id, f"eval-{output_dir.parent.name}-{output_dir.name}")

    result: dict[str, Any] = {
        "instance_id": instance_id,
        "error": None,
    }

    if not pull_image(image, logger):
        result["error"] = "failed to pull image"
        return result

    # Retry container startup and workspace setup to absorb rootless Podman
    # storage races when many evaluations run concurrently.
    container_ready = False
    last_error = ""
    for attempt in range(3):
        stop_and_remove_container(name, logger)
        if start_container(image, name, logger, timeout=120):
            mkdir = podman("exec", name, "mkdir", "-p", "/workspace", logger=logger)
            if mkdir.returncode == 0:
                container_ready = True
                break
            last_error = f"failed to create /workspace: {mkdir.stderr.strip()}"
            if not _is_retryable_container_error(mkdir.stderr):
                break
        else:
            # Try to capture the last podman stderr from logs.
            last_error = "failed to start container"
        logger.warning(
            "Container setup attempt %s failed for %s, retrying: %s",
            attempt + 1,
            instance_id,
            last_error,
        )
        time.sleep(2 ** attempt)

    if not container_ready:
        result["error"] = last_error or "failed to start container"
        return result

    try:

        run_script_src = SWEBENCH_PRO_ROOT / "run_scripts" / instance_id / "run_script.sh"
        parser_src = SWEBENCH_PRO_ROOT / "run_scripts" / instance_id / "parser.py"
        if not run_script_src.exists() or not parser_src.exists():
            result["error"] = "missing official evaluation files"
            return result

        # Stage artifacts on the host.
        patch_file = output_dir / f"{instance_id}.patch.diff"
        entryscript_file = output_dir / f"{instance_id}.entryscript.sh"
        patch_file.write_text(prediction.get("patch", ""), encoding="utf-8")
        entryscript_file.write_text(_build_entryscript(instance), encoding="utf-8")

        ok = (
            copy_into_container(patch_file, "/workspace/patch.diff", name, logger)
            and copy_into_container(run_script_src, "/workspace/run_script.sh", name, logger)
            and copy_into_container(parser_src, "/workspace/parser.py", name, logger)
            and copy_into_container(entryscript_file, "/workspace/entryscript.sh", name, logger)
        )
        if not ok:
            result["error"] = "failed to copy artifacts into container"
            return result

        logger.info("Running evaluation for %s in %s", instance_id, name)
        proc = podman(
            "exec",
            name,
            "bash",
            "/workspace/entryscript.sh",
            timeout=test_timeout,
            logger=logger,
        )
        if proc.returncode != 0:
            logger.warning("Evaluation entryscript for %s exited with code %s", instance_id, proc.returncode)

        output_file = output_dir / f"{instance_id}.output.json"
        stdout_file = output_dir / f"{instance_id}.stdout.log"
        stderr_file = output_dir / f"{instance_id}.stderr.log"
        patch_apply_log = output_dir / f"{instance_id}.patch_apply.log"
        patch_apply_status = output_dir / f"{instance_id}.patch_apply_status.txt"

        _copy_artifact_out(name, "/workspace/output.json", output_file, logger)
        _copy_artifact_out(name, "/workspace/stdout.log", stdout_file, logger)
        _copy_artifact_out(name, "/workspace/stderr.log", stderr_file, logger)
        _copy_artifact_out(name, "/workspace/patch_apply.log", patch_apply_log, logger)
        _copy_artifact_out(name, "/workspace/patch_apply_status.txt", patch_apply_status, logger)

        if patch_apply_status.exists():
            status_text = patch_apply_status.read_text(encoding="utf-8").strip()
            result["patch_apply_status"] = status_text
            if status_text == "PATCH_EMPTY":
                result["error"] = "empty patch"
                return result
            if status_text != "0":
                result["error"] = "patch apply failed"
                return result

        if not output_file.exists():
            result["error"] = "no output.json produced"
            return result

        try:
            output = json.loads(output_file.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            result["error"] = f"output.json parse error: {exc}"
            return result

        score = _score_output(output, instance)
        result.update(score)
        logger.info(
            "Evaluation for %s: fail %s/%s, pass %s/%s, overall=%s",
            instance_id,
            score["fail_to_pass_passed"],
            score["fail_to_pass_total"],
            score["pass_to_pass_passed"],
            score["pass_to_pass_total"],
            score["overall_pass"],
        )
        return result
    except Exception as exc:
        logger.error("Unexpected error evaluating %s: %s", instance_id, exc)
        result["error"] = str(exc)
        return result
    finally:
        stop_and_remove_container(name, logger)


def _write_report(output_dir: Path, results: list[dict[str, Any]]) -> dict[str, Any]:
    """Aggregate results and write JSON report + Markdown summary."""
    total = len(results)
    completed = [r for r in results if r.get("error") is None]
    overall_passed = sum(1 for r in completed if r.get("overall_pass"))

    fail_tp_passed = sum(r.get("fail_to_pass_passed", 0) for r in completed)
    fail_tp_total = sum(r.get("fail_to_pass_total", 0) for r in completed)
    pass_tp_passed = sum(r.get("pass_to_pass_passed", 0) for r in completed)
    pass_tp_total = sum(r.get("pass_to_pass_total", 0) for r in completed)

    report = {
        "total_instances": total,
        "completed_instances": len(completed),
        "errored_instances": total - len(completed),
        "overall_passed_instances": overall_passed,
        "overall_pass_rate": overall_passed / len(completed) if completed else 0.0,
        "fail_to_pass_passed": fail_tp_passed,
        "fail_to_pass_total": fail_tp_total,
        "fail_to_pass_rate": fail_tp_passed / fail_tp_total if fail_tp_total else 0.0,
        "pass_to_pass_passed": pass_tp_passed,
        "pass_to_pass_total": pass_tp_total,
        "pass_to_pass_rate": pass_tp_passed / pass_tp_total if pass_tp_total else 0.0,
        "per_instance": results,
    }

    report_path = output_dir / "evaluation_report.json"
    with PREDICTIONS_LOCK:
        report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")

    lines = [
        "# SWE-bench Pro Evaluation Summary",
        "",
        f"- Total instances: **{total}**",
        f"- Completed: **{len(completed)}**",
        f"- Errored: **{total - len(completed)}**",
        f"- Overall passed instances: **{overall_passed}/{len(completed)}** "
        f"({report['overall_pass_rate']:.2%})",
        f"- Fail-to-pass: **{fail_tp_passed}/{fail_tp_total}** "
        f"({report['fail_to_pass_rate']:.2%})",
        f"- Pass-to-pass: **{pass_tp_passed}/{pass_tp_total}** "
        f"({report['pass_to_pass_rate']:.2%})",
        "",
        "| Instance | Fail-to-pass | Pass-to-pass | Overall |",
        "|----------|--------------|--------------|---------|",
    ]
    for r in results:
        iid = r["instance_id"]
        if r.get("error"):
            lines.append(f"| {iid} | error | error | ❌ ({r['error']}) |")
        else:
            fpp = f"{r['fail_to_pass_passed']}/{r['fail_to_pass_total']}"
            ppp = f"{r['pass_to_pass_passed']}/{r['pass_to_pass_total']}"
            overall = "✅" if r["overall_pass"] else "❌"
            lines.append(f"| {iid} | {fpp} | {ppp} | {overall} |")

    summary_path = output_dir / "evaluation_summary.md"
    with PREDICTIONS_LOCK:
        summary_path.write_text("\n".join(lines), encoding="utf-8")

    return report


def main() -> int:
    args = parse_args()
    output_dir = Path(args.output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
        handlers=[logging.FileHandler(output_dir / "evaluate.log"), logging.StreamHandler(sys.stdout)],
    )
    logger = logging.getLogger("evaluate_predictions")

    predictions = load_predictions(Path(args.predictions))
    instances = load_instances(Path(args.sample_file))
    logger.info("Loaded %s predictions and %s instance metadata rows", len(predictions), len(instances))

    results: list[dict[str, Any]] = []

    def _process(pred: dict[str, Any]) -> dict[str, Any]:
        iid = pred.get("instance_id", "")
        if iid not in instances:
            logger.warning("No instance metadata for %s; skipping", iid)
            return {"instance_id": iid, "error": "missing instance metadata"}
        return evaluate_instance(
            instances[iid],
            pred,
            output_dir,
            logger,
            test_timeout=args.test_timeout,
        )

    if args.workers > 1:
        with ThreadPoolExecutor(max_workers=args.workers) as executor:
            future_to_pred = {
                executor.submit(_process, pred): pred for pred in predictions
            }
            for future in as_completed(future_to_pred):
                try:
                    results.append(future.result())
                except Exception as exc:
                    pred = future_to_pred[future]
                    logger.error("Unexpected error processing %s: %s", pred.get("instance_id"), exc)
                    results.append({
                        "instance_id": pred.get("instance_id", ""),
                        "error": str(exc),
                    })
    else:
        for pred in predictions:
            results.append(_process(pred))

    report = _write_report(output_dir, results)
    logger.info(
        "Evaluation complete. Overall passed: %s/%s",
        report["overall_passed_instances"],
        report["completed_instances"],
    )
    logger.info("Report: %s", output_dir / "evaluation_report.json")
    logger.info("Summary: %s", output_dir / "evaluation_summary.md")
    return 0


if __name__ == "__main__":
    sys.exit(main())
