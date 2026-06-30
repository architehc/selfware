"""Tests for SWE-bench Pro evaluation entryscript and scoring."""

import json
import logging
import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from evaluate_predictions import (
    _augment_results_with_missing_predictions,
    _build_entryscript,
    _ensure_image,
    _is_patch_empty,
    _no_tests_were_executed,
    _prepull_images,
    _score_output,
    _write_report,
    main,
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
    """Simulate the post-scoring classification used by evaluate_instance.

    Errored instances keep the expected test totals so they still count in the
    headline fail-to-pass / pass-to-pass denominators.
    """
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
        result["pass_to_pass_passed"] = 0

    assert result["error"] == "no tests executed"
    assert result["overall_pass"] is False
    assert result["fail_to_pass_passed"] == 0
    assert result["fail_to_pass_total"] == 1
    assert result["pass_to_pass_passed"] == 0
    assert result["pass_to_pass_total"] == 1


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


def test_augment_results_adds_missing_prediction(tmp_path):
    """A missing prediction row must appear as errored and count in totals."""
    instances = {
        "present": {
            "instance_id": "present",
            "fail_to_pass": ['["f1"]'],
            "pass_to_pass": ['["p1"]'],
        },
        "missing": {
            "instance_id": "missing",
            "fail_to_pass": ["f2"],
            "pass_to_pass": ["p2", "p3"],
        },
    }
    results = [
        {
            "instance_id": "present",
            "error": None,
            "overall_pass": True,
            "fail_to_pass_passed": 1,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 1,
            "pass_to_pass_total": 1,
            "patch": "diff --git\n",
            "metadata": {},
        },
    ]

    augmented = _augment_results_with_missing_predictions(results, instances)
    report = _write_report(tmp_path, augmented)

    assert report["total_instances"] == 2
    assert report["completed_instances"] == 1
    assert report["errored_instances"] == 1
    assert report["overall_passed_instances"] == 1
    assert report["overall_pass_rate_total"] == 1 / 2
    assert report["overall_pass_rate"] == 1 / 1
    assert report["missing_prediction_count"] == 1

    missing = next(r for r in report["per_instance"] if r["instance_id"] == "missing")
    assert missing["error"] == "missing_prediction"
    assert missing["overall_pass"] is False
    assert missing["fail_to_pass_passed"] == 0
    assert missing["fail_to_pass_total"] == 1
    assert missing["pass_to_pass_passed"] == 0
    assert missing["pass_to_pass_total"] == 2
    assert missing["metadata"] == {}

    summary_text = (tmp_path / "evaluation_summary.md").read_text(encoding="utf-8")
    assert "Missing prediction: **1**" in summary_text


def test_write_report_headline_uses_total_rate(tmp_path):
    """The honest headline denominator is the total sample, counting errors as failed."""
    patch_text = "diff --git a/foo b/foo\n--- a/foo\n+++ b/foo\n@@ -1 +1 @@\n-old\n+new\n"
    results = [
        {
            "instance_id": "passed",
            "error": None,
            "overall_pass": True,
            "fail_to_pass_passed": 1,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 1,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "metadata": {},
        },
        {
            "instance_id": "failed",
            "error": None,
            "overall_pass": False,
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 1,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "metadata": {},
        },
        {
            "instance_id": "errored",
            "error": "no tests executed",
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 0,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "metadata": {},
        },
    ]

    report = _write_report(tmp_path, results)

    assert report["total_instances"] == 3
    assert report["completed_instances"] == 2
    assert report["errored_instances"] == 1
    assert report["overall_passed_instances"] == 1
    assert report["overall_pass_rate_total"] == 1 / 3
    assert report["overall_pass_rate"] == 1 / 2

    summary_text = (tmp_path / "evaluation_summary.md").read_text(encoding="utf-8")
    # The summary must lead with the total-instance headline.
    assert "Overall passed instances (errors counted as failed): **1/3**" in summary_text
    assert "Overall passed instances (completed only): **1/2**" in summary_text


def test_write_report_f2p_p2p_completed_only_excludes_errors(tmp_path):
    """Fail-to-pass/pass-to-pass completed-only rates exclude errored instances."""
    patch_text = "diff --git a/foo b/foo\n--- a/foo\n+++ b/foo\n@@ -1 +1 @@\n-old\n+new\n"
    results = [
        {
            "instance_id": "passed",
            "error": None,
            "overall_pass": True,
            "fail_to_pass_passed": 1,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 1,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "metadata": {},
        },
        {
            "instance_id": "failed",
            "error": None,
            "overall_pass": False,
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 1,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "metadata": {},
        },
        {
            "instance_id": "errored",
            "error": "no tests executed",
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 0,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "metadata": {},
        },
    ]

    report = _write_report(tmp_path, results)

    assert report["total_instances"] == 3
    assert report["completed_instances"] == 2

    # Total rates include the errored instance's expected tests.
    assert report["fail_to_pass_passed"] == 1
    assert report["fail_to_pass_total"] == 3
    assert report["fail_to_pass_rate_total"] == 1 / 3
    assert report["pass_to_pass_passed"] == 2
    assert report["pass_to_pass_total"] == 3
    assert report["pass_to_pass_rate_total"] == 2 / 3

    # Completed-only rates exclude the errored instance.
    assert report["fail_to_pass_passed_completed"] == 1
    assert report["fail_to_pass_total_completed"] == 2
    assert report["fail_to_pass_rate"] == 1 / 2
    assert report["pass_to_pass_passed_completed"] == 2
    assert report["pass_to_pass_total_completed"] == 2
    assert report["pass_to_pass_rate"] == 1.0

    summary_text = (tmp_path / "evaluation_summary.md").read_text(encoding="utf-8")
    assert "Fail-to-pass (total): **1/3**" in summary_text
    assert "Fail-to-pass (completed only): **1/2**" in summary_text
    assert "Pass-to-pass (total): **2/3**" in summary_text
    assert "Pass-to-pass (completed only): **2/2**" in summary_text


def test_pre_pull_called_with_unique_images_before_evaluation(tmp_path, monkeypatch):
    """Unique required images are pre-pulled once before workers start."""
    pred_path = tmp_path / "predictions.jsonl"
    sample_path = tmp_path / "sample.jsonl"

    pred_path.write_text(
        json.dumps({"instance_id": "inst-1", "patch": "diff --git\n"}) + "\n"
        + json.dumps({"instance_id": "inst-2", "patch": "diff --git\n"}) + "\n"
        + json.dumps({"instance_id": "inst-3", "patch": "diff --git\n"}) + "\n",
        encoding="utf-8",
    )
    sample_path.write_text(
        json.dumps({
            "instance_id": "inst-1",
            "dockerhub_tag": "tag-a",
            "fail_to_pass": [],
            "pass_to_pass": [],
        }) + "\n"
        + json.dumps({
            "instance_id": "inst-2",
            "dockerhub_tag": "tag-b",
            "fail_to_pass": [],
            "pass_to_pass": [],
        }) + "\n"
        + json.dumps({
            "instance_id": "inst-3",
            "dockerhub_tag": "tag-a",
            "fail_to_pass": [],
            "pass_to_pass": [],
        }) + "\n",
        encoding="utf-8",
    )

    captured_images: list[set[str]] = []

    def _fake_prepull(images, logger):
        captured_images.append(set(images))

    def _fake_evaluate_instance(instance, prediction, output_dir, logger, test_timeout):
        return {
            "instance_id": instance["instance_id"],
            "error": None,
            "overall_pass": True,
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 0,
            "pass_to_pass_passed": 0,
            "pass_to_pass_total": 0,
            "patch": prediction.get("patch", ""),
            "metadata": {},
        }

    monkeypatch.setattr("evaluate_predictions._prepull_images", _fake_prepull)
    monkeypatch.setattr("evaluate_predictions.evaluate_instance", _fake_evaluate_instance)
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "evaluate_predictions.py",
            "--predictions",
            str(pred_path),
            "--sample-file",
            str(sample_path),
            "--output-dir",
            str(tmp_path),
            "--workers",
            "2",
        ],
    )

    main()

    assert len(captured_images) == 1
    assert captured_images[0] == {
        "docker.io/jefzda/sweap-images:tag-a",
        "docker.io/jefzda/sweap-images:tag-b",
    }


def test_pre_pull_can_be_skipped(tmp_path, monkeypatch):
    """--skip-pre-pull disables the pre-pull step."""
    pred_path = tmp_path / "predictions.jsonl"
    sample_path = tmp_path / "sample.jsonl"
    pred_path.write_text(
        json.dumps({"instance_id": "inst-1", "patch": "diff --git\n"}) + "\n",
        encoding="utf-8",
    )
    sample_path.write_text(
        json.dumps({
            "instance_id": "inst-1",
            "dockerhub_tag": "tag-x",
            "fail_to_pass": [],
            "pass_to_pass": [],
        }) + "\n",
        encoding="utf-8",
    )

    called = False

    def _fake_prepull(images, logger):
        nonlocal called
        called = True

    monkeypatch.setattr("evaluate_predictions._prepull_images", _fake_prepull)
    monkeypatch.setattr(
        "evaluate_predictions.evaluate_instance",
        lambda *args, **kwargs: {
            "instance_id": args[0]["instance_id"],
            "error": None,
            "overall_pass": True,
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 0,
            "pass_to_pass_passed": 0,
            "pass_to_pass_total": 0,
            "patch": "",
            "metadata": {},
        },
    )
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "evaluate_predictions.py",
            "--predictions",
            str(pred_path),
            "--sample-file",
            str(sample_path),
            "--output-dir",
            str(tmp_path),
            "--skip-pre-pull",
        ],
    )

    main()
    assert called is False


def test_ensure_image_skips_pull_when_image_exists_locally(monkeypatch):
    """_ensure_image does not call pull when ``podman image exists`` succeeds."""
    monkeypatch.setattr(
        shutil,
        "which",
        lambda cmd: "/usr/bin/podman" if cmd == "podman" else None,
    )

    calls: list[list[str]] = []

    def _fake_run(cmd, **kwargs):
        calls.append(cmd)
        return subprocess.CompletedProcess(args=cmd, returncode=0, stdout="", stderr="")

    monkeypatch.setattr(subprocess, "run", _fake_run)
    logger = logging.getLogger("test_ensure_image")

    assert _ensure_image("docker.io/img:tag", logger) is True

    assert any(c[0] == "podman" and "image" in c and "exists" in c for c in calls)
    assert not any("pull" in c for c in calls)


def test_prepull_images_warns_on_failure_but_does_not_abort(monkeypatch, caplog):
    """A failed pre-pull is logged as a warning and does not raise."""
    monkeypatch.setattr(
        shutil,
        "which",
        lambda cmd: "/usr/bin/podman" if cmd == "podman" else None,
    )

    def _fake_run(cmd, **kwargs):
        return subprocess.CompletedProcess(
            args=cmd, returncode=1, stdout="", stderr="pull failed"
        )

    monkeypatch.setattr(subprocess, "run", _fake_run)
    caplog.set_level(logging.WARNING)

    logger = logging.getLogger("test_prepull")
    _prepull_images({"img:1", "img:2"}, logger)

    assert any("Pre-pull failed" in rec.message for rec in caplog.records)
    assert any("img:1" in rec.message or "img:2" in rec.message for rec in caplog.records)


def test_write_report_counts_compile_gate_skipped_when_toolchain_missing(
    tmp_path, monkeypatch
):
    """A compile-gate rejection caused by a missing host toolchain is counted separately."""
    patch_text = "diff --git a/a.py b/a.py\n--- a/a.py\n+++ b/a.py\n@@ -1 +1 @@\n-old\n+new\n"
    monkeypatch.setattr(
        "shutil.which",
        lambda name: None if name == "go" else "/fake/bin/" + name,
    )
    results = [
        {
            "instance_id": "go-skipped",
            "error": None,
            "overall_pass": False,
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 0,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "repo_language": "go",
            "metadata": {"compile_gate_rejected": True},
        },
        {
            "instance_id": "go-real-fail",
            "error": None,
            "overall_pass": False,
            "fail_to_pass_passed": 0,
            "fail_to_pass_total": 1,
            "pass_to_pass_passed": 0,
            "pass_to_pass_total": 1,
            "patch": patch_text,
            "repo_language": "go",
            "metadata": {"compile_gate_rejected": True},
        },
    ]
    # Second case: pretend go *is* present so it is a real rejection, not a skip.
    real_fail_seen = {"count": 0}

    def _which(name):
        if name == "go":
            real_fail_seen["count"] += 1
            return None if real_fail_seen["count"] == 1 else "/fake/go"
        return "/fake/bin/" + name

    monkeypatch.setattr("shutil.which", _which)

    report = _write_report(tmp_path, results)
    assert report["compile_gate_rejected_count"] == 2
    assert report["compile_gate_skipped_count"] == 1
    summary_text = (tmp_path / "evaluation_summary.md").read_text(encoding="utf-8")
    assert "Compile gate skipped (missing host toolchain): **1**" in summary_text
