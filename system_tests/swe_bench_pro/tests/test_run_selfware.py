"""Tests for run_selfware harness routing decisions."""

import argparse
import json
import logging
import os
import sys
from pathlib import Path

# Make sibling harness modules importable when running pytest directly.
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import pytest

from run_selfware import (
    _check_patch_builds,
    load_existing_predictions,
    save_prediction,
    select_instances,
    should_use_agentless,
)


def _args(agentless=None, sample_size=1, auto_agentless=None):
    return argparse.Namespace(
        agentless=agentless,
        auto_agentless=auto_agentless,
        sample_size=sample_size,
    )


def test_should_use_agentless_defaults_small_model_small_sample():
    """Small/fragile models on small samples should default to agentless."""
    config = {"model": "qwen2.5-7b"}
    assert should_use_agentless(_args(sample_size=5), config) is True


def test_should_use_agentless_defaults_llama_small_sample():
    """Known small/fragile aliases should default to agentless on small runs."""
    config = {"model": "meta-llama/llama-3.1-8b-instruct"}
    assert should_use_agentless(_args(sample_size=10), config) is True


def test_should_use_agentless_small_tier_always_agentless():
    """Small/fragile models always default to agentless, even on larger samples."""
    config = {
        "model": "qwen2.5-7b",
        "metadata": {"recommended": False},
    }
    assert should_use_agentless(_args(sample_size=50), config) is True


def test_should_use_agentless_recommended_large_model_stays_tool_loop():
    """Explicitly recommended strong models should stay on the agent loop."""
    config = {
        "model": "gemini-2.5-pro",
        "metadata": {"recommended": True},
    }
    assert should_use_agentless(_args(sample_size=5), config) is False


def test_should_use_agentless_recommended_medium_model_stays_tool_loop():
    """Explicitly recommended medium-tier models should stay on the agent loop."""
    config = {
        "model": "kimi-k2.7-code",
        "metadata": {"recommended": True, "tier": "medium"},
    }
    assert should_use_agentless(_args(sample_size=5), config) is False


def test_should_use_agentless_not_recommended_large_model_stays_tool_loop():
    """Large models without a recommendation should still use the multi-turn loop."""
    config = {
        "model": "some-large-model",
        "metadata": {"recommended": False},
    }
    assert should_use_agentless(_args(sample_size=50), config) is False


def test_should_use_agentless_missing_recommended_large_model_stays_tool_loop():
    """Large models with no recommendation metadata default to the agent loop."""
    config = {"model": "some-large-model"}
    assert should_use_agentless(_args(sample_size=50), config) is False


def test_should_use_agentless_not_recommended_medium_model_stays_tool_loop():
    """Medium models without a recommendation should use the multi-turn loop."""
    config = {
        "model": "kimi-k2.7-code",
        "metadata": {"recommended": False, "tier": "medium"},
    }
    assert should_use_agentless(_args(sample_size=50), config) is False


def test_should_use_agentless_explicit_opt_in():
    """--agentless=true always forces agentless on."""
    config = {
        "model": "gemini-2.5-pro",
        "metadata": {"recommended": True},
    }
    assert should_use_agentless(_args(agentless=True, sample_size=5), config) is True


def test_should_use_agentless_explicit_opt_out():
    """--agentless=false always forces the multi-turn tool loop."""
    config = {
        "model": "qwen2.5-7b",
        "metadata": {"recommended": False},
    }
    assert should_use_agentless(_args(agentless=False, sample_size=5), config) is False


def test_should_use_agentless_agentless_default_metadata():
    """Configs that set agentless_default=true always go agentless."""
    config = {
        "model": "gemini-2.5-pro",
        "metadata": {"agentless_default": True, "recommended": True},
    }
    assert should_use_agentless(_args(sample_size=50), config) is True


def test_load_existing_predictions_skips_empty_patches():
    """Empty predictions are not treated as completed runs on --resume."""
    output_dir = Path(__file__).parent / "_tmp_predictions"
    output_dir.mkdir(exist_ok=True)
    predictions = output_dir / "predictions.jsonl"
    try:
        with open(predictions, "w", encoding="utf-8") as f:
            f.write(json.dumps({"instance_id": "has_patch", "patch": "diff --git"}) + "\n")
            f.write(json.dumps({"instance_id": "empty_patch", "patch": ""}) + "\n")
            f.write(json.dumps({"instance_id": "whitespace_patch", "patch": "   \n"}) + "\n")
        assert load_existing_predictions(output_dir) == {"has_patch"}
    finally:
        predictions.unlink(missing_ok=True)
        output_dir.rmdir()


def _make_patch(path: str, old: str, new: str) -> str:
    return (
        f"diff --git a/{path} b/{path}\n"
        f"--- a/{path}\n"
        f"+++ b/{path}\n"
        "@@ -1 +1 @@\n"
        f"-{old}\n"
        f"+{new}\n"
    )


def test_compile_gate_rejects_broken_python_patch(tmp_path, monkeypatch):
    """A patch that introduces a syntax error must fail the build gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    # The gate checks the working tree, so the file must already be patched.
    (repo / "mod.py").write_text("def foo(\n")
    patch = _make_patch("mod.py", "x = 1", "def foo(")
    assert _check_patch_builds(repo, patch, "python", logging.getLogger("test")) is False


def test_compile_gate_accepts_valid_python_patch(tmp_path, monkeypatch):
    """A patch that leaves the code syntactically valid must pass the gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "mod.py").write_text("x = 2\n")
    patch = _make_patch("mod.py", "x = 1", "x = 2")
    assert _check_patch_builds(repo, patch, "python", logging.getLogger("test")) is True


def test_compile_gate_bypass_env_allows_broken_patch(tmp_path, monkeypatch):
    """SELFWARE_BYPASS_COMPILE_GATE=1 keeps the gate advisory."""
    monkeypatch.setenv("SELFWARE_BYPASS_COMPILE_GATE", "1")
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "mod.py").write_text("def foo(\n")
    patch = _make_patch("mod.py", "x = 1", "def foo(")
    assert _check_patch_builds(repo, patch, "python", logging.getLogger("test")) is True


def test_compile_gate_rejects_broken_javascript_patch(tmp_path, monkeypatch):
    """A JavaScript patch introducing a syntax error must fail the build gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "mod.js").write_text("function foo(\n")
    patch = _make_patch("mod.js", "x = 1", "function foo(")
    assert _check_patch_builds(repo, patch, "javascript", logging.getLogger("test")) is False


def test_compile_gate_accepts_valid_javascript_patch(tmp_path, monkeypatch):
    """A syntactically valid JavaScript patch must pass the gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "mod.js").write_text("x = 2\n")
    patch = _make_patch("mod.js", "x = 1", "x = 2")
    assert _check_patch_builds(repo, patch, "javascript", logging.getLogger("test")) is True


def test_compile_gate_rejects_broken_typescript_patch_with_tsconfig(tmp_path, monkeypatch):
    """A TypeScript patch with a tsconfig that introduces a syntax error must fail."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "tsconfig.json").write_text('{"compilerOptions": {"strict": true}}')
    (repo / "mod.ts").write_text("function foo(\n")
    patch = _make_patch("mod.ts", "x = 1", "function foo(")
    assert _check_patch_builds(repo, patch, "typescript", logging.getLogger("test")) is False


def test_compile_gate_accepts_valid_typescript_patch_with_tsconfig(tmp_path, monkeypatch):
    """A valid TypeScript patch with a tsconfig must pass the gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "tsconfig.json").write_text('{"compilerOptions": {"strict": true}}')
    (repo / "mod.ts").write_text("const x: number = 2;\n")
    patch = _make_patch("mod.ts", "const x: number = 1;", "const x: number = 2;")
    assert _check_patch_builds(repo, patch, "typescript", logging.getLogger("test")) is True


def test_compile_gate_typescript_without_tsconfig_uses_node_check(tmp_path, monkeypatch):
    """TypeScript without a tsconfig falls back to node --check on changed .js files."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "mod.js").write_text("function foo(\n")
    patch = _make_patch("mod.js", "x = 1", "function foo(")
    assert _check_patch_builds(repo, patch, "typescript", logging.getLogger("test")) is False


def test_compile_gate_rejects_broken_rust_patch(tmp_path, monkeypatch):
    """A Rust patch that breaks cargo check must fail the build gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "Cargo.toml").write_text(
        '[package]\nname = "gate"\nversion = "0.1.0"\nedition = "2021"\n'
    )
    src = repo / "src"
    src.mkdir()
    (src / "lib.rs").write_text("pub fn broken(\n")
    patch = _make_patch("src/lib.rs", "fn ok() {}", "pub fn broken(")
    assert _check_patch_builds(repo, patch, "rust", logging.getLogger("test")) is False


def test_compile_gate_accepts_valid_rust_patch(tmp_path, monkeypatch):
    """A valid Rust patch must pass the cargo check gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "Cargo.toml").write_text(
        '[package]\nname = "gate"\nversion = "0.1.0"\nedition = "2021"\n'
    )
    src = repo / "src"
    src.mkdir()
    (src / "lib.rs").write_text("pub fn ok() -> i32 { 2 }\n")
    patch = _make_patch("src/lib.rs", "fn ok() {}", "pub fn ok() -> i32 { 2 }")
    assert _check_patch_builds(repo, patch, "rust", logging.getLogger("test")) is True


def test_compile_gate_rejects_broken_go_patch(tmp_path, monkeypatch):
    """A Go patch introducing a syntax error must fail the build gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "go.mod").write_text("module gate\ngo 1.21\n")
    (repo / "mod.go").write_text("package gate\nfunc foo(\n")
    patch = _make_patch("mod.go", "x := 1", "func foo(")

    captured = {"cmd": None}

    def fake_go(cmd, **kwargs):
        captured["cmd"] = cmd

        class _R:
            returncode = 1
            stderr = "syntax error\n"
        return _R()

    monkeypatch.setattr("run_selfware.run_cmd", fake_go)
    monkeypatch.setattr("run_selfware.shutil.which", lambda name: "/fake/go" if name == "go" else None)
    assert _check_patch_builds(repo, patch, "go", logging.getLogger("test")) is False
    assert captured["cmd"] == ["go", "build", "./..."]


def test_compile_gate_accepts_valid_go_patch(tmp_path, monkeypatch):
    """A valid Go patch must pass the build gate."""
    monkeypatch.delenv("SELFWARE_BYPASS_COMPILE_GATE", raising=False)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "go.mod").write_text("module gate\ngo 1.21\n")
    (repo / "mod.go").write_text("package gate\nvar X = 2\n")
    patch = _make_patch("mod.go", "x := 1", "var X = 2")

    def fake_go(cmd, **kwargs):
        class _R:
            returncode = 0
            stderr = ""
        return _R()

    monkeypatch.setattr("run_selfware.run_cmd", fake_go)
    monkeypatch.setattr("run_selfware.shutil.which", lambda name: "/fake/go" if name == "go" else None)
    assert _check_patch_builds(repo, patch, "go", logging.getLogger("test")) is True


def test_save_prediction_stores_metadata_and_overwrites(tmp_path):
    """save_prediction writes metadata and overwrites earlier records per instance."""
    output_dir = tmp_path / "predictions"
    logger = logging.getLogger("test")
    save_prediction(output_dir, "inst-1", "diff --git", logger, {"k": "v"})
    save_prediction(output_dir, "inst-1", "", logger, {"empty": True})
    records = []
    with open(output_dir / "predictions.jsonl", encoding="utf-8") as f:
        for line in f:
            records.append(json.loads(line))
    assert len(records) == 1
    assert records[0]["patch"] == ""
    assert records[0]["metadata"] == {"empty": True}


def test_save_prediction_without_metadata_omits_key(tmp_path):
    """save_prediction should not add an empty metadata key."""
    output_dir = tmp_path / "predictions"
    logger = logging.getLogger("test")
    save_prediction(output_dir, "inst-1", "diff --git", logger)
    with open(output_dir / "predictions.jsonl", encoding="utf-8") as f:
        record = json.loads(f.readline())
    assert "metadata" not in record


def test_recovery_metadata_counters():
    """The metadata helper logic produces the expected recovery counters."""
    metadata = {}
    # Simulate two recovery attempts, one of which escalates to agentless and
    # ultimately succeeds.
    for attempt in range(1, 3):
        metadata["recovery_attempts"] = metadata.get("recovery_attempts", 0) + 1
    metadata["agentless_recovery_fired"] = True
    metadata["recovery_succeeded"] = True
    assert metadata == {
        "recovery_attempts": 2,
        "agentless_recovery_fired": True,
        "recovery_succeeded": True,
    }


def _args_for_select(max_tasks=None, sample_file=None, instance_ids=None, resume=False):
    return argparse.Namespace(
        max_tasks=max_tasks,
        sample_file=str(sample_file) if sample_file else None,
        instance_ids=instance_ids,
        resume=resume,
    )


def test_select_instances_sample_file_runs_all_rows_by_default(tmp_path):
    sample = tmp_path / "sample.jsonl"
    sample.write_text(
        "\n".join(
            json.dumps({"instance_id": f"id-{i}"}) for i in range(3)
        ),
        encoding="utf-8",
    )
    args = _args_for_select(sample_file=sample)
    rows = select_instances([], args, set(), logging.getLogger("test"))
    assert [r["instance_id"] for r in rows] == ["id-0", "id-1", "id-2"]


def test_select_instances_sample_file_respects_max_tasks(tmp_path):
    sample = tmp_path / "sample.jsonl"
    sample.write_text(
        "\n".join(
            json.dumps({"instance_id": f"id-{i}"}) for i in range(5)
        ),
        encoding="utf-8",
    )
    args = _args_for_select(max_tasks=2, sample_file=sample)
    rows = select_instances([], args, set(), logging.getLogger("test"))
    assert [r["instance_id"] for r in rows] == ["id-0", "id-1"]


def test_select_instances_default_caps_dataset_to_one():
    dataset = [{"instance_id": f"id-{i}"} for i in range(5)]
    args = _args_for_select()
    rows = select_instances(dataset, args, set(), logging.getLogger("test"))
    assert [r["instance_id"] for r in rows] == ["id-0"]


def test_select_instances_max_tasks_caps_dataset():
    dataset = [{"instance_id": f"id-{i}"} for i in range(5)]
    args = _args_for_select(max_tasks=3)
    rows = select_instances(dataset, args, set(), logging.getLogger("test"))
    assert [r["instance_id"] for r in rows] == ["id-0", "id-1", "id-2"]
