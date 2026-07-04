"""Tests for per-repo prompt templates."""

import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from harness_recovery import build_diff_fallback_prompt
from repo_templates import _sanitize_repo_name, load_repo_template
from run_selfware import build_plan_prompt
from small_model_adapter import build_agentless_prompt, build_agentless_retry_prompt


def test_sanitize_repo_name_handles_slash_and_double_underscore():
    assert _sanitize_repo_name("NodeBB/NodeBB") == "NodeBB__NodeBB"
    assert _sanitize_repo_name("NodeBB__NodeBB") == "NodeBB__NodeBB"
    assert _sanitize_repo_name("ansible/ansible") == "ansible__ansible"
    assert _sanitize_repo_name("owner/name/extra") == "owner__name__extra"
    assert _sanitize_repo_name("../etc/passwd") == "etc__passwd"


def test_load_repo_template_known_repos():
    nodebb = load_repo_template("NodeBB/NodeBB")
    assert "Do NOT create new source files" in nodebb
    assert "public/language/en-GB" in nodebb

    ansible = load_repo_template("ansible/ansible")
    assert "Do NOT replace an entire module" in ansible
    assert "Preserve all existing imports" in ansible


def test_load_repo_template_unknown_repo_returns_empty():
    assert load_repo_template("unknown/repo") == ""
    assert load_repo_template("") == ""


def _nodebb_instance():
    return {
        "repo": "NodeBB/NodeBB",
        "base_commit": "abc123",
        "repo_language": "javascript",
        "problem_statement": "Database keys do not return foo",
        "requirements": "Fix the database key lookup",
        "selected_test_files_to_run": ["test/database/keys.js"],
        "fail_to_pass": ["test/database/keys.js"],
        "test_patch": "",
    }


def _ansible_instance():
    return {
        "repo": "ansible/ansible",
        "base_commit": "def456",
        "repo_language": "python",
        "problem_statement": "Module foo replaces entire function",
        "requirements": "Make a surgical edit",
        "selected_test_files_to_run": ["test/foo.py"],
        "fail_to_pass": ["test/foo.py"],
        "test_patch": "",
    }


def test_build_agentless_prompt_includes_repo_template():
    with tempfile.TemporaryDirectory() as tmp:
        prompt = build_agentless_prompt(_nodebb_instance(), tmp)
        assert "REPO-SPECIFIC INSTRUCTIONS:" in prompt
        assert "Do NOT create new source files" in prompt

        prompt = build_agentless_prompt(_ansible_instance(), tmp)
        assert "REPO-SPECIFIC INSTRUCTIONS:" in prompt
        assert "Do NOT replace an entire module" in prompt


def test_build_agentless_prompt_unknown_repo_no_extra_text():
    with tempfile.TemporaryDirectory() as tmp:
        instance = _nodebb_instance()
        instance["repo"] = "unknown/repo"
        prompt = build_agentless_prompt(instance, tmp)
        assert "REPO-SPECIFIC INSTRUCTIONS:" not in prompt


def test_build_agentless_retry_prompt_includes_repo_template():
    with tempfile.TemporaryDirectory() as tmp:
        prompt = build_agentless_retry_prompt(_nodebb_instance(), tmp, "")
        assert "REPO-SPECIFIC INSTRUCTIONS:" in prompt
        assert "Do NOT create new source files" in prompt

        prompt = build_agentless_retry_prompt(_ansible_instance(), tmp, "")
        assert "REPO-SPECIFIC INSTRUCTIONS:" in prompt
        assert "Do NOT replace an entire module" in prompt


def test_build_agentless_retry_prompt_unknown_repo_no_extra_text():
    with tempfile.TemporaryDirectory() as tmp:
        instance = _nodebb_instance()
        instance["repo"] = "unknown/repo"
        prompt = build_agentless_retry_prompt(instance, tmp, "")
        assert "REPO-SPECIFIC INSTRUCTIONS:" not in prompt


def test_build_plan_prompt_includes_repo_template():
    prompt = build_plan_prompt(_nodebb_instance())
    assert "Repo-specific instructions:" in prompt
    assert "Do NOT create new source files" in prompt

    prompt = build_plan_prompt(_ansible_instance())
    assert "Repo-specific instructions:" in prompt
    assert "Do NOT replace an entire module" in prompt


def test_build_plan_prompt_unknown_repo_no_extra_text():
    instance = _nodebb_instance()
    instance["repo"] = "unknown/repo"
    prompt = build_plan_prompt(instance)
    assert "Repo-specific instructions:" not in prompt


def test_build_diff_fallback_prompt_includes_repo_template():
    with tempfile.TemporaryDirectory() as tmp:
        prompt = build_diff_fallback_prompt(
            "Repo: NodeBB/NodeBB @ abc123\nIssue:\nfix bug\n",
            ["a.js"],
            tmp,
            max_chars=100,
        )
        assert "Repo-specific instructions:" in prompt
        assert "Do NOT create new source files" in prompt

        prompt = build_diff_fallback_prompt(
            "Repo: /app (ansible/ansible @ def456)\nIssue:\nfix bug\n",
            ["a.py"],
            tmp,
            max_chars=100,
        )
        assert "Repo-specific instructions:" in prompt
        assert "Do NOT replace an entire module" in prompt


def test_build_diff_fallback_prompt_unknown_repo_no_extra_text():
    with tempfile.TemporaryDirectory() as tmp:
        prompt = build_diff_fallback_prompt(
            "Repo: unknown/repo @ abc123\nIssue:\nfix bug\n",
            ["a.js"],
            tmp,
            max_chars=100,
        )
        assert "Repo-specific instructions:" not in prompt
