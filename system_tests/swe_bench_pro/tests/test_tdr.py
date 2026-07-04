"""Tests for the TDR evaluation harness."""

import logging
import os
import subprocess
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
    # Fallback must come after the parser invocation (the trap has a similar
    # guard earlier, so search from the parser invocation).
    parser_idx = script.index("python /workspace/parser.py")
    fallback_idx = script.index("if [ ! -f /workspace/output.json ]; then", parser_idx)
    assert fallback_idx > parser_idx


def test_entryscript_has_exit_trap_for_output_json():
    """A container crash must still leave an output.json for the harness."""
    script = tdr._build_entryscript(_minimal_instance())
    assert "trap " in script and "/workspace/output.json" in script
    assert "EXIT" in script


def test_entryscript_clears_stale_output_before_tests():
    """Stale output.json from a prior iteration must be removed before tests run."""
    script = tdr._build_entryscript(_minimal_instance())
    status_write_idx = script.index("echo $apply_status > /workspace/patch_apply_status.txt")
    run_idx = script.index("bash /workspace/run_script.sh")
    clear_idx = script.index("rm -f /workspace/output.json\n", status_write_idx)
    assert clear_idx < run_idx


def test_entryscript_preserves_patch_status_for_compile_gate():
    """Successful patch status must survive test execution for host compile checks."""
    script = tdr._build_entryscript(_minimal_instance())
    initial_clear_idx = script.index("rm -f /workspace/output.json /workspace/patch_apply_status.txt")
    apply_idx = script.index("git apply -v /workspace/patch.diff")
    status_write_idx = script.index("echo $apply_status > /workspace/patch_apply_status.txt")
    test_clear_idx = script.index("rm -f /workspace/output.json\n", status_write_idx)
    run_idx = script.index("bash /workspace/run_script.sh")

    assert initial_clear_idx < apply_idx
    assert status_write_idx < test_clear_idx < run_idx
    assert "/workspace/patch_apply_status.txt" not in script[test_clear_idx:run_idx]


def test_entryscript_passes_tests_as_separate_quoted_args():
    """Selected tests must be passed as separate shell arguments, not comma-joined."""
    instance = {
        **_minimal_instance(),
        "selected_test_files_to_run": ["tests/test_a.py", "tests/test_b.py"],
    }
    script = tdr._build_entryscript(instance)
    assert "bash /workspace/run_script.sh tests/test_a.py tests/test_b.py" in script
    assert "tests/test_a.py,tests/test_b.py" not in script


def test_entryscript_quotes_paths_with_special_chars():
    """Paths containing spaces or globs must be shell-quoted."""
    instance = {
        **_minimal_instance(),
        "selected_test_files_to_run": ["tests/my test.py", "tests/*.py"],
    }
    script = tdr._build_entryscript(instance)
    assert "bash /workspace/run_script.sh 'tests/my test.py' 'tests/*.py'" in script
    assert "tests/my test.py,tests/*.py" not in script


def test_compile_check_js_does_not_ignore_failure():
    """The JS/TS compile check must not be masked by '|| true'."""
    js_instance = {**_minimal_instance(), "repo_language": "javascript"}
    cmd = tdr._compile_check_command(js_instance)
    assert cmd is not None
    joined = " ".join(cmd)
    assert "|| true" not in joined


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


def test_repair_rejects_partial_search_replace(tmp_path):
    """TDR repair must not capture patches when one SEARCH/REPLACE block failed."""
    repo = tmp_path / "repo"
    repo.mkdir()
    subprocess.run(["git", "-C", str(repo), "init"], check=True, capture_output=True)
    subprocess.run(
        ["git", "-C", str(repo), "config", "user.email", "test@example.com"],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(repo), "config", "user.name", "Test User"],
        check=True,
    )
    (repo / "a.txt").write_text("old\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "a.txt"], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base"], check=True)
    base_commit = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()

    search = "<" * 7 + " SEARCH"
    divider = "=" * 7
    replace = ">" * 7 + " REPLACE"
    response = f"""### FILE: a.txt
{search}
old
{divider}
new
{replace}

### FILE: missing.txt
{search}
old
{divider}
new
{replace}
"""

    patch = tdr._apply_repair_and_capture(
        repo,
        base_commit,
        current_patch="",
        response=response,
        logger=logging.getLogger("test_tdr"),
    )

    assert patch is None
    assert (repo / "a.txt").read_text(encoding="utf-8") == "old\n"
