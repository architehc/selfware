"""Tests for SWE-bench Pro patch utilities."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from patch_utils import (
    extract_partial_diff,
    filter_patch_excluding_paths,
    filter_patch_to_source_files,
    paths_from_patch,
)


COMPLETE_TWO_HUNK_DIFF = """\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,7 +10,7 @@ fn old_name() -> i32 {
-    1
+    2
 }

 fn unchanged() {}
@@ -25,3 +25,4 @@ fn another() {
     3
+    4
 }
"""

TRAILING_INCOMPLETE_HUNK_DIFF = """\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,7 +10,7 @@ fn old_name() -> i32 {
-    1
+    2
 }

@@ -25,3 +25,4 @@ fn another() {
"""

MIXED_SOURCE_AND_TEST_DIFF = """\
diff --git a/src/widget.py b/src/widget.py
--- a/src/widget.py
+++ b/src/widget.py
@@ -1,3 +1,3 @@
 def widget():
-    return 1
+    return 2

diff --git a/test_widget.py b/test_widget.py
--- /dev/null
+++ b/test_widget.py
@@ -0,0 +1,3 @@
+def test_widget():
+    assert widget() == 2
"""


def test_extract_partial_diff_keeps_complete_hunks():
    result = extract_partial_diff(COMPLETE_TWO_HUNK_DIFF)
    assert result is not None
    assert result.count("@@") == 4  # two hunks, each header has two @@
    assert "fn old_name" in result
    assert "fn another" in result


def test_extract_partial_diff_drops_incomplete_trailing_hunk():
    result = extract_partial_diff(TRAILING_INCOMPLETE_HUNK_DIFF)
    assert result is not None
    assert result.count("@@") == 2  # only first hunk kept
    assert "fn old_name" in result
    assert "fn another" not in result


def test_extract_partial_diff_returns_none_for_no_hunks():
    assert extract_partial_diff("just some prose\n") is None


def test_extract_partial_diff_keeps_last_hunk_without_final_newline():
    # The diff ends immediately after the last hunk body; it is complete.
    diff = COMPLETE_TWO_HUNK_DIFF.rstrip("\n")
    result = extract_partial_diff(diff)
    assert result is not None
    assert "fn another" in result


def test_paths_from_patch_extracts_target_paths():
    patch = """\
diff --git a/src/a.py b/src/a.py
--- a/src/a.py
+++ b/src/a.py
@@ -1 +1 @@
-old
+new

diff --git a/b/c.go b/b/c.go
new file mode 100644
--- /dev/null
+++ b/b/c.go
@@ -0,0 +1 @@
+package c
"""
    assert paths_from_patch(patch) == {"src/a.py", "b/c.go"}


def test_paths_from_patch_returns_empty_set_for_empty_patch():
    assert paths_from_patch("") == set()


def test_filter_patch_excluding_paths_drops_excluded_hunks():
    result = filter_patch_excluding_paths(
        MIXED_SOURCE_AND_TEST_DIFF, {"test_widget.py"}
    )
    assert "src/widget.py" in result
    assert "test_widget.py" not in result


def test_filter_patch_excluding_paths_returns_unchanged_when_no_exclusions():
    result = filter_patch_excluding_paths(MIXED_SOURCE_AND_TEST_DIFF, set())
    assert "src/widget.py" in result
    assert "test_widget.py" in result


def test_filter_patch_to_source_files_excludes_test_patch_paths():
    # The test file from the official test patch must be dropped even though it
    # is a source-like path, while the legitimate source edit is kept.
    result = filter_patch_to_source_files(
        MIXED_SOURCE_AND_TEST_DIFF,
        test_patch_paths={"test_widget.py"},
    )
    assert "src/widget.py" in result
    assert "test_widget.py" not in result


def test_filter_patch_to_source_files_drops_test_files_by_default():
    result = filter_patch_to_source_files(MIXED_SOURCE_AND_TEST_DIFF)
    # Without test_patch_paths, the generic test-file filter still drops it.
    assert "test_widget.py" not in result


CONFIG_AND_SOURCE_DIFF = """\
diff --git a/src/widget.py b/src/widget.py
--- a/src/widget.py
+++ b/src/widget.py
@@ -1,3 +1,3 @@
 def widget():
-    return 1
+    return 2

diff --git a/package.json b/package.json
--- a/package.json
+++ b/package.json
@@ -1,3 +1,3 @@
 {
-  "version": "1.0.0"
+  "version": "1.0.1"
 }

diff --git a/pyproject.toml b/pyproject.toml
--- a/pyproject.toml
+++ b/pyproject.toml
@@ -1,2 +1,2 @@
 [project]
-name = "old"
+name = "new"

diff --git a/config/settings.yaml b/config/settings.yaml
--- a/config/settings.yaml
+++ b/config/settings.yaml
@@ -1,2 +1,2 @@
-old: value
+new: value

diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,2 +1,2 @@
-Old
+New

diff --git a/test_widget.py b/test_widget.py
--- /dev/null
+++ b/test_widget.py
@@ -0,0 +1,3 @@
+def test_widget():
+    assert widget() == 2
"""


def test_filter_patch_keeps_config_files_in_official_fix_paths():
    result = filter_patch_to_source_files(
        CONFIG_AND_SOURCE_DIFF,
        official_fix_paths={"package.json", "pyproject.toml", "config/settings.yaml"},
    )
    assert "src/widget.py" in result
    assert "package.json" in result
    assert "pyproject.toml" in result
    assert "config/settings.yaml" in result
    assert "README.md" not in result
    assert "test_widget.py" not in result


def test_filter_patch_drops_config_files_not_in_official_fix_paths():
    result = filter_patch_to_source_files(
        CONFIG_AND_SOURCE_DIFF,
        official_fix_paths={"package.json"},
    )
    assert "src/widget.py" in result
    assert "package.json" in result
    assert "pyproject.toml" not in result
    assert "config/settings.yaml" not in result
    assert "README.md" not in result
    assert "test_widget.py" not in result


def test_filter_patch_drops_docs_and_tests_even_when_in_official_fix_paths():
    # Docs and tests should never be allowed, even if the official patch touches
    # them (official docs/test changes are not part of the fix the model should
    # reproduce).
    result = filter_patch_to_source_files(
        CONFIG_AND_SOURCE_DIFF,
        official_fix_paths={"README.md", "test_widget.py"},
    )
    assert "src/widget.py" in result
    assert "README.md" not in result
    assert "test_widget.py" not in result


# -----------------------------------------------------------------------------
# SEARCH/REPLACE edit-block normalization tests
# -----------------------------------------------------------------------------

from patch_utils import (
    _normalize_edit_response,
    _strip_line_number_gutter,
    apply_edits,
)


def test_strip_line_number_gutter_strips_space_only_gutter():
    assert _strip_line_number_gutter("  1  hello") == "hello"
    assert _strip_line_number_gutter("12 hello") == "hello"
    assert _strip_line_number_gutter("123 | still here") == "still here"
    assert _strip_line_number_gutter("4: content") == "content"
    assert _strip_line_number_gutter("5. content") == "content"


def test_normalize_edit_response_canonicalizes_headers_and_markers():
    raw = (
        "### file: foo.py\n"
        "<<<<<<< search\n"
        "old\n"
        "=======\n"
        "new\n"
        ">>>>>>> replace\n"
    )
    normalized = _normalize_edit_response(raw)
    assert "### FILE: foo.py" in normalized
    assert "<<<<<<< SEARCH" in normalized
    assert "=======\n" in normalized
    assert ">>>>>>> REPLACE" in normalized
    assert "<<<<<<< search" not in normalized
    assert ">>>>>>> replace" not in normalized


def test_normalize_edit_response_handles_path_header_variants():
    for header, path in (
        ("### Path: bar.py", "bar.py"),
        ("### PATH bar.py", "bar.py"),
        ("### file baz.py", "baz.py"),
    ):
        normalized = _normalize_edit_response(f"{header}\n<<<<<<< SEARCH\n\n=======\n\n>>>>>>> REPLACE\n")
        assert normalized.startswith("### FILE:") and path in normalized, header


def test_normalize_edit_response_strips_gutters_inside_blocks_only():
    raw = (
        "### FILE: x.py\n"
        "<<<<<<< SEARCH\n"
        " 1 | keep\n"
        "2: indent\n"
        "3. stuff\n"
        "=======\n"
        " 1 | new\n"
        ">>>>>>> REPLACE\n"
    )
    normalized = _normalize_edit_response(raw)
    assert " 1 | keep" not in normalized
    assert "keep" in normalized
    assert "indent" in normalized
    assert "stuff" in normalized
    assert "new" in normalized


def test_apply_edits_lowercase_search_replace_markers(tmp_path):
    target = tmp_path / "file.py"
    target.write_text("def foo():\n    return 1\n")
    response = (
        "### file: file.py\n"
        "<<<<<<< search\n"
        "def foo():\n"
        "    return 1\n"
        "=======\n"
        "def foo():\n"
        "    return 2\n"
        ">>>>>>> replace\n"
    )
    assert apply_edits(tmp_path, response, None) is True
    assert target.read_text() == "def foo():\n    return 2\n"


def test_apply_edits_numbered_line_gutters(tmp_path):
    target = tmp_path / "file.py"
    target.write_text("def foo():\n    return 1\n")
    response = (
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        " 1 | def foo():\n"
        " 2 |     return 1\n"
        "=======\n"
        " 1 | def foo():\n"
        " 2 |     return 2\n"
        ">>>>>>> REPLACE\n"
    )
    assert apply_edits(tmp_path, response, None) is True
    assert target.read_text() == "def foo():\n    return 2\n"


def test_apply_edits_mixed_file_header_variants(tmp_path):
    target = tmp_path / "file.py"
    for header in ("### file: file.py", "### FILE file.py", "### Path: file.py", "### path file.py"):
        target.write_text("x = 1\n")
        response = (
            f"{header}\n"
            "<<<<<<< SEARCH\n"
            "x = 1\n"
            "=======\n"
            "x = 2\n"
            ">>>>>>> REPLACE\n"
        )
        assert apply_edits(tmp_path, response, None) is True, header
        assert target.read_text() == "x = 2\n", header


def test_apply_edits_only_applies_after_normalization(tmp_path):
    """A block with non-canonical headers, gutters, trailing spaces and CRLF."""
    target = tmp_path / "file.py"
    target.write_text("def foo():\n    return 1\n")
    response = (
        "### Path: file.py\r\n"
        "<<<<<<< search\r\n"
        " 1 | def foo():   \r\n"
        " 2 |     return 1\r\n"
        "=======\r\n"
        " 1 | def foo():\r\n"
        " 2 |     return 2\r\n"
        ">>>>>>> replace\r\n"
    )
    assert apply_edits(tmp_path, response, None) is True
    assert target.read_text() == "def foo():\n    return 2\n"


def test_apply_edits_reports_failed_search_block(tmp_path):
    """A SEARCH block that does not match must fail loudly, not silently succeed."""
    target = tmp_path / "file.py"
    target.write_text("def foo():\n    return 1\n")
    response = (
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        "def not_present():\n"
        "    return 1\n"
        "=======\n"
        "def not_present():\n"
        "    return 2\n"
        ">>>>>>> REPLACE\n"
    )
    assert apply_edits(tmp_path, response, None) is False
    assert target.read_text() == "def foo():\n    return 1\n"


def test_strip_line_number_gutter_handles_blank_content():
    """A guttered blank line is reduced to an empty string."""
    from patch_utils import _strip_line_number_gutter

    assert _strip_line_number_gutter("  1 | ") == ""
    assert _strip_line_number_gutter("42:  ") == ""
    assert _strip_line_number_gutter("  5 |") == ""
    assert _strip_line_number_gutter("7.") == ""


def test_apply_edits_strips_blank_line_gutters(tmp_path):
    """SEARCH/REPLACE blocks may include guttered blank lines."""
    target = tmp_path / "file.py"
    target.write_text("a\n\nb\n")
    response = (
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        "a\n"
        "  2 |\n"
        "b\n"
        "=======\n"
        "a\n"
        "  2 |\n"
        "changed\n"
        ">>>>>>> REPLACE\n"
    )
    assert apply_edits(tmp_path, response, None) is True
    assert target.read_text() == "a\n\nchanged\n"
