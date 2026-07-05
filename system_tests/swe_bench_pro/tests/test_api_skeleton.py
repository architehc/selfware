"""Tests for the qutebrowser API skeleton injector."""

import os
import sys

# Make sibling harness modules importable when running pytest directly.
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import logging
from pathlib import Path

import pytest

from api_skeleton import (
    extract_api_skeleton,
    inject_api_skeleton,
    should_inject_api_skeleton,
)


def _make_module(repo: Path, rel_path: str, content: str) -> Path:
    path = repo / rel_path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return path


def test_extract_api_skeleton_imports_classes_and_functions(tmp_path):
    """The skeleton should surface imports, classes, and top-level functions."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _make_module(
        repo,
        "src/widget.py",
        '''\
"""Widget helpers."""
import os
from collections.abc import Mapping
from ._internal import secret

class Widget(Mapping):
    def __init__(self, name: str) -> None:
        self.name = name

    def get_name(self) -> str:
        return self.name

def create_widget(name: str = "default") -> "Widget":
    return Widget(name)
''',
    )

    skeleton = extract_api_skeleton(repo, ["src/widget.py"])
    assert "### src/widget.py" in skeleton
    assert '"""Widget helpers."""' in skeleton
    assert "import os" in skeleton
    assert "from collections.abc import Mapping" in skeleton
    assert "from ._internal import secret" not in skeleton
    assert "class Widget(Mapping):" in skeleton
    assert "def __init__(self, name: str) -> None:" in skeleton
    assert "def get_name(self) -> str:" in skeleton
    assert "def create_widget(" in skeleton
    assert "default" in skeleton
    assert "Widget" in skeleton


def test_extract_api_skeleton_skips_private_names(tmp_path):
    """Private names (single underscore) should be omitted from non-test files."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _make_module(
        repo,
        "src/internal.py",
        '''\
"""Internal utilities."""

class _HiddenClass:
    def _hidden_method(self):
        pass

class PublicClass:
    def public_method(self):
        pass

def _private_func():
    pass

def public_func():
    pass
''',
    )

    skeleton = extract_api_skeleton(repo, ["src/internal.py"])
    assert "PublicClass" in skeleton
    assert "public_method" in skeleton
    assert "public_func" in skeleton
    assert "_HiddenClass" not in skeleton
    assert "_hidden_method" not in skeleton
    assert "_private_func" not in skeleton


def test_extract_api_skeleton_keeps_private_names_in_tests(tmp_path):
    """Test files may reference private helpers, so keep their names."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _make_module(
        repo,
        "tests/test_internal.py",
        '''\
"""Tests for internal utilities."""

def _helper():
    return 1

def test_public():
    assert _helper() == 1
''',
    )

    skeleton = extract_api_skeleton(repo, ["tests/test_internal.py"])
    assert "_helper" in skeleton
    assert "test_public" in skeleton


def test_extract_api_skeleton_truncates_docstring(tmp_path):
    """Only the first line of the module docstring is kept."""
    repo = tmp_path / "repo"
    repo.mkdir()
    long_line = "x" * 200
    _make_module(
        repo,
        "src/doc.py",
        f'"""{long_line}\n\nMore details here."""\n\ndef func():\n    pass\n',
    )

    skeleton = extract_api_skeleton(repo, ["src/doc.py"])
    assert long_line[:100] in skeleton
    assert "More details here" not in skeleton
    assert "..." in skeleton


def test_extract_api_skeleton_respects_budget(tmp_path):
    """The skeleton should stop once ``max_total_chars`` is reached."""
    repo = tmp_path / "repo"
    repo.mkdir()
    # Build a file large enough to exceed a small budget.
    functions = "\n\n".join(f"def func_{i}(x: int) -> int:\n    return x + {i}" for i in range(100))
    _make_module(repo, "src/big.py", f'"""Big module."""\n\n{functions}\n')

    skeleton = extract_api_skeleton(repo, ["src/big.py"], max_total_chars=500)
    assert len(skeleton) <= 550
    assert "(truncated" in skeleton or "..." in skeleton


def test_extract_api_skeleton_ignores_non_python_files(tmp_path):
    """Non-Python paths should be silently skipped."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _make_module(repo, "README.md", "# title\n")
    assert extract_api_skeleton(repo, ["README.md"]) == ""


def test_should_inject_api_skeleton_for_qutebrowser():
    """Injection should be enabled for qutebrowser instances."""
    assert should_inject_api_skeleton({"repo": "qutebrowser/qutebrowser"}) is True
    assert should_inject_api_skeleton({"repo": "qutebrowser__qutebrowser"}) is True


def test_should_inject_api_skeleton_for_other_repos():
    """Injection should stay off for repos that do not need it."""
    assert should_inject_api_skeleton({"repo": "psf/requests"}) is False
    assert should_inject_api_skeleton({"repo": "django/django"}) is False
    assert should_inject_api_skeleton({"repo": ""}) is False


def test_inject_api_skeleton_appends_block(tmp_path, caplog):
    """The prompt should gain a 'Current API skeleton' block for qutebrowser."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _make_module(
        repo,
        "src/widget.py",
        '''\
"""Widget module."""

class Widget:
    def __init__(self, name: str) -> None:
        self.name = name
''',
    )
    _make_module(
        repo,
        "tests/test_widget.py",
        '''\
"""Tests."""

def test_widget():
    pass
''',
    )

    instance = {
        "instance_id": "qutebrowser-1",
        "repo": "qutebrowser/qutebrowser",
        "problem_statement": "Fix the widget.",
        "selected_test_files_to_run": ["tests/test_widget.py"],
        "fail_to_pass": [],
    }
    logger = logging.getLogger("test-api-skeleton")
    original = "Fix the widget.\n\nUse relative paths."
    with caplog.at_level(logging.INFO, logger="test-api-skeleton"):
        result = inject_api_skeleton(repo, instance, original, logger)

    assert result.startswith(original)
    assert "Current API skeleton" in result
    assert "class Widget:" in result
    assert "def __init__(self, name: str) -> None:" in result
    assert "Injecting API skeleton" in caplog.text


def test_inject_api_skeleton_disabled_for_other_repos(tmp_path):
    """Non-qutebrowser prompts are returned unchanged."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _make_module(repo, "src/widget.py", "class Widget:\n    pass\n")
    instance = {
        "instance_id": "requests-1",
        "repo": "psf/requests",
        "selected_test_files_to_run": ["src/widget.py"],
    }
    original = "Fix the widget."
    logger = logging.getLogger("test-api-skeleton")
    assert inject_api_skeleton(repo, instance, original, logger) == original


def test_inject_api_skeleton_parses_pytest_nodeids(tmp_path):
    """Pytest nodeids in test lists should be mapped to file paths."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _make_module(
        repo,
        "tests/unit/test_widget.py",
        '''\
"""Widget tests."""

class TestWidget:
    def test_create(self):
        pass
''',
    )

    instance = {
        "instance_id": "qutebrowser-2",
        "repo": "qutebrowser/qutebrowser",
        "problem_statement": "Fix widget creation.",
        "selected_test_files_to_run": [
            "tests/unit/test_widget.py::TestWidget::test_create",
        ],
        "fail_to_pass": [],
    }
    logger = logging.getLogger("test-api-skeleton")
    original = "Fix widget creation."
    result = inject_api_skeleton(repo, instance, original, logger)
    assert "Current API skeleton" in result
    assert "tests/unit/test_widget.py" in result
    assert "class TestWidget:" in result


def test_inject_api_skeleton_respects_max_total_chars(tmp_path):
    """Passing a budget should cap the appended skeleton."""
    repo = tmp_path / "repo"
    repo.mkdir()
    functions = "\n\n".join(f"def func_{i}(x: int) -> int:\n    return x" for i in range(50))
    _make_module(repo, "src/big.py", f'"""Big."""\n\n{functions}\n')

    instance = {
        "instance_id": "qutebrowser-3",
        "repo": "qutebrowser/qutebrowser",
        "problem_statement": "Fix big.",
        "selected_test_files_to_run": ["src/big.py"],
    }
    logger = logging.getLogger("test-api-skeleton")
    original = "Fix big."
    result = inject_api_skeleton(repo, instance, original, logger, max_total_chars=300)
    skeleton_part = result.split("Current API skeleton")[1]
    assert len(skeleton_part) <= 400


def test_inject_api_skeleton_no_candidates_returns_original(tmp_path, caplog):
    """If no candidate Python files exist, the original prompt is returned."""
    repo = tmp_path / "repo"
    repo.mkdir()
    instance = {
        "instance_id": "qutebrowser-4",
        "repo": "qutebrowser/qutebrowser",
        "problem_statement": "Fix missing.",
        "selected_test_files_to_run": ["tests/missing.py::test_x"],
    }
    logger = logging.getLogger("test-api-skeleton")
    original = "Fix missing."
    with caplog.at_level(logging.INFO, logger="test-api-skeleton"):
        result = inject_api_skeleton(repo, instance, original, logger)
    assert result == original
    assert "no candidate Python files found" in caplog.text
