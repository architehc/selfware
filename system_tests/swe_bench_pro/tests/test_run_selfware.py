"""Tests for run_selfware harness routing decisions."""

import argparse
import json
import os
import sys
from pathlib import Path

# Make sibling harness modules importable when running pytest directly.
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import pytest

from run_selfware import load_existing_predictions, should_use_agentless


def _args(agentless=None, sample_size=1, auto_agentless=None):
    return argparse.Namespace(
        agentless=agentless,
        auto_agentless=auto_agentless,
        sample_size=sample_size,
    )


def test_should_use_agentless_defaults_small_model_small_sample():
    """Small/fragile models on small samples should default to agentless."""
    config = {"model": "qwen2.5-7b"}
    assert should_use_agentless(_args(sample_size=5), config) is True


def test_should_use_agentless_defaults_llama_small_sample():
    """Known small/fragile aliases should default to agentless on small runs."""
    config = {"model": "meta-llama/llama-3.1-8b-instruct"}
    assert should_use_agentless(_args(sample_size=10), config) is True


def test_should_use_agentless_respects_large_sample():
    """Small models on larger samples should still go agentless if not recommended."""
    config = {
        "model": "qwen2.5-7b",
        "metadata": {"recommended": False},
    }
    assert should_use_agentless(_args(sample_size=50), config) is True


def test_should_use_agentless_recommended_large_model_stays_tool_loop():
    """Explicitly recommended strong models should stay on the agent loop."""
    config = {
        "model": "gemini-2.5-pro",
        "metadata": {"recommended": True},
    }
    assert should_use_agentless(_args(sample_size=5), config) is False


def test_should_use_agentless_recommended_medium_model_stays_tool_loop():
    """Explicitly recommended medium-tier models should stay on the agent loop."""
    config = {
        "model": "kimi-k2.7-code",
        "metadata": {"recommended": True, "tier": "medium"},
    }
    assert should_use_agentless(_args(sample_size=5), config) is False


def test_should_use_agentless_not_recommended_goes_agentless():
    """Models without a strong recommendation default to agentless."""
    config = {
        "model": "some-large-model",
        "metadata": {"recommended": False},
    }
    assert should_use_agentless(_args(sample_size=50), config) is True


def test_should_use_agentless_missing_recommended_goes_agentless():
    """Models with no recommendation metadata default to agentless."""
    config = {"model": "some-large-model"}
    assert should_use_agentless(_args(sample_size=50), config) is True


def test_should_use_agentless_explicit_opt_in():
    """--agentless=true always forces agentless on."""
    config = {
        "model": "gemini-2.5-pro",
        "metadata": {"recommended": True},
    }
    assert should_use_agentless(_args(agentless=True, sample_size=5), config) is True


def test_should_use_agentless_explicit_opt_out():
    """--agentless=false always forces the multi-turn tool loop."""
    config = {
        "model": "qwen2.5-7b",
        "metadata": {"recommended": False},
    }
    assert should_use_agentless(_args(agentless=False, sample_size=5), config) is False


def test_should_use_agentless_agentless_default_metadata():
    """Configs that set agentless_default=true always go agentless."""
    config = {
        "model": "gemini-2.5-pro",
        "metadata": {"agentless_default": True, "recommended": True},
    }
    assert should_use_agentless(_args(sample_size=50), config) is True


def test_load_existing_predictions_skips_empty_patches():
    """Empty predictions are not treated as completed runs on --resume."""
    output_dir = Path(__file__).parent / "_tmp_predictions"
    output_dir.mkdir(exist_ok=True)
    predictions = output_dir / "predictions.jsonl"
    try:
        with open(predictions, "w", encoding="utf-8") as f:
            f.write(json.dumps({"instance_id": "has_patch", "patch": "diff --git"}) + "\n")
            f.write(json.dumps({"instance_id": "empty_patch", "patch": ""}) + "\n")
            f.write(json.dumps({"instance_id": "whitespace_patch", "patch": "   \n"}) + "\n")
        assert load_existing_predictions(output_dir) == {"has_patch"}
    finally:
        predictions.unlink(missing_ok=True)
        output_dir.rmdir()
