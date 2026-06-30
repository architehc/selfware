#!/usr/bin/env python3
"""YAML config loader for small-model SWE-bench Pro harness profiles.

The harness traditionally loads ``openrouter_<profile>.toml`` from
``system_tests/projecte2e/config``.  This module lets per-model YAML files in
``system_tests/swe_bench_pro/configs/`` override those defaults when a small
model is selected (e.g. by ``--auto-agentless``).
"""

from __future__ import annotations

import copy
import logging
from pathlib import Path
from typing import Any

try:
    import yaml
except Exception as exc:  # pragma: no cover - harness dependency check
    raise SystemExit(
        "The 'pyyaml' library is required for small-model YAML configs. "
        "Install it with: pip install pyyaml"
    ) from exc


DEFAULT_SMALL_MODEL_CONFIG_DIR = Path(__file__).resolve().parent / "configs"


def load_small_model_config(
    profile: str,
    config_dir: Path | str | None = None,
    logger: logging.Logger | None = None,
) -> dict[str, Any] | None:
    """Load a small-model YAML config by profile name.

    Returns ``None`` if the file does not exist, so callers can fall back to
    the TOML config without failing.
    """
    if config_dir is None:
        config_dir = DEFAULT_SMALL_MODEL_CONFIG_DIR
    config_dir = Path(config_dir)
    path = config_dir / f"{profile}.yaml"
    if not path.exists():
        return None
    with open(path, encoding="utf-8") as f:
        data = yaml.safe_load(f)
    if not isinstance(data, dict):
        if logger is not None:
            logger.warning(
                "Small-model YAML config %s is not a mapping; ignoring", path
            )
        return None
    return data


def merge_over_toml(
    toml_config: dict[str, Any], yaml_config: dict[str, Any]
) -> dict[str, Any]:
    """Return a new dict with ``yaml_config`` deep-merged over ``toml_config``.

    Nested dictionaries are merged recursively; scalar values and lists in
    ``yaml_config`` replace the corresponding TOML values.
    """
    merged = copy.deepcopy(toml_config)
    for key, value in yaml_config.items():
        if isinstance(value, dict) and isinstance(merged.get(key), dict):
            merged[key] = merge_over_toml(merged[key], value)
        else:
            merged[key] = copy.deepcopy(value)
    return merged
