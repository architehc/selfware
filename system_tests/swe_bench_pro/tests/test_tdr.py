"""Tests for the TDR evaluation harness."""

import logging
import os
import sys
import types
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import run_selfware
import tdr


def _minimal_instance() -> dict:
    return {
        "instance_id": "test-tdr-1",
        "base_commit": "abc123",
        "selected_test_files_to_run": ["tests/test_foo.py"],
        "before_repo_set_cmd": "",
        "repo_language": "go",
        "fail_to_pass": [],
        "pass_to_pass": [],
    }


def test_entryscript_has_parser_fallback():
    script = tdr._build_entryscript(_minimal_instance())
    # Fallback after parser if output.json missing.
    assert "python /workspace/parser.py" in script
    assert "if [ ! -f /workspace/output.json ]; then" in script
    assert "echo '{\"tests\": []}' > /workspace/output.json" in script
    # Fallback must come after the parser invocation.
    parser_idx = script.index("python /workspace/parser.py")
    fallback_idx = script.index("if [ ! -f /workspace/output.json ]; then")
    assert fallback_idx > parser_idx


def test_entryscript_detects_no_op_patch():
    script = tdr._build_entryscript(_minimal_instance())
    assert "git diff --name-only HEAD" in script
    assert "PATCH_NO_OP" in script


def test_compile_check_runs_after_patch_application(tmp_path, monkeypatch):
    """The compile check must run after the entryscript has applied the patch."""
    log_dir = tmp_path / "logs"
    log_dir.mkdir()

    root = tmp_path / "swebench_pro"
    (root / "run_scripts" / "test-tdr-1").mkdir(parents=True)
    (root / "run_scripts" / "test-tdr-1" / "run_script.sh").write_text(
        "#!/bin/bash\n", encoding="utf-8"
    )
    (root / "run_scripts" / "test-tdr-1" / "parser.py").write_text("", encoding="utf-8")
    monkeypatch.setattr(tdr, "SWEBENCH_PRO_ROOT", root)

    calls = []

    def fake_podman(*args, **kwargs):
        calls.append(args)

        class FakeProc:
            returncode = 0
            stdout = ""
            stderr = ""

        return FakeProc()

    monkeypatch.setattr(run_selfware, "podman", fake_podman)
    monkeypatch.setattr(run_selfware, "pull_image", lambda image, logger: True)
    monkeypatch.setattr(run_selfware, "start_container", lambda image, container, logger, timeout: True)
    monkeypatch.setattr(run_selfware, "copy_into_container", lambda src, dst, container, logger: True)

    # Pre-stage output artifacts that the harness will read back.
    (log_dir / "cont.tdr.0.output.json").write_text('{"tests": []}', encoding="utf-8")
    (log_dir / "cont.tdr.0.stdout.log").write_text("", encoding="utf-8")
    (log_dir / "cont.tdr.0.stderr.log").write_text("", encoding="utf-8")
    (log_dir / "cont.tdr.0.patch_apply.log").write_text("", encoding="utf-8")
    (log_dir / "cont.tdr.0.patch_apply_status.txt").write_text("0", encoding="utf-8")

    args = types.SimpleNamespace(tdr_test_timeout=600, tdr_compile_timeout=180)
    tdr._run_tests_once(
        image="img",
        container="cont",
        instance=_minimal_instance(),
        patch="diff --git a/foo b/foo\n",
        args=args,
        log_dir=log_dir,
        logger=logging.getLogger("test_tdr"),
        iteration=0,
    )

    exec_calls = [c for c in calls if c[0] == "exec"]
    entryscript_idx = next(
        (i for i, c in enumerate(exec_calls) if len(c) > 3 and c[3] == "/workspace/entryscript.sh"),
        None,
    )
    compile_idx = next(
        (
            i
            for i, c in enumerate(exec_calls)
            if len(c) > 3 and "go build" in " ".join(str(x) for x in c[3:])
        ),
        None,
    )

    assert entryscript_idx is not None, "entryscript exec not found"
    assert compile_idx is not None, "compile check exec not found"
    assert compile_idx > entryscript_idx, "compile check must run after the entryscript applies the patch"


def test_compile_check_skipped_when_patch_not_applied(tmp_path, monkeypatch):
    """No compile check should run against the base commit if the patch failed."""
    calls = _run_tdr_once(tmp_path, monkeypatch, patch_status="1")
    compile_calls = [
        c for c in calls if c[0] == "exec" and any("go build" in str(x) for x in c)
    ]
    assert not compile_calls, "compile check must not run when patch apply failed"


def test_compile_check_skipped_for_no_op_patch(tmp_path, monkeypatch):
    """No compile check should run when the patch was a no-op."""
    calls = _run_tdr_once(tmp_path, monkeypatch, patch_status="PATCH_NO_OP")
    compile_calls = [
        c for c in calls if c[0] == "exec" and any("go build" in str(x) for x in c)
    ]
    assert not compile_calls, "compile check must not run for a no-op patch"


# Helpers for the two skip tests above.

def _log_dir_setup(tmp_path, monkeypatch, patch_status: str):
    log_dir = tmp_path / "logs"
    log_dir.mkdir()

    root = tmp_path / "swebench_pro"
    (root / "run_scripts" / "test-tdr-1").mkdir(parents=True)
    (root / "run_scripts" / "test-tdr-1" / "run_script.sh").write_text(
        "#!/bin/bash\n", encoding="utf-8"
    )
    (root / "run_scripts" / "test-tdr-1" / "parser.py").write_text("", encoding="utf-8")
    monkeypatch.setattr(tdr, "SWEBENCH_PRO_ROOT", root)

    (log_dir / "cont.tdr.0.output.json").write_text('{"tests": []}', encoding="utf-8")
    (log_dir / "cont.tdr.0.stdout.log").write_text("", encoding="utf-8")
    (log_dir / "cont.tdr.0.stderr.log").write_text("", encoding="utf-8")
    (log_dir / "cont.tdr.0.patch_apply.log").write_text("", encoding="utf-8")
    (log_dir / "cont.tdr.0.patch_apply_status.txt").write_text(patch_status, encoding="utf-8")
    return log_dir


def _run_tdr_once(tmp_path, monkeypatch, patch_status: str):
    log_dir = _log_dir_setup(tmp_path, monkeypatch, patch_status)

    calls = []

    def fake_podman(*args, **kwargs):
        calls.append(args)

        class FakeProc:
            returncode = 0
            stdout = ""
            stderr = ""

        return FakeProc()

    monkeypatch.setattr(run_selfware, "podman", fake_podman)
    monkeypatch.setattr(run_selfware, "pull_image", lambda image, logger: True)
    monkeypatch.setattr(run_selfware, "start_container", lambda image, container, logger, timeout: True)
    monkeypatch.setattr(run_selfware, "copy_into_container", lambda src, dst, container, logger: True)

    args = types.SimpleNamespace(tdr_test_timeout=600, tdr_compile_timeout=180)
    tdr._run_tests_once(
        image="img",
        container="cont",
        instance=_minimal_instance(),
        patch="diff --git a/foo b/foo\n",
        args=args,
        log_dir=log_dir,
        logger=logging.getLogger("test_tdr"),
        iteration=0,
    )
    return calls
