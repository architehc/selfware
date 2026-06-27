"""Tests for harness_recovery failure classification and escalation."""

import os
import sys
import tempfile

# Make sibling harness modules importable when running pytest directly.
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import pytest

from harness_recovery import (
    AGENTLESS_MODE_KEY,
    EMPTY_PATCH,
    PROMPT_SUFFIX_KEY,
    SYSTEM_MESSAGE_KEY,
    _is_patch_empty,
    classify_failure,
    escalation_config,
    should_retry,
)


def test_is_patch_empty():
    assert _is_patch_empty("") is True
    assert _is_patch_empty("   \n") is True
    assert _is_patch_empty("   ") is True
    assert _is_patch_empty(None) is True
    assert _is_patch_empty("diff --git a/file.py b/file.py\n+foo") is False


def test_classify_failure_empty_patch_log():
    with tempfile.NamedTemporaryFile("w", suffix=".log", delete=False) as f:
        f.write("The run finished but produced an empty patch.\n")
        path = f.name
    try:
        assert classify_failure(path) == EMPTY_PATCH
    finally:
        os.unlink(path)


def test_should_retry_empty_patch():
    assert should_retry(EMPTY_PATCH, 1, 2) is True
    assert should_retry(EMPTY_PATCH, 2, 2) is True
    assert should_retry(EMPTY_PATCH, 3, 2) is False


def test_escalation_config_empty_patch_enables_agentless():
    base = {
        "temperature": 0.1,
        "agent": {"native_function_calling": True, "streaming": True},
    }
    result = escalation_config(base, EMPTY_PATCH)

    assert result.get(AGENTLESS_MODE_KEY) is True
    assert "SEARCH/REPLACE" in (result.get(PROMPT_SUFFIX_KEY) or "")
    assert "empty patch" in (result.get(SYSTEM_MESSAGE_KEY) or "").lower()
    assert result["temperature"] < base["temperature"]
    assert result["agent"]["native_function_calling"] is False
    assert result["agent"]["streaming"] is False
    assert result["agent"]["minimal_tool_catalog"] is True
