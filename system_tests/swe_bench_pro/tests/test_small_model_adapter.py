"""Tests for small_model_adapter test-command formatting and repo matching."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from small_model_adapter import _format_test_command, _is_test_file, _repo_is


def test_format_test_command_nodebb_uses_npx_mocha():
    cmd = _format_test_command(
        "javascript",
        ["test/database/keys.js", "test/controllers.js"],
        repo="NodeBB/NodeBB",
    )
    assert "npx mocha" in cmd
    assert "npm test --" not in cmd
    assert "test/database/keys.js" in cmd
    assert "test/controllers.js" in cmd


def test_format_test_command_nodebb_instance_id_form():
    cmd = _format_test_command(
        "javascript",
        ["test/user/emails.js"],
        repo="NodeBB__NodeBB",
    )
    assert "npx mocha" in cmd


def test_format_test_command_qutebrowser_uses_offscreen_dbus():
    cmd = _format_test_command(
        "python",
        ["tests/unit/utils/test_urlutils.py"],
        repo="qutebrowser/qutebrowser",
    )
    assert "QT_QPA_PLATFORM=offscreen" in cmd
    assert "dbus-run-session" in cmd
    assert "python -m pytest" in cmd
    assert "tests/unit/utils/test_urlutils.py" in cmd


def test_format_test_command_qutebrowser_instance_id_form():
    cmd = _format_test_command(
        "python",
        [],
        repo="qutebrowser__qutebrowser",
    )
    assert "QT_QPA_PLATFORM=offscreen" in cmd
    assert "dbus-run-session" in cmd


def test_format_test_command_go_multiple_tests_no_comma():
    cmd = _format_test_command(
        "go",
        ["TestWiden/test_widen_hostnames", "TestFoo/bar"],
        repo="some/go-repo",
    )
    assert "," not in cmd
    assert "go test -run" in cmd
    assert "TestWiden" in cmd
    assert "TestFoo" in cmd
    assert "./..." in cmd


def test_repo_is_exact_owner_name_match():
    assert _repo_is("NodeBB/NodeBB", "nodebb/nodebb") is True
    assert _repo_is("NodeBB__NodeBB", "nodebb/nodebb") is True
    assert _repo_is("qutebrowser/qutebrowser", "qutebrowser/qutebrowser") is True


def test_repo_is_rejects_suffix_only_match():
    """A repo named ``foo/bar`` must not match the target ``bar``."""
    assert _repo_is("foo/bar", "bar") is False
    assert _repo_is("owner/some-repo", "some-repo") is False
    assert _repo_is("a/b/c", "b/c") is False


def test_repo_is_rejects_mismatched_owner():
    assert _repo_is("owner/repo", "other/repo") is False


def test_is_test_file_recognises_common_conventions():
    assert _is_test_file("foo/bar_test.go") is True
    assert _is_test_file("foo/bar_test.py") is True
    assert _is_test_file("foo/bar_test.js") is True
    assert _is_test_file("foo/bar_test.ts") is True
    assert _is_test_file("foo/bar_test.tsx") is True
    assert _is_test_file("tests/unit/test_widget.py") is True
    assert _is_test_file("test_widget.py") is True


def test_is_test_file_recognises_tests_directory_layouts():
    assert _is_test_file("test/database/keys.js") is True
    assert _is_test_file("tests/unit/utils/test_urlutils.py") is True


def test_is_test_file_rejects_source_files():
    assert _is_test_file("src/widget.js") is False
    assert _is_test_file("src/widget.ts") is False
    assert _is_test_file("src/widget.tsx") is False
    assert _is_test_file("src/widget.go") is False
    assert _is_test_file("src/widget.py") is False
