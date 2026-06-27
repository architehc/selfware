"""Tests for P0.3 (target API injection) and P0.4 (focused test oracle)."""

import os
import sys

# Make sibling harness modules importable when running pytest directly.
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import pytest

from small_model_adapter import (
    _build_focused_test_oracle,
    _format_target_api_section,
    _parse_interface,
)
from run_selfware import build_prompt


INTERFACE_TEXT = """\
Type: Function

Name: hide_qt_warning

Path: qutebrowser/utils/qtlog.py

Input: pattern: str, logger: str = 'qt'

Output: context manager (Iterator[None])

Description: Temporarily suppresses Qt log warnings.

Type: Class

Name: QtWarningFilter

Path: qutebrowser/utils/qtlog.py

Public API: logging.Filter subclass with constructor (pattern: str)

Description: Logging filter used to hide Qt warnings.
"""

PYTHON_TEST_PATCH = """\
diff --git a/tests/test_widget.py b/tests/test_widget.py
--- a/tests/test_widget.py
+++ b/tests/test_widget.py
@@ -10,0 +11,6 @@ class WidgetTests:
+    def test_widget_returns_ok(self):
+        w = Widget()
+        result = w.run()
+        assert result == "ok", f"unexpected result {result}"
+
"""

GO_TEST_PATCH = """\
diff --git a/foo/bar_test.go b/foo/bar_test.go
--- a/foo/bar_test.go
+++ b/foo/bar_test.go
@@ -1,5 +1,15 @@ package foo
+
+func TestBar(t *testing.T) {
+    got := Bar()
+    want := 42
+    if got != want {
+        t.Errorf("Bar() = %d, want %d", got, want)
+    }
+}
"""


def test_parse_interface_extracts_entries():
    entries = _parse_interface(INTERFACE_TEXT)
    assert len(entries) == 2
    assert entries[0]["type"] == "Function"
    assert entries[0]["name"] == "hide_qt_warning"
    assert entries[0]["path"] == "qutebrowser/utils/qtlog.py"
    assert "pattern: str" in entries[0]["input"]
    assert entries[1]["type"] == "Class"
    assert entries[1]["name"] == "QtWarningFilter"


def test_parse_interface_returns_empty_for_none():
    assert _parse_interface(None) == []
    assert _parse_interface("") == []


def test_format_target_api_section_includes_names_and_paths():
    section = _format_target_api_section(INTERFACE_TEXT)
    assert "hide_qt_warning" in section
    assert "QtWarningFilter" in section
    assert "qutebrowser/utils/qtlog.py" in section


def test_format_target_api_section_empty_for_missing_interface():
    assert _format_target_api_section(None) == ""
    assert _format_target_api_section("") == ""


def test_build_focused_test_oracle_extracts_python_test():
    oracle = _build_focused_test_oracle(PYTHON_TEST_PATCH)
    assert "tests/test_widget.py" in oracle
    assert "def test_widget_returns_ok" in oracle
    assert "assert result == \"ok\"" in oracle
    assert "unexpected result" in oracle


def test_build_focused_test_oracle_extracts_go_test():
    oracle = _build_focused_test_oracle(GO_TEST_PATCH)
    assert "foo/bar_test.go" in oracle
    assert "func TestBar" in oracle
    assert "Bar() = %d, want %d" in oracle


def test_build_focused_test_oracle_omits_non_test_files():
    patch = "diff --git a/src/main.py b/src/main.py\n--- a/src/main.py\n+++ b/src/main.py\n@@ -1 +1,2 @@\n+def helper():\n+    pass\n"
    oracle = _build_focused_test_oracle(patch)
    assert oracle == ""


def test_build_focused_test_oracle_truncates_large_output():
    # A single long test function exceeds the small max_chars budget.
    long_line = '+        assert result == "ok", "unexpected result; this line is intentionally long to force truncation"\n'
    big_patch = PYTHON_TEST_PATCH.replace(
        '+        assert result == "ok", f"unexpected result {result}"\n',
        long_line * 25,
    )
    oracle = _build_focused_test_oracle(big_patch, max_chars=500)
    assert oracle.endswith("... (truncated) ...")


def test_build_prompt_includes_target_api_section():
    instance = {
        "repo": "test/repo",
        "base_commit": "abc123",
        "repo_language": "python",
        "problem_statement": "Qt warnings are noisy",
        "requirements": "Suppress matching Qt warnings",
        "interface": INTERFACE_TEXT,
        "selected_test_files_to_run": ["tests/test_qtlog.py"],
        "fail_to_pass": ["tests/test_qtlog.py::test_hide_warning"],
        "pass_to_pass": [],
        "test_patch": PYTHON_TEST_PATCH,
    }
    prompt = build_prompt(instance)
    assert "Target API" in prompt
    assert "hide_qt_warning" in prompt
    assert "QtWarningFilter" in prompt


def test_build_prompt_includes_focused_test_oracle():
    instance = {
        "repo": "test/repo",
        "base_commit": "abc123",
        "repo_language": "python",
        "problem_statement": "Widget returns wrong value",
        "requirements": "Make Widget.run return ok",
        "selected_test_files_to_run": ["tests/test_widget.py"],
        "fail_to_pass": ["tests/test_widget.py::test_widget_returns_ok"],
        "pass_to_pass": [],
        "test_patch": PYTHON_TEST_PATCH,
    }
    prompt = build_prompt(instance)
    assert "Focused test oracle" in prompt
    assert "tests/test_widget.py" in prompt
    assert "def test_widget_returns_ok" in prompt


NODEBB_TEST_PATCH = """\
diff --git a/test/database/keys.js b/test/database/keys.js
--- a/test/database/keys.js
+++ b/test/database/keys.js
@@ -10,6 +10,10 @@ describe('database keys', () => {
+
+    it('should return a key when requested', (done) => {
+        db.getObject('someKey', (err, data) => {
+            assert.strictEqual(data.foo, 'bar');
+            done();
+        });
+    });
+
 });
"""

INTERFACE_PATHFILE_TEXT = """\
Type: Function

Name: open_url

Pathfile: qutebrowser/browser/commands.py

Input: url: str

Description: Open the given URL.
"""

INTERFACE_LOCATION_TEXT = """\
Type: Function

Name: close_tab

Location: qutebrowser/browser/commands.py

Description: Close the current tab.
"""


def test_build_focused_test_oracle_extracts_nodebb_js_test():
    oracle = _build_focused_test_oracle(NODEBB_TEST_PATCH)
    assert "test/database/keys.js" in oracle
    assert "it('should return a key when requested')" in oracle
    assert "assert.strictEqual(data.foo, 'bar')" in oracle


def test_parse_interface_accepts_pathfile_alias():
    entries = _parse_interface(INTERFACE_PATHFILE_TEXT)
    assert len(entries) == 1
    assert entries[0]["name"] == "open_url"
    assert entries[0]["path"] == "qutebrowser/browser/commands.py"


def test_parse_interface_accepts_location_alias():
    entries = _parse_interface(INTERFACE_LOCATION_TEXT)
    assert len(entries) == 1
    assert entries[0]["name"] == "close_tab"
    assert entries[0]["path"] == "qutebrowser/browser/commands.py"


def test_format_target_api_section_includes_pathfile_path():
    section = _format_target_api_section(INTERFACE_PATHFILE_TEXT)
    assert "open_url" in section
    assert "qutebrowser/browser/commands.py" in section
