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
        "Overall passed instances: **2/10**\n"
        "Fail-to-pass: **5/20**\n"
        "Pass-to-pass: **90/100**\n",
        encoding="utf-8",
    )
    report = tmp_path / "evaluation_report.json"
    result = benchmark.parse_summary(report)
    assert result["total"] == 10
    assert result["passed"] == 2
    assert result["fail_to_pass"] == (5, 20)


def test_parse_summary_missing_files_returns_zeros(tmp_path):
    report = tmp_path / "evaluation_report.json"
    result = benchmark.parse_summary(report)
    assert result["total"] == 0
    assert result["passed"] == 0
    assert result["fail_to_pass"] == (0, 0)
