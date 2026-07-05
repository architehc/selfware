"""Tests for small_model_configs YAML loader and merge helper."""

import os
import sys
from pathlib import Path
from typing import Any

# Make sibling harness modules importable when running pytest directly.
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import pytest

from small_model_configs import (
    DEFAULT_SMALL_MODEL_CONFIG_DIR,
    load_small_model_config,
    merge_over_toml,
)


EXPECTED_SMALL_MODEL_PROFILES = frozenset({
    "llama-3.1-8b",
    "qwen2.5-7b",
    "gemma-3-12b",
    "mistral-nemo",
    "cohere-north-mini-code-free-sweap",
    "granite-4.1-8b",
    "lfm-2.5-1.2b-thinking-free-sweap",
    "nova-lite",
    "poolside-laguna-xs.2-free-sweap",
    "tencent-hy3-preview",
    "stepfun-step-3.7-flash",
    "minimax-minimax-m3",
    "qwen-qwen3.7-plus",
    "deepseek-deepseek-v4-flash",
})


def test_default_config_dir_points_at_configs():
    """The default config dir is the ``configs`` sibling of this module."""
    assert DEFAULT_SMALL_MODEL_CONFIG_DIR.name == "configs"
    assert DEFAULT_SMALL_MODEL_CONFIG_DIR.exists()


def test_all_small_model_yaml_configs_load():
    """Every expected small-model profile has a loadable YAML config."""
    for profile in EXPECTED_SMALL_MODEL_PROFILES:
        config = load_small_model_config(profile)
        assert config is not None, f"Missing YAML config for {profile}"
        assert config.get("model"), f"{profile}.yaml missing model string"
        assert "metadata" in config, f"{profile}.yaml missing metadata"
        assert config["metadata"].get("tier") == "small", f"{profile} is not tier small"


def test_llama_3_1_8b_uses_validated_smoke_grid_defaults():
    """Llama 3.1 8B should keep the best sample_3 tuning-grid defaults."""
    config = load_small_model_config("llama-3.1-8b")
    assert config is not None
    assert config["max_tokens"] == 6000
    assert config["agent"]["edit_deadline_step"] == 6


def test_load_small_model_config_returns_none_for_missing_profile():
    """Missing profiles fall back to None rather than raising."""
    assert load_small_model_config("definitely-not-a-real-model") is None


def test_load_small_model_config_returns_none_for_invalid_yaml(tmp_path) -> None:
    """Non-mapping YAML files are ignored."""
    config_dir = tmp_path / "configs"
    config_dir.mkdir()
    (config_dir / "bad.yaml").write_text("- just\n- a\n- list\n", encoding="utf-8")
    assert load_small_model_config("bad", config_dir) is None


def test_merge_overrides_toml_scalar():
    """YAML scalar values replace TOML values."""
    toml: dict[str, Any] = {
        "model": "old/model",
        "max_tokens": 1024,
        "temperature": 0.5,
    }
    yaml = {
        "model": "new/model",
        "max_tokens": 8192,
    }
    merged = merge_over_toml(toml, yaml)
    assert merged["model"] == "new/model"
    assert merged["max_tokens"] == 8192
    assert merged["temperature"] == 0.5


def test_merge_overrides_nested_dict():
    """YAML nested dictionaries are merged recursively with TOML."""
    toml: dict[str, Any] = {
        "agent": {
            "max_iterations": 25,
            "context_window": 0,
            "streaming": False,
        },
        "metadata": {
            "tier": "small",
            "recommended": True,
        },
    }
    yaml = {
        "agent": {
            "max_iterations": 60,
            "require_edit_before_completion": True,
        },
        "metadata": {
            "recommended": False,
        },
    }
    merged = merge_over_toml(toml, yaml)
    assert merged["agent"]["max_iterations"] == 60
    assert merged["agent"]["context_window"] == 0
    assert merged["agent"]["streaming"] is False
    assert merged["agent"]["require_edit_before_completion"] is True
    assert merged["metadata"]["tier"] == "small"
    assert merged["metadata"]["recommended"] is False


def test_merge_does_not_mutate_inputs():
    """merge_over_toml returns a fresh dict and leaves inputs untouched."""
    toml: dict[str, Any] = {"agent": {"max_iterations": 25}}
    yaml = {"agent": {"max_iterations": 60}}
    merged = merge_over_toml(toml, yaml)
    assert merged["agent"]["max_iterations"] == 60
    assert toml["agent"]["max_iterations"] == 25
    assert yaml["agent"]["max_iterations"] == 60


def test_merge_replaces_lists():
    """YAML lists replace TOML lists rather than being appended."""
    toml: dict[str, Any] = {"safety": {"allowed_paths": ["./**"]}}
    yaml = {"safety": {"allowed_paths": ["./**", "/app/**"]}}
    merged = merge_over_toml(toml, yaml)
    assert merged["safety"]["allowed_paths"] == ["./**", "/app/**"]
