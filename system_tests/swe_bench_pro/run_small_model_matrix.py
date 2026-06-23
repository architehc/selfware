#!/usr/bin/env python3
"""Small-model evaluation matrix for the SWE-bench Pro harness.

Runs a subset of SWE-bench Pro instances through ``run_selfware.py`` using
several cheap OpenRouter model profiles and produces a JSON/CSV summary of
patch sizes and success flags.

This script does not call any model API itself; it shells out to the harness.
"""

from __future__ import annotations

import argparse
import csv
import json
import logging
import os
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


DEFAULT_SAMPLE_FILE = Path(__file__).resolve().parent / "sample_50.jsonl"
DEFAULT_HARNESS = Path(__file__).resolve().parent / "run_selfware.py"
DEFAULT_CONFIG_DIR = Path(__file__).resolve().parents[1] / "projecte2e" / "config"
DEFAULT_BINARY = Path(__file__).resolve().parents[2] / "target" / "release" / "selfware"
DEFAULT_OUTPUT_BASE = Path(__file__).resolve().parent / "matrix_outputs"
DEFAULT_API_KEY_FILE = Path("/tmp/selfware_api_key.env")

DEFAULT_MODELS = [
    "qwen3.5-9b-sweap",
    "qwen3.5-flash-sweap",
    "deepseek-v4-flash-sweap",
    "qwen3.6-27b-sweap",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a small instance matrix across cheap model profiles."
    )
    parser.add_argument(
        "--sample-file",
        default=str(DEFAULT_SAMPLE_FILE),
        help="Path to the JSONL sample file (default: sample_50.jsonl).",
    )
    parser.add_argument(
        "--instance-ids",
        default=None,
        help="Comma-separated instance IDs to test (overrides --max-instances).",
    )
    parser.add_argument(
        "--max-instances",
        type=int,
        default=3,
        help="Pick the first N instances from the sample (default: 3).",
    )
    parser.add_argument(
        "--models",
        default=",".join(DEFAULT_MODELS),
        help="Comma-separated model profile names (default: four cheap profiles).",
    )
    parser.add_argument(
        "--compact",
        action="store_true",
        help="Pass --compact-prompt to the harness.",
    )
    parser.add_argument(
        "--small-model",
        action="store_true",
        help="Pass --small-model to the harness (compact context adapter).",
    )
    parser.add_argument(
        "--retry-failures",
        action="store_true",
        default=True,
        help="Pass --retry-failures to the harness (default: True).",
    )
    parser.add_argument(
        "--no-retry-failures",
        dest="retry_failures",
        action="store_false",
        help="Pass --no-retry-failures to the harness.",
    )
    parser.add_argument(
        "--force-edit",
        action="store_true",
        help="Pass --force-edit to the harness.",
    )
    parser.add_argument(
        "--adaptive",
        action="store_true",
        help="Pass --adaptive to the harness.",
    )
    parser.add_argument(
        "--plan-then-patch",
        action="store_true",
        help="Pass --plan-then-patch to the harness.",
    )
    parser.add_argument(
        "--auto-agentless",
        action="store_true",
        help="Pass --auto-agentless to the harness for weak models.",
    )
    parser.add_argument(
        "--agentless",
        action="store_true",
        help="Pass --agentless to the harness (force one-shot patch generation for every model).",
    )
    parser.add_argument(
        "--few-shot-examples",
        default=None,
        help="Path to few-shot examples to pass to the harness.",
    )
    parser.add_argument(
        "--parallel",
        type=int,
        default=1,
        help="Number of models to run in parallel (default: 1).",
    )
    parser.add_argument(
        "--output-base",
        default=str(DEFAULT_OUTPUT_BASE),
        help="Base directory for per-model/instance outputs and summary files.",
    )
    parser.add_argument(
        "--config-dir",
        default=str(DEFAULT_CONFIG_DIR),
        help="Directory containing openrouter_<profile>.toml configs.",
    )
    parser.add_argument(
        "--binary",
        default=str(DEFAULT_BINARY),
        help="Path to the selfware release binary.",
    )
    parser.add_argument(
        "--harness",
        default=str(DEFAULT_HARNESS),
        help="Path to run_selfware.py.",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=1800,
        help="Per-instance timeout passed to the harness (default: 1800).",
    )
    parser.add_argument(
        "--api-key-file",
        default=str(DEFAULT_API_KEY_FILE),
        help="Path to a shell-style env file containing SELFWARE_API_KEY (default: /tmp/selfware_api_key.env).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the commands that would be run without executing them.",
    )
    return parser.parse_args()


def setup_logging(output_base: Path) -> logging.Logger:
    output_base.mkdir(parents=True, exist_ok=True)
    log_path = output_base / "matrix.log"
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
        handlers=[logging.FileHandler(log_path), logging.StreamHandler(sys.stdout)],
    )
    return logging.getLogger("small-model-matrix")


def load_sample(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise ValueError(f"Malformed JSONL in {path}: {exc}") from exc
    return rows


def load_api_key(api_key_file: Path) -> str:
    """Load SELFWARE_API_KEY from a shell-style env file."""
    if not api_key_file.exists():
        raise FileNotFoundError(
            f"API key file not found: {api_key_file}. Set --api-key-file or export SELFWARE_API_KEY."
        )
    text = api_key_file.read_text(encoding="utf-8")
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[len("export "):]
        if "=" in line:
            key, value = line.split("=", 1)
            if key.strip() == "SELFWARE_API_KEY":
                return value.strip().strip('"').strip("'")
    raise ValueError(f"SELFWARE_API_KEY not found in {api_key_file}")


def select_instance_ids(args: argparse.Namespace) -> list[str]:
    sample_path = Path(args.sample_file)
    if not sample_path.exists():
        raise FileNotFoundError(f"Sample file not found: {sample_path}")

    sample = load_sample(sample_path)
    if not sample:
        raise ValueError(f"Sample file is empty: {sample_path}")

    all_ids = [row["instance_id"] for row in sample if "instance_id" in row]

    if args.instance_ids:
        wanted = [x.strip() for x in args.instance_ids.split(",") if x.strip()]
        missing = set(wanted) - set(all_ids)
        if missing:
            raise ValueError(
                f"Requested instance IDs not found in sample: {sorted(missing)}"
            )
        # Preserve requested order.
        id_set = set(wanted)
        return [iid for iid in wanted if iid in id_set]

    n = args.max_instances
    if n <= 0:
        raise ValueError("--max-instances must be a positive integer")
    return all_ids[:n]


def validate_models(models: list[str], config_dir: Path) -> None:
    missing: list[str] = []
    for profile in models:
        config_path = config_dir / f"openrouter_{profile}.toml"
        if not config_path.exists():
            missing.append(str(config_path))
    if missing:
        raise FileNotFoundError(
            f"Missing model config file(s): {', '.join(missing)}"
        )


def build_harness_command(
    args: argparse.Namespace,
    profile: str,
    instance_id: str,
    output_dir: Path,
) -> list[str]:
    cmd = [
        sys.executable,
        str(args.harness),
        "--model-profile",
        profile,
        "--instance-ids",
        instance_id,
        "--output-dir",
        str(output_dir),
        "--config-dir",
        str(args.config_dir),
        "--binary",
        str(args.binary),
        "--max-tasks",
        "1",
        "--workers",
        "1",
        "--timeout",
        str(args.timeout),
    ]
    if args.compact:
        cmd.append("--compact-prompt")
    if args.small_model:
        cmd.append("--small-model")
    if args.retry_failures:
        cmd.append("--retry-failures")
    if args.force_edit:
        cmd.append("--force-edit")
    if args.adaptive:
        cmd.append("--adaptive")
    if args.plan_then_patch:
        cmd.append("--plan-then-patch")
    if args.auto_agentless:
        cmd.append("--auto-agentless")
    if args.agentless:
        cmd.append("--agentless")
    if args.few_shot_examples:
        cmd.extend(["--few-shot-examples", str(args.few_shot_examples)])
    return cmd


def read_patch_size(output_dir: Path, instance_id: str) -> tuple[int, int]:
    """Return (patch_chars, patch_lines) from the last matching record in predictions.jsonl."""
    predictions_path = output_dir / "predictions.jsonl"
    if not predictions_path.exists():
        return 0, 0
    try:
        last_patch = ""
        with open(predictions_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if record.get("instance_id") == instance_id:
                    patch = record.get("patch", "") or ""
                    last_patch = patch
        return len(last_patch), len(last_patch.splitlines())
    except OSError:
        pass
    return 0, 0


def run_model_matrix(
    args: argparse.Namespace,
    profile: str,
    instance_ids: list[str],
    logger: logging.Logger,
) -> list[dict[str, Any]]:
    """Run every selected instance for a single model profile sequentially."""
    results: list[dict[str, Any]] = []
    output_base = Path(args.output_base)

    api_key = load_api_key(Path(args.api_key_file))
    for instance_id in instance_ids:
        output_dir = output_base / profile / instance_id
        output_dir.mkdir(parents=True, exist_ok=True)
        cmd = build_harness_command(args, profile, instance_id, output_dir)

        logger.info("[%s / %s] starting harness run", profile, instance_id)
        logger.debug("command: %s", " ".join(cmd))

        env = os.environ.copy()
        env["SELFWARE_API_KEY"] = api_key

        start = time.monotonic()
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=None,
                env=env,
            )
        except subprocess.SubprocessError as exc:
            logger.error(
                "[%s / %s] failed to launch harness: %s", profile, instance_id, exc
            )
            results.append(
                {
                    "model_profile": profile,
                    "instance_id": instance_id,
                    "output_dir": str(output_dir),
                    "returncode": -1,
                    "patch_chars": 0,
                    "patch_lines": 0,
                    "success": False,
                    "duration_seconds": 0.0,
                    "command": " ".join(cmd),
                    "error": str(exc),
                }
            )
            continue

        duration = time.monotonic() - start
        patch_chars, patch_lines = read_patch_size(output_dir, instance_id)
        success = proc.returncode == 0

        if not success:
            logger.warning(
                "[%s / %s] harness exited %s (stderr tail: %s)",
                profile,
                instance_id,
                proc.returncode,
                proc.stderr[-500:].strip(),
            )
        else:
            logger.info(
                "[%s / %s] finished in %.1fs; patch %s chars / %s lines",
                profile,
                instance_id,
                duration,
                patch_chars,
                patch_lines,
            )

        results.append(
            {
                "model_profile": profile,
                "instance_id": instance_id,
                "output_dir": str(output_dir),
                "returncode": proc.returncode,
                "patch_chars": patch_chars,
                "patch_lines": patch_lines,
                "success": success,
                "duration_seconds": round(duration, 2),
                "command": " ".join(cmd),
            }
        )

        # Keep a copy of the per-instance harness log inside the matrix log for
        # quick debugging without opening each output directory.
        if proc.stdout:
            logger.debug(
                "[%s / %s] harness stdout:\n%s", profile, instance_id, proc.stdout[-2000:]
            )
        if proc.stderr:
            logger.debug(
                "[%s / %s] harness stderr:\n%s", profile, instance_id, proc.stderr[-2000:]
            )

    return results


def plan_runs(
    args: argparse.Namespace,
    models: list[str],
    instance_ids: list[str],
) -> list[list[str]]:
    """Return the list of harness commands that would be executed."""
    commands: list[list[str]] = []
    output_base = Path(args.output_base)
    for profile in models:
        for instance_id in instance_ids:
            output_dir = output_base / profile / instance_id
            commands.append(
                build_harness_command(args, profile, instance_id, output_dir)
            )
    return commands


def write_summary(
    output_base: Path,
    results: list[dict[str, Any]],
    args: argparse.Namespace,
) -> None:
    output_base.mkdir(parents=True, exist_ok=True)
    summary = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "sample_file": args.sample_file,
        "instance_ids": [r["instance_id"] for r in results],
        "models": sorted({r["model_profile"] for r in results}),
        "compact": args.compact,
        "small_model": args.small_model,
        "retry_failures": args.retry_failures,
        "force_edit": args.force_edit,
        "few_shot_examples": args.few_shot_examples,
        "total_runs": len(results),
        "successful_runs": sum(1 for r in results if r["success"]),
        "results": results,
    }

    json_path = output_base / "summary.json"
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)

    csv_path = output_base / "summary.csv"
    if results:
        fieldnames = [
            "model_profile",
            "instance_id",
            "output_dir",
            "returncode",
            "patch_chars",
            "patch_lines",
            "success",
            "duration_seconds",
            "command",
        ]
        with open(csv_path, "w", encoding="utf-8", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=fieldnames, extrasaction="ignore")
            writer.writeheader()
            writer.writerows(results)
    else:
        csv_path.write_text("", encoding="utf-8")

    logging.getLogger("small-model-matrix").info(
        "Summary written to %s and %s", json_path, csv_path
    )


def main() -> int:
    args = parse_args()
    output_base = Path(args.output_base)

    harness_path = Path(args.harness)
    if not harness_path.exists():
        print(f"Harness not found: {harness_path}", file=sys.stderr)
        return 1

    binary_path = Path(args.binary)
    if not binary_path.exists():
        print(f"Selfware binary not found: {binary_path}", file=sys.stderr)
        return 1

    models = [m.strip() for m in args.models.split(",") if m.strip()]
    if not models:
        print("No model profiles specified", file=sys.stderr)
        return 1

    try:
        validate_models(models, Path(args.config_dir))
        instance_ids = select_instance_ids(args)
    except (FileNotFoundError, ValueError) as exc:
        print(exc, file=sys.stderr)
        return 1

    if args.dry_run:
        print(f"Dry-run: would test {len(instance_ids)} instance(s) against {len(models)} model profile(s)")
        print(f"Output base: {output_base}")
        print()
        for cmd in plan_runs(args, models, instance_ids):
            print(" ".join(cmd))
        return 0

    logger = setup_logging(output_base)
    logger.info("Starting small-model matrix")
    logger.info("Models: %s", ", ".join(models))
    logger.info("Instances: %s", ", ".join(instance_ids))
    logger.info("Parallel models: %d", args.parallel)

    all_results: list[dict[str, Any]] = []
    parallel = max(1, args.parallel)

    if parallel == 1 or len(models) == 1:
        for profile in models:
            all_results.extend(run_model_matrix(args, profile, instance_ids, logger))
    else:
        with ThreadPoolExecutor(max_workers=parallel) as executor:
            future_to_profile = {
                executor.submit(
                    run_model_matrix, args, profile, instance_ids, logger
                ): profile
                for profile in models
            }
            for future in as_completed(future_to_profile):
                profile = future_to_profile[future]
                try:
                    results = future.result()
                    all_results.extend(results)
                    logger.info(
                        "Model profile %s completed (%d run(s))", profile, len(results)
                    )
                except Exception as exc:
                    logger.error("Model profile %s failed: %s", profile, exc)

    all_results.sort(key=lambda r: (r["model_profile"], r["instance_id"]))
    write_summary(output_base, all_results, args)

    success_count = sum(1 for r in all_results if r["success"])
    logger.info(
        "Matrix complete: %d/%d runs succeeded", success_count, len(all_results)
    )
    return 0 if success_count == len(all_results) else 1


if __name__ == "__main__":
    sys.exit(main())
