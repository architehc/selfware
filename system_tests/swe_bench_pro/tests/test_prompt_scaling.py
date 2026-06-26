"""Tests for P1.4 (adaptive Go package expansion) and P1.5 (prompt budgets / pass-to-pass noise cap)."""

import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from small_model_adapter import _context_budgets, _expand_to_package
from run_selfware import _cap_pass_to_pass, build_prompt


def test_context_budgets_scales_with_window():
    small_chars, small_lines = _context_budgets(8192)
    assert small_chars > 0
    assert small_lines > 0
    assert small_chars < 50_000

    default_chars, default_lines = _context_budgets(32768)
    assert default_chars > small_chars
    assert default_lines > small_lines

    huge_chars, huge_lines = _context_budgets(1_000_000)
    assert huge_chars <= 300_000
    assert huge_lines <= 10_000


def test_context_budgets_defaults_to_32k_for_invalid():
    chars, lines = _context_budgets(0)
    assert chars > 0
    assert lines > 0


def test_cap_pass_to_pass_limits_count():
    tests = [f"tests/test_{i}.py::test_x" for i in range(20)]
    capped = _cap_pass_to_pass(tests)
    assert len(capped) == 5


def test_cap_pass_to_pass_preserves_order():
    tests = ["a", "b", "c", "d", "e", "f"]
    capped = _cap_pass_to_pass(tests)
    assert capped == ["a", "b", "c", "d", "e"]


def test_cap_pass_to_pass_empty():
    assert _cap_pass_to_pass([]) == []


def test_build_prompt_caps_pass_to_pass():
    instance = {
        "repo": "test/repo",
        "base_commit": "abc123",
        "repo_language": "python",
        "problem_statement": "Widget returns wrong value",
        "requirements": "Make Widget.run return ok",
        "selected_test_files_to_run": ["tests/test_widget.py"],
        "fail_to_pass": ["tests/test_widget.py::test_widget_returns_ok"],
        "pass_to_pass": [f"tests/test_{i}.py::test_ok" for i in range(12)],
        "test_patch": "",
    }
    prompt = build_prompt(instance)
    # The prompt should list at most 5 pass-to-pass tests plus an ellipsis note.
    assert "Pass-to-pass:" in prompt
    listed = [
        line
        for line in prompt.splitlines()
        if line.startswith("- tests/test_") and "::test_ok" in line
    ]
    assert len(listed) <= 5


def test_expand_to_package_includes_go_siblings():
    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        pkg = repo / "pkg"
        pkg.mkdir()
        (pkg / "a.go").write_text("package pkg\n")
        (pkg / "b.go").write_text("package pkg\n")
        (pkg / "a_test.go").write_text("package pkg\n")
        (pkg / "README.md").write_text("docs\n")

        expanded = _expand_to_package(repo, ["pkg/a.go"])
        assert "pkg/a.go" in expanded
        assert "pkg/b.go" in expanded
        assert "pkg/a_test.go" not in expanded
        assert "pkg/README.md" not in expanded


def test_expand_to_package_respects_max_extra():
    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        pkg = repo / "pkg"
        pkg.mkdir()
        (pkg / "a.go").write_text("package pkg\n")
        for i in range(10):
            (pkg / f"extra_{i}.go").write_text("package pkg\n")

        expanded = _expand_to_package(repo, ["pkg/a.go"], max_extra=3)
        extras = [f for f in expanded if f.startswith("pkg/extra_")]
        assert len(extras) <= 3


def test_expand_to_package_leaves_non_go_files_untouched():
    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        (repo / "main.py").write_text("print('hi')\n")
        expanded = _expand_to_package(repo, ["main.py"])
        assert expanded == ["main.py"]
