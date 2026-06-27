"""Tests for SWE-bench Pro evaluation entryscript."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from evaluate_predictions import _build_entryscript, _is_patch_empty


def _minimal_instance() -> dict:
    return {
        "instance_id": "test-1",
        "base_commit": "abc123",
        "selected_test_files_to_run": ["tests/test_foo.py"],
        "before_repo_set_cmd": "",
    }


def test_entryscript_has_no_global_set_e():
    script = _build_entryscript(_minimal_instance())
    lines = script.splitlines()
    # Allow "set -uo pipefail" but not "set -e" anywhere.
    for line in lines:
        assert "set -e" not in line, f"unexpected set -e in: {line}"


def test_entryscript_always_writes_artifacts():
    script = _build_entryscript(_minimal_instance())
    assert "/workspace/output.json" in script
    assert "/workspace/patch_apply_status.txt" in script
    # It should write output.json in every early-exit path, including failures.
    assert script.count('> /workspace/output.json') >= 3


def test_entryscript_detects_no_op_patch():
    script = _build_entryscript(_minimal_instance())
    assert "git diff --name-only HEAD" in script
    assert "git ls-files --others --exclude-standard" in script
    assert "PATCH_NO_OP" in script


def test_entryscript_no_op_requires_both_empty():
    script = _build_entryscript(_minimal_instance())
    # The no-op branch must only fire when both changed and untracked files are empty.
    assert "[ -z \"$changed_files\" ] && [ -z \"$untracked_files\" ]" in script


def test_entryscript_patch_fallback_uses_no_backup():
    script = _build_entryscript(_minimal_instance())
    assert "patch -p1 --no-backup-if-mismatch" in script


def test_entryscript_handles_before_command_failure():
    instance = _minimal_instance()
    instance["before_repo_set_cmd"] = "false"
    script = _build_entryscript(instance)
    # The before command should not kill the script; status should be checked.
    assert "before_status=$?" in script or "$before_status" in script


def test_is_patch_empty():
    assert _is_patch_empty("") is True
    assert _is_patch_empty("   \n\n  ") is True
    assert _is_patch_empty("diff --git a/foo b/foo\n") is False


def test_entryscript_redirects_run_stderr():
    script = _build_entryscript(_minimal_instance())
    assert "2> /workspace/stderr.log" in script
