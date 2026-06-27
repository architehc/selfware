"""Tests for small_model_adapter test-command formatting."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from small_model_adapter import _format_test_command


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
