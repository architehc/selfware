"""Tests for benchmark.py aggregation."""

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import benchmark


def test_parse_summary_reads_json_report(tmp_path):
    report = tmp_path / "evaluation_report.json"
    report.write_text(
        json.dumps(
            {
                "total_instances": 10,
                "completed_instances": 7,
                "errored_instances": 3,
                "overall_passed_instances": 2,
                "fail_to_pass_passed": 5,
                "fail_to_pass_total": 20,
                "pass_to_pass_passed": 90,
                "pass_to_pass_total": 100,
            }
        ),
        encoding="utf-8",
    )
    result = benchmark.parse_summary(report)
    assert result["total"] == 10
    assert result["completed"] == 7
    assert result["errored"] == 3
    assert result["passed"] == 2
    assert result["fail_to_pass"] == (5, 20)
    assert result["pass_to_pass"] == (90, 100)


def test_parse_summary_falls_back_to_markdown(tmp_path):
    summary = tmp_path / "evaluation_summary.md"
    summary.write_text(
        "Total instances: **10**\n"
        "Completed: **7**\n"
        "Errored: **3**\n"
        "Overall passed instances (errors counted as failed): **2/10** (20.00%)\n"
        "Overall passed instances (completed only): **2/7** (28.57%)\n"
        "Fail-to-pass (total): **5/20** (25.00%)\n"
        "Fail-to-pass (completed only): **5/14** (35.71%)\n"
        "Pass-to-pass (total): **90/100** (90.00%)\n"
        "Pass-to-pass (completed only): **90/97** (92.78%)\n",
        encoding="utf-8",
    )
    report = tmp_path / "evaluation_report.json"
    result = benchmark.parse_summary(report)
    # The fallback must capture the total-instance headline denominator.
    assert result["total"] == 10
    assert result["passed"] == 2
    assert result["fail_to_pass"] == (5, 20)
    assert result["pass_to_pass"] == (90, 100)


def test_compute_conservative_pass_rate_counts_errors_as_failed():
    assert benchmark.compute_conservative_pass_rate({"total": 10, "passed": 2}) == 0.2
    assert benchmark.compute_conservative_pass_rate({"total": 0, "passed": 0}) == 0.0
    assert benchmark.compute_conservative_pass_rate({"total": 10, "passed": 0}) == 0.0


def test_parse_summary_missing_files_returns_zeros(tmp_path):
    report = tmp_path / "evaluation_report.json"
    result = benchmark.parse_summary(report)
    assert result["total"] == 0
    assert result["passed"] == 0
    assert result["fail_to_pass"] == (0, 0)
