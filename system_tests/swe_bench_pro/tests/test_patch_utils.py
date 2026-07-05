"""Tests for SWE-bench Pro patch utilities."""

import logging
import os
import subprocess
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from patch_utils import (
    _filter_edit_blocks_to_source_files,
    _is_config_or_metadata,
    _is_rejected_file,
    _resolve_safe_path,
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
    apply_edits_with_missing,
    apply_model_response_with_missing,
    verify_edits_apply,
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


def test_apply_edits_with_missing_rejects_leftover_patch_markers(tmp_path):
    """A malformed REPLACE block that leaves markers in the result must fail."""
    target = tmp_path / "file.py"
    target.write_text("x = 1\n")
    response = (
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        "x = 1\n"
        "=======\n"
        "x = 2\n"
        "<<<<<<< SEARCH\n"
        "oops\n"
        ">>>>>>> REPLACE\n"
    )
    applied, missing, failed = apply_edits_with_missing(tmp_path, response, None)
    assert "file.py" in failed
    assert target.read_text() == "x = 1\n"
    assert not (applied and not failed)


def test_apply_edits_with_missing_records_partial_failure_with_markers(tmp_path):
    """Repro: first edit applies, second malformed edit is skipped and recorded."""
    first = tmp_path / "first.py"
    first.write_text("a = 1\n")
    second = tmp_path / "second.py"
    second.write_text("b = 1\n")
    response = (
        "### FILE: first.py\n"
        "<<<<<<< SEARCH\n"
        "a = 1\n"
        "=======\n"
        "a = 2\n"
        ">>>>>>> REPLACE\n"
        "### FILE: second.py\n"
        "<<<<<<< SEARCH\n"
        "b = 1\n"
        "=======\n"
        "b = 2\n"
        "<<<<<<< SEARCH\n"
        "oops\n"
        ">>>>>>> REPLACE\n"
    )
    applied, missing, failed = apply_edits_with_missing(tmp_path, response, None)
    assert applied is True
    assert "second.py" in failed
    assert "first.py" not in failed
    assert first.read_text() == "a = 2\n"
    assert second.read_text() == "b = 1\n"
    assert failed  # patch is not fully applied


def test_apply_diff_with_check_falls_back_to_patch_command(tmp_path):
    """If git apply rejects a diff, the applier should try patch -p1."""
    from patch_utils import _apply_diff_with_check

    repo = tmp_path / "repo"
    repo.mkdir()
    subprocess.run(["git", "-C", str(repo), "init", "-q"], check=True)
    (repo / "file.txt").write_text("line1\nline2\nline3\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    # A diff with trailing whitespace changes that git apply may reject
    # depending on config, but patch -p1 can usually apply.
    diff_text = (
        "diff --git a/file.txt b/file.txt\n"
        "--- a/file.txt\n"
        "+++ b/file.txt\n"
        "@@ -1,3 +1,3 @@\n"
        " line1\n"
        "-line2\n"
        "+line2 modified\n"
        " line3\n"
    )
    import shutil
    if shutil.which("patch") is None:
        pytest.skip("patch binary not available")

    assert _apply_diff_with_check(repo, diff_text, logging.getLogger("test")) is True
    assert "line2 modified" in (repo / "file.txt").read_text(encoding="utf-8")


def test_build_diff_fallback_prompt_allows_full_file_replacement():
    """When allow_full_file_replacement=True the prompt drops the whole-file ban."""
    from harness_recovery import build_diff_fallback_prompt

    prompt = build_diff_fallback_prompt(
        "Issue:\nfix bug\n",
        ["a.py"],
        "/tmp",
        max_chars=100,
        allow_full_file_replacement=True,
    )
    assert "Full-file replacement is allowed" in prompt
    assert "Do NOT rewrite whole files" not in prompt


def test_build_diff_fallback_prompt_has_strict_diff_contract():
    """The one-shot fallback prompt discourages malformed model diffs."""
    from harness_recovery import build_diff_fallback_prompt

    prompt = build_diff_fallback_prompt(
        "Issue:\nfix bug\n",
        ["a.py"],
        "/tmp",
        max_chars=100,
        allow_full_file_replacement=True,
    )
    assert "VALID DIFF CONTRACT:" in prompt
    assert "Do not invent fake SHA values" in prompt
    assert "Do not repeat a `diff --git` section" in prompt
    example = prompt.split("Valid minimal example format:", 1)[1]
    assert "\nindex " not in example


def test_build_diff_fallback_prompt_caps_snippet_length(tmp_path):
    """max_chars is respected in the generated prompt."""
    from harness_recovery import build_diff_fallback_prompt

    (tmp_path / "a.py").write_text("# line\n" * 50, encoding="utf-8")
    prompt = build_diff_fallback_prompt(
        "Issue:\nfix bug\n",
        ["a.py"],
        str(tmp_path),
        max_chars=50,
        allow_full_file_replacement=False,
    )
    assert "... (truncated due to char limit) ..." in prompt


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


# -----------------------------------------------------------------------------
# Security and robustness tests
# -----------------------------------------------------------------------------


def test_resolve_safe_path_rejects_traversal_and_absolute(tmp_path):
    repo = tmp_path / "repo"
    repo.mkdir()
    assert _resolve_safe_path(repo, "../outside.txt") is None
    assert _resolve_safe_path(repo, "foo/../../outside.txt") is None
    assert _resolve_safe_path(repo, "/etc/passwd") is None
    assert _resolve_safe_path(repo, "src/file.py\npackage main") is None
    assert _resolve_safe_path(repo, "src/file.py\0") is None
    assert _resolve_safe_path(repo, "a" * 513) is None
    assert _resolve_safe_path(repo, "") is None
    assert _resolve_safe_path(repo, ".") is None
    assert _resolve_safe_path(repo, "src/file.py") == (repo / "src/file.py").resolve()


def test_parse_edit_blocks_rejects_multiline_file_header(tmp_path):
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "file.py").write_text("old\n", encoding="utf-8")
    response = (
        "### FILE: file.py\n"
        "package main\n"
        "<<<<<<< SEARCH\n"
        "old\n"
        "=======\n"
        "new\n"
        ">>>>>>> REPLACE\n"
    )

    applied, missing, failed = apply_edits_with_missing(repo, response, None)

    assert applied is False
    assert failed
    assert (repo / "file.py").read_text(encoding="utf-8") == "old\n"


def test_apply_edits_rejects_path_traversal(tmp_path):
    """Paths that escape the repo directory must not be read or written."""
    repo = tmp_path / "repo"
    repo.mkdir()
    outside = tmp_path / "outside.txt"
    outside.write_text("secret\n", encoding="utf-8")

    response = (
        "### FILE: ../outside.txt\n"
        "<<<<<<< SEARCH\n"
        "secret\n"
        "=======\n"
        "changed\n"
        ">>>>>>> REPLACE\n"
    )
    assert apply_edits(repo, response, None) is False
    assert outside.read_text(encoding="utf-8") == "secret\n"

    response_abs = (
        "### FILE: /etc/passwd\n"
        "<<<<<<< SEARCH\n"
        "root\n"
        "=======\n"
        "other\n"
        ">>>>>>> REPLACE\n"
    )
    assert apply_edits(repo, response_abs, None) is False


def test_apply_edits_rejects_path_traversal_creation(tmp_path):
    repo = tmp_path / "repo"
    repo.mkdir()
    outside = tmp_path / "outside.txt"
    response = (
        "### FILE: ../../outside.txt\n"
        "<<<<<<< SEARCH\n"
        "=======\n"
        "created\n"
        ">>>>>>> REPLACE\n"
    )
    applied, missing, failed = apply_edits_with_missing(repo, response, None)
    assert outside.exists() is False
    assert not applied
    assert failed or missing


def test_verify_edits_apply_rejects_unsafe_paths(tmp_path):
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "file.py").write_text("x = 1\n", encoding="utf-8")
    response = (
        "### FILE: ../outside.txt\n"
        "<<<<<<< SEARCH\n"
        "x = 1\n"
        "=======\n"
        "x = 2\n"
        ">>>>>>> REPLACE\n"
    )
    assert verify_edits_apply(repo, response, None) is False


# -----------------------------------------------------------------------------
# Blank-line fuzzy replacement
# -----------------------------------------------------------------------------


def test_fuzzy_replace_blank_lines_maps_to_original_indices(tmp_path):
    """Extra blank lines must not shift the replacement location."""
    target = tmp_path / "file.py"
    target.write_text("start\n\na\n\n\nb\n", encoding="utf-8")
    response = (
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        "a\n"
        "\n"
        "b\n"
        "=======\n"
        "a\n"
        "CHANGED\n"
        "b\n"
        ">>>>>>> REPLACE\n"
    )
    assert apply_edits(tmp_path, response, None) is True
    assert target.read_text(encoding="utf-8") == "start\n\na\nCHANGED\nb"


# -----------------------------------------------------------------------------
# Source-file filtering
# -----------------------------------------------------------------------------


def test_is_rejected_file_uses_precise_matching():
    assert _is_rejected_file("src/test_helpers.py") is False
    assert _is_rejected_file("my.json_parser.py") is False
    assert _is_rejected_file("test_widget.py") is True
    assert _is_rejected_file("tests/foo.py") is True
    assert _is_rejected_file("docs/readme.md") is True
    assert _is_rejected_file("README.md") is True
    assert _is_rejected_file("widget.py.bak") is True
    assert _is_rejected_file("widget.py.orig") is True


def test_is_config_or_metadata_uses_exact_suffix_and_basename():
    assert _is_config_or_metadata("package.json") is True
    assert _is_config_or_metadata("config/settings.yaml") is True
    assert _is_config_or_metadata("pyproject.toml") is True
    assert _is_config_or_metadata("my.json_parser.py") is False
    assert _is_config_or_metadata("src/parser.py") is False


def test_filter_patch_to_source_files_keeps_legitimate_source_files():
    diff = """\
diff --git a/src/test_helpers.py b/src/test_helpers.py
--- a/src/test_helpers.py
+++ b/src/test_helpers.py
@@ -1 +1 @@
-old
+new

diff --git a/my.json_parser.py b/my.json_parser.py
--- a/my.json_parser.py
+++ b/my.json_parser.py
@@ -1 +1 @@
-old
+new

diff --git a/widget.py.bak b/widget.py.bak
--- a/widget.py.bak
+++ b/widget.py.bak
@@ -1 +1 @@
-old
+new

diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old
+new
"""
    result = filter_patch_to_source_files(diff)
    assert "src/test_helpers.py" in result
    assert "my.json_parser.py" in result
    assert "widget.py.bak" not in result
    assert "README.md" not in result


def test_filter_edit_blocks_to_source_files_drops_non_source():
    response = (
        "### FILE: src/widget.py\n"
        "<<<<<<< SEARCH\n"
        "x = 1\n"
        "=======\n"
        "x = 2\n"
        ">>>>>>> REPLACE\n"
        "### FILE: test_widget.py\n"
        "<<<<<<< SEARCH\n"
        "y = 1\n"
        "=======\n"
        "y = 2\n"
        ">>>>>>> REPLACE\n"
        "### FILE: README.md\n"
        "<<<<<<< SEARCH\n"
        "old\n"
        "=======\n"
        "new\n"
        ">>>>>>> REPLACE\n"
    )
    filtered = _filter_edit_blocks_to_source_files(response)
    assert "src/widget.py" in filtered
    assert "test_widget.py" not in filtered
    assert "README.md" not in filtered


def test_apply_model_response_filters_edits_to_source_files(tmp_path):
    """Raw SEARCH/REPLACE edits on test/doc files must be ignored before apply."""
    repo = tmp_path / "repo"
    repo.mkdir()
    source = repo / "src" / "widget.py"
    source.parent.mkdir()
    source.write_text("x = 1\n", encoding="utf-8")
    test_file = repo / "test_widget.py"
    test_file.write_text("y = 1\n", encoding="utf-8")
    readme = repo / "README.md"
    readme.write_text("old\n", encoding="utf-8")

    response = (
        "### FILE: src/widget.py\n"
        "<<<<<<< SEARCH\n"
        "x = 1\n"
        "=======\n"
        "x = 2\n"
        ">>>>>>> REPLACE\n"
        "### FILE: test_widget.py\n"
        "<<<<<<< SEARCH\n"
        "y = 1\n"
        "=======\n"
        "y = 2\n"
        ">>>>>>> REPLACE\n"
        "### FILE: README.md\n"
        "<<<<<<< SEARCH\n"
        "old\n"
        "=======\n"
        "new\n"
        ">>>>>>> REPLACE\n"
    )
    applied, unapplied = apply_model_response_with_missing(repo, response, None)
    assert applied is True
    assert source.read_text(encoding="utf-8") == "x = 2\n"
    assert test_file.read_text(encoding="utf-8") == "y = 1\n"
    assert readme.read_text(encoding="utf-8") == "old\n"


# -----------------------------------------------------------------------------
# Overlapping edit blocks
# -----------------------------------------------------------------------------


def test_apply_edits_rejects_overlapping_blocks(tmp_path):
    target = tmp_path / "file.py"
    target.write_text("a\nb\nc\n", encoding="utf-8")
    response = (
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        "a\n"
        "b\n"
        "=======\n"
        "x\n"
        ">>>>>>> REPLACE\n"
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        "b\n"
        "=======\n"
        "y\n"
        ">>>>>>> REPLACE\n"
    )
    assert apply_edits(tmp_path, response, None) is False
    assert target.read_text(encoding="utf-8") == "a\nb\nc\n"


def test_apply_edits_allows_disjoint_blocks(tmp_path):
    target = tmp_path / "file.py"
    target.write_text("a\nb\nc\n", encoding="utf-8")
    response = (
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        "a\n"
        "=======\n"
        "x\n"
        ">>>>>>> REPLACE\n"
        "### FILE: file.py\n"
        "<<<<<<< SEARCH\n"
        "c\n"
        "=======\n"
        "z\n"
        ">>>>>>> REPLACE\n"
    )
    assert apply_edits(tmp_path, response, None) is True
    assert target.read_text(encoding="utf-8") == "x\nb\nz\n"
