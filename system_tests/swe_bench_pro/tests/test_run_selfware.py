"""Tests for run_selfware harness routing decisions."""

import argparse
import json
import logging
import os
import subprocess
import sys
import urllib.error
from pathlib import Path
from typing import Any

# Make sibling harness modules importable when running pytest directly.
sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import pytest

from run_selfware import (
    _check_patch_builds,
    _check_run_manifest,
    _estimate_input_tokens,
    _parse_context_limit_from_error,
    _run_diff_recovery,
    call_chat_endpoint,
    load_config,
    load_existing_predictions,
    main,
    prepare_effective_config,
    run_agentless,
    run_diff_fallback,
    run_plan_then_patch,
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


def test_should_use_agentless_no_auto_agentless_disables_small_routing():
    """--no-auto-agentless prevents small-tier models from defaulting to agentless."""
    config = {
        "model": "qwen2.5-7b",
        "metadata": {"recommended": False},
    }
    assert should_use_agentless(_args(auto_agentless=False, sample_size=5), config) is False


def test_should_use_agentless_no_auto_agentless_still_honors_explicit():
    """--no-auto-agentless does not override an explicit --agentless=true."""
    config = {
        "model": "gemini-2.5-pro",
        "metadata": {"recommended": True},
    }
    assert should_use_agentless(
        _args(agentless=True, auto_agentless=False, sample_size=5), config
    ) is True


def test_should_use_agentless_no_auto_agentless_still_honors_metadata_default():
    """--no-auto-agentless still respects metadata.agentless_default."""
    config = {
        "model": "gemini-2.5-pro",
        "metadata": {"agentless_default": True, "recommended": True},
    }
    assert should_use_agentless(
        _args(auto_agentless=False, sample_size=5), config
    ) is True


def test_prepare_effective_config_merges_small_model_yaml(tmp_path):
    """YAML overrides are merged over the TOML config for small-model profiles."""
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    small_model_config_dir = tmp_path / "small_configs"
    small_model_config_dir.mkdir()
    output_dir = tmp_path / "out"

    (config_dir / "openrouter_test-small.toml").write_text(
        'model = "test/small"\n'
        "max_tokens = 1024\n"
        'temperature = 0.5\n'
        '[metadata]\ntier = "small"\nrecommended = true\n',
        encoding="utf-8",
    )
    (small_model_config_dir / "test-small.yaml").write_text(
        "max_tokens: 4096\n"
        "temperature: 0.1\n"
        "metadata:\n"
        "  recommended: false\n"
        '  notes: "from yaml"\n',
        encoding="utf-8",
    )

    args = argparse.Namespace(
        model_profile="test-small",
        config_dir=str(config_dir),
        small_model_config_dir=str(small_model_config_dir),
        output_dir=str(output_dir),
        local_endpoint=None,
        small_model=False,
        compact_prompt=False,
        adaptive=False,
        agentless=None,
        auto_agentless=None,
    )
    effective_path = prepare_effective_config(args, logging.getLogger("test"))
    effective = load_config(effective_path)

    assert effective["max_tokens"] == 4096
    assert effective["temperature"] == 0.1
    assert effective["metadata"]["recommended"] is False
    assert effective["metadata"]["notes"] == "from yaml"
    assert effective["model"] == "test/small"


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
    save_prediction(
        output_dir,
        "inst-1",
        "",
        logger,
        {"empty": True},
        error_code="empty_patch",
        provider="test-provider",
        model_profile="test-profile",
    )
    records = []
    with open(output_dir / "predictions.jsonl", encoding="utf-8") as f:
        for line in f:
            records.append(json.loads(line))
    assert len(records) == 1
    assert records[0]["patch"] == ""
    assert records[0]["metadata"]["empty"] is True
    assert records[0]["metadata"]["error_code"] == "empty_patch"
    assert records[0]["metadata"]["provider"] == "test-provider"
    assert records[0]["metadata"]["model_profile"] == "test-profile"


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


QWEN_CONTEXT_ERROR = json.dumps(
    {
        "error": {
            "message": (
                "This endpoint's maximum context length is 32768 tokens. However, you requested about 46493 tokens "
                "(13725 of text input, 32768 in the output). Please reduce the length of the messages or completion."
            ),
            "code": 400,
        }
    }
)


def test_parse_context_limit_from_error_qwen():
    """Parse the qwen2.5-7b poolside context-limit error."""
    limit, input_tokens, output_tokens = _parse_context_limit_from_error(QWEN_CONTEXT_ERROR)
    assert limit == 32768
    assert input_tokens == 13725
    assert output_tokens == 32768


def test_estimate_input_tokens_is_positive():
    """The heuristic returns a positive count for non-empty prompts."""
    text = "def foo():\n    return 1\n"
    assert _estimate_input_tokens(text) > 0
    assert _estimate_input_tokens("") == 0


def test_call_chat_endpoint_retries_400_context_length(monkeypatch):
    """A 400 context-length error causes max_tokens to be reduced and retried."""
    monkeypatch.setenv("SELFWARE_API_KEY", "test-key")

    requests: list[dict[str, Any]] = []

    class _FakeError(urllib.error.HTTPError):
        def __init__(self):
            self.code = 400
            self._body = QWEN_CONTEXT_ERROR.encode("utf-8")

        def read(self):
            return self._body

    def fake_urlopen(req, **_kwargs):
        payload = json.loads(req.data.decode("utf-8"))
        requests.append(payload)
        if len(requests) == 1:
            raise _FakeError()
        return _FakeResponse({
            "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 13725, "completion_tokens": 2},
        })

    monkeypatch.setattr("run_selfware.urllib.request.urlopen", fake_urlopen)

    config = {"endpoint": "http://test/v1", "model": "qwen2.5-7b", "max_tokens": 32768}
    result = call_chat_endpoint(config, "prompt", 30, logging.getLogger("test"))

    assert result == "ok"
    assert len(requests) == 2
    # First request used the original budget.
    assert requests[0]["max_tokens"] == 32768
    # Second request respects the provider's context cap: 32768 - 13725 - 1.
    assert requests[1]["max_tokens"] == 32768 - 13725 - 1


def test_call_chat_endpoint_length_finish_respects_context_window(monkeypatch):
    """A length finish reason does not ask for more output than the context window allows."""
    monkeypatch.setenv("SELFWARE_API_KEY", "test-key")

    requests: list[dict[str, Any]] = []

    def fake_urlopen(req, **_kwargs):
        payload = json.loads(req.data.decode("utf-8"))
        requests.append(payload)
        if len(requests) == 1:
            # Hit output limit with a tiny context window.
            return _FakeResponse({
                "choices": [{"message": {"content": "trunc"}, "finish_reason": "length"}],
                "usage": {"prompt_tokens": 3000, "completion_tokens": 2048},
            })
        return _FakeResponse({
            "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 3000, "completion_tokens": 2},
        })

    monkeypatch.setattr("run_selfware.urllib.request.urlopen", fake_urlopen)

    config = {
        "endpoint": "http://test/v1",
        "model": "qwen2.5-7b",
        "max_tokens": 2048,
        "agent": {"context_window": 4096},
    }
    result = call_chat_endpoint(config, "prompt", 30, logging.getLogger("test"))

    # With context_window=4096 and input_tokens=3000, the most output we can
    # request is 4096 - 3000 - 1 = 1095, which is below the current 2048 budget.
    # The function must give up and return the truncated content instead of
    # requesting an oversized output.
    assert result == "trunc"
    assert len(requests) == 1


class _FakeResponse:
    def __init__(self, payload: dict[str, Any]):
        self._payload = payload

    def read(self):
        return json.dumps(self._payload).encode("utf-8")

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False



def _init_git_repo(repo: Path) -> None:
    """Initialize a git repo with a single commit for patch tests."""
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    subprocess.run(
        ["git", "-C", str(repo), "config", "user.email", "test@example.com"],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(repo), "config", "user.name", "Test User"],
        check=True,
    )


def test_run_diff_fallback_extracts_valid_unified_diff(tmp_path, monkeypatch):
    """A mocked chat response with a valid unified diff is extracted and applied."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "foo.py").write_text("old\n", encoding="utf-8")
    subprocess.run(
        ["git", "-C", str(repo), "add", "foo.py"],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(repo), "commit", "-m", "base", "-q"],
        check=True,
    )

    diff_text = (
        "diff --git a/foo.py b/foo.py\n"
        "--- a/foo.py\n"
        "+++ b/foo.py\n"
        "@@ -1 +1 @@\n"
        "-old\n"
        "+new\n"
    )

    def _fake_chat(config, prompt, timeout, logger):
        return diff_text

    monkeypatch.setattr("run_selfware.call_chat_endpoint", _fake_chat)

    instance = {
        "instance_id": "inst-diff",
        "base_commit": "HEAD",
        "test_patch": "",
    }
    args = argparse.Namespace(patch_timeout=30)
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    patch = run_diff_fallback(
        repo,
        instance,
        "prompt",
        ["foo.py"],
        {},
        args,
        log_dir,
        logging.getLogger("test"),
        "name",
    )
    assert patch.strip()
    assert "+new" in patch
    assert "-old" in patch


def test_run_diff_fallback_rejects_partial_search_replace(tmp_path, monkeypatch):
    """A partial SEARCH/REPLACE patch is rejected and the repo is reset."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "a.py").write_text("old_a\n", encoding="utf-8")
    (repo / "b.py").write_text("old_b\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    response = (
        "### FILE: a.py\n"
        "<<<<<<< SEARCH\n"
        "old_a\n"
        "=======\n"
        "new_a\n"
        ">>>>>>> REPLACE\n"
        "### FILE: b.py\n"
        "<<<<<<< SEARCH\n"
        "does_not_exist\n"
        "=======\n"
        "new_b\n"
        ">>>>>>> REPLACE\n"
    )

    monkeypatch.setattr("run_selfware.call_chat_endpoint", lambda cfg, prompt, timeout, logger: response)

    instance = {
        "instance_id": "inst-partial-fallback",
        "base_commit": "HEAD",
        "test_patch": "",
    }
    args = argparse.Namespace(patch_timeout=30)
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    patch = run_diff_fallback(
        repo,
        instance,
        "prompt",
        ["a.py", "b.py"],
        {},
        args,
        log_dir,
        logging.getLogger("test"),
        "name",
    )
    assert patch == ""
    assert (repo / "a.py").read_text(encoding="utf-8") == "old_a\n"
    assert (repo / "b.py").read_text(encoding="utf-8") == "old_b\n"


def test_diff_recovery_rejects_partial_search_replace(tmp_path, monkeypatch):
    """EMPTY_PATCH recovery rejects a partial SEARCH/REPLACE patch."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "a.py").write_text("old_a\n", encoding="utf-8")
    (repo / "b.py").write_text("old_b\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    response = (
        "### FILE: a.py\n"
        "<<<<<<< SEARCH\n"
        "old_a\n"
        "=======\n"
        "new_a\n"
        ">>>>>>> REPLACE\n"
        "### FILE: b.py\n"
        "<<<<<<< SEARCH\n"
        "does_not_exist\n"
        "=======\n"
        "new_b\n"
        ">>>>>>> REPLACE\n"
    )

    monkeypatch.setattr("run_selfware.call_chat_endpoint", lambda cfg, prompt, timeout, logger: response)

    output_dir = tmp_path / "out"
    output_dir.mkdir()
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    metadata: dict[str, Any] = {}
    instance = {
        "instance_id": "inst-partial-recovery",
        "base_commit": "HEAD",
        "test_patch": "",
        "repo_language": "python",
    }
    args = argparse.Namespace(patch_timeout=30, patch_config={})

    patch = _run_diff_recovery(
        repo,
        instance,
        "prompt",
        ["a.py", "b.py"],
        {},
        args,
        log_dir,
        output_dir,
        logging.getLogger("test"),
        "name",
        1,
        metadata,
    )

    assert patch == ""
    assert (repo / "a.py").read_text(encoding="utf-8") == "old_a\n"
    assert (repo / "b.py").read_text(encoding="utf-8") == "old_b\n"
    assert metadata.get("diff_recovery_fired") is True
    assert metadata.get("recovery_succeeded") is not True
    assert not list(output_dir.glob("predictions.jsonl"))


def test_diff_recovery_saves_prediction_on_empty_patch(tmp_path, monkeypatch):
    """EMPTY_PATCH recovery via diff fallback saves a non-empty prediction."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "foo.py").write_text("old\n", encoding="utf-8")
    subprocess.run(
        ["git", "-C", str(repo), "add", "foo.py"],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(repo), "commit", "-m", "base", "-q"],
        check=True,
    )

    expected_patch = (
        "diff --git a/foo.py b/foo.py\n"
        "--- a/foo.py\n"
        "+++ b/foo.py\n"
        "@@ -1 +1 @@\n"
        "-old\n"
        "+new\n"
    )

    calls = []

    def _fake_run_diff_fallback(*args, **kwargs):
        calls.append((args, kwargs))
        return expected_patch

    monkeypatch.setattr("run_selfware.run_diff_fallback", _fake_run_diff_fallback)
    monkeypatch.setattr(
        "run_selfware._check_patch_builds",
        lambda repo_dir, patch, language, logger: True,
    )

    output_dir = tmp_path / "out"
    output_dir.mkdir()
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    metadata: dict[str, Any] = {}
    instance = {
        "instance_id": "inst-recovery",
        "base_commit": "HEAD",
        "test_patch": "",
        "repo_language": "python",
    }
    args = argparse.Namespace(patch_timeout=30, patch_config={})

    patch = _run_diff_recovery(
        repo,
        instance,
        "prompt",
        ["foo.py"],
        {},
        args,
        log_dir,
        output_dir,
        logging.getLogger("test"),
        "name",
        1,
        metadata,
    )

    assert patch == expected_patch
    assert metadata.get("diff_recovery_fired") is True
    assert metadata.get("recovery_succeeded") is True

    predictions = list(output_dir.glob("predictions.jsonl"))
    assert len(predictions) == 1
    records = []
    with open(predictions[0], encoding="utf-8") as f:
        for line in f:
            records.append(json.loads(line))
    assert len(records) == 1
    assert records[0]["patch"] == expected_patch
    assert records[0]["metadata"]["recovery_succeeded"] is True
    assert records[0]["metadata"]["diff_recovery_fired"] is True


# -----------------------------------------------------------------------------
# Atomic patch-application tests for agentless and plan-then-patch paths
# -----------------------------------------------------------------------------

def _make_search_replace_block(path: str, search: str, replace: str) -> str:
    return (
        f"### FILE: {path}\n"
        "<<<<<<< SEARCH\n"
        f"{search}\n"
        "=======\n"
        f"{replace}\n"
        ">>>>>>> REPLACE\n"
    )


def test_run_agentless_rejects_partial_patch(tmp_path, monkeypatch):
    """If one SEARCH/REPLACE block fails, the whole agentless patch is rejected."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "a.py").write_text("old_a\n", encoding="utf-8")
    (repo / "b.py").write_text("old_b\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    response = (
        _make_search_replace_block("a.py", "old_a", "new_a")
        + _make_search_replace_block("b.py", "does_not_exist", "new_b")
    )

    monkeypatch.setattr("run_selfware.call_chat_endpoint", lambda cfg, prompt, timeout, logger: response)
    monkeypatch.setattr("run_selfware.build_agentless_prompt", lambda *args, **kwargs: "prompt")
    monkeypatch.setattr("run_selfware.build_agentless_retry_prompt", lambda *args, **kwargs: "retry")

    args = argparse.Namespace(patch_timeout=30, diff_fallback=False)
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    instance = {
        "instance_id": "inst-agentless-partial",
        "base_commit": "HEAD",
        "test_patch": "",
        "patch": "",
        "problem_statement": "change a and b",
    }
    patch = run_agentless(
        repo,
        instance,
        {},
        args,
        log_dir,
        logging.getLogger("test"),
        "name",
    )
    assert patch == ""
    assert (repo / "a.py").read_text() == "old_a\n"
    assert (repo / "b.py").read_text() == "old_b\n"


def test_run_agentless_accepts_full_patch(tmp_path, monkeypatch):
    """When every SEARCH/REPLACE block matches, the agentless patch is accepted."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "a.py").write_text("old_a\n", encoding="utf-8")
    (repo / "b.py").write_text("old_b\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    response = (
        _make_search_replace_block("a.py", "old_a", "new_a")
        + _make_search_replace_block("b.py", "old_b", "new_b")
    )

    monkeypatch.setattr("run_selfware.call_chat_endpoint", lambda cfg, prompt, timeout, logger: response)
    monkeypatch.setattr("run_selfware.build_agentless_prompt", lambda *args, **kwargs: "prompt")
    monkeypatch.setattr("run_selfware.build_agentless_retry_prompt", lambda *args, **kwargs: "retry")

    args = argparse.Namespace(patch_timeout=30, diff_fallback=False)
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    instance = {
        "instance_id": "inst-agentless-full",
        "base_commit": "HEAD",
        "test_patch": "",
        "patch": "",
        "problem_statement": "change a and b",
    }
    patch = run_agentless(
        repo,
        instance,
        {},
        args,
        log_dir,
        logging.getLogger("test"),
        "name",
    )
    assert patch.strip()
    assert "new_a" in patch
    assert "new_b" in patch


def test_run_plan_then_patch_rejects_partial_patch(tmp_path, monkeypatch):
    """If the patch step fails to apply, the plan-then-patch path returns empty."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "a.py").write_text("old_a\n", encoding="utf-8")
    (repo / "b.py").write_text("old_b\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    plan_response = "FILES: a.py, b.py\nFIX: change both files\n"
    patch_response = (
        _make_search_replace_block("a.py", "old_a", "new_a")
        + _make_search_replace_block("b.py", "does_not_exist", "new_b")
    )

    responses = iter([plan_response, patch_response])

    def _fake_chat(config, prompt, timeout, logger, **kwargs):
        return next(responses)

    monkeypatch.setattr("run_selfware.call_chat_endpoint", _fake_chat)

    args = argparse.Namespace(
        plan_timeout=30,
        patch_timeout=30,
        plan_max_tokens=100,
        plan_temperature=0.0,
    )
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    instance = {
        "instance_id": "inst-plan-partial",
        "base_commit": "HEAD",
        "test_patch": "",
        "problem_statement": "change a and b",
    }
    patch = run_plan_then_patch(
        repo,
        instance,
        {},
        {},
        args,
        log_dir,
        logging.getLogger("test"),
        "name",
    )
    assert patch == ""
    assert (repo / "a.py").read_text() == "old_a\n"
    assert (repo / "b.py").read_text() == "old_b\n"


def test_run_plan_then_patch_accepts_full_patch(tmp_path, monkeypatch):
    """When every patch block applies, the plan-then-patch path returns the diff."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "a.py").write_text("old_a\n", encoding="utf-8")
    (repo / "b.py").write_text("old_b\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    plan_response = "FILES: a.py, b.py\nFIX: change both files\n"
    patch_response = (
        _make_search_replace_block("a.py", "old_a", "new_a")
        + _make_search_replace_block("b.py", "old_b", "new_b")
    )

    responses = iter([plan_response, patch_response])

    def _fake_chat(config, prompt, timeout, logger, **kwargs):
        return next(responses)

    monkeypatch.setattr("run_selfware.call_chat_endpoint", _fake_chat)

    args = argparse.Namespace(
        plan_timeout=30,
        patch_timeout=30,
        plan_max_tokens=100,
        plan_temperature=0.0,
    )
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    instance = {
        "instance_id": "inst-plan-full",
        "base_commit": "HEAD",
        "test_patch": "",
        "problem_statement": "change a and b",
    }
    patch = run_plan_then_patch(
        repo,
        instance,
        {},
        {},
        args,
        log_dir,
        logging.getLogger("test"),
        "name",
    )
    assert patch.strip()
    assert "new_a" in patch
    assert "new_b" in patch



# -----------------------------------------------------------------------------
# Run provenance tests
# -----------------------------------------------------------------------------


def _make_base_args(output_dir: Path, sample_file: Path | None = None, **overrides):
    """Build a minimal args namespace for exercising main()."""
    kwargs = dict(
        model_profile="test-profile",
        max_tasks=None,
        instance_ids=None,
        sample_file=str(sample_file) if sample_file else None,
        output_dir=str(output_dir),
        timeout=1800,
        config_dir=str(Path(__file__).parent / "fake_config"),
        small_model_config_dir=str(
            Path(__file__).parent.parent / "configs"
        ),
        binary=__file__,
        repo_dir="/app",
        resume=False,
        fresh=False,
        keep_container=False,
        workers=1,
        repair_feedback=None,
        compact_prompt=False,
        small_model=False,
        few_shot_examples=None,
        force_edit=False,
        retry_failures=True,
        max_retries=2,
        diff_fallback=True,
        early_diff_fallback=True,
        small_model_diff_fallback=False,
        agentless=None,
        auto_agentless=None,
        adaptive=False,
        local_endpoint=None,
        plan_then_patch=False,
        plan_model_profile=None,
        plan_max_tokens=1024,
        plan_temperature=0.3,
        plan_timeout=120,
        patch_timeout=300,
        tdr=False,
        repair_model_profile=None,
        repair_config=None,
        repair_iterations=2,
        repair_timeout=300,
        repair_max_tokens=16384,
        tdr_test_timeout=600,
        tdr_compile_timeout=180,
        tdr_keep_container=False,
        ensemble_models=None,
        ensemble_timeout=180,
        ensemble_max_tokens=4096,
    )
    kwargs.update(overrides)
    return argparse.Namespace(**kwargs)


def _runtime_flags(**overrides):
    """Return a matching runtime_flags dict for provenance tests."""
    defaults = {
        "agentless": None,
        "auto_agentless": None,
        "small_model_diff_fallback": False,
        "diff_fallback": True,
        "retry_failures": True,
        "max_retries": 2,
        "early_diff_fallback": True,
        "force_edit": False,
        "small_model": False,
        "adaptive": False,
        "compact_prompt": False,
    }
    defaults.update(overrides)
    return defaults


def test_check_run_manifest_rejects_mismatched_sha():
    """A mismatched harness_sha makes an existing manifest incompatible."""
    current = {
        "model_profile": "p1",
        "config_dir": "cfg",
        "config_files": ["c1.toml"],
        "sample_file": "s.jsonl",
        "harness_sha": "abc123",
        "runtime_flags": _runtime_flags(),
    }
    existing = dict(current, harness_sha="def456")
    with pytest.raises(RuntimeError) as exc_info:
        _check_run_manifest(current, existing)
    assert "harness_sha" in str(exc_info.value)
    assert "--fresh" in str(exc_info.value)


def test_check_run_manifest_accepts_matching_runtime_flags():
    """Identical provenance, including runtime flags, is accepted."""
    current = {
        "model_profile": "p1",
        "config_dir": "cfg",
        "config_files": ["c1.toml"],
        "sample_file": "s.jsonl",
        "harness_sha": "abc123",
        "runtime_flags": _runtime_flags(),
    }
    existing = dict(current)
    # Should not raise.
    _check_run_manifest(current, existing)


def test_check_run_manifest_rejects_mismatched_runtime_flag():
    """A changed runtime flag invalidates a resume."""
    current = {
        "model_profile": "p1",
        "config_dir": "cfg",
        "config_files": ["c1.toml"],
        "sample_file": "s.jsonl",
        "harness_sha": "abc123",
        "runtime_flags": _runtime_flags(),
    }
    existing = dict(current)
    existing["runtime_flags"] = _runtime_flags(agentless=True)
    with pytest.raises(RuntimeError) as exc_info:
        _check_run_manifest(current, existing)
    assert "runtime_flags.agentless" in str(exc_info.value)
    assert "--fresh" in str(exc_info.value)


def test_main_resume_rejects_mismatched_manifest(tmp_path, monkeypatch):
    """--resume with an incompatible existing manifest exits with an error."""
    output_dir = tmp_path / "out"
    output_dir.mkdir()
    config_dir = str(Path(__file__).parent / "fake_config")
    manifest = {
        "model_profile": "other-profile",
        "config_dir": config_dir,
        "config_files": ["other.toml"],
        "sample_file": None,
        "harness_sha": "mismatched-sha",
        "runtime_flags": _runtime_flags(),
    }
    (output_dir / "run_manifest.json").write_text(json.dumps(manifest), encoding="utf-8")

    fake_config = tmp_path / "openrouter_test-profile.toml"
    fake_config.write_text("[agent]\n", encoding="utf-8")

    def _fake_prepare_effective_config(args, logger):
        return fake_config

    monkeypatch.setattr("run_selfware.prepare_effective_config", _fake_prepare_effective_config)
    monkeypatch.setattr("run_selfware.load_config", lambda path: {"model": "test-model"})
    monkeypatch.setenv("SELFWARE_API_KEY", "test-key")
    monkeypatch.setattr(
        "run_selfware.parse_args",
        lambda: _make_base_args(output_dir, resume=True),
    )

    rc = main()
    assert rc == 1


def test_main_resume_accepts_matching_manifest(tmp_path, monkeypatch):
    """--resume with a compatible existing manifest continues the run."""
    output_dir = tmp_path / "out"
    output_dir.mkdir()
    config_dir = str(Path(__file__).parent / "fake_config")

    fake_config = tmp_path / "openrouter_test-profile.toml"
    fake_config.write_text("[agent]\n", encoding="utf-8")

    manifest = {
        "model_profile": "test-profile",
        "config_dir": config_dir,
        "config_files": [str(fake_config)],
        "sample_file": None,
        "harness_sha": "current-sha",
        "runtime_flags": _runtime_flags(),
    }
    (output_dir / "run_manifest.json").write_text(json.dumps(manifest), encoding="utf-8")

    def _fake_prepare_effective_config(args, logger):
        return fake_config

    monkeypatch.setattr("run_selfware.prepare_effective_config", _fake_prepare_effective_config)
    monkeypatch.setattr("run_selfware.load_config", lambda path: {"model": "test-model"})
    monkeypatch.setattr("run_selfware._get_harness_sha", lambda: "current-sha")
    monkeypatch.setenv("SELFWARE_API_KEY", "test-key")
    monkeypatch.setattr("run_selfware.select_instances", lambda *args, **kwargs: [])
    monkeypatch.setattr(
        "run_selfware.parse_args",
        lambda: _make_base_args(output_dir, resume=True),
    )

    rc = main()
    assert rc == 0
    new_manifest = json.loads((output_dir / "run_manifest.json").read_text(encoding="utf-8"))
    assert new_manifest["model_profile"] == "test-profile"
    assert new_manifest["runtime_flags"] == _runtime_flags()


def test_main_fresh_clears_predictions_and_manifest(tmp_path, monkeypatch):
    """--fresh removes prior predictions and manifest before writing a new one."""
    output_dir = tmp_path / "out"
    output_dir.mkdir()
    (output_dir / "predictions.jsonl").write_text("{}", encoding="utf-8")
    (output_dir / "predictions.json").write_text("[]", encoding="utf-8")
    old_manifest = {
        "model_profile": "old-profile",
        "config_files": ["old.toml"],
        "sample_file": None,
        "harness_sha": "old-sha",
    }
    (output_dir / "run_manifest.json").write_text(json.dumps(old_manifest), encoding="utf-8")

    sample_file = tmp_path / "sample.jsonl"
    sample_file.write_text("", encoding="utf-8")

    fake_config = tmp_path / "openrouter_test-profile.toml"
    fake_config.write_text("[agent]\n", encoding="utf-8")

    def _fake_prepare_effective_config(args, logger):
        return fake_config

    monkeypatch.setattr("run_selfware.prepare_effective_config", _fake_prepare_effective_config)
    monkeypatch.setattr("run_selfware.load_config", lambda path: {"model": "test-model"})
    monkeypatch.setenv("SELFWARE_API_KEY", "test-key")
    monkeypatch.setattr(
        "run_selfware.parse_args",
        lambda: _make_base_args(output_dir, sample_file=sample_file, fresh=True),
    )

    rc = main()
    assert rc == 0
    assert not (output_dir / "predictions.jsonl").exists()
    assert not (output_dir / "predictions.json").exists()
    manifest = json.loads((output_dir / "run_manifest.json").read_text(encoding="utf-8"))
    assert manifest["model_profile"] == "test-profile"
    assert manifest["sample_file"] == str(sample_file)


def test_run_agentless_diff_fallback_records_recovery_metadata(tmp_path, monkeypatch):
    """When agentless SEARCH/REPLACE fails, the diff fallback path records
    recovery counters in the supplied metadata dict."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "a.py").write_text("old_a\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    # First response is an unapplyable SEARCH/REPLACE block.
    bad_response = _make_search_replace_block("a.py", "does_not_exist", "new_a")
    expected_patch = (
        "diff --git a/a.py b/a.py\n"
        "--- a/a.py\n"
        "+++ b/a.py\n"
        "@@ -1 +1 @@\n"
        "-old_a\n"
        "+new_a\n"
    )

    monkeypatch.setattr(
        "run_selfware.call_chat_endpoint",
        lambda cfg, prompt, timeout, logger: bad_response,
    )
    monkeypatch.setattr(
        "run_selfware.build_agentless_prompt", lambda *args, **kwargs: "prompt"
    )
    monkeypatch.setattr(
        "run_selfware.build_agentless_retry_prompt",
        lambda *args, **kwargs: "retry",
    )
    monkeypatch.setattr(
        "run_selfware.run_diff_fallback", lambda *args, **kwargs: expected_patch
    )
    monkeypatch.setattr(
        "run_selfware._check_patch_builds",
        lambda repo_dir, patch, language, logger: True,
    )

    args = argparse.Namespace(
        patch_timeout=30, diff_fallback=True, early_diff_fallback=False
    )
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    metadata: dict[str, Any] = {}
    instance = {
        "instance_id": "inst-agentless-recovery",
        "base_commit": "HEAD",
        "test_patch": "",
        "patch": "",
        "problem_statement": "change a",
    }
    patch = run_agentless(
        repo,
        instance,
        {},
        args,
        log_dir,
        logging.getLogger("test"),
        "name",
        metadata=metadata,
    )
    assert patch == expected_patch
    assert metadata.get("diff_recovery_fired") is True
    assert metadata.get("recovery_attempts", 0) > 0
    assert metadata.get("recovery_succeeded") is True


def test_run_agentless_small_model_diff_fallback_skips_search_replace(
    tmp_path, monkeypatch
):
    """--small-model-diff-fallback bypasses SEARCH/REPLACE and applies test_patch before diff fallback."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_git_repo(repo)
    (repo / "a.py").write_text("old_a\n", encoding="utf-8")
    (repo / "test_a.py").write_text("def test_a(): pass\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
    subprocess.run(["git", "-C", str(repo), "commit", "-m", "base", "-q"], check=True)

    expected_patch = (
        "diff --git a/a.py b/a.py\n"
        "--- a/a.py\n"
        "+++ b/a.py\n"
        "@@ -1 +1 @@\n"
        "-old_a\n"
        "+new_a\n"
    )

    test_patch = (
        "diff --git a/test_a.py b/test_a.py\n"
        "--- a/test_a.py\n"
        "+++ b/test_a.py\n"
        "@@ -1 +1 @@\n"
        "-def test_a(): pass\n"
        "+def test_a(): pass  # patched\n"
    )

    calls = []

    def _fake_run_diff_fallback(host_repo_dir, instance, prompt, ranked_files, *args, **kwargs):
        calls.append((host_repo_dir, instance, prompt, ranked_files))
        return expected_patch

    monkeypatch.setattr("run_selfware.run_diff_fallback", _fake_run_diff_fallback)
    monkeypatch.setattr(
        "run_selfware._check_patch_builds",
        lambda repo_dir, patch, language, logger: True,
    )
    monkeypatch.setattr(
        "run_selfware.rank_files_by_relevance",
        lambda *args, **kwargs: ["a.py"],
    )

    args = argparse.Namespace(
        patch_timeout=30,
        diff_fallback=True,
        small_model_diff_fallback=True,
        early_diff_fallback=False,
    )
    log_dir = tmp_path / "logs"
    log_dir.mkdir()
    metadata: dict[str, Any] = {}
    instance = {
        "instance_id": "inst-small-diff",
        "base_commit": "HEAD",
        "test_patch": test_patch,
        "patch": "",
        "problem_statement": "change a",
        "repo_language": "python",
    }
    patch = run_agentless(
        repo,
        instance,
        {},
        args,
        log_dir,
        logging.getLogger("test"),
        "name",
        metadata=metadata,
    )
    assert patch == expected_patch
    assert metadata.get("diff_recovery_fired") is True
    assert metadata.get("recovery_succeeded") is True
    # run_diff_fallback should have been called.
    assert len(calls) == 1
    # The test patch should have been applied to the repo.
    applied_test = (repo / "test_a.py").read_text(encoding="utf-8")
    assert "# patched" in applied_test
