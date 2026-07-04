#!/usr/bin/env python3
"""Minimal Selfware harness for SWE-bench Pro.

Generates patch predictions by running the selfware agent inside the official
SWE-bench Pro containers and capturing the resulting git diff.
"""

import argparse
import ast
import datetime
import hashlib
import json
import logging
import os
import random
import re
import shlex
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import tomllib
import tomli_w
import traceback
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

from harness_recovery import (
    AGENTLESS_MODE_KEY,
    EMPTY_PATCH,
    JSON_PARSE_ERROR,
    MAX_ITERATIONS,
    _is_patch_empty,
    build_diff_fallback_prompt,
    build_recovery_prompt,
    classify_failure,
    escalation_config,
    should_retry,
    write_recovery_config,
)

from repo_templates import load_repo_template

from small_model_adapter import (
    _build_focused_test_oracle,
    _context_budgets,
    _extract_source_paths_from_text,
    _format_target_api_section,
    _format_test_command,
    _is_strong_identifier,
    _new_files_from_patch,
    _read_agentless_file_snippets,
    _tokenize_problem,
    build_agentless_prompt,
    build_agentless_retry_prompt,
    build_small_model_prompt,
    rank_files_by_relevance,
    truncate_file_reads,
)

from small_model_configs import (
    DEFAULT_SMALL_MODEL_CONFIG_DIR,
    load_small_model_config,
    merge_over_toml,
)

from patch_utils import (
    apply_edits,
    apply_model_response,
    apply_model_response_with_missing,
    apply_patch,
    clean_captured_diff,
    extract_diff,
    extract_partial_diff,
    filter_patch_excluding_paths,
    filter_patch_to_files,
    filter_patch_to_source_files,
    is_truncated_diff,
    paths_from_patch,
    verify_edits_apply,
    _apply_diff_with_check,
)

from critic import (
    build_critic_prompt,
    run_critic_loop,
)

try:
    from datasets import load_dataset
except Exception as exc:  # pragma: no cover - harness dependency check
    raise SystemExit(
        "The 'datasets' library is required. "
        "Activate the swebench venv or run: pip install datasets"
    ) from exc


DEFAULT_CONFIG_DIR = Path(__file__).resolve().parents[1] / "projecte2e" / "config"
DEFAULT_BINARY = Path(__file__).resolve().parents[2] / "target" / "release" / "selfware"
DEFAULT_OUTPUT_DIR = Path(__file__).resolve().parent / "predictions"
CONTAINER_SELFWARE_BIN = "/usr/local/bin/selfware"
CONTAINER_CONFIG_PATH = "/tmp/selfware_config.toml"
CONTAINER_PROMPT_PATH = "/tmp/task_prompt.txt"
CONTAINER_REPO_DIR = "/app"
DEFAULT_DIFF_FALLBACK_MAX_TOKENS = 6000

# Repo root used for git provenance. run_selfware.py lives under
# system_tests/swe_bench_pro/, so three parents point at the project root.
REPO_ROOT = Path(__file__).resolve().parents[2]

# Lock for serializing appends to predictions.jsonl when running instances in parallel.
PREDICTIONS_LOCK = threading.Lock()


def _get_harness_version() -> str:
    """Return a harness version string, falling back to 'dev'."""
    try:
        from importlib.metadata import version
        return version("selfware")
    except Exception:
        return "dev"


HARNESS_VERSION = _get_harness_version()


def _get_harness_sha() -> str:
    """Return the current git HEAD sha, or 'unknown' if not available."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True,
            check=True,
        )
        return result.stdout.strip()
    except Exception:
        return "unknown"


# Runtime flags that affect harness behavior enough that changing them between
# invocations should invalidate a --resume.
_RUN_RESUME_FLAG_FIELDS: tuple[str, ...] = (
    "agentless",
    "auto_agentless",
    "small_model_diff_fallback",
    "diff_fallback",
    "retry_failures",
    "max_retries",
    "early_diff_fallback",
    "force_edit",
    "small_model",
    "adaptive",
    "compact_prompt",
    "critic_iterations",
    "critic_model_profile",
)


# Provenance fields that must match exactly for a resume to be safe.
_RESUME_PATH_FIELDS: tuple[str, ...] = (
    "model_profile",
    "config_dir",
    "config_files",
    "sample_file",
    "harness_sha",
)


def _build_run_manifest(
    args: argparse.Namespace,
    config_files: list[str],
) -> dict[str, Any]:
    """Build the provenance manifest for this harness invocation."""
    return {
        "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "model_profile": args.model_profile,
        "config_dir": args.config_dir,
        "config_files": config_files,
        "sample_file": args.sample_file,
        "harness_sha": _get_harness_sha(),
        "command": sys.argv,
        "version": HARNESS_VERSION,
        "runtime_flags": {
            field: getattr(args, field, None) for field in _RUN_RESUME_FLAG_FIELDS
        },
    }


def _write_run_manifest(
    output_dir: Path,
    manifest: dict[str, Any],
) -> None:
    """Write run_manifest.json with provenance for this harness invocation."""
    output_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = output_dir / "run_manifest.json"
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)


def _read_run_manifest(output_dir: Path) -> dict[str, Any] | None:
    """Return the existing run manifest, or None if it does not exist."""
    manifest_path = output_dir / "run_manifest.json"
    if not manifest_path.exists():
        return None
    with open(manifest_path, encoding="utf-8") as f:
        return json.load(f)


def _check_run_manifest(
    current: dict[str, Any],
    existing: dict[str, Any],
) -> None:
    """Raise an error if provenance differs between the current and existing run."""
    diffs: list[str] = []
    for field in _RESUME_PATH_FIELDS:
        if existing.get(field) != current.get(field):
            diffs.append(
                f"{field}: existing={existing.get(field)!r} current={current.get(field)!r}"
            )

    current_flags = current.get("runtime_flags") or {}
    existing_flags = existing.get("runtime_flags") or {}
    for field in _RUN_RESUME_FLAG_FIELDS:
        if existing_flags.get(field) != current_flags.get(field):
            diffs.append(
                f"runtime_flags.{field}: "
                f"existing={existing_flags.get(field)!r} current={current_flags.get(field)!r}"
            )

    if diffs:
        raise RuntimeError(
            "Output directory contains a run_manifest.json from an incompatible run.\n"
            + "\n".join(diffs)
            + "\nUse --fresh to clear the output directory or change --output-dir."
        )

# Global Podman options applied to every podman invocation.  The most important
# one for SWE-bench Pro is --storage-opt ignore_chown_errors=true: the official
# images contain files owned by UIDs/GIDs far outside the host user's subuid
# range, and without this option rootless podman cannot unpack the layers.
_PODMAN_GLOBAL_OPTS: list[str] = ["--storage-opt", "ignore_chown_errors=true"]


class TristateAction(argparse.Action):
    """Action that supports --flag, --no-flag, --flag=true and --flag=false.

    The default remains ``None`` so the caller can distinguish "not specified"
    from an explicit ``True`` or ``False``.
    """

    def __init__(self, option_strings, dest, default=None, **kwargs):
        super().__init__(option_strings, dest, default=default, nargs="?", const=True, **kwargs)

    def __call__(self, parser, namespace, values, option_string=None):
        if option_string is not None and option_string.startswith("--no-"):
            setattr(namespace, self.dest, False)
        elif values is None:
            setattr(namespace, self.dest, True)
        else:
            setattr(namespace, self.dest, str(values).lower() in ("true", "1", "yes", "on"))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run Selfware on SWE-bench Pro instances and produce predictions."
    )
    parser.add_argument(
        "--model-profile",
        required=True,
        help="OpenRouter profile name, e.g. 'kimi-k2.7-code'. "
             "Config is loaded from <config-dir>/openrouter_<profile>.toml.",
    )
    parser.add_argument(
        "--max-tasks",
        type=int,
        default=None,
        help="Maximum number of dataset instances to process. "
             "When omitted, only the first instance is processed unless "
             "--sample-file or --instance-ids is given, in which case all "
             "requested instances are run.",
    )
    parser.add_argument(
        "--instance-ids",
        default=None,
        help="Comma-separated instance IDs to run (overrides --max-tasks and --sample-file).",
    )
    parser.add_argument(
        "--sample-file",
        default=None,
        help="Path to a JSONL file of pre-selected instances (overrides --max-tasks).",
    )
    parser.add_argument(
        "--output-dir",
        default=str(DEFAULT_OUTPUT_DIR),
        help="Directory for predictions.jsonl and per-instance logs.",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=1800,
        help="Timeout in seconds for the selfware run inside the container.",
    )
    parser.add_argument(
        "--config-dir",
        default=str(DEFAULT_CONFIG_DIR),
        help="Directory containing openrouter_<profile>.toml config files.",
    )
    parser.add_argument(
        "--small-model-config-dir",
        default=str(DEFAULT_SMALL_MODEL_CONFIG_DIR),
        help="Directory containing <profile>.yaml small-model config files. "
             "When a small model is selected, these YAML files override the TOML config.",
    )
    parser.add_argument(
        "--binary",
        default=str(DEFAULT_BINARY),
        help="Path to the selfware release binary on the host.",
    )
    parser.add_argument(
        "--repo-dir",
        default=CONTAINER_REPO_DIR,
        help="Path inside the container where the repo is checked out (default: /app).",
    )
    parser.add_argument(
        "--resume",
        action="store_true",
        help="Skip instances that already have a prediction in predictions.jsonl.",
    )
    parser.add_argument(
        "--fresh",
        action="store_true",
        help="Delete existing predictions and run_manifest.json in the output directory before starting.",
    )
    parser.add_argument(
        "--keep-container",
        action="store_true",
        help="Do not remove the container after processing (useful for debugging).",
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=1,
        help="Number of instances to process in parallel (default: 1).",
    )
    parser.add_argument(
        "--repair-feedback",
        default=None,
        help="JSON file mapping instance_id to extra feedback text appended to the prompt.",
    )
    parser.add_argument(
        "--compact-prompt",
        action="store_true",
        help="Use a compact prompt that omits pass-to-pass tests and shortens requirements for smaller models.",
    )
    parser.add_argument(
        "--small-model",
        action="store_true",
        help="Activate the small-model context adapter (shallow repo tree + ranked file snippets). "
             "Auto-enabled for models inferred as the 'small' tier when --compact-prompt is used.",
    )
    parser.add_argument(
        "--few-shot-examples",
        default=None,
        help="Path to a file containing 1-2 short problem→patch examples to inject into the prompt. "
             "The file is copied into the repo but excluded from the captured git diff.",
    )
    parser.add_argument(
        "--force-edit",
        action="store_true",
        help="If the model produces an empty patch, re-run once with a stronger directive that mandates a file edit.",
    )
    parser.add_argument(
        "--retry-failures",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="After --force-edit, retry failed instances with an escalation config tailored to the failure mode (default: true).",
    )
    parser.add_argument(
        "--max-retries",
        type=int,
        default=2,
        help="Maximum recovery retries per instance when --retry-failures is enabled (default: 2).",
    )
    parser.add_argument(
        "--diff-fallback",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="After recovery retries are exhausted, make a final one-shot API call asking for a unified diff and apply it directly (default: true).",
    )
    parser.add_argument(
        "--early-diff-fallback",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="If the first agent attempt fails with JSON_PARSE_ERROR or MAX_ITERATIONS, run the one-shot diff fallback before the recovery loop (default: true).",
    )
    parser.add_argument(
        "--small-model-diff-fallback",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="For small/fragile models that default to agentless, skip SEARCH/REPLACE and ask the model directly for a unified diff (with the test patch applied).",
    )
    parser.add_argument(
        "--agentless",
        "--no-agentless",
        action=TristateAction,
        default=None,
        help="Skip the Selfware agent loop and ask the model directly for a patch. Useful for cheap/fragile models. "
             "Use --no-agentless or --agentless=false to force the multi-turn tool loop even when the model would otherwise default to agentless.",
    )
    parser.add_argument(
        "--auto-agentless",
        action=argparse.BooleanOptionalAction,
        default=None,
        help="Automatically enable --agentless for small models that are marked not recommended in the config registry. "
             "This is now the default behavior; use --no-agentless to override it for a specific run.",
    )
    parser.add_argument(
        "--adaptive",
        action="store_true",
        help="Adapt agent settings (prompt length, tool catalog, iterations, edit deadline) to the model's inferred capability tier.",
    )
    parser.add_argument(
        "--local-endpoint",
        default=None,
        help="Override the 'endpoint' value in the TOML config (e.g. http://localhost:8000/v1).",
    )
    parser.add_argument(
        "--plan-then-patch",
        action="store_true",
        help="Enable dual-model plan-then-patch mode. A cheap model plans which files to edit, then the main model generates the patch.",
    )
    parser.add_argument(
        "--plan-model-profile",
        default=None,
        help="OpenRouter profile for the planning step. Defaults to --model-profile.",
    )
    parser.add_argument(
        "--plan-max-tokens",
        type=int,
        default=1024,
        help="Maximum tokens for the planning step (default: 1024).",
    )
    parser.add_argument(
        "--plan-temperature",
        type=float,
        default=0.3,
        help="Temperature for the planning step (default: 0.3).",
    )
    parser.add_argument(
        "--plan-timeout",
        type=int,
        default=120,
        help="Timeout in seconds for the planning API call (default: 120).",
    )
    parser.add_argument(
        "--patch-timeout",
        type=int,
        default=300,
        help="Timeout in seconds for the patch-generation API call (default: 300).",
    )
    parser.add_argument(
        "--test-driven-repair",
        "--tdr",
        dest="tdr",
        action="store_true",
        help="Enable test-driven repair: run SWE-bench Pro tests in a container and iterate with a repair model.",
    )
    parser.add_argument(
        "--repair-model-profile",
        default=None,
        help="OpenRouter profile for the TDR repair model. Required unless --repair-config is given.",
    )
    parser.add_argument(
        "--repair-config",
        default=None,
        help="Path to the repair model TOML config. Defaults to <config-dir>/openrouter_<repair-model-profile>.toml.",
    )
    parser.add_argument(
        "--repair-iterations",
        type=int,
        default=2,
        help="Maximum repair iterations after the initial patch (default: 2).",
    )
    parser.add_argument(
        "--repair-timeout",
        type=int,
        default=300,
        help="Timeout in seconds for each repair-model API call (default: 300).",
    )
    parser.add_argument(
        "--repair-max-tokens",
        type=int,
        default=16384,
        help="Maximum tokens for each repair-model API call (default: 16384).",
    )
    parser.add_argument(
        "--tdr-test-timeout",
        type=int,
        default=600,
        help="Timeout in seconds for running tests inside the TDR container (default: 600).",
    )
    parser.add_argument(
        "--tdr-compile-timeout",
        type=int,
        default=180,
        help="Timeout in seconds for the TDR compile check (default: 180).",
    )
    parser.add_argument(
        "--tdr-keep-container",
        action="store_true",
        help="Do not remove the TDR container after processing.",
    )
    parser.add_argument(
        "--ensemble-models",
        default=None,
        help="Comma-separated list of OpenRouter profiles for ensemble seed generation.",
    )
    parser.add_argument(
        "--ensemble-timeout",
        type=int,
        default=180,
        help="Timeout in seconds for each ensemble seed generation API call (default: 180).",
    )
    parser.add_argument(
        "--ensemble-max-tokens",
        type=int,
        default=4096,
        help="Maximum tokens for each ensemble seed generation API call (default: 4096).",
    )
    parser.add_argument(
        "--critic-iterations",
        type=int,
        default=0,
        help="Enable P2 critic loop: after a patch passes the compile gate, re-run fail-to-pass tests and ask a critic model to refine the patch up to N times (default: 0).",
    )
    parser.add_argument(
        "--critic-model-profile",
        default=None,
        help="OpenRouter profile for the P2 critic model. Defaults to --model-profile.",
    )
    parser.add_argument(
        "--critic-timeout",
        type=int,
        default=300,
        help="Timeout in seconds for each critic model API call (default: 300).",
    )
    parser.add_argument(
        "--critic-max-tokens",
        type=int,
        default=16384,
        help="Maximum tokens for each critic model API call (default: 16384).",
    )
    return parser.parse_args()


def setup_logging(output_dir: Path) -> logging.Logger:
    output_dir.mkdir(parents=True, exist_ok=True)
    log_path = output_dir / "harness.log"
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
        handlers=[logging.FileHandler(log_path), logging.StreamHandler(sys.stdout)],
    )
    logger = logging.getLogger("selfware-sweap")
    logger.propagate = False
    return logger


def load_list_field(value: Any) -> list[str]:
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


DEFAULT_CHAT_ENDPOINT = "https://openrouter.ai/api/v1/chat/completions"


def load_config(config_path: Path) -> dict[str, Any]:
    """Load an OpenRouter / local LLM TOML config."""
    with open(config_path, "rb") as f:
        return tomllib.load(f)


def _parse_model_size(model_id: str) -> float | None:
    """Extract a parameter count in billions from the model id, if present."""
    match = re.search(r"(\d+(?:\.\d+)?)\s*b", model_id, re.IGNORECASE)
    if match:
        return float(match.group(1))
    return None


def infer_capability_tier(model_id: str, config: dict[str, Any] | None = None) -> str:
    """Infer a capability tier from the configured model id and config metadata.

    Tiers:
      - small:  local endpoints, explicitly small metadata, or models <=13B
      - medium: known mid-size models (qwen3.5-27b, qwen3.6-27b, gemma4-12b) or <=31B
      - large:  everything else
    """
    if config is not None:
        metadata_tier = (config.get("metadata", {}) or {}).get("tier")
        if metadata_tier in ("small", "medium", "large"):
            return metadata_tier

    lower = model_id.lower()
    if "local" in lower:
        return "small"

    # Explicit aliases for models whose names do not encode parameter counts.
    small_aliases = ("gpt-5-mini", "gemini-3.5-flash", "llama-3.1-8b", "qwen2.5-7b")
    medium_aliases = (
        "qwen3.5-27b", "qwen3.6-27b", "gemma4-12b", "gemma-3-12b",
        "granite-4.1-8b", "llama-4-scout", "mistral-nemo", "nova-lite",
        "ling-2.6-flash",
    )
    if any(alias in lower for alias in small_aliases):
        return "small"
    if any(alias in lower for alias in medium_aliases):
        return "medium"

    size = _parse_model_size(model_id)
    if size is not None:
        if size <= 13:
            return "small"
        if size <= 31:
            return "medium"
    return "large"


def apply_adaptive_overrides(config: dict[str, Any], tier: str) -> None:
    """Patch the [agent] table of a loaded TOML config in place.

    Adaptive mode supplies conservative ceilings and defaults; it must not
    clobber tighter per-model tuning from YAML/TOML configs.
    """
    agent = config.setdefault("agent", {})
    if tier == "small":
        agent["max_iterations"] = min(agent.get("max_iterations", 30), 30)
        agent["edit_deadline_step"] = min(agent.get("edit_deadline_step", 6), 6)
        agent["max_no_edit_steps"] = min(agent.get("max_no_edit_steps", 6), 6)
        agent["disable_episodic_memory"] = True
        agent["minimal_tool_catalog"] = True
        agent["context_window"] = min(agent.get("context_window", 0), 0)
    elif tier == "medium":
        agent["max_iterations"] = min(agent.get("max_iterations", 45), 45)
        agent["edit_deadline_step"] = min(agent.get("edit_deadline_step", 8), 8)
    # large: leave defaults unchanged.


def apply_small_model_overrides(config: dict[str, Any], tier: str) -> None:
    """Apply extra-aggressive [agent] limits for the small-model adapter.

    Values are capped (not blindly overwritten) so a model that already has
    tighter defaults keeps them.
    """
    agent = config.setdefault("agent", {})
    agent["max_iterations"] = min(agent.get("max_iterations", 60), 25)
    agent["max_no_edit_steps"] = min(agent.get("max_no_edit_steps", 6), 5)
    agent["edit_deadline_step"] = min(agent.get("edit_deadline_step", 6), 5)
    agent["disable_episodic_memory"] = True
    agent["minimal_tool_catalog"] = True
    # Disable LLM-based context compression for small models; it is unreliable
    # on cheap endpoints and the snippet adapter already limits context.
    agent["context_window"] = 0


def _small_model_adapter_enabled(
    args: argparse.Namespace,
    config: dict[str, Any],
) -> tuple[bool, str]:
    """Return (enabled, tier) for the small-model adapter."""
    if args.small_model:
        model_id = config.get("model", args.model_profile)
        return True, infer_capability_tier(model_id, config)
    if args.compact_prompt:
        model_id = config.get("model", args.model_profile)
        tier = infer_capability_tier(model_id, config)
        if tier == "small":
            return True, tier
    return False, ""


def prepare_effective_config(
    args: argparse.Namespace,
    logger: logging.Logger,
) -> Path:
    """Load the requested config, apply runtime overrides, and write a temp copy.

    When --adaptive is set the [agent] section is capped based on the
    inferred capability tier.  --small-model (or a small-tier model with
    --compact-prompt) applies additional aggressive [agent] limits and
    signals process_instance to build a ranked-snippet prompt.  --local-endpoint
    overrides endpoint and clears the API key for local servers.
    """
    config_path = Path(args.config_dir) / f"openrouter_{args.model_profile}.toml"
    if not config_path.exists():
        raise FileNotFoundError(f"Config not found: {config_path}")

    config = load_config(config_path)
    patched = False

    # Prefer small-model YAML overrides when agentless routing is selected.
    if should_use_agentless(args, config):
        yaml_config = load_small_model_config(
            args.model_profile, args.small_model_config_dir, logger
        )
        if yaml_config is not None:
            config = merge_over_toml(config, yaml_config)
            patched = True
            logger.info(
                "Merged small-model YAML config for '%s' from %s",
                args.model_profile,
                Path(args.small_model_config_dir) / f"{args.model_profile}.yaml",
            )

    if args.local_endpoint:
        config["endpoint"] = args.local_endpoint
        config["api_key"] = ""
        logger.info("Overriding endpoint with --local-endpoint: %s", args.local_endpoint)
        patched = True

    model_id = config.get("model", args.model_profile)
    tier = infer_capability_tier(model_id, config)
    adapter_enabled, adapter_tier = _small_model_adapter_enabled(args, config)
    args.small_model_adapter = adapter_enabled
    if adapter_enabled:
        logger.info(
            "Small-model adapter active for '%s' (tier=%s)",
            model_id,
            adapter_tier or tier,
        )
        apply_small_model_overrides(config, tier)
        patched = True

    if args.adaptive:
        logger.info(
            "Adaptive mode: model '%s' inferred as '%s' tier",
            model_id,
            tier,
        )
        apply_adaptive_overrides(config, tier)
        patched = True

    if not patched:
        return config_path

    output_dir = Path(args.output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    effective_path = output_dir / f"openrouter_{args.model_profile}.adaptive.toml"
    with open(effective_path, "wb") as f:
        tomli_w.dump(config, f)
    logger.info("Wrote effective config: %s", effective_path)
    return effective_path


def add_host_repo_to_safety_allowed_paths(
    config_path: Path,
    host_repo_dir: Path,
    output_dir: Path,
    model_profile: str,
    logger: logging.Logger,
) -> Path:
    """Add the absolute host repo path to ``[safety].allowed_paths``.

    The SWE-bench Pro container exposes the repo at ``/app``, and the selfware
    binary maps ``/app`` references to the host repo directory.  When running
    on the host directly, absolute ``/app/...`` paths would otherwise be
    rejected by the safety checker.  This patch keeps the existing ``./**``
    and ``/app/**`` entries and appends the resolved host repo path.
    """
    config = load_config(config_path)
    safety = config.setdefault("safety", {})
    allowed = list(safety.get("allowed_paths", []))

    for entry in ("./**", "/app", "/app/**"):
        if entry not in allowed:
            allowed.append(entry)

    abs_repo = str(host_repo_dir.resolve())
    if abs_repo not in allowed:
        allowed.append(abs_repo)

    safety["allowed_paths"] = allowed

    output_dir.mkdir(parents=True, exist_ok=True)
    effective_path = output_dir / f"openrouter_{model_profile}.adaptive.toml"
    with open(effective_path, "wb") as f:
        tomli_w.dump(config, f)
    logger.info(
        "Wrote safety-patched config: %s (added host repo path %s)",
        effective_path,
        abs_repo,
    )
    return effective_path


def normalize_endpoint(endpoint: str) -> str:
    """Ensure the endpoint ends with /chat/completions for direct API calls."""
    endpoint = endpoint.rstrip("/")
    if endpoint.endswith("/chat/completions"):
        return endpoint
    if endpoint.endswith("/v1"):
        return endpoint + "/chat/completions"
    return endpoint


def _parse_context_limit_from_error(error_body: str) -> tuple[int | None, int | None, int | None]:
    """Extract context limit and token breakdown from a provider error body."""
    limit: int | None = None
    input_tokens: int | None = None
    output_tokens: int | None = None
    try:
        data = json.loads(error_body)
        message = (data.get("error", {}).get("message") or data.get("message") or "")
    except (json.JSONDecodeError, AttributeError):
        message = error_body
    m = re.search(r"maximum context length is (\d+) tokens?", message, re.IGNORECASE)
    if m:
        limit = int(m.group(1))
    m = re.search(r"\((\d+) of text input", message)
    if m:
        input_tokens = int(m.group(1))
    m = re.search(r"(\d+) in the output", message)
    if m:
        output_tokens = int(m.group(1))
    return limit, input_tokens, output_tokens


def _estimate_input_tokens(text: str) -> int:
    """Rough token count heuristic for prompt text."""
    if not text:
        return 0
    return max(1, len(text.split()))


def call_chat_endpoint(
    config: dict[str, Any],
    prompt: str,
    timeout: int,
    logger: logging.Logger,
    *,
    max_tokens: int | None = None,
    temperature: float | None = None,
    allow_token_growth: bool = True,
) -> str:
    """Send a single-turn prompt to the configured chat endpoint."""
    endpoint = normalize_endpoint(config.get("endpoint", DEFAULT_CHAT_ENDPOINT))
    model = config["model"]
    effective_max_tokens = max_tokens if max_tokens is not None else config.get("max_tokens", 4096)
    effective_temperature = temperature if temperature is not None else config.get("temperature", 0.1)
    api_key = os.environ.get("SELFWARE_API_KEY")
    if not api_key:
        raise RuntimeError("SELFWARE_API_KEY is not set")

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }
    payload = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": effective_max_tokens,
        "temperature": effective_temperature,
    }
    reasoning_effort = config.get("reasoning_effort")
    if reasoning_effort:
        payload["reasoning_effort"] = reasoning_effort
    data = json.dumps(payload).encode("utf-8")

    max_retries = 4
    base_delay = 2.0
    max_delay = 60.0
    # Cap any auto-increased token budget so we do not accidentally request
    # an unsupported or absurdly large value from the provider.
    absolute_max_tokens = 65536
    last_exc: Exception | None = None

    for attempt in range(max_retries + 1):
        req = urllib.request.Request(endpoint, data=data, headers=headers, method="POST")
        logger.info(
            "Calling chat endpoint model=%s max_tokens=%s temp=%s attempt=%s/%s",
            model,
            effective_max_tokens,
            effective_temperature,
            attempt + 1,
            max_retries + 1,
        )
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                raw = resp.read().decode("utf-8")
            try:
                result = json.loads(raw)
            except json.JSONDecodeError as exc:
                last_exc = exc
                delay = min(base_delay * (2 ** attempt), max_delay) + random.uniform(0, 1)
                logger.warning(
                    "Chat response was not valid JSON on attempt %s; retrying in %.1fs: %s",
                    attempt + 1,
                    delay,
                    raw[:500],
                )
                time.sleep(delay)
                continue
            try:
                choice = result["choices"][0]
                content = choice["message"].get("content")
                if not content:
                    content = choice["message"].get("reasoning", "")
                content = content if content is not None else ""
                finish_reason = choice.get("finish_reason") or choice.get("native_finish_reason")
                usage = result.get("usage") or {}
                completion_tokens = usage.get("completion_tokens")
                reasoning_tokens = usage.get("completion_tokens_details", {}).get("reasoning_tokens")
                logger.info(
                    "Chat response model=%s finish_reason=%s completion_tokens=%s reasoning_tokens=%s content_len=%s",
                    model,
                    finish_reason,
                    completion_tokens,
                    reasoning_tokens,
                    len(content),
                )
                # If the provider stopped because it hit the output-token limit,
                # the patch is almost certainly truncated. Retry with a larger
                # budget so reasoning-heavy models (e.g. Gemini 2.5 Pro) have
                # enough headroom to emit the actual code patch.
                if finish_reason in ("length", "MAX_TOKENS", "max_tokens") and attempt < max_retries:
                    if not allow_token_growth:
                        logger.warning(
                            "Chat response hit token limit (finish_reason=%s, completion_tokens=%s); "
                            "token growth disabled for this call, returning truncated content",
                            finish_reason,
                            completion_tokens,
                        )
                        return content
                    next_max_tokens = min(effective_max_tokens * 2, absolute_max_tokens)
                    context_window = config.get("agent", {}).get("context_window") if config.get("agent") else None
                    prompt_tokens = usage.get("prompt_tokens")
                    if context_window and prompt_tokens is not None:
                        max_allowed = context_window - prompt_tokens - 1
                        if max_allowed < next_max_tokens:
                            next_max_tokens = max_allowed
                    elif context_window and prompt_tokens is None:
                        max_allowed = context_window - _estimate_input_tokens(prompt) - 1
                        if max_allowed < next_max_tokens:
                            next_max_tokens = max_allowed
                    if next_max_tokens <= effective_max_tokens:
                        logger.warning(
                            "Chat response hit token limit but cannot increase max_tokens within context window (finish_reason=%s); returning truncated content",
                            finish_reason,
                        )
                        return content
                    logger.warning(
                        "Chat response hit token limit (finish_reason=%s, completion_tokens=%s); retrying with max_tokens=%s",
                        finish_reason,
                        completion_tokens,
                        next_max_tokens,
                    )
                    effective_max_tokens = next_max_tokens
                    payload["max_tokens"] = effective_max_tokens
                    data = json.dumps(payload).encode("utf-8")
                    continue
                return content
            except (KeyError, IndexError) as exc:
                raise RuntimeError(f"Unexpected chat response shape: {result}") from exc
        except urllib.error.HTTPError as exc:
            body = exc.read().decode("utf-8", errors="ignore")
            if exc.code == 400:
                limit, input_tokens, _ = _parse_context_limit_from_error(body)
                if limit is not None and input_tokens is not None:
                    next_max_tokens = limit - input_tokens - 1
                    if next_max_tokens < effective_max_tokens:
                        logger.warning(
                            "Chat request hit context limit (%s tokens); reducing max_tokens from %s to %s and retrying",
                            limit,
                            effective_max_tokens,
                            next_max_tokens,
                        )
                        effective_max_tokens = next_max_tokens
                        payload["max_tokens"] = effective_max_tokens
                        data = json.dumps(payload).encode("utf-8")
                        continue
            if exc.code == 429 or exc.code >= 500:
                last_exc = exc
                delay = min(base_delay * (2 ** attempt), max_delay) + random.uniform(0, 1)
                logger.warning(
                    "Chat request failed (%s) on attempt %s; retrying in %.1fs: %s",
                    exc.code,
                    attempt + 1,
                    delay,
                    body[:200],
                )
                time.sleep(delay)
                continue
            raise RuntimeError(f"Chat request failed ({exc.code}): {body}") from exc
        except (urllib.error.URLError, socket.timeout, TimeoutError) as exc:
            last_exc = exc
            delay = min(base_delay * (2 ** attempt), max_delay) + random.uniform(0, 1)
            logger.warning(
                "Chat request failed on attempt %s; retrying in %.1fs: %s",
                attempt + 1,
                delay,
                exc,
            )
            time.sleep(delay)
            continue

    raise RuntimeError(f"Chat request failed after {max_retries + 1} attempts: {last_exc}") from last_exc


def build_prompt(
    instance: dict[str, Any],
    repair_feedback: str | None = None,
    compact: bool = False,
    few_shot_examples: str | None = None,
    repo_path: str | Path = CONTAINER_REPO_DIR,
    ranked_files: list[str] | None = None,
) -> str:
    tests = load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = load_list_field(instance.get("fail_to_pass", []))
    pass_to_pass = load_list_field(instance.get("pass_to_pass", []))
    language = (instance.get("repo_language") or "").lower()
    repo = instance.get("repo", "")
    test_cmd = _format_test_command(language, tests, repo=repo)
    target_api = _format_target_api_section(instance.get("interface", "") or "")
    test_oracle = _build_focused_test_oracle(instance.get("test_patch", "") or "")

    if compact:
        requirements_text = (
            "Fix the issue above with the smallest source-code patch. "
            "Make the failing tests pass. Do not modify tests, configs, docs, or unrelated code."
        )
    else:
        requirements_text = instance.get("requirements", "")

    repo_display = str(repo_path)

    def _is_test_file(rel: str) -> bool:
        name = rel.lower()
        return name.endswith("_test.go") or name.startswith("test_") or name.endswith("_test.py") or name.endswith("_test.js") or name.endswith("_test.ts")

    source_ranked = [f for f in (ranked_files or []) if not _is_test_file(f)]
    editable_manifest = "\n".join(f"- {f}" for f in source_ranked[:10]) or "- (none identified; use file_read to locate the relevant source file)"

    search_text = f"{instance.get('problem_statement', '')}\n{requirements_text}".strip()
    snippet_terms = [t for t in _tokenize_problem(search_text) if _is_strong_identifier(t)]
    snippets = truncate_file_reads(
        source_ranked[:3],
        repo_path=repo_path,
        max_lines=150,
        max_chars=6000,
        highlight_terms=snippet_terms,
    ) if source_ranked else "(no source snippets available)\n"

    sections = [
        f"Repo: {repo_display} ({instance.get('repo')} @ {instance.get('base_commit')})",
        "Working directory: the repo root shown above. Use relative paths only (e.g. lib/auth/grpcserver.go). Do NOT use absolute paths like /app/...; they will be rejected.",
        "",
        "GOAL: Fix the issue with the smallest source-code patch. Your final output is a git diff, so you MUST modify source files.",
        "",
        "Issue:",
        instance.get("problem_statement", ""),
        "",
        "Requirements:",
        requirements_text,
        "",
        target_api or "Target API: (none identified from the task interface)",
        "",
        "Test files:",
        "\n".join(f"- {t}" for t in tests) or "- (none specified)",
        "",
        "Fail-to-pass:",
        "\n".join(f"- {t}" for t in fail_to_pass) or "- (none specified)",
    ]
    if not compact:
        sections.extend([
            "",
            "Pass-to-pass:",
            "\n".join(f"- {t}" for t in _cap_pass_to_pass(pass_to_pass)) or "- (none specified)",
        ])
    sections.extend([
        "",
        f"Run tests: {test_cmd}",
        "",
        "Focused test oracle (concrete failing tests and key assertions; do NOT edit tests):",
        test_oracle or "- (none extracted from test patch)",
        "",
        "Relevant source excerpts (line numbers are for reference only):",
        snippets,
        "",
        "Editable file candidates — prioritize these source files:",
        editable_manifest,
        "You may read or edit other source files if the fix clearly requires it, but state why in one sentence first.",
        "",
        "MANDATORY WORKFLOW:",
    ])
    if compact:
        sections.extend([
            "1. Read the issue and the most relevant source file from the excerpts above.",
            "2. Your FIRST concrete action must be file_edit on the relevant source file.",
            "3. Run the test command. If any fail-to-pass test still fails, edit again.",
            "4. Finish only after at least one file_edit and the fail-to-pass tests pass (or you prove the failure is unrelated to your change).",
        ])
    else:
        sections.extend([
            "1. READ (steps 1-3): Read the issue + the top-ranked source file excerpt. Stop once you know the root cause.",
            "2. EDIT (by step 4): Use file_edit to apply the minimal source fix. Do not read past step 4 without editing.",
            "3. VERIFY: Run the fail-to-pass tests. If any still fail, edit again.",
            "4. COMPLETE: Only finish after at least one file_edit and the fail-to-pass tests pass (or you prove the failure is unrelated to your change).",
        ])
    sections.extend([
        "",
        "CORRECT file_edit EXAMPLE (format only; replace with the real old/new lines from the file you edit):",
        "",
        "### FILE: lib/auth/grpcserver.go",
        "<<<<<<< SEARCH",
        "func processRequest() error {",
        "    err := doWork()",
        "    return trace.Wrap(err)",
        "}",
        "=======",
        "func processRequest() error {",
        "    if err := validate(); err != nil {",
        "        return trace.Wrap(err)",
        "    }",
        "    err := doWork()",
        "    return trace.Wrap(err)",
        "}",
        ">>>>>>> REPLACE",
        "",
        "CRITICAL RULES:",
        "- Modify source files only. Do NOT edit tests, Dockerfiles, binaries, configs, docs, or unrelated code.",
        "- Do NOT produce an empty patch. At least one source file must change.",
        "- You MUST call file_edit at least once before finishing. No exceptions.",
        "- Your final deliverable is the git diff of source-file changes.",
        "- Make your first file_edit by step 4.",
        "- Prefer file_edit over file_write; include 3-5 lines of context.",
        "- Keep the patch minimal: no formatting, comment, or unrelated changes.",
        "- Use relative paths only. Do NOT use /app/... or other absolute paths.",
        "- Do not create new source files unless the test patch explicitly requires them.",
        "- Implement only the smallest change that makes the fail-to-pass tests pass. Do not gold-plate.",
    ])
    if few_shot_examples:
        sections.extend([
            "",
            "EXAMPLES (problem → patch style):",
            few_shot_examples,
        ])
    if repair_feedback:
        sections.extend([
            "",
            "REPAIR FEEDBACK (fix the remaining failure):",
            repair_feedback,
        ])
    return "\n".join(sections)


def _cap_pass_to_pass(pass_to_pass: list[str], max_count: int = 5) -> list[str]:
    """Cap the number of pass-to-pass tests shown in the prompt.

    A long list of already-passing tests drowns out the failing tests the
    model actually needs to focus on.  Show only the first ``max_count`` and
    let the evaluator run the full set.
    """
    return pass_to_pass[:max_count]


# ---------------------------------------------------------------------------
# Plan-then-patch helpers (direct API calls, bypass the selfware binary)
# ---------------------------------------------------------------------------


def build_plan_prompt(instance: dict[str, Any]) -> str:
    """Build a very short prompt that asks the model for a fix plan."""
    tests = load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = load_list_field(instance.get("fail_to_pass", []))
    repo = instance.get("repo", "")
    repo_template = load_repo_template(repo)
    template_section = (
        f"\n\nRepo-specific instructions:\n{repo_template}"
        if repo_template
        else ""
    )
    return (
        "You are a planning assistant. Given the issue below, identify the source files "
        "that need to be read/edited and describe the fix strategy in one sentence.\n\n"
        "Issue:\n"
        f"{instance.get('problem_statement', '')}\n\n"
        "Failing tests:\n"
        f"{'\n'.join(f'- {t}' for t in fail_to_pass) or '- (none specified)'}\n\n"
        "Test files:\n"
        f"{'\n'.join(f'- {t}' for t in tests) or '- (none specified)'}\n\n"
        "Reply exactly in this format (no markdown, no extra text):\n"
        "FILES: <comma-separated list of source file paths relative to the repo root>\n"
        "FIX: <one-sentence fix strategy>"
        f"{template_section}"
    )


def parse_plan(response: str) -> tuple[list[str], str]:
    """Parse the plan response into (file_paths, fix_sentence)."""
    files: list[str] = []
    fix = ""
    for line in response.splitlines():
        line = line.strip()
        if line.upper().startswith("FILES:"):
            raw = line.split(":", 1)[1].strip()
            files = [p.strip() for p in raw.split(",") if p.strip()]
        elif line.upper().startswith("FIX:"):
            fix = line.split(":", 1)[1].strip()
    return files, fix


def _normalize_plan_file(repo_dir: Path, file_path: str) -> Path | None:
    """Resolve a path from the plan into a file under repo_dir, if possible."""
    file_path = file_path.strip().strip("`\"'")
    if not file_path or ".." in Path(file_path).parts:
        return None

    # Absolute paths inside the repo.
    as_path = Path(file_path)
    if as_path.is_absolute():
        try:
            as_path.relative_to(repo_dir)
            return as_path
        except ValueError:
            return None

    # Relative paths.
    candidate = repo_dir / as_path
    if candidate.exists() and candidate.is_file():
        return candidate

    # Bare filename: search under repo_dir.
    if len(as_path.parts) == 1:
        matches = list(repo_dir.rglob(as_path.name))
        for m in matches:
            if m.is_file():
                return m

    return None


def read_file_excerpts(
    repo_dir: Path,
    file_paths: list[str],
    max_lines: int = 300,
) -> dict[str, str]:
    """Read excerpts for the planned files, keyed by repo-relative path."""
    excerpts: dict[str, str] = {}
    for raw in file_paths:
        path = _normalize_plan_file(repo_dir, raw)
        if path is None:
            continue
        try:
            rel = path.relative_to(repo_dir).as_posix()
            lines = path.read_text(encoding="utf-8", errors="ignore").splitlines()
            if len(lines) <= max_lines:
                excerpts[rel] = "\n".join(lines)
            else:
                half = max_lines // 2
                excerpts[rel] = "\n".join(
                    lines[:half]
                    + [f"\n... ({len(lines) - max_lines} lines omitted) ...\n"]
                    + lines[-half:]
                )
        except Exception:
            continue
    return excerpts


def build_patch_prompt(
    instance: dict[str, Any],
    plan_files: list[str],
    plan_fix: str,
    excerpts: dict[str, str],
) -> str:
    """Build a focused prompt for the patch-generation step."""
    tests = load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = load_list_field(instance.get("fail_to_pass", []))
    language = (instance.get("repo_language") or "").lower()

    if language == "python":
        test_cmd = f"python -m pytest {' '.join(tests)}" if tests else "python -m pytest"
    elif language == "javascript" or language == "typescript":
        test_cmd = f"npm test -- {' '.join(tests)}" if tests else "npm test"
    elif language == "go":
        test_cmd = f"go test {' '.join(tests)}" if tests else "go test ./..."
    else:
        test_cmd = "run the relevant test suite"

    sections = [
        f"Repo: {CONTAINER_REPO_DIR} ({instance.get('repo')} @ {instance.get('base_commit')})",
        "",
        "GOAL: Produce the smallest source-code patch that fixes the issue.",
        "Your response must contain a patch in exactly one of these two formats:",
        "  1) A unified git diff that can be applied with `git apply`.",
        "  2) One or more file edit blocks using this exact pattern:",
        "",
        "### FILE: path/to/file.py",
        "<<<<<<< SEARCH",
        "old lines",
        "=======",
        "new lines",
        ">>>>>>> REPLACE",
        "",
        "Issue:",
        instance.get("problem_statement", ""),
        "",
        "Planned files:",
        "\n".join(f"- {f}" for f in plan_files) or "- (none specified)",
        "",
        "Fix strategy:",
        plan_fix or "- (none specified)",
        "",
        f"Run tests: {test_cmd}",
        "",
        "Failing tests:",
        "\n".join(f"- {t}" for t in fail_to_pass) or "- (none specified)",
        "",
    ]
    if excerpts:
        sections.append("Relevant source file excerpts:")
        for rel, excerpt in excerpts.items():
            sections.extend([f"\n--- {rel} ---\n", excerpt])
        sections.append("")
    sections.extend([
        "CRITICAL RULES:",
        "- Reply ONLY with a patch. Do not add explanations, summaries, or markdown outside the patch.",
        "- For a unified diff: start with `diff --git a/... b/...` and make sure `git apply` accepts it.",
        "- For file edits: each block must start with `### FILE: <path>` followed by `<<<<<<< SEARCH`, `=======`, and `>>>>>>> REPLACE`.",
        "- Modify source files only. Do NOT edit tests, configs, docs, or unrelated code.",
        "- Keep the patch minimal: no formatting, comment, or unrelated changes.",
        "- Do NOT produce an empty patch.",
    ])
    return "\n".join(sections)


def _filter_prediction_patch_for_instance(
    instance: dict[str, Any],
    patch: str,
) -> str:
    """Drop tests/docs/config noise from a candidate prediction patch."""
    official_patch = instance.get("patch", "") or ""
    official_fix_paths = set(_changed_files_from_patch(official_patch)) | set(
        _new_files_from_patch(official_patch)
    )
    test_patch = instance.get("test_patch", "") or ""
    extra_allowed = set(_new_files_from_patch(test_patch))
    return filter_patch_to_source_files(
        patch,
        extra_allowed=extra_allowed,
        official_fix_paths=official_fix_paths,
        test_patch_paths=paths_from_patch(test_patch),
    )


def run_diff_fallback(
    host_repo_dir: Path,
    instance: dict[str, Any],
    prompt_text: str,
    ranked_files: list[str],
    patch_config: dict[str, Any],
    args: argparse.Namespace,
    log_dir: Path,
    logger: logging.Logger,
    name: str,
) -> str:
    """Make a one-shot diff request and, if it applies, return the git diff."""
    instance_id = instance["instance_id"]

    # Cap source snippets so the full fallback prompt fits inside the model's
    # context window after reserving space for the output and prompt overhead.
    context_window = patch_config.get("agent", {}).get("context_window") or 0
    max_tokens = int(patch_config.get("max_tokens") or 4096)
    try:
        configured_fallback_max_tokens = int(
            patch_config.get("diff_fallback_max_tokens")
            or DEFAULT_DIFF_FALLBACK_MAX_TOKENS
        )
    except (TypeError, ValueError):
        configured_fallback_max_tokens = DEFAULT_DIFF_FALLBACK_MAX_TOKENS
    fallback_max_tokens = max(1, min(max_tokens, configured_fallback_max_tokens))
    prompt_tokens = _estimate_input_tokens(prompt_text)
    if context_window:
        available_tokens = context_window - fallback_max_tokens - prompt_tokens - 300
        max_snippet_chars = max(2000, min(16000, available_tokens * 3))
    else:
        max_snippet_chars = 8000

    fallback_prompt = build_diff_fallback_prompt(
        prompt_text,
        ranked_files,
        host_repo_dir,
        max_chars=int(max_snippet_chars),
        allow_full_file_replacement=True,
    )
    (log_dir / f"{name}.diff_fallback.prompt.txt").write_text(
        fallback_prompt, encoding="utf-8"
    )
    logger.info(
        "Running diff fallback for %s (context_window=%s max_tokens=%s fallback_max_tokens=%s prompt_tokens=%s max_snippet_chars=%s)",
        instance_id,
        context_window,
        max_tokens,
        fallback_max_tokens,
        prompt_tokens,
        max_snippet_chars,
    )

    response = call_chat_endpoint(
        patch_config,
        fallback_prompt,
        args.patch_timeout,
        logger,
        max_tokens=fallback_max_tokens,
        allow_token_growth=False,
    )
    (log_dir / f"{name}.diff_fallback.response.md").write_text(
        response, encoding="utf-8"
    )

    diff_text = extract_diff(response)
    if diff_text:
        filtered_diff = _filter_prediction_patch_for_instance(instance, diff_text)
        if not filtered_diff.strip():
            logger.warning(
                "Diff fallback for %s was filtered to empty before apply",
                instance_id,
            )
            diff_text = ""
        elif filtered_diff != diff_text:
            logger.info(
                "Diff fallback for %s dropped non-source/test/config hunks before apply",
                instance_id,
            )
            diff_text = filtered_diff

    if diff_text:
        if _apply_diff_with_check(host_repo_dir, diff_text, logger):
            logger.info("Diff fallback applied unified diff for %s", instance_id)
        else:
            logger.warning("Diff fallback for %s could not apply diff", instance_id)
            diff_text = ""

    if not diff_text:
        # Some models return SEARCH/REPLACE blocks even when asked for a diff.
        # Accept any applyable patch shape before giving up, but reject partial
        # patches where some referenced files could not be edited.
        applied, missing_files = apply_model_response_with_missing(
            host_repo_dir, response, logger
        )
        if not applied:
            logger.warning(
                "Diff fallback for %s produced no applyable patch", instance_id
            )
            return ""
        if missing_files:
            logger.warning(
                "Diff fallback for %s applied only partially; missing files: %s",
                instance_id,
                sorted(missing_files),
            )
            _reset_repo(
                host_repo_dir, instance.get("base_commit"), logger
            )
            return ""
        logger.info("Diff fallback applied SEARCH/REPLACE edits for %s", instance_id)

    patch = capture_patch_on_host(
        host_repo_dir,
        logger,
        base_commit=instance.get("base_commit"),
        test_patch_paths=paths_from_patch(instance.get("test_patch", "") or ""),
    )
    patch = _filter_prediction_patch_for_instance(instance, patch)
    if patch.strip():
        logger.info("Diff fallback produced a non-empty patch for %s", instance_id)
    else:
        logger.warning("Diff fallback applied but patch capture is empty for %s", instance_id)
    return patch


def _run_diff_recovery(
    host_repo_dir: Path,
    instance: dict[str, Any],
    prompt_text: str,
    ranked_files: list[str],
    patch_config: dict[str, Any],
    args: argparse.Namespace,
    log_dir: Path,
    output_dir: Path,
    logger: logging.Logger,
    name: str,
    attempt: int,
    metadata: dict[str, Any],
) -> str:
    """Run the one-shot diff fallback as an EMPTY_PATCH recovery step.

    Resets the repo, re-applies the official test patch, asks the model for a
    raw unified diff, and runs the compile gate.  On success the prediction is
    saved and ``recovery_succeeded`` is recorded in ``metadata``.
    """
    instance_id = instance["instance_id"]
    base_commit = instance["base_commit"]
    logger.info("Diff recovery for %s attempt %s", instance_id, attempt)
    metadata["diff_recovery_fired"] = True

    if not _reset_repo(host_repo_dir, base_commit, logger):
        logger.error(
            "Failed to reset repo for diff recovery %s attempt %s",
            instance_id,
            attempt,
        )
        return ""

    if not _apply_test_patch(
        host_repo_dir,
        instance.get("test_patch", "") or "",
        logger,
    ):
        logger.error(
            "Failed to apply test_patch for diff recovery %s attempt %s",
            instance_id,
            attempt,
        )
        return ""

    fallback_patch = run_diff_fallback(
        host_repo_dir,
        instance,
        prompt_text,
        ranked_files,
        patch_config,
        args,
        log_dir,
        logger,
        f"{name}_attempt{attempt}",
    )
    if not fallback_patch.strip():
        logger.info(
            "Diff recovery for %s attempt %s produced no applyable patch",
            instance_id,
            attempt,
        )
        return ""

    language = (instance.get("repo_language") or "").lower()
    if not _check_patch_builds(host_repo_dir, fallback_patch, language, logger, metadata):
        logger.warning(
            "Diff recovery patch for %s attempt %s failed compile gate; treating as empty",
            instance_id,
            attempt,
        )
        metadata["compile_gate_rejected"] = True
        return ""

    logger.info("Diff recovery succeeded for %s attempt %s", instance_id, attempt)
    metadata["recovery_succeeded"] = True
    save_prediction(output_dir, instance_id, fallback_patch, logger, metadata)
    return fallback_patch


def should_use_agentless(args: argparse.Namespace, config: dict[str, Any]) -> bool:
    """Return True when the agent loop should be skipped entirely.

    Default routing uses the multi-turn XML tool loop for all capable models.
    Direct SEARCH/REPLACE patch generation is reserved for small/fragile
    models and configs that explicitly request it.

    Explicit flags always win:
      * ``--agentless`` / ``--agentless=true`` forces agentless on.
      * ``--no-agentless`` / ``--agentless=false`` forces the tool loop on.

    ``--auto-agentless`` (the default) routes small-tier models to agentless.
    ``--no-auto-agentless`` disables that automatic routing while still
    respecting explicit flags and ``metadata.agentless_default``.
    """
    # Explicit opt-in / opt-out always wins.
    if args.agentless is True:
        return True
    if args.agentless is False:
        return False

    metadata = config.get("metadata", {}) or {}
    if metadata.get("agentless_default"):
        return True

    # Honor --no-auto-agentless: do not route to agentless solely based on tier.
    if args.auto_agentless is False:
        return False

    model_id = config.get("model", "")
    tier = infer_capability_tier(model_id, config)

    # Small/fragile models default to direct patch generation because the
    # multi-turn tool loop wastes iterations on models that cannot use it.
    if tier == "small":
        return True

    # Everything else defaults to the multi-turn agent loop.
    return False


def _reset_repo(host_repo_dir: Path, base_commit: str, logger: logging.Logger) -> bool:
    """Reset the host repo to the base commit."""
    proc = run_cmd(
        ["git", "-C", str(host_repo_dir), "reset", "--hard", base_commit],
        logger=logger,
    )
    if proc.returncode != 0:
        logger.error("Failed to reset repo to %s: %s", base_commit, proc.stderr.strip())
        return False
    return True


def _apply_test_patch(host_repo_dir: Path, test_patch: str, logger: logging.Logger) -> bool:
    """Apply the official test patch to the host repo so the agent can run failing tests."""
    if not test_patch or not test_patch.strip():
        return True
    patch_path = host_repo_dir / ".selfware_test_patch.diff"
    patch_path.write_text(test_patch, encoding="utf-8")
    try:
        proc = run_cmd(
            ["git", "-C", str(host_repo_dir), "apply", str(patch_path)],
            logger=logger,
        )
        if proc.returncode != 0:
            logger.warning("git apply test_patch failed: %s", proc.stderr.strip())
            # Fallback to patch -p1 --no-backup-if-mismatch
            proc = run_cmd(
                ["patch", "-p1", "--no-backup-if-mismatch", "-i", str(patch_path)],
                cwd=host_repo_dir,
                logger=logger,
            )
            if proc.returncode != 0:
                logger.error("patch fallback for test_patch failed: %s", proc.stderr.strip())
                return False
        return True
    finally:
        patch_path.unlink(missing_ok=True)


def _agentless_needs_package_expansion(
    instance: dict[str, Any],
    host_repo_dir: Path,
    top_k: int = 5,
    context_window: int = 32768,
) -> bool:
    """Return True when focused source snippets miss required identifiers.

    ``build_agentless_prompt`` disables package expansion by default because
    expanding to the whole package can shift context enough to regress focused
    cases.  If the strongest identifiers from the issue/requirements are not
    covered by the focused source snippets, enable expansion so the model sees
    the relevant definitions.
    """
    repo_path = Path(host_repo_dir)
    problem = instance.get("problem_statement", "") or ""
    requirements = instance.get("requirements", "") or ""
    search_text = problem
    if requirements:
        search_text += "\n" + requirements

    tests = load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = load_list_field(instance.get("fail_to_pass", []))

    ranked = rank_files_by_relevance(
        repo_path,
        search_text,
        test_names=tests + fail_to_pass,
        top_k=top_k * 3,
    )

    def _is_test_file(rel: str) -> bool:
        name = rel.lower()
        return (
            name.endswith("_test.go")
            or name.startswith("test_")
            or name.endswith("_test.py")
        )

    source_ranked = [f for f in ranked if not _is_test_file(f)]
    if len(source_ranked) < top_k:
        source_ranked = ranked[:top_k]

    mentioned_files = _extract_source_paths_from_text(search_text, repo_path)
    mentioned_files += _extract_source_paths_from_text("\n".join(fail_to_pass), repo_path)
    seen_files: set[str] = set()
    source_ranked = [
        f
        for f in (mentioned_files + source_ranked)
        if not _is_test_file(f) and not (f in seen_files or seen_files.add(f))
    ]

    snippet_files = source_ranked[:top_k]
    required_identifiers = list(
        dict.fromkeys(
            t for t in _tokenize_problem(search_text) if _is_strong_identifier(t)
        )
    )
    if not required_identifiers:
        return False

    snippet_max_chars, snippet_max_lines = _context_budgets(context_window)
    snippets = _read_agentless_file_snippets(
        snippet_files,
        repo_path,
        max_total_chars=snippet_max_chars,
        max_file_lines=snippet_max_lines,
        required_identifiers=required_identifiers,
        highlight_terms=required_identifiers,
    )
    combined = snippets.lower()
    return any(ident.lower() not in combined for ident in required_identifiers)


def run_agentless(
    host_repo_dir: Path,
    instance: dict[str, Any],
    patch_config: dict[str, Any],
    args: argparse.Namespace,
    log_dir: Path,
    logger: logging.Logger,
    name: str,
    few_shot_examples: str | None = None,
    system_message: str | None = None,
    prompt_suffix: str | None = None,
    metadata: dict[str, Any] | None = None,
) -> str:
    """Run the direct one-shot patch path and return the captured git diff.

    The optional ``metadata`` dict is mutated in place so callers can record
    recovery counters (``recovery_attempts``, ``diff_recovery_fired``,
    ``recovery_succeeded``) even when the agentless path is used.

    If the first response cannot be applied, reset the repo and retry once with
    a stricter prompt that includes exact file contents. If the retry also fails
    and ``args.diff_fallback`` is set, reset the repo again and try a one-shot
    unified-diff fallback.

    Recovery directives (``system_message`` / ``prompt_suffix``) are applied to
    each agentless prompt so that SEARCH/REPLACE recovery mode can be reused
    from the main harness recovery loop.
    """
    instance_id = instance["instance_id"]
    base_commit = instance.get("base_commit", "")
    if metadata is None:
        metadata = {}
    logger.info("Running agentless patch generation for %s", instance_id)

    def _attempt(
        prompt: str,
        suffix: str,
        allow_retry: bool,
    ) -> tuple[str, bool, set[str]]:
        if system_message or prompt_suffix:
            prompt = build_recovery_prompt(
                prompt,
                system_message or "",
                prompt_suffix or "",
            )
        (log_dir / f"{name}.agentless{suffix}.prompt.txt").write_text(
            prompt, encoding="utf-8"
        )
        logger.info(
            "Agentless prompt length for %s%s: %s chars",
            instance_id,
            suffix,
            len(prompt),
        )

        response = call_chat_endpoint(
            patch_config,
            prompt,
            args.patch_timeout,
            logger,
        )
        (log_dir / f"{name}.agentless{suffix}.response.md").write_text(
            response, encoding="utf-8"
        )

        # Detect truncated responses early so we do not try to apply garbage.
        if is_truncated_diff(response):
            logger.warning("Agentless response for %s%s looks truncated", instance_id, suffix)
            partial = extract_partial_diff(response)
            if partial:
                response = partial

        applied, missing_files = apply_model_response_with_missing(
            host_repo_dir, response, logger
        )
        if not applied or missing_files:
            logger.warning(
                "Agentless response for %s%s could not be fully applied "
                "(applied=%s, missing_files=%s); rejecting patch",
                instance_id,
                suffix,
                applied,
                sorted(missing_files),
            )
            _reset_repo(host_repo_dir, base_commit, logger)
            return "", False, missing_files

        test_patch_paths = paths_from_patch(instance.get("test_patch", "") or "")
        patch = capture_patch_on_host(
            host_repo_dir,
            logger,
            base_commit=base_commit,
            test_patch_paths=test_patch_paths,
        )
        if not patch.strip():
            logger.warning(
                "Agentless path produced no patch for %s%s after successful apply",
                instance_id,
                suffix,
            )
            if allow_retry and not verify_edits_apply(host_repo_dir, response, logger):
                logger.warning(
                    "Agentless response for %s%s contains unapplyable edits; retrying",
                    instance_id,
                    suffix,
                )
            return "", False, missing_files

        search_text = instance.get("problem_statement", "") or ""
        requirements = instance.get("requirements", "") or ""
        if requirements:
            search_text += "\n" + requirements
        tests = load_list_field(instance.get("selected_test_files_to_run", []))
        fail_to_pass = load_list_field(instance.get("fail_to_pass", []))
        relevant = rank_files_by_relevance(
            host_repo_dir,
            search_text,
            test_names=tests + fail_to_pass,
            top_k=20,
        )
        # Keep any new files the model created (e.g., test fixtures) and any
        # paths touched by the official fix patch, rather than filtering them
        # out because they were not in the ranked set.
        official_fix_paths = set(_changed_files_from_patch(instance.get("patch", "") or "")) | set(
            _new_files_from_patch(instance.get("patch", "") or "")
        )
        extra_allowed = set(_new_files_from_patch(instance.get("test_patch", "") or ""))
        patch = filter_patch_to_source_files(
            patch,
            extra_allowed=extra_allowed,
            official_fix_paths=official_fix_paths,
            test_patch_paths=test_patch_paths,
        )
        if not patch.strip():
            logger.warning(
                "Agentless patch for %s%s was filtered to empty",
                instance_id,
                suffix,
            )
            _reset_repo(host_repo_dir, base_commit, logger)
            return "", False, missing_files

        if not _verify_patch_applies(host_repo_dir, patch, base_commit, logger):
            logger.warning(
                "Agentless patch for %s%s does not apply cleanly on base commit; rejecting",
                instance_id,
                suffix,
            )
            _reset_repo(host_repo_dir, base_commit, logger)
            return "", False, missing_files

        logger.info(
            "Agentless path produced a non-empty patch for %s%s",
            instance_id,
            suffix,
        )
        return patch, True, set()


    context_window = patch_config.get("agent", {}).get("context_window", 32768)
    expand_to_package = _agentless_needs_package_expansion(
        instance, host_repo_dir, top_k=5, context_window=context_window
    )
    agentless_prompt = build_agentless_prompt(
        instance,
        host_repo_dir,
        few_shot_examples=few_shot_examples,
        top_k=5,
        context_window=context_window,
        expand_to_package=expand_to_package,
    )

    tests = load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = load_list_field(instance.get("fail_to_pass", []))
    search_text = instance.get("problem_statement", "") or ""
    requirements_text = instance.get("requirements", "") or ""
    if requirements_text:
        search_text = f"{search_text}\n{requirements_text}".strip()
    ranked_files = rank_files_by_relevance(
        host_repo_dir,
        search_text,
        test_names=tests + fail_to_pass,
        top_k=20,
    )

    # ------------------------------------------------------------------
    # Small-model fast path: skip SEARCH/REPLACE entirely and ask the
    # model for a unified diff with the failing test patch applied.  This
    # matches the productive signal seen in swarm_diff_recovery/.
    # ------------------------------------------------------------------
    if getattr(args, "small_model_diff_fallback", False):
        logger.info(
            "Small-model diff-fallback-first path for %s (ranked_files=%s)",
            instance_id,
            ranked_files,
        )
        metadata["recovery_attempts"] = metadata.get("recovery_attempts", 0) + 1
        metadata["diff_recovery_fired"] = True
        if not _reset_repo(host_repo_dir, base_commit, logger):
            return ""
        if not _apply_test_patch(
            host_repo_dir,
            instance.get("test_patch", "") or "",
            logger,
        ):
            logger.error(
                "Failed to apply test_patch for small-model diff fallback %s",
                instance_id,
            )
            return ""

        fallback_patch = run_diff_fallback(
            host_repo_dir,
            instance,
            agentless_prompt,
            ranked_files,
            patch_config,
            args,
            log_dir,
            logger,
            name,
        )
        if fallback_patch.strip():
            language = (instance.get("repo_language") or "").lower()
            if _check_patch_builds(host_repo_dir, fallback_patch, language, logger, metadata):
                logger.info(
                    "Small-model diff fallback produced a non-empty patch for %s",
                    instance_id,
                )
                metadata["recovery_succeeded"] = True
                return fallback_patch
            logger.warning(
                "Small-model diff fallback patch for %s failed compile gate; treating as empty",
                instance_id,
            )
            metadata["compile_gate_rejected"] = True
        return ""

    patch, ok, _ = _attempt(agentless_prompt, "", allow_retry=True)
    if ok:
        return patch

    # Retry once with exact file contents and stricter instructions.
    if not _reset_repo(host_repo_dir, base_commit, logger):
        return ""
    retry_prompt = build_agentless_retry_prompt(
        instance,
        host_repo_dir,
        "",
        few_shot_examples=few_shot_examples,
        top_k=5,
        context_window=context_window,
        expand_to_package=expand_to_package,
    )
    patch, ok, _ = _attempt(retry_prompt, ".retry", allow_retry=False)
    if patch.strip():
        return patch

    # Final one-shot unified-diff fallback when SEARCH/REPLACE edits fail.
    if args.diff_fallback:
        metadata["recovery_attempts"] = metadata.get("recovery_attempts", 0) + 1
        metadata["diff_recovery_fired"] = True
        logger.info(
            "RECOVERY_ESCALATION instance=%s agentless_diff_fallback=%s",
            instance_id,
            True,
        )
        if not _reset_repo(host_repo_dir, base_commit, logger):
            return ""
        if not _apply_test_patch(
            host_repo_dir,
            instance.get("test_patch", "") or "",
            logger,
        ):
            logger.error(
                "Failed to apply test_patch for agentless diff fallback %s",
                instance_id,
            )
            return ""

        logger.info(
            "Ranked files for agentless diff fallback for %s: %s",
            instance_id,
            ranked_files,
        )

        fallback_patch = run_diff_fallback(
            host_repo_dir,
            instance,
            agentless_prompt,
            ranked_files,
            patch_config,
            args,
            log_dir,
            logger,
            name,
        )
        if fallback_patch.strip():
            language = (instance.get("repo_language") or "").lower()
            if _check_patch_builds(host_repo_dir, fallback_patch, language, logger, metadata):
                logger.info(
                    "Agentless diff fallback produced a non-empty patch for %s",
                    instance_id,
                )
                metadata["recovery_succeeded"] = True
                return fallback_patch
            logger.warning(
                "Agentless diff fallback patch for %s failed compile gate; treating as empty",
                instance_id,
            )
            metadata["compile_gate_rejected"] = True

    return ""


def run_plan_then_patch(
    host_repo_dir: Path,
    instance: dict[str, Any],
    plan_config: dict[str, Any],
    patch_config: dict[str, Any],
    args: argparse.Namespace,
    log_dir: Path,
    logger: logging.Logger,
    name: str,
) -> str:
    """Run the dual-model plan-then-patch flow and return the captured git diff."""
    instance_id = instance["instance_id"]

    # ---- Plan step ----
    plan_prompt = build_plan_prompt(instance)
    (log_dir / f"{name}.plan.prompt.txt").write_text(plan_prompt, encoding="utf-8")
    logger.info("Running plan step for %s", instance_id)
    plan_response = call_chat_endpoint(
        plan_config,
        plan_prompt,
        args.plan_timeout,
        logger,
        max_tokens=args.plan_max_tokens,
        temperature=args.plan_temperature,
    )
    (log_dir / f"{name}.plan.response.md").write_text(plan_response, encoding="utf-8")

    plan_files, plan_fix = parse_plan(plan_response)
    logger.info(
        "Plan for %s: files=%s fix=%s",
        instance_id,
        plan_files,
        plan_fix[:200] if plan_fix else "",
    )
    if not plan_files:
        logger.warning("Plan step returned no files for %s; falling back to empty patch", instance_id)

    # ---- Patch step ----
    excerpts = read_file_excerpts(host_repo_dir, plan_files, max_lines=300)
    patch_prompt = build_patch_prompt(instance, plan_files, plan_fix, excerpts)
    (log_dir / f"{name}.patch.prompt.txt").write_text(patch_prompt, encoding="utf-8")
    logger.info("Running patch step for %s", instance_id)
    patch_response = call_chat_endpoint(
        patch_config,
        patch_prompt,
        args.patch_timeout,
        logger,
    )
    (log_dir / f"{name}.patch.response.md").write_text(patch_response, encoding="utf-8")

    applied, missing_files = apply_model_response_with_missing(
        host_repo_dir, patch_response, logger
    )
    if not applied or missing_files:
        logger.warning(
            "Plan-then-patch response for %s could not be fully applied "
            "(applied=%s, missing_files=%s); rejecting patch",
            instance_id,
            applied,
            sorted(missing_files),
        )
        _reset_repo(host_repo_dir, instance.get("base_commit", ""), logger)
        return ""

    patch = capture_patch_on_host(
        host_repo_dir,
        logger,
        base_commit=instance.get("base_commit"),
        test_patch_paths=paths_from_patch(instance.get("test_patch", "") or ""),
    )
    if patch.strip() and not _verify_patch_applies(
        host_repo_dir, patch, instance.get("base_commit"), logger
    ):
        logger.warning(
            "Plan-then-patch patch for %s does not apply cleanly on base commit; rejecting",
            instance_id,
        )
        _reset_repo(host_repo_dir, instance.get("base_commit", ""), logger)
        return ""

    return patch


def run_cmd(
    cmd: list[str],
    *,
    input_text: str | None = None,
    timeout: int | None = None,
    check: bool = False,
    cwd: str | Path | None = None,
    logger: logging.Logger | None = None,
) -> subprocess.CompletedProcess:
    """Run a command and return a CompletedProcess object."""
    if logger:
        logger.debug("Running: %s", " ".join(shlex.quote(str(c)) for c in cmd))
    try:
        return subprocess.run(
            cmd,
            input=input_text,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=check,
            cwd=str(cwd) if cwd is not None else None,
            errors="replace",
        )
    except subprocess.TimeoutExpired as exc:
        if logger:
            logger.error("Command timed out after %ss: %s", timeout, cmd)
        raise


def podman(
    *args: str,
    input_text: str | None = None,
    timeout: int | None = None,
    check: bool = False,
    logger: logging.Logger | None = None,
) -> subprocess.CompletedProcess:
    return run_cmd(
        ["podman", *_PODMAN_GLOBAL_OPTS, *args],
        input_text=input_text,
        timeout=timeout,
        check=check,
        logger=logger,
    )


def container_name(instance_id: str, suffix: str = "") -> str:
    """Return a short, filesystem-safe container name.

    The full instance_id can be ~120 chars, and doubling it in the name plus a
    profile suffix can exceed 255-byte filename limits. Use a SHA-256 hash for
    uniqueness and append a sanitized suffix for human readability.
    """
    base = f"{instance_id}-{suffix}" if suffix else instance_id
    h = hashlib.sha256(base.encode("utf-8")).hexdigest()[:10]
    name = f"selfware-sweap-{h}"
    if suffix:
        safe_suffix = re.sub(r"[^a-zA-Z0-9_.-]+", "_", suffix).strip("_")[:30]
        if safe_suffix:
            name = f"{name}-{safe_suffix}"
    return name


def image_exists_locally(image: str, logger: logging.Logger) -> bool:
    """Return True if the image is already present in the local Podman store."""
    proc = podman("image", "exists", image, logger=logger)
    return proc.returncode == 0


def pull_image(image: str, logger: logging.Logger, timeout: int = 600) -> bool:
    if image_exists_locally(image, logger):
        logger.info("Image %s already exists locally; skipping pull", image)
        return True
    logger.info("Pulling image %s", image)
    proc = podman("pull", image, timeout=timeout, logger=logger)
    if proc.returncode != 0:
        logger.error("Failed to pull image %s: %s", image, proc.stderr.strip())
        return False
    logger.info("Pulled image %s", image)
    return True


def _is_retryable_podman_error(stderr: str) -> bool:
    """Return True when a Podman stderr looks like a transient storage race."""
    retryable = (
        "container state improper",
        "no such container",
        "directory not empty",
        "failed to start container",
        "removing mount point",
        "resource temporarily unavailable",
        "device or resource busy",
    )
    lowered = stderr.lower()
    return any(needle in lowered for needle in retryable)


def start_container(
    image: str,
    name: str,
    logger: logging.Logger,
    timeout: int = 60,
    max_retries: int = 2,
) -> bool:
    logger.info("Starting container %s", name)
    for attempt in range(max_retries + 1):
        # Clean up any stale container with the same name before starting.
        stop_and_remove_container(name, logger)
        # Keep the container alive with a sleep loop so we can exec into it.
        proc = podman(
            "run",
            "-d",
            "--replace",
            "--name",
            name,
            "--entrypoint",
            "",
            image,
            "bash",
            "-c",
            "trap 'exit 0' TERM; while true; do sleep 1; done",
            timeout=timeout,
            logger=logger,
        )
        if proc.returncode == 0:
            # Wait until the container is running.
            for _ in range(timeout):
                info = podman("inspect", name, "--format", "{{.State.Status}}", logger=logger)
                if info.stdout.strip() == "running":
                    logger.info("Container %s is running", name)
                    return True
                time.sleep(1)
            logger.error("Container %s did not reach running state in time", name)
        else:
            logger.error("Failed to start container %s: %s", name, proc.stderr.strip())
            if attempt < max_retries and _is_retryable_podman_error(proc.stderr):
                logger.warning("Retrying container %s startup after %s s", name, 2 ** attempt)
                time.sleep(2 ** attempt)
                continue
        return False
    return False


def stop_and_remove_container(name: str, logger: logging.Logger) -> None:
    logger.info("Stopping and removing container %s", name)
    podman("stop", "-t", "5", name, logger=logger, check=False)
    podman("rm", "-f", name, logger=logger, check=False)


def copy_into_container(
    src: Path,
    dst: str,
    container: str,
    logger: logging.Logger,
) -> bool:
    proc = podman("cp", str(src), f"{container}:{dst}", logger=logger)
    if proc.returncode != 0:
        logger.error(
            "Failed to copy %s into %s:%s: %s",
            src,
            container,
            dst,
            proc.stderr.strip(),
        )
        return False
    return True


def reset_repo_to_base(
    container: str,
    base_commit: str,
    repo_dir: str,
    logger: logging.Logger,
) -> bool:
    proc = podman(
        "exec",
        container,
        "git",
        "-C",
        repo_dir,
        "reset",
        "--hard",
        base_commit,
        logger=logger,
    )
    if proc.returncode != 0:
        logger.error(
            "Failed to reset repo to %s: %s",
            base_commit,
            proc.stderr.strip(),
        )
        return False
    return True


def run_selfware_in_container(
    container: str,
    repo_dir: str,
    prompt_text: str,
    timeout: int,
    log_dir: Path,
    logger: logging.Logger,
) -> bool:
    logger.info("Running selfware in container %s", container)
    proc = podman(
        "exec",
        "-i",
        "-e",
        "SELFWARE_API_KEY",
        "-w",
        repo_dir,
        container,
        CONTAINER_SELFWARE_BIN,
        "--config",
        CONTAINER_CONFIG_PATH,
        "-p",
        "-",
        "-y",
        "--no-tui",
        input_text=prompt_text,
        timeout=timeout,
        logger=logger,
    )
    # Save stdout/stderr for debugging.
    log_dir.mkdir(parents=True, exist_ok=True)
    (log_dir / f"{container}.selfware.stdout.log").write_text(proc.stdout, errors="replace")
    (log_dir / f"{container}.selfware.stderr.log").write_text(proc.stderr, errors="replace")

    if proc.returncode != 0:
        logger.error(
            "selfware exited with code %s for %s; see %s.selfware.stderr.log",
            proc.returncode,
            container,
            container,
        )
        return False
    logger.info("selfware completed in container %s", container)
    return True


def capture_patch(
    container: str,
    repo_dir: str,
    logger: logging.Logger,
) -> str:
    """Capture the git diff (including new files) inside the container."""
    # Stage all changes so that new files appear in the diff.
    podman(
        "exec",
        container,
        "git",
        "-C",
        repo_dir,
        "add",
        "-A",
        logger=logger,
    )
    proc = podman(
        "exec",
        container,
        "git",
        "-C",
        repo_dir,
        "diff",
        "--cached",
        "--no-color",
        logger=logger,
    )
    if proc.returncode != 0:
        logger.error("Failed to capture patch: %s", proc.stderr.strip())
        return ""
    return clean_captured_diff(proc.stdout)


def _read_predictions_jsonl(path: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    if not path.exists():
        return records
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


def save_prediction(
    output_dir: Path,
    instance_id: str,
    patch: str,
    logger: logging.Logger,
    metadata: dict[str, Any] | None = None,
    *,
    error_code: str | None = None,
    provider: str | None = None,
    model_profile: str | None = None,
) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    predictions_path = output_dir / "predictions.jsonl"
    record: dict[str, Any] = {"instance_id": instance_id, "patch": patch}
    if metadata:
        record["metadata"] = dict(metadata)
    if error_code is not None or provider is not None or model_profile is not None:
        record.setdefault("metadata", {})
        if error_code is not None:
            record["metadata"]["error_code"] = error_code
        if provider is not None:
            record["metadata"]["provider"] = provider
        if model_profile is not None:
            record["metadata"]["model_profile"] = model_profile
    with PREDICTIONS_LOCK:
        records = _read_predictions_jsonl(predictions_path)
        # Keep the latest record per instance_id so recovery attempts overwrite
        # earlier failures rather than producing duplicate predictions.
        seen = False
        for i in range(len(records) - 1, -1, -1):
            if records[i].get("instance_id") == instance_id:
                if not seen:
                    records[i] = record
                    seen = True
                else:
                    records.pop(i)
        if not seen:
            records.append(record)
        with open(predictions_path, "w", encoding="utf-8") as f:
            for rec in records:
                f.write(json.dumps(rec, ensure_ascii=False) + "\n")
    logger.info("Saved prediction for %s to %s", instance_id, predictions_path)


def write_predictions_json(output_dir: Path, logger: logging.Logger) -> None:
    """Convert predictions.jsonl to the JSON array format expected by the evaluator."""
    jsonl_path = output_dir / "predictions.jsonl"
    json_path = output_dir / "predictions.json"
    if not jsonl_path.exists():
        return
    records = _read_predictions_jsonl(jsonl_path)
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(records, f, ensure_ascii=False, indent=2)
    logger.info("Wrote evaluator predictions to %s", json_path)


def load_existing_predictions(output_dir: Path) -> set[str]:
    predictions_path = output_dir / "predictions.jsonl"
    if not predictions_path.exists():
        return set()
    done: set[str] = set()
    with open(predictions_path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
                # Empty predictions are not real results; resume should retry them.
                if rec.get("patch", "").strip():
                    done.add(rec["instance_id"])
            except (json.JSONDecodeError, KeyError):
                continue
    return done


def load_sample_file(path: Path, logger: logging.Logger) -> list[dict[str, Any]]:
    rows = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                logger.warning("Skipping malformed sample line: %s", line[:200])
    return rows


def select_instances(
    dataset: Any,
    args: argparse.Namespace,
    existing: set[str],
    logger: logging.Logger,
) -> list[dict[str, Any]]:
    if args.instance_ids:
        wanted = {x.strip() for x in args.instance_ids.split(",")}
        rows = [dict(row) for row in dataset if row["instance_id"] in wanted]
        # Preserve requested order.
        row_by_id = {row["instance_id"]: row for row in rows}
        rows = [row_by_id[iid] for iid in wanted if iid in row_by_id]
    elif args.sample_file:
        rows = load_sample_file(Path(args.sample_file), logger)
    else:
        rows = [dict(row) for row in dataset]

    # Apply the explicit cap if one was provided.  The historical default is
    # a single-instance smoke run, but --sample-file and --instance-ids should
    # run everything they specify unless an explicit cap is given.
    if args.max_tasks is not None:
        rows = rows[: args.max_tasks]
    elif not args.sample_file and not args.instance_ids:
        rows = rows[:1]

    if args.resume:
        rows = [r for r in rows if r["instance_id"] not in existing]

    # Stash the selected sample size so process_instance can decide whether to
    # default fragile models to the agentless path.
    args.sample_size = len(rows)

    logger.info(
        "Selected %s instance(s) to process (sample_file=%s, max_tasks=%s, resume=%s)",
        len(rows),
        args.sample_file,
        args.max_tasks,
        args.resume,
    )
    return rows


def extract_repo_from_image(
    image: str,
    name: str,
    repo_dir: str,
    output_dir: Path,
    logger: logging.Logger,
) -> Path | None:
    """Create a container from the SWE-bench image and copy /app to the host."""
    logger.info("Extracting repo from image %s", image)
    if not start_container(image, name, logger, timeout=120):
        return None

    # Use a short fixed directory name inside the per-run output_dir so the
    # tar extraction path does not hit filesystem name-length limits.
    host_repo_dir = output_dir / "repos" / "repo"
    # Remove any previously-extracted repo state so tar does not leave stale
    # untracked files from earlier instances in this run.
    if host_repo_dir.exists():
        shutil.rmtree(host_repo_dir, ignore_errors=True)
    host_repo_dir.mkdir(parents=True, exist_ok=True)

    try:
        # podman cp can fail with "read/write on closed pipe" on large repos.
        # Use podman export piped to tar instead; it is slower but reliable.
        export_proc = subprocess.Popen(
            ["podman", *_PODMAN_GLOBAL_OPTS, "export", name],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        tar_proc = subprocess.Popen(
            ["tar", "-xf", "-", "-C", str(host_repo_dir), "--strip-components=1", repo_dir.lstrip("/")],
            stdin=export_proc.stdout,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        export_proc.stdout.close()  # allow export to receive SIGPIPE if tar exits
        export_stdout, export_stderr = export_proc.communicate(timeout=300)
        tar_stdout, tar_stderr = tar_proc.communicate(timeout=300)

        if export_proc.returncode != 0 or tar_proc.returncode != 0:
            logger.error(
                "Failed to export repo: export_rc=%s tar_rc=%s stderr=%s %s",
                export_proc.returncode,
                tar_proc.returncode,
                export_stderr.decode("utf-8", errors="replace").strip(),
                tar_stderr.decode("utf-8", errors="replace").strip(),
            )
            return None

        git_dir = host_repo_dir / ".git"
        if not git_dir.exists():
            logger.error("No .git directory found after extraction; aborting")
            return None

        logger.info("Extracted repo to %s", host_repo_dir)
        return host_repo_dir
    finally:
        stop_and_remove_container(name, logger)


def _stream_to_file(proc: subprocess.Popen, stream, path: Path) -> None:
    """Read lines from a stream and append to a file until the stream closes."""
    with open(path, "wb") as f:
        for line in iter(stream.readline, b""):
            f.write(line)
            f.flush()
    stream.close()


def run_selfware_on_host(
    instance_id: str,
    repo_dir: Path,
    config_path: Path,
    prompt_text: str,
    timeout: int,
    binary: Path,
    log_dir: Path,
    output_dir: Path,
    logger: logging.Logger,
    few_shot_examples: str | None = None,
    post_edit_test_command: str | None = None,
    metadata: dict[str, Any] | None = None,
) -> bool:
    """Run selfware directly on the host against the extracted repo."""
    logger.info("Running selfware on host repo %s", repo_dir)

    # Write the prompt inside the repo so selfware's safety check allows reading
    # it, then delete it before capturing the patch. Exclude it from git so the
    # diff only contains the real fix.
    prompt_path = repo_dir / ".selfware_prompt.txt"
    git_info = repo_dir / ".git" / "info"
    git_info.mkdir(parents=True, exist_ok=True)
    exclude_path = git_info / "exclude"

    exclude_entries = [".selfware_prompt.txt\n"]
    few_shot_path: Path | None = None
    if few_shot_examples is not None:
        few_shot_path = repo_dir / ".selfware_few_shot_examples.txt"
        exclude_entries.append(".selfware_few_shot_examples.txt\n")
        few_shot_path.write_text(few_shot_examples, encoding="utf-8")

    existing_exclude = exclude_path.read_text(encoding="utf-8") if exclude_path.exists() else ""
    for line in exclude_entries:
        if line not in existing_exclude:
            with open(exclude_path, "a", encoding="utf-8") as f:
                f.write(line)

    prompt_path.write_text(prompt_text, encoding="utf-8")

    # Also keep copies in the log dir for debugging.
    (log_dir / f"{instance_id}.prompt.txt").write_text(prompt_text, encoding="utf-8")
    if few_shot_path is not None:
        (log_dir / f"{instance_id}.few_shot_examples.txt").write_text(
            few_shot_examples, encoding="utf-8"
        )

    # Isolate per-instance HOME/XDG directories so selfware's global episodic
    # memory (loaded from dirs::data_local_dir()) does not leak between runs.
    agent_data_dir = output_dir / "agent_data" / instance_id
    agent_data_dir.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env.setdefault("RUST_LOG", "warn")
    env["HOME"] = str(agent_data_dir)
    env["XDG_DATA_HOME"] = str(agent_data_dir / ".local" / "share")
    env["XDG_CONFIG_HOME"] = str(agent_data_dir / ".config")
    env["XDG_CACHE_HOME"] = str(agent_data_dir / ".cache")
    if post_edit_test_command is not None:
        env["SELFWARE_POST_EDIT_TEST_COMMAND"] = post_edit_test_command

    cmd = [
        str(binary),
        "--config",
        str(config_path),
        "-p",
        str(prompt_path),
        "-y",
        "--no-tui",
    ]

    stdout_path = log_dir / f"{instance_id}.selfware.stdout.log"
    stderr_path = log_dir / f"{instance_id}.selfware.stderr.log"

    proc = subprocess.Popen(
        cmd,
        cwd=repo_dir,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    stdout_thread = threading.Thread(
        target=_stream_to_file, args=(proc, proc.stdout, stdout_path), daemon=True
    )
    stderr_thread = threading.Thread(
        target=_stream_to_file, args=(proc, proc.stderr, stderr_path), daemon=True
    )
    stdout_thread.start()
    stderr_thread.start()

    try:
        returncode = proc.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        logger.error("selfware timed out after %ss", timeout)
        proc.kill()
        proc.wait()
        prompt_path.unlink(missing_ok=True)
        if few_shot_path is not None:
            few_shot_path.unlink(missing_ok=True)
        if metadata is not None:
            metadata["error_code"] = "timeout"
        return False

    stdout_thread.join(timeout=5)
    stderr_thread.join(timeout=5)
    prompt_path.unlink(missing_ok=True)
    if few_shot_path is not None:
        few_shot_path.unlink(missing_ok=True)

    if returncode != 0:
        logger.error(
            "selfware exited with code %s for %s; see %s.selfware.stderr.log",
            returncode,
            repo_dir.name,
            repo_dir.name,
        )
        if metadata is not None:
            stderr_path = log_dir / f"{instance_id}.selfware.stderr.log"
            failure_class = classify_failure(stderr_path)
            if failure_class == MAX_ITERATIONS:
                metadata["error_code"] = "max_iterations"
            elif failure_class == JSON_PARSE_ERROR:
                metadata["error_code"] = "json_parse_error"
        return False

    logger.info("selfware completed on host repo %s", repo_dir.name)
    return True


def _verify_patch_applies(repo_dir: Path, patch: str, base_commit: str | None, logger: logging.Logger) -> bool:
    """Check whether a captured patch applies cleanly on a reset base commit.

    Returns True if the patch can be applied, False otherwise.  The repo is
    restored to its current state afterwards.
    """
    if not patch.strip():
        return False
    commit = base_commit or "HEAD"
    # Write the patch outside the repo so it survives "git stash -u".
    patch_file = Path(tempfile.gettempdir()) / f"selfware_verify_{repo_dir.name}_{os.getpid()}.diff"
    patch_file.write_text(patch, encoding="utf-8")
    try:
        # Reset to base, try apply, then restore.
        run_cmd(["git", "-C", str(repo_dir), "stash", "push", "-u", "-m", "verify"], logger=logger)
        reset_proc = run_cmd(["git", "-C", str(repo_dir), "reset", "--hard", commit], logger=logger)
        if reset_proc.returncode != 0:
            logger.warning("Failed to reset for patch verification: %s", reset_proc.stderr.strip())
            run_cmd(["git", "-C", str(repo_dir), "stash", "pop"], logger=logger)
            return False
        check = run_cmd(["git", "-C", str(repo_dir), "apply", "--check", str(patch_file)], logger=logger)
        applies = check.returncode == 0
        if not applies:
            logger.warning("Captured patch does not apply cleanly on base commit: %s", check.stderr.strip())
        run_cmd(["git", "-C", str(repo_dir), "reset", "--hard", commit], logger=logger)
        stash_proc = run_cmd(["git", "-C", str(repo_dir), "stash", "pop"], logger=logger)
        if stash_proc.returncode != 0:
            logger.warning("Failed to restore repo after patch verification: %s", stash_proc.stderr.strip())
        return applies
    finally:
        patch_file.unlink(missing_ok=True)


def _changed_files_from_patch(patch: str) -> list[str]:
    """Return repo-relative paths introduced or modified by a git diff."""
    files: list[str] = []
    for line in patch.splitlines():
        if line.startswith("diff --git a/"):
            match = re.match(r"^diff --git a/(.+?) b/(.+?)(?:\s|$)", line)
            if match:
                files.append(match.group(2))
    return files


def _language_toolchain(language: str) -> str | None:
    """Return the host binary required to compile/type-check a repo language."""
    mapping = {
        "go": "go",
        "rust": "cargo",
        "typescript": "npx",
        "javascript": "node",
    }
    return mapping.get((language or "").lower())


def _check_host_toolchains(
    instances: list[dict[str, Any]],
    logger: logging.Logger,
) -> bool:
    """Exit early if the sample requires a host toolchain that is not installed."""
    languages = sorted({inst.get("repo_language", "") for inst in instances if inst.get("repo_language")})
    missing: list[tuple[str, str]] = []
    for lang in languages:
        binary = _language_toolchain(lang)
        if binary and shutil.which(binary) is None:
            missing.append((lang, binary))
    if missing:
        for lang, binary in missing:
            logger.error(
                "Host toolchain missing for repo_language=%s: %s not found in PATH",
                lang,
                binary,
            )
        logger.error(
            "Aborting because %s required toolchain(s) are missing. "
            "Install them or set SELFWARE_BYPASS_COMPILE_GATE=1 to skip the compile gate.",
            len(missing),
        )
        return False
    return True


def _check_patch_builds(
    repo_dir: Path,
    patch: str,
    language: str,
    logger: logging.Logger,
    metadata: dict[str, Any] | None = None,
) -> bool:
    """Run a lightweight pre-submission compile/type-check gate.

    The patch is assumed to have already been applied.  A failing gate now
    rejects the patch (returns ``False``) so broken predictions are not
    submitted to the evaluator.  Set ``SELFWARE_BYPASS_COMPILE_GATE=1`` to
    keep the old advisory behaviour for host environments with missing
    dependencies.
    """
    if not patch.strip():
        return True

    if os.environ.get("SELFWARE_BYPASS_COMPILE_GATE"):
        logger.info("Compile gate bypassed via SELFWARE_BYPASS_COMPILE_GATE")
        return True

    language = (language or "").lower()
    logger.info("Running build/type-check gate for language=%s", language)

    def _reject_missing_tool(binary: str, message: str) -> bool:
        logger.warning(message)
        if metadata is not None:
            metadata["compile_gate_skip_reason"] = "missing_toolchain"
            metadata["compile_gate_missing_tool"] = binary
        return False

    if language == "go":
        if shutil.which("go") is None:
            return _reject_missing_tool(
                "go",
                "Build gate rejecting patch: go binary not available on host",
            )
        proc = run_cmd(
            ["go", "build", "./..."],
            cwd=repo_dir,
            timeout=180,
            logger=logger,
        )
        if proc.returncode != 0:
            logger.warning(
                "Build gate failed (go build): %s. Rejecting patch.",
                proc.stderr.strip()[:500],
            )
            return False
        return True

    if language == "python":
        files = _changed_files_from_patch(patch)
        py_files = [f for f in files if f.endswith(".py")]
        if not py_files:
            return True
        proc = run_cmd(
            [sys.executable, "-m", "py_compile", *py_files],
            cwd=repo_dir,
            timeout=120,
            logger=logger,
        )
        if proc.returncode != 0:
            logger.warning(
                "Build gate failed (py_compile): %s. Rejecting patch.",
                proc.stderr.strip()[:500],
            )
            return False
        return True

    if language == "rust":
        if shutil.which("cargo") is None:
            return _reject_missing_tool(
                "cargo",
                "Build gate rejecting patch: cargo binary not available on host",
            )
        proc = run_cmd(
            ["cargo", "check"],
            cwd=repo_dir,
            timeout=300,
            logger=logger,
        )
        if proc.returncode != 0:
            logger.warning(
                "Build gate failed (cargo check): %s. Rejecting patch.",
                proc.stderr.strip()[:500],
            )
            return False
        return True

    if language in ("javascript", "typescript"):
        files = _changed_files_from_patch(patch)
        tsconfig = repo_dir / "tsconfig.json"
        ts_files = [f for f in files if f.endswith((".ts", ".tsx"))]
        should_run_tsc = tsconfig.is_file() and (
            language == "typescript" or bool(ts_files)
        )
        if should_run_tsc:
            if shutil.which("npx") is None:
                return _reject_missing_tool(
                    "npx",
                    "Build gate rejecting patch: npx not available on host",
                )
            proc = run_cmd(
                ["npx", "tsc", "--noEmit"],
                cwd=repo_dir,
                timeout=180,
                logger=logger,
            )
            if proc.returncode != 0:
                logger.warning(
                    "Build gate failed (tsc --noEmit): %s. Rejecting patch.",
                    proc.stderr.strip()[:500],
                )
                return False

        # JavaScript gate (also used for TypeScript without a tsconfig).
        js_files = [f for f in files if f.endswith(".js")]
        if not js_files:
            return True
        node_bin = shutil.which("node")
        if node_bin is None:
            return _reject_missing_tool(
                "node",
                "Build gate rejecting patch: node binary not available on host",
            )
        for js_file in js_files:
            proc = run_cmd(
                [node_bin, "--check", js_file],
                cwd=repo_dir,
                timeout=60,
                logger=logger,
            )
            if proc.returncode != 0:
                logger.warning(
                    "Build gate failed (node --check %s): %s. Rejecting patch.",
                    js_file,
                    proc.stderr.strip()[:500],
                )
                return False
        return True

    # No gate for other languages.
    return True


def capture_patch_on_host(
    repo_dir: Path,
    logger: logging.Logger,
    base_commit: str | None = None,
    test_patch_paths: set[str] | None = None,
) -> str:
    """Capture the git diff on the host repo.

    Compares against the instance's base commit.  We stage modifications and
    new files so that created files (e.g., test fixtures) are included in the
    captured patch.  Build artifacts are limited because the repo is reset and
    cleaned before each instance run, and ``clean_captured_diff`` strips harness
    files and excluded prefixes.  Any path in ``test_patch_paths`` (the official
    benchmark test patch) is excluded from the captured diff.
    """
    excluded = test_patch_paths or set()
    if base_commit:
        # Stage tracked modifications and any new untracked files so the diff
        # against base_commit includes both edits and new-file creations.
        add_proc = run_cmd(["git", "-C", str(repo_dir), "add", "-A"], logger=logger)
        if add_proc.returncode != 0:
            logger.warning("git add -A failed before capture: %s", add_proc.stderr.strip())
        proc = run_cmd(
            ["git", "-C", str(repo_dir), "diff", "--cached", base_commit, "--no-color"],
            logger=logger,
        )
        if proc.returncode == 0:
            return filter_patch_excluding_paths(clean_captured_diff(proc.stdout), excluded)
        logger.warning(
            "git diff --cached against %s failed, falling back to unstaged diff: %s",
            base_commit,
            proc.stderr.strip(),
        )

    run_cmd(["git", "-C", str(repo_dir), "add", "-A"], logger=logger)
    proc = run_cmd(
        ["git", "-C", str(repo_dir), "diff", "--cached", "--no-color"],
        logger=logger,
    )
    if proc.returncode != 0:
        logger.error("Failed to capture patch: %s", proc.stderr.strip())
        return ""
    return filter_patch_excluding_paths(clean_captured_diff(proc.stdout), excluded)


def _run_tdr_block(
    host_repo_dir: Path,
    instance: dict[str, Any],
    patch: str,
    args: argparse.Namespace,
    log_dir: Path,
    logger: logging.Logger,
    output_dir: Path,
    instance_id: str,
    metadata: dict[str, Any] | None = None,
) -> tuple[str, bool]:
    """Run optional ensemble seed generation and/or TDR and return the best patch + success flag."""
    from tdr import run_test_driven_repair, run_ensemble_seed_generation

    if args.tdr and not getattr(args, "repair_config", None):
        logger.warning("TDR requested but no repair config loaded; skipping")
        return patch, bool(patch.strip())

    image = f"docker.io/jefzda/sweap-images:{instance['dockerhub_tag']}"
    run_suffix = f"{output_dir.parent.parent.name}-{output_dir.parent.name}-{output_dir.name}"
    name = container_name(instance_id, run_suffix)
    tdr_name = f"{name}-tdr"

    tests = load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = load_list_field(instance.get("fail_to_pass", []))
    search_text = instance.get("problem_statement", "") or ""
    requirements_text = instance.get("requirements", "") or ""
    if requirements_text:
        search_text = f"{search_text}\n{requirements_text}".strip()
    ranked_files = rank_files_by_relevance(
        host_repo_dir,
        search_text,
        test_names=tests + fail_to_pass,
        top_k=5,
    )
    logger.info("Ranked files for TDR: %s", ranked_files)

    if args.ensemble_models:
        logger.info("Running ensemble seed generation for %s", instance_id)
        ensemble_patch = run_ensemble_seed_generation(
            host_repo_dir,
            instance,
            args.ensemble_models,
            args,
            log_dir,
            logger,
            ranked_files=ranked_files,
        )
        if ensemble_patch.strip():
            patch = ensemble_patch
            save_prediction(output_dir, instance_id, patch, logger, metadata)
            logger.info("Ensemble selected best seed for %s", instance_id)

    if not args.tdr:
        return patch, bool(patch.strip())

    patch = run_test_driven_repair(
        host_repo_dir,
        tdr_name,
        image,
        instance,
        patch,
        args.repair_config,
        args,
        log_dir,
        logger,
        ranked_files=ranked_files,
    )
    save_prediction(output_dir, instance_id, patch, logger, metadata)
    return patch, bool(patch.strip())


def process_instance(
    instance: dict[str, Any],
    args: argparse.Namespace,
    config_path: Path,
    logger: logging.Logger,
) -> bool:
    instance_id = instance["instance_id"]
    image = f"docker.io/jefzda/sweap-images:{instance['dockerhub_tag']}"
    output_dir = Path(args.output_dir).resolve()
    # Use enough of the output path to make the run suffix unique across
    # concurrent matrix runners while keeping the container name short.
    # output_dir is typically <base>/<profile>/<instance_id>.
    run_suffix = f"{output_dir.parent.parent.name}-{output_dir.parent.name}-{output_dir.name}"
    name = container_name(instance_id, run_suffix)
    tdr_name = f"{name}-tdr"
    log_dir = output_dir
    binary_path = Path(args.binary)
    provider: str | None = None
    model_profile: str | None = args.model_profile
    logger.info("=" * 60)
    logger.info("Processing %s", instance_id)
    logger.info("Image: %s", image)

    try:
        if not pull_image(image, logger):
            return False

        host_repo_dir = extract_repo_from_image(
            image, name, args.repo_dir, output_dir, logger
        )
        if host_repo_dir is None:
            return False

        # Reset repo to base commit before running the agent, and remove any
        # leftover untracked files from previous extraction retries.
        reset_proc = run_cmd(
            ["git", "-C", str(host_repo_dir), "reset", "--hard", instance["base_commit"]],
            logger=logger,
        )
        if reset_proc.returncode != 0:
            logger.error(
                "Failed to reset repo to %s: %s",
                instance["base_commit"],
                reset_proc.stderr.strip(),
            )
            return False
        clean_proc = run_cmd(
            ["git", "-C", str(host_repo_dir), "clean", "-fd"],
            logger=logger,
        )
        if clean_proc.returncode != 0:
            logger.warning(
                "Failed to clean untracked files in repo: %s",
                clean_proc.stderr.strip(),
            )

        if not _apply_test_patch(
            host_repo_dir,
            instance.get("test_patch", "") or "",
            logger,
        ):
            logger.error("Failed to apply test_patch; aborting instance %s", instance_id)
            return False

        language = instance.get("repo_language", "")
        repo = instance.get("repo", "")
        selected_tests = (
            instance.get("selected_test_files_to_run", [])
            or instance.get("fail_to_pass", [])
            or []
        )
        test_cmd = _format_test_command(language, selected_tests, repo=repo)

        # Allow absolute /app/... paths in selfware's safety check when running
        # on the extracted host repo (the binary maps /app to the host repo).
        config_path = add_host_repo_to_safety_allowed_paths(
            config_path, host_repo_dir, output_dir, args.model_profile, logger
        )
        patch_config = load_config(config_path)

        feedback = args.repair_feedback_map.get(instance_id)
        few_shot_examples: str | None = None
        ranked_files: list[str] = []
        if args.few_shot_examples:
            few_shot_path = Path(args.few_shot_examples)
            if few_shot_path.exists():
                few_shot_examples = few_shot_path.read_text(encoding="utf-8")
                logger.info("Loaded few-shot examples (%s chars)", len(few_shot_examples))
            else:
                logger.warning("Few-shot examples file not found: %s", few_shot_path)

        metadata: dict[str, Any] = {}
        error_code: str | None = None
        provider = patch_config.get("model")

        if should_use_agentless(args, patch_config):
            patch = run_agentless(
                host_repo_dir,
                instance,
                patch_config,
                args,
                log_dir,
                logger,
                name,
                few_shot_examples=few_shot_examples,
                metadata=metadata,
            )
            if patch.strip() and not _verify_patch_applies(
                host_repo_dir, patch, instance.get("base_commit"), logger
            ):
                logger.warning("Agentless patch for %s does not apply; treating as empty", instance_id)
                patch = ""
            if patch.strip():
                language = (instance.get("repo_language") or "").lower()
                if not _check_patch_builds(host_repo_dir, patch, language, logger, metadata):
                    logger.warning(
                        "Agentless patch for %s failed compile gate; treating as empty",
                        instance_id,
                    )
                    patch = ""
                    error_code = "compile_gate_rejected"
                    metadata["compile_gate_rejected"] = True
            save_prediction(
                output_dir, instance_id, patch, logger, metadata,
                error_code=error_code, provider=provider, model_profile=model_profile,
            )
            if (args.tdr and patch.strip()) or args.ensemble_models:
                patch, success = _run_tdr_block(
                    host_repo_dir,
                    instance,
                    patch,
                    args,
                    log_dir,
                    logger,
                    output_dir,
                    instance_id,
                    metadata=metadata,
                )
                return success and bool(patch.strip())
            if not patch.strip():
                metadata["empty_patch"] = True
                error_code = error_code or "empty_patch"
                save_prediction(
                    output_dir, instance_id, patch, logger, metadata,
                    error_code=error_code, provider=provider, model_profile=model_profile,
                )
                logger.warning("Empty patch for %s", instance_id)
                return False
            return True

        if args.plan_then_patch:
            patch = run_plan_then_patch(
                host_repo_dir,
                instance,
                args.plan_config,
                patch_config,
                args,
                log_dir,
                logger,
                name,
            )
            if patch.strip():
                language = (instance.get("repo_language") or "").lower()
                if not _check_patch_builds(host_repo_dir, patch, language, logger, metadata):
                    logger.warning(
                        "Plan-then-patch patch for %s failed compile gate; treating as empty",
                        instance_id,
                    )
                    patch = ""
                    error_code = "compile_gate_rejected"
                    metadata["compile_gate_rejected"] = True
            save_prediction(
                output_dir, instance_id, patch, logger, metadata,
                error_code=error_code, provider=provider, model_profile=model_profile,
            )
            if (args.tdr and patch.strip()) or args.ensemble_models:
                patch, success = _run_tdr_block(
                    host_repo_dir,
                    instance,
                    patch,
                    args,
                    log_dir,
                    logger,
                    output_dir,
                    instance_id,
                    metadata=metadata,
                )
                return success and bool(patch.strip())
            if not patch.strip():
                metadata["empty_patch"] = True
                error_code = error_code or "empty_patch"
                save_prediction(
                    output_dir, instance_id, patch, logger, metadata,
                    error_code=error_code, provider=provider, model_profile=model_profile,
                )
                logger.warning("Empty patch for %s", instance_id)
                return False
            return True

        # Pre-compute ranked files so the prompt can anchor the agent to concrete
        # source targets and so the diff fallback has focused snippets.
        tests = load_list_field(instance.get("selected_test_files_to_run", []))
        fail_to_pass = load_list_field(instance.get("fail_to_pass", []))
        search_text = instance.get("problem_statement", "") or ""
        requirements_text = instance.get("requirements", "") or ""
        if requirements_text:
            search_text = f"{search_text}\n{requirements_text}".strip()
        ranked_files = rank_files_by_relevance(
            host_repo_dir,
            search_text,
            test_names=tests + fail_to_pass,
            top_k=10,
        )
        logger.info("Ranked files for prompt/fallback: %s", ranked_files)

        if args.small_model_adapter:
            context_window = patch_config.get("agent", {}).get("context_window", 32768)
            prompt_text = build_small_model_prompt(
                instance,
                host_repo_dir,
                repair_feedback=feedback,
                few_shot_examples=few_shot_examples,
                container_repo_dir=args.repo_dir,
                context_window=context_window,
            )
            logger.info("Built small-model prompt (%s chars)", len(prompt_text))
        else:
            prompt_text = build_prompt(
                instance,
                repair_feedback=feedback,
                compact=args.compact_prompt,
                few_shot_examples=few_shot_examples,
                repo_path=host_repo_dir,
                ranked_files=ranked_files,
            )
        log_dir.mkdir(parents=True, exist_ok=True)
        (log_dir / f"{name}.prompt.txt").write_text(
            prompt_text, encoding="utf-8"
        )

        # Run selfware on the host against the extracted repo.
        success = run_selfware_on_host(
            instance_id,
            host_repo_dir,
            config_path,
            prompt_text,
            args.timeout,
            binary_path,
            log_dir,
            output_dir,
            logger,
            few_shot_examples=few_shot_examples,
            post_edit_test_command=test_cmd,
            metadata=metadata,
        )

        # Capture patch regardless of success, so we record partial work.
        test_patch_paths = paths_from_patch(instance.get("test_patch", "") or "")
        patch = capture_patch_on_host(
            host_repo_dir,
            logger,
            base_commit=instance.get("base_commit"),
            test_patch_paths=test_patch_paths,
        )

        # Verify the captured patch applies cleanly on the base commit.  A
        # patch that does not apply is useless for evaluation and should trigger
        # the same recovery path as an empty patch.
        if patch.strip() and not _verify_patch_applies(
            host_repo_dir, patch, instance.get("base_commit"), logger
        ):
            logger.warning("Captured patch for %s does not apply; treating as empty", instance_id)
            patch = ""

        # Compile / type-check gate: hard reject broken patches before submission.
        if patch.strip():
            language = (instance.get("repo_language") or "").lower()
            if not _check_patch_builds(host_repo_dir, patch, language, logger, metadata):
                logger.warning(
                    "Captured patch for %s failed compile gate; treating as empty",
                    instance_id,
                )
                patch = ""
                error_code = "compile_gate_rejected"
                metadata["compile_gate_rejected"] = True

        save_prediction(
            output_dir, instance_id, patch, logger, metadata,
            error_code=error_code, provider=provider, model_profile=model_profile,
        )

        if not patch.strip():
            logger.warning("Empty patch for %s", instance_id)
            if args.force_edit:
                logger.warning("Re-running %s with --force-edit directive", instance_id)
                force_prompt = prompt_text + (
                    "\n\nFORCE EDIT MODE: The previous attempt produced no source changes. "
                    "You MUST call file_edit at least once before finishing. "
                    "Do not complete until `git diff` shows a non-empty patch."
                )
                success = run_selfware_on_host(
                    instance_id,
                    host_repo_dir,
                    config_path,
                    force_prompt,
                    args.timeout,
                    binary_path,
                    log_dir,
                    output_dir,
                    logger,
                    few_shot_examples=few_shot_examples,
                    post_edit_test_command=test_cmd,
                    metadata=metadata,
                )
                patch = capture_patch_on_host(
                    host_repo_dir,
                    logger,
                    base_commit=instance.get("base_commit"),
                    test_patch_paths=test_patch_paths,
                )
                if patch.strip():
                    language = (instance.get("repo_language") or "").lower()
                    if not _check_patch_builds(host_repo_dir, patch, language, logger, metadata):
                        logger.warning(
                            "Force-edit patch for %s failed compile gate; treating as empty",
                            instance_id,
                        )
                        patch = ""
                        error_code = "compile_gate_rejected"
                        metadata["compile_gate_rejected"] = True
                save_prediction(
                    output_dir, instance_id, patch, logger, metadata,
                    error_code=error_code, provider=provider, model_profile=model_profile,
                )

        # ------------------------------------------------------------------
        # Early diff fallback: fragile models often fail on the first agent
        # attempt with JSON parse errors or max iterations. Try a one-shot
        # unified diff before spending time on recovery retries.
        # ------------------------------------------------------------------
        if not patch.strip() and args.early_diff_fallback:
            stderr_path = log_dir / f"{instance_id}.selfware.stderr.log"
            failure_class = classify_failure(stderr_path)
            if failure_class in (JSON_PARSE_ERROR, MAX_ITERATIONS):
                logger.warning(
                    "Early diff fallback for %s (failure=%s)",
                    instance_id,
                    failure_class,
                )
                reset_proc = run_cmd(
                    [
                        "git",
                        "-C",
                        str(host_repo_dir),
                        "reset",
                        "--hard",
                        instance["base_commit"],
                    ],
                    logger=logger,
                )
                if reset_proc.returncode == 0:
                    early_patch = run_diff_fallback(
                        host_repo_dir,
                        instance,
                        prompt_text,
                        ranked_files,
                        args.patch_config,
                        args,
                        log_dir,
                        logger,
                        f"{name}_early",
                    )
                    if early_patch.strip():
                        patch = early_patch
                        language = (instance.get("repo_language") or "").lower()
                        if not _check_patch_builds(host_repo_dir, patch, language, logger, metadata):
                            logger.warning(
                                "Early diff fallback patch for %s failed compile gate; treating as empty",
                                instance_id,
                            )
                            patch = ""
                            error_code = "compile_gate_rejected"
                            metadata["compile_gate_rejected"] = True
                        save_prediction(
                            output_dir, instance_id, patch, logger, metadata,
                            error_code=error_code, provider=provider, model_profile=model_profile,
                        )
                        success = bool(patch.strip())
                        logger.info("Early diff fallback succeeded for %s", instance_id)

        # ------------------------------------------------------------------
        # Failure-recovery loop: classify the failure, escalate the config,
        # and retry up to --max-retries times with a tailored prompt.
        # ------------------------------------------------------------------
        if (not success or not patch.strip()) and args.retry_failures:
            stderr_path = log_dir / f"{instance_id}.selfware.stderr.log"
            failure_class = classify_failure(stderr_path)
            if _is_patch_empty(patch):
                failure_class = EMPTY_PATCH
            logger.warning(
                "Classified failure for %s as '%s'",
                instance_id,
                failure_class,
            )

            for attempt in range(1, args.max_retries + 1):
                metadata["recovery_attempts"] = metadata.get("recovery_attempts", 0) + 1
                if not should_retry(failure_class, attempt, args.max_retries):
                    logger.info(
                        "No further recovery retries for %s (class=%s)",
                        instance_id,
                        failure_class,
                    )
                    break

                logger.warning(
                    "Recovery attempt %s/%s for %s (class=%s)",
                    attempt,
                    args.max_retries,
                    instance_id,
                    failure_class,
                )

                # Start each retry from a clean base commit so failed edits do
                # not accumulate.
                reset_proc = run_cmd(
                    [
                        "git",
                        "-C",
                        str(host_repo_dir),
                        "reset",
                        "--hard",
                        instance["base_commit"],
                    ],
                    logger=logger,
                )
                if reset_proc.returncode != 0:
                    logger.error(
                        "Failed to reset repo for recovery attempt %s: %s",
                        attempt,
                        reset_proc.stderr.strip(),
                    )
                    break

                # Re-apply the official test patch so recovery attempts can run
                # the failing tests against the fresh base commit.
                if not _apply_test_patch(
                    host_repo_dir,
                    instance.get("test_patch", "") or "",
                    logger,
                ):
                    logger.error(
                        "Failed to apply test_patch for recovery attempt %s",
                        attempt,
                    )
                    break

                recovery_result = escalation_config(patch_config, failure_class)
                recovery_path, system_message, prompt_suffix = write_recovery_config(
                    recovery_result,
                    log_dir,
                    f"{name}_attempt{attempt}",
                )

                if recovery_result.get(AGENTLESS_MODE_KEY):
                    logger.info(
                        "Switching to agentless SEARCH/REPLACE recovery for %s attempt %s",
                        instance_id,
                        attempt,
                    )
                    logger.info(
                        "RECOVERY_ESCALATION instance=%s attempt=%s agentless_mode=%s",
                        instance_id,
                        attempt,
                        AGENTLESS_MODE_KEY,
                    )
                    metadata["agentless_recovery_fired"] = True
                    recovery_patch_config = load_config(recovery_path)
                    patch = run_agentless(
                        host_repo_dir,
                        instance,
                        recovery_patch_config,
                        args,
                        log_dir,
                        logger,
                        f"{name}_attempt{attempt}",
                        few_shot_examples=few_shot_examples,
                        system_message=system_message,
                        prompt_suffix=prompt_suffix,
                        metadata=metadata,
                    )
                    success = bool(patch.strip())
                else:
                    recovery_prompt = build_recovery_prompt(
                        prompt_text,
                        system_message,
                        prompt_suffix,
                    )
                    (log_dir / f"{name}_attempt{attempt}.prompt.txt").write_text(
                        recovery_prompt, encoding="utf-8"
                    )
                    logger.info(
                        "Wrote recovery config for %s attempt %s: %s",
                        instance_id,
                        attempt,
                        recovery_path,
                    )

                    success = run_selfware_on_host(
                        instance_id,
                        host_repo_dir,
                        recovery_path,
                        recovery_prompt,
                        args.timeout,
                        binary_path,
                        log_dir,
                        output_dir,
                        logger,
                        few_shot_examples=few_shot_examples,
                        post_edit_test_command=test_cmd,
                        metadata=metadata,
                    )
                    patch = capture_patch_on_host(
                        host_repo_dir,
                        logger,
                        base_commit=instance.get("base_commit"),
                        test_patch_paths=test_patch_paths,
                    )
                save_prediction(
                    output_dir, instance_id, patch, logger, metadata,
                    error_code=error_code, provider=provider, model_profile=model_profile,
                )

                if success and patch.strip():
                    logger.info(
                        "Recovery attempt %s succeeded for %s", attempt, instance_id
                    )
                    metadata["recovery_succeeded"] = True
                    break

                # Re-classify for the next attempt. If the patch is still empty,
                # keep EMPTY_PATCH so we keep trying the agentless recovery path.
                if _is_patch_empty(patch):
                    failure_class = EMPTY_PATCH
                else:
                    stderr_path = log_dir / f"{instance_id}.selfware.stderr.log"
                    failure_class = classify_failure(stderr_path)
                logger.warning(
                    "Re-classified failure for %s as '%s' after attempt %s",
                    instance_id,
                    failure_class,
                    attempt,
                )

        # ------------------------------------------------------------------
        # One-shot diff fallback: when the agent loop (and optional recovery
        # retries) produced no patch, ask the model for a unified diff directly.
        # ------------------------------------------------------------------
        if not patch.strip() and args.diff_fallback:
            logger.warning(
                "Running one-shot diff fallback for %s", instance_id
            )
            reset_proc = run_cmd(
                [
                    "git",
                    "-C",
                    str(host_repo_dir),
                    "reset",
                    "--hard",
                    instance["base_commit"],
                ],
                logger=logger,
            )
            if reset_proc.returncode != 0:
                logger.error(
                    "Failed to reset repo for diff fallback: %s",
                    reset_proc.stderr.strip(),
                )
            else:
                fallback_patch = run_diff_fallback(
                    host_repo_dir,
                    instance,
                    prompt_text,
                    ranked_files,
                    args.patch_config,
                    args,
                    log_dir,
                    logger,
                    name,
                )
                if fallback_patch.strip():
                    patch = fallback_patch
                    language = (instance.get("repo_language") or "").lower()
                    if not _check_patch_builds(host_repo_dir, patch, language, logger, metadata):
                        logger.warning(
                            "Diff fallback patch for %s failed compile gate; treating as empty",
                            instance_id,
                        )
                        patch = ""
                        error_code = "compile_gate_rejected"
                        metadata["compile_gate_rejected"] = True
                    save_prediction(
                        output_dir, instance_id, patch, logger, metadata,
                        error_code=error_code, provider=provider, model_profile=model_profile,
                    )
                    success = bool(patch.strip())
                    logger.info("Diff fallback succeeded for %s", instance_id)

        if not patch.strip():
            metadata["empty_patch"] = True
            error_code = error_code or "empty_patch"
            save_prediction(
                output_dir, instance_id, patch, logger, metadata,
                error_code=error_code, provider=provider, model_profile=model_profile,
            )

        if not success:
            logger.warning("selfware did not succeed for %s; empty/short patch likely", instance_id)

        # ------------------------------------------------------------------
        # P2 critic loop: if the patch passes the compile gate but fails F2P,
        # ask a critic model to refine it.
        # ------------------------------------------------------------------
        if patch.strip() and args.critic_iterations > 0:
            critic_config = getattr(args, "critic_config", None) or args.patch_config
            patch = run_critic_loop(
                host_repo_dir,
                instance,
                patch,
                critic_config,
                args,
                log_dir,
                logger,
                metadata=metadata,
            )
            save_prediction(
                output_dir, instance_id, patch, logger, metadata,
                error_code=error_code, provider=provider, model_profile=model_profile,
            )
            success = bool(patch.strip())

        # ------------------------------------------------------------------
        # Test-driven repair loop: validate in the official container and
        # ask a repair model to fix remaining test failures.
        # ------------------------------------------------------------------
        if (args.tdr and patch.strip()) or args.ensemble_models:
            patch, success = _run_tdr_block(
                host_repo_dir,
                instance,
                patch,
                args,
                log_dir,
                logger,
                output_dir,
                instance_id,
                metadata=metadata,
            )

        # A "successful" run that produced no diff is not a real fix for SWE-bench Pro.
        return success and bool(patch.strip())
    except Exception as exc:  # pragma: no cover - defensive
        logger.error("Unexpected error processing %s: %s", instance_id, exc)
        logger.debug(traceback.format_exc())
        # Save an empty prediction so we don't retry on --resume.
        if isinstance(exc, RuntimeError) and "api" in str(exc).lower():
            error_code = "api_error"
        save_prediction(
            output_dir, instance_id, "", logger, metadata,
            error_code=error_code or "api_error", provider=provider, model_profile=model_profile,
        )
        return False
    finally:
        if not args.keep_container:
            stop_and_remove_container(name, logger)
        if args.tdr and not args.tdr_keep_container:
            stop_and_remove_container(tdr_name, logger)


def main() -> int:
    args = parse_args()
    output_dir = Path(args.output_dir).resolve()
    logger = setup_logging(output_dir)

    if "SELFWARE_API_KEY" not in os.environ:
        logger.error(
            "SELFWARE_API_KEY is not set. Export it before running this harness."
        )
        return 1

    try:
        config_path = prepare_effective_config(args, logger)
    except FileNotFoundError as exc:
        logger.error("%s", exc)
        return 1
    logger.info("Using config: %s", config_path)

    # Load effective config dict for direct API calls (used by plan-then-patch).
    args.patch_config = load_config(config_path)

    if args.plan_then_patch:
        plan_profile = args.plan_model_profile or args.model_profile
        plan_config_path = Path(args.config_dir) / f"openrouter_{plan_profile}.toml"
        if not plan_config_path.exists():
            logger.error("Plan config not found: %s", plan_config_path)
            return 1
        args.plan_config = load_config(plan_config_path)
        if args.local_endpoint:
            args.plan_config["endpoint"] = args.local_endpoint
            logger.info("Overriding plan endpoint with --local-endpoint: %s", args.local_endpoint)
        logger.info("Using plan config: %s", plan_config_path)

    # ------------------------------------------------------------------
    # Run provenance: enforce --fresh/--resume semantics and write the
    # run manifest so later invocations can verify they are compatible.
    # ------------------------------------------------------------------
    if args.resume and args.fresh:
        logger.error("Cannot use both --resume and --fresh")
        return 1

    config_files = [str(config_path)]
    if args.plan_then_patch:
        config_files.append(str(plan_config_path))

    current_manifest = _build_run_manifest(args, config_files)
    existing_manifest = _read_run_manifest(output_dir)

    if args.fresh:
        for stale in ("predictions.jsonl", "predictions.json", "run_manifest.json"):
            stale_path = output_dir / stale
            if stale_path.exists():
                stale_path.unlink()
                logger.info("Removed stale %s", stale_path.name)
    elif existing_manifest is not None:
        if args.resume:
            try:
                _check_run_manifest(current_manifest, existing_manifest)
            except RuntimeError as exc:
                logger.error("%s", exc)
                return 1
        else:
            logger.warning(
                "Existing run_manifest.json found; assuming --resume (provenance not re-verified). "
                "Pass --fresh to restart or --resume to confirm."
            )
            args.resume = True
            try:
                _check_run_manifest(current_manifest, existing_manifest)
            except RuntimeError as exc:
                logger.error("%s", exc)
                return 1
    else:
        if args.resume:
            logger.info("No run_manifest.json found; --resume will skip existing predictions only")

    _write_run_manifest(output_dir, current_manifest)

    binary_path = Path(args.binary)
    if not args.plan_then_patch and not binary_path.exists():
        logger.error("Selfware binary not found: %s", binary_path)
        return 1
    if not args.plan_then_patch:
        logger.info("Using binary: %s", binary_path)

    # Only load the full HF dataset when we actually need it.  When a sample
    # file or explicit instance IDs are supplied we can work from those rows
    # alone, which avoids long HF startup and rate-limit issues.
    if args.sample_file or args.instance_ids:
        dataset = None
        logger.info("Skipping full HF dataset load; using provided sample file or instance IDs")
    else:
        logger.info("Loading SWE-bench Pro dataset...")
        dataset = load_dataset("ScaleAI/SWE-bench_Pro", split="test")
        logger.info("Loaded %s instances", len(dataset))

    existing = load_existing_predictions(output_dir)
    if existing:
        logger.info("Found %s existing prediction(s)", len(existing))

    repair_feedback_map: dict[str, str] = {}
    if args.repair_feedback:
        feedback_path = Path(args.repair_feedback)
        if feedback_path.exists():
            repair_feedback_map = json.loads(feedback_path.read_text(encoding="utf-8"))
            logger.info("Loaded repair feedback for %s instance(s)", len(repair_feedback_map))
        else:
            logger.warning("Repair feedback file not found: %s", feedback_path)
    args.repair_feedback_map = repair_feedback_map

    if args.tdr:
        if args.repair_config:
            repair_config_path = Path(args.repair_config)
        elif args.repair_model_profile:
            repair_config_path = Path(args.config_dir) / f"openrouter_{args.repair_model_profile}.toml"
        else:
            logger.error(
                "--test-driven-repair requires either --repair-config or --repair-model-profile"
            )
            return 1
        if not repair_config_path.exists():
            logger.error("Repair config not found: %s", repair_config_path)
            return 1
        args.repair_config = load_config(repair_config_path)
        logger.info("Using repair config: %s", repair_config_path)

    if args.critic_iterations > 0:
        if args.critic_model_profile:
            critic_config_path = Path(args.config_dir) / f"openrouter_{args.critic_model_profile}.toml"
            if not critic_config_path.exists():
                logger.error("Critic config not found: %s", critic_config_path)
                return 1
            args.critic_config = load_config(critic_config_path)
            logger.info("Using critic config: %s", critic_config_path)
        else:
            args.critic_config = args.patch_config
            logger.info("Using main model config for critic loop")

    instances = select_instances(dataset, args, existing, logger)
    if not instances:
        logger.info("No instances to process.")
        return 0

    if not os.environ.get("SELFWARE_BYPASS_COMPILE_GATE") and not _check_host_toolchains(
        instances, logger
    ):
        return 1

    failures = 0
    if args.workers > 1:
        with ThreadPoolExecutor(max_workers=args.workers) as executor:
            future_to_instance = {
                executor.submit(process_instance, instance, args, config_path, logger): instance
                for instance in instances
            }
            for future in as_completed(future_to_instance):
                instance = future_to_instance[future]
                try:
                    ok = future.result()
                except Exception as exc:
                    logger.error("Unexpected error processing %s: %s", instance["instance_id"], exc)
                    ok = False
                if not ok:
                    failures += 1
    else:
        for instance in instances:
            ok = process_instance(instance, args, config_path, logger)
            if not ok:
                failures += 1

    logger.info("Finished. Failures: %s/%s", failures, len(instances))
    logger.info("Predictions written to %s", output_dir / "predictions.jsonl")
    write_predictions_json(output_dir, logger)

    # Sanity check: if recovery is requested for a non-trivial run, at least
    # one instance should have triggered a recovery step.  Zero recoveries
    # across many instances means the escalation path is misconfigured or
    # bypassed (e.g. agentless mode returns before the recovery loop).
    if args.retry_failures and len(instances) >= 10:
        recovery_fired = 0
        predictions_path = output_dir / "predictions.jsonl"
        if predictions_path.exists():
            for line in predictions_path.read_text(encoding="utf-8").splitlines():
                if not line.strip():
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if rec.get("metadata", {}).get("recovery_attempts", 0) > 0:
                    recovery_fired += 1
        if recovery_fired == 0:
            logger.error(
                "SANITY CHECK FAILED: --retry-failures is enabled but "
                "recovery_fired_count=0 across %s instances. The recovery "
                "escalation path is likely bypassed; check agentless routing.",
                len(instances),
            )
            failures += 1
        else:
            logger.info(
                "SANITY CHECK PASSED: recovery fired for %s/%s instances",
                recovery_fired,
                len(instances),
            )

    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
