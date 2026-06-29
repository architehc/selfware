"""Tests for SWE-bench Pro evaluation entryscript and scoring."""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from evaluate_predictions import (
    _build_entryscript,
    _is_patch_empty,
    _no_tests_were_executed,
    _score_output,
    _write_report,
)


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


def test_entryscript_has_exit_trap_for_output_json():
    """A container crash must still leave an output.json for the harness."""
    script = _build_entryscript(_minimal_instance())
    assert "trap " in script and "/workspace/output.json" in script
    assert "EXIT" in script


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


def test_entryscript_passes_tests_as_separate_args():
    instance = _minimal_instance()
    instance["selected_test_files_to_run"] = ["tests/test_foo.py", "tests/test_bar.py"]
    script = _build_entryscript(instance)
    # Selected targets must be separate shell arguments, not a single comma-joined blob.
    assert "tests/test_foo.py,tests/test_bar.py" not in script
    assert "bash /workspace/run_script.sh tests/test_foo.py tests/test_bar.py" in script


def test_entryscript_quotes_test_args_with_spaces():
    instance = _minimal_instance()
    instance["selected_test_files_to_run"] = ["test file.py"]
    script = _build_entryscript(instance)
    assert "bash /workspace/run_script.sh 'test file.py'" in script


def _instance_with_tests(fail_to_pass=None, pass_to_pass=None):
    instance = _minimal_instance()
    instance["fail_to_pass"] = fail_to_pass or []
    instance["pass_to_pass"] = pass_to_pass or []
    return instance


def test_no_tests_were_executed_detects_sentinel_only():
    instance = _instance_with_tests(
        fail_to_pass=["test_fail.py::test_x"],
        pass_to_pass=["test_pass.py::test_y"],
    )
    output = {
        "tests": [
            {"name": "NO_TESTS_FOUND_OR_PARSING_ERROR", "status": "ERROR"},
        ]
    }
    score = _score_output(output, instance)
    assert _no_tests_were_executed(score, output) is True


def test_no_tests_were_executed_detects_empty_output():
    instance = _instance_with_tests(
        fail_to_pass=["test_fail.py::test_x"],
        pass_to_pass=["test_pass.py::test_y"],
    )
    output = {"tests": []}
    score = _score_output(output, instance)
    assert _no_tests_were_executed(score, output) is True


def test_no_tests_were_executed_detects_all_missing():
    instance = _instance_with_tests(
        fail_to_pass=["test_fail.py::test_x"],
        pass_to_pass=["test_pass.py::test_y"],
    )
    # Parser produced some unrelated test, but expected ones are absent.
    output = {
        "tests": [
            {"name": "some_other_test", "status": "PASSED"},
        ]
    }
    score = _score_output(output, instance)
    assert _no_tests_were_executed(score, output) is True


def test_no_tests_were_executed_false_when_expected_tests_present():
    instance = _instance_with_tests(
        fail_to_pass=["test_fail.py::test_x"],
        pass_to_pass=["test_pass.py::test_y"],
    )
    output = {
        "tests": [
            {"name": "test_fail.py::test_x", "status": "PASSED"},
            {"name": "test_pass.py::test_y", "status": "PASSED"},
        ]
    }
    score = _score_output(output, instance)
    assert _no_tests_were_executed(score, output) is False


def test_score_and_classify_marks_no_tests_as_errored():
    """Simulate the post-scoring classification used by evaluate_instance."""
    instance = _instance_with_tests(
        fail_to_pass=["test_fail.py::test_x"],
        pass_to_pass=["test_pass.py::test_y"],
    )
    output = {
        "tests": [
            {"name": "NO_TESTS_FOUND_OR_PARSING_ERROR", "status": "ERROR"},
        ]
    }
    result = {
        "instance_id": "test-1",
        "error": None,
        "metadata": {},
    }
    score = _score_output(output, instance)
    result.update(score)
    if _no_tests_were_executed(score, output):
        result["error"] = "no tests executed"
        result["overall_pass"] = False
        result["fail_to_pass_passed"] = 0
        result["fail_to_pass_total"] = 0
        result["pass_to_pass_passed"] = 0
        result["pass_to_pass_total"] = 0

    assert result["error"] == "no tests executed"
    assert result["overall_pass"] is False
    assert result["fail_to_pass_passed"] == 0
    assert result["fail_to_pass_total"] == 0
    assert result["pass_to_pass_passed"] == 0
    assert result["pass_to_pass_total"] == 0


def test_write_report_includes_diagnostic_counters(tmp_path):
    patch_text = "diff --git a/foo b/foo\n--- a/foo\n+++ b/foo\n@@ -1 +1 @@\n-old\n+new\n"
    results = [
        {
            "instance_id": "empty-patch",
            "error": "empty patch",
            "patch": "",
            "metadata": {},
        },
        {
            "instance_id": "compile-gate",
            "error": None,
            "overall_pass": False,
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 1,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "metadata": {"compile_gate_rejected": True},
        },
        {
            "instance_id": "recovery-fired",
            "error": None,
            "overall_pass": True,
            "fail_to_pass_passed": 1,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 1,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "metadata": {"recovery_attempts": 2, "recovery_succeeded": True},
        },
        {
            "instance_id": "no-op",
            "error": "patch applied but changed no files",
            "patch": patch_text,
            "metadata": {},
        },
        {
            "instance_id": "no-tests",
            "error": "no tests executed",
            "patch": patch_text,
            "metadata": {},
        },
        {
            "instance_id": "f2p-failed",
            "error": None,
            "overall_pass": False,
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 2,
            "pass_to_pass_passed": 2,
            "pass_to_pass_total": 2,
            "patch": patch_text,
            "metadata": {},
        },
        {
            "instance_id": "p2p-regressed",
            "error": None,
            "overall_pass": False,
            "fail_to_pass_passed": 1,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 1,
            "pass_to_pass_total": 2,
            "patch": patch_text,
            "metadata": {},
        },
    ]

    report = _write_report(tmp_path, results)

    assert report["empty_patch_count"] == 1
    assert report["compile_gate_rejected_count"] == 1
    assert report["recovery_fired_count"] == 1
    assert report["recovery_succeeded_count"] == 1
    assert report["applied_no_op_count"] == 1
    assert report["applied_compile_failed_count"] == 1
    assert report["applied_f2p_failed_count"] == 2  # compile-gate + f2p-failed
    assert report["applied_p2p_regressed_count"] == 1  # p2p-regressed

    report_path = tmp_path / "evaluation_report.json"
    assert report_path.exists()
    loaded = json.loads(report_path.read_text(encoding="utf-8"))
    for key in (
        "empty_patch_count",
        "compile_gate_rejected_count",
        "recovery_fired_count",
        "recovery_succeeded_count",
        "applied_no_op_count",
        "applied_compile_failed_count",
        "applied_f2p_failed_count",
        "applied_p2p_regressed_count",
    ):
        assert key in loaded, key

    summary_path = tmp_path / "evaluation_summary.md"
    summary_text = summary_path.read_text(encoding="utf-8")
    assert "## Diagnostic counters" in summary_text
    assert "Empty patch: **1**" in summary_text
    assert "Compile gate rejected: **1**" in summary_text
    assert "Recovery fired: **1**" in summary_text
    assert "Recovery succeeded: **1**" in summary_text
    assert "Patch applied but changed no files: **1**" in summary_text
    assert "Patch applied but no tests executed: **1**" in summary_text
    assert "Fail-to-pass failed: **2**" in summary_text
    assert "Pass-to-pass regressed: **1**" in summary_text
