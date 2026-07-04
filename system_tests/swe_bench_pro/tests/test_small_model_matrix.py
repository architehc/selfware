"""Tests for the small-model matrix runner."""

import json
import os
import sys
import types

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import run_small_model_matrix as matrix


def test_apply_evaluation_report_makes_success_mean_overall_pass():
    results = [
        {
            "model_profile": "test-model",
            "instance_id": "inst-1",
            "harness_success": True,
            "success": True,
        }
    ]
    report = {
        "per_instance": [
            {
                "instance_id": "inst-1",
                "overall_pass": False,
                "error": None,
                "fail_to_pass_passed": 1,
                "fail_to_pass_total": 2,
                "pass_to_pass_passed": 3,
                "pass_to_pass_total": 3,
            }
        ]
    }

    matrix.apply_evaluation_report(results, "test-model", report)

    assert results[0]["harness_success"] is True
    assert results[0]["success"] is False
    assert results[0]["overall_pass"] is False
    assert results[0]["fail_to_pass_passed"] == 1
    assert results[0]["pass_to_pass_total"] == 3


def test_write_profile_eval_inputs_combines_latest_predictions(tmp_path):
    output_base = tmp_path / "matrix"
    instance_dir = output_base / "test-model" / "inst-1"
    instance_dir.mkdir(parents=True)
    with open(instance_dir / "predictions.jsonl", "w", encoding="utf-8") as f:
        f.write(json.dumps({"instance_id": "inst-1", "patch": "old"}) + "\n")
        f.write(json.dumps({"instance_id": "inst-1", "patch": "new"}) + "\n")

    args = types.SimpleNamespace(output_base=str(output_base))
    sample_rows = {
        "inst-1": {"instance_id": "inst-1", "pass_to_pass": ["p"]},
        "inst-2": {"instance_id": "inst-2", "pass_to_pass": ["p"]},
    }

    predictions_path, sample_path, missing = matrix.write_profile_eval_inputs(
        args,
        "test-model",
        ["inst-1", "inst-2"],
        sample_rows,
    )

    predictions = [
        json.loads(line)
        for line in predictions_path.read_text(encoding="utf-8").splitlines()
    ]
    samples = [
        json.loads(line)
        for line in sample_path.read_text(encoding="utf-8").splitlines()
    ]
    assert predictions == [{"instance_id": "inst-1", "patch": "new"}]
    assert [row["instance_id"] for row in samples] == ["inst-1", "inst-2"]
    assert missing == ["inst-2"]


def test_write_summary_separates_harness_and_solve_success(tmp_path):
    args = types.SimpleNamespace(
        sample_file="sample.jsonl",
        compact=False,
        small_model=True,
        retry_failures=True,
        force_edit=False,
        evaluate=True,
        few_shot_examples=None,
    )
    results = [
        {
            "model_profile": "test-model",
            "instance_id": "inst-1",
            "output_dir": "out",
            "returncode": 0,
            "patch_chars": 10,
            "patch_lines": 1,
            "harness_success": True,
            "success": False,
            "overall_pass": False,
            "duration_seconds": 1.0,
            "command": "cmd",
        }
    ]

    matrix.write_summary(tmp_path, results, args)

    summary = json.loads((tmp_path / "summary.json").read_text(encoding="utf-8"))
    assert summary["harness_successful_runs"] == 1
    assert summary["successful_runs"] == 0
    assert summary["solved_runs"] == 0
    assert summary["evaluated_runs"] == 1
