"""Tests for the P2 critic loop."""

import argparse
import logging
import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

import critic


def _fake_logger():
    return logging.getLogger("test-critic")


def _fake_args(**kwargs):
    defaults = {
        "critic_iterations": 1,
        "critic_timeout": 300,
        "critic_max_tokens": 4096,
        "critic_temperature": 0.1,
    }
    defaults.update(kwargs)
    return argparse.Namespace(**defaults)


def test_run_critic_loop_zero_iterations_returns_patch_unchanged():
    args = _fake_args(critic_iterations=0)
    patch = "diff --git a/src.py b/src.py\n..."
    metadata = {}
    result = critic.run_critic_loop(
        Path("/tmp"),
        {},
        patch,
        {},
        {},
        args,
        Path("/tmp"),
        _fake_logger(),
        metadata=metadata,
    )
    assert result == patch
    assert metadata.get("critic_fired") is None


def test_run_critic_loop_stops_early_when_f2p_already_passes(monkeypatch):
    args = _fake_args(critic_iterations=3)
    patch = "original patch"

    class FakeRunSelfware:
        load_list_field = staticmethod(lambda v: list(v) if isinstance(v, list) else [])
        _format_test_command = staticmethod(
            lambda lang, tests, repo=None: "pytest " + " ".join(tests)
        )
        capture_patch_on_host = staticmethod(
            lambda repo, logger, base_commit=None: "final patch"
        )
        clean_captured_diff = staticmethod(lambda d: d)

    monkeypatch.setattr(critic, "_lazy_run_selfware", lambda: FakeRunSelfware())
    monkeypatch.setattr(critic, "_run_f2p_tests", lambda *a, **k: (True, "passing"))

    metadata = {}
    result = critic.run_critic_loop(
        Path("/tmp"),
        {"fail_to_pass": ["test_x.py::test_x"]},
        patch,
        {},
        {},
        args,
        Path("/tmp"),
        _fake_logger(),
        metadata=metadata,
    )
    assert result == "final patch"
    assert metadata["critic_fired"] is True
    assert metadata["critic_succeeded"] is True
    assert metadata["critic_failed"] is False
    assert metadata["critic_iterations"] == 0


def test_run_critic_loop_applies_refinement_until_f2p_passes(monkeypatch):
    args = _fake_args(critic_iterations=3)
    patch = "original patch"

    f2p_results = [(False, "fail output"), (True, "pass output")]

    class FakeRunSelfware:
        load_list_field = staticmethod(lambda v: list(v) if isinstance(v, list) else [])
        _format_test_command = staticmethod(
            lambda lang, tests, repo=None: "pytest " + " ".join(tests)
        )
        capture_patch_on_host = staticmethod(
            lambda repo, logger, base_commit=None: "critic patch"
        )
        clean_captured_diff = staticmethod(lambda d: d)
        call_chat_endpoint = staticmethod(
            lambda config, prompt, timeout, logger, **kwargs: "critic response"
        )

    class FakePatchUtils:
        apply_model_response_with_missing = staticmethod(
            lambda repo, response, logger: (True, set())
        )
        extract_diff = staticmethod(lambda response: None)
        paths_from_patch = staticmethod(lambda patch: set())

    monkeypatch.setattr(critic, "_lazy_run_selfware", lambda: FakeRunSelfware())
    monkeypatch.setattr(critic, "_lazy_patch_utils", lambda: FakePatchUtils())
    monkeypatch.setattr(
        critic, "_run_f2p_tests", lambda *a, **k: f2p_results.pop(0)
    )

    metadata = {}
    result = critic.run_critic_loop(
        Path("/tmp"),
        {"fail_to_pass": ["test_x.py::test_x"]},
        patch,
        {},
        {},
        args,
        Path("/tmp"),
        _fake_logger(),
        metadata=metadata,
    )
    assert result == "critic patch"
    assert metadata["critic_succeeded"] is True
    assert metadata["critic_failed"] is False
    assert metadata["critic_iterations"] == 1


def test_run_critic_loop_records_failure_when_response_unapplyable(monkeypatch):
    args = _fake_args(critic_iterations=2)
    patch = "original patch"

    class FakeRunSelfware:
        load_list_field = staticmethod(lambda v: list(v) if isinstance(v, list) else [])
        _format_test_command = staticmethod(
            lambda lang, tests, repo=None: "pytest " + " ".join(tests)
        )
        capture_patch_on_host = staticmethod(
            lambda repo, logger, base_commit=None: "should not be used"
        )
        clean_captured_diff = staticmethod(lambda d: d)
        call_chat_endpoint = staticmethod(
            lambda config, prompt, timeout, logger, **kwargs: "bad response"
        )

    class FakePatchUtils:
        apply_model_response_with_missing = staticmethod(
            lambda repo, response, logger: (False, {"src.py"})
        )
        extract_diff = staticmethod(lambda response: None)
        paths_from_patch = staticmethod(lambda patch: set())

    monkeypatch.setattr(critic, "_lazy_run_selfware", lambda: FakeRunSelfware())
    monkeypatch.setattr(critic, "_lazy_patch_utils", lambda: FakePatchUtils())
    monkeypatch.setattr(
        critic, "_run_f2p_tests", lambda *a, **k: (False, "still failing")
    )

    metadata = {}
    result = critic.run_critic_loop(
        Path("/tmp"),
        {"fail_to_pass": ["test_x.py::test_x"]},
        patch,
        {},
        {},
        args,
        Path("/tmp"),
        _fake_logger(),
        metadata=metadata,
    )
    assert result == patch
    assert metadata["critic_succeeded"] is False
    assert metadata["critic_failed"] is True
    assert metadata["critic_iterations"] == 1
