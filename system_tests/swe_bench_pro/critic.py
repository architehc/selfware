#!/usr/bin/env python3
"""P2 critic loop for the SWE-bench Pro harness.

After the main model produces a patch, the critic loop re-runs the fail-to-pass
(F2P) tests and asks a (potentially stronger) model to refine the patch when the
tests still fail. It returns the best patch seen and records critic metadata.
"""

from __future__ import annotations

import hashlib
import logging
import os
import shlex
import subprocess
from pathlib import Path
from typing import Any


DEFAULT_F2P_OUTPUT_MAX_CHARS = 6000
CRITIC_TEST_TIMEOUT_SECONDS = 600


def _lazy_run_selfware() -> Any:
    """Import run_selfware lazily to avoid a circular import at module load."""
    import run_selfware as rs
    return rs


def _lazy_patch_utils() -> Any:
    """Import patch_utils lazily to avoid a circular import at module load."""
    import patch_utils as pu
    return pu


def _run_f2p_tests(
    host_repo_dir: Path,
    instance: dict[str, Any],
    patch: str,
    log_dir: Path,
    logger: logging.Logger,
) -> tuple[bool, str]:
    """Run the instance's fail-to-pass tests on the host repo.

    Returns ``(passed, output)``. Output is cached in ``log_dir`` keyed by the
    patch and test list so repeated critic invocations can reuse prior results.
    """
    rs = _lazy_run_selfware()
    language = (instance.get("repo_language") or "").lower()
    repo = instance.get("repo", "")
    fail_to_pass = rs.load_list_field(instance.get("fail_to_pass", []))

    if not fail_to_pass:
        return True, "(no fail-to-pass tests specified)"

    test_cmd = rs._format_test_command(language, fail_to_pass, repo=repo)
    if not test_cmd or test_cmd == "run the relevant test suite":
        return False, "No usable test command for fail-to-pass tests"

    cache_key = hashlib.sha256(
        (patch + "\n" + "\n".join(fail_to_pass) + "\n" + test_cmd).encode("utf-8")
    ).hexdigest()
    cache_path = Path(log_dir) / f"critic_f2p_{cache_key}.txt"
    if cache_path.exists():
        cached = cache_path.read_text(encoding="utf-8", errors="ignore")
        passed = cached.startswith("PASS\n")
        return passed, cached

    cache_path.parent.mkdir(parents=True, exist_ok=True)
    logger.info("Critic running fail-to-pass tests: %s", test_cmd)
    try:
        proc = subprocess.run(
            test_cmd,
            shell=True,
            cwd=str(host_repo_dir),
            capture_output=True,
            text=True,
            timeout=CRITIC_TEST_TIMEOUT_SECONDS,
            errors="replace",
        )
    except subprocess.TimeoutExpired as exc:
        output = f"F2P tests timed out after {CRITIC_TEST_TIMEOUT_SECONDS}s\n{exc}"
        cache_path.write_text("FAIL\n" + output, encoding="utf-8")
        return False, output

    passed = proc.returncode == 0
    output = (
        f"exit_code={proc.returncode}\n"
        f"stdout:\n{proc.stdout}\n"
        f"stderr:\n{proc.stderr}"
    )
    cache_path.write_text(("PASS\n" if passed else "FAIL\n") + output, encoding="utf-8")
    return passed, output


def _source_snippets(
    host_repo_dir: Path,
    patch: str,
    max_files: int = 8,
    max_lines: int = 80,
) -> str:
    """Read the first chunk of each file touched by ``patch``."""
    pu = _lazy_patch_utils()
    paths = sorted(pu.paths_from_patch(patch))[:max_files]
    snippets: list[str] = []
    for rel in paths:
        path = host_repo_dir / rel
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        lines = text.splitlines()[:max_lines]
        snippets.append(f"### {rel}\n" + "\n".join(lines))
    return "\n\n".join(snippets)


def _build_critic_prompt(
    instance: dict[str, Any],
    patch: str,
    test_output: str,
    host_repo_dir: Path,
    logger: logging.Logger,
) -> str:
    """Build a prompt asking the critic model to fix the failing patch."""
    if len(test_output) > DEFAULT_F2P_OUTPUT_MAX_CHARS:
        test_output = test_output[:DEFAULT_F2P_OUTPUT_MAX_CHARS] + "\n... (truncated) ..."

    snippets = _source_snippets(host_repo_dir, patch)

    problem = instance.get("problem_statement", "") or ""
    requirements = instance.get("requirements", "") or ""

    return f"""You are a code-review critic. A patch was proposed for the issue below, but the fail-to-pass tests still fail. Produce a corrected patch that makes those tests pass while keeping the change minimal.

Issue:
{problem}

Requirements:
{requirements}

Current patch:
```diff
{patch}
```

Fail-to-pass test output:
```
{test_output}
```

Relevant source files:
{snippets}

Return ONLY the corrected patch as a unified git diff or as ### FILE / SEARCH / REPLACE blocks. Do not explain.
"""


def _apply_critic_response(
    host_repo_dir: Path,
    response: str,
    logger: logging.Logger,
) -> bool:
    """Apply the critic's response to the host repo.

    Tries the same tolerant appliers used elsewhere: unified diff first, then
    SEARCH/REPLACE edits.
    """
    pu = _lazy_patch_utils()
    applied, _ = pu.apply_model_response_with_missing(host_repo_dir, response, logger)
    return applied


def _capture_current_patch(
    host_repo_dir: Path,
    base_commit: str | None,
    logger: logging.Logger,
) -> str:
    """Capture the git diff for the current working tree state."""
    rs = _lazy_run_selfware()
    diff = rs.capture_patch_on_host(host_repo_dir, logger, base_commit=base_commit)
    return rs.clean_captured_diff(diff)


def run_critic_loop(
    host_repo_dir: Path,
    instance: dict[str, Any],
    patch: str,
    patch_config: dict[str, Any],
    critic_config: dict[str, Any],
    args: Any,
    log_dir: Path,
    logger: logging.Logger,
    name: str | None = None,
    *,
    metadata: dict[str, Any] | None = None,
) -> str:
    """Iterate with a critic model until F2P tests pass or iterations are exhausted.

    ``patch_config`` is the configuration used to generate the original patch;
    ``critic_config`` is the configuration used for the critic model. When the
    F2P tests already pass, the original patch is returned immediately.

    Metadata fields ``critic_fired``, ``critic_iterations``,
    ``critic_succeeded``, and ``critic_failed`` are updated in place.
    """
    iterations = getattr(args, "critic_iterations", 0)
    if iterations <= 0:
        return patch

    if metadata is None:
        metadata = {}
    metadata["critic_fired"] = True
    metadata.setdefault("critic_succeeded", False)
    metadata.setdefault("critic_failed", False)

    rs = _lazy_run_selfware()
    base_commit = instance.get("base_commit")
    best_patch = patch
    passed = False

    for i in range(iterations):
        passed, test_output = _run_f2p_tests(
            host_repo_dir, instance, best_patch, log_dir, logger
        )
        metadata["critic_iterations"] = i
        if passed:
            logger.info("Critic: fail-to-pass tests passed on iteration %d", i + 1)
            metadata["critic_succeeded"] = True
            metadata["critic_failed"] = False
            return _capture_current_patch(host_repo_dir, base_commit, logger) or best_patch

        logger.info(
            "Critic iteration %d/%d: F2P tests failed, asking critic for refinement",
            i + 1,
            iterations,
        )
        metadata["critic_iterations"] = i + 1
        prompt = _build_critic_prompt(instance, best_patch, test_output, host_repo_dir, logger)
        response = rs.call_chat_endpoint(
            critic_config,
            prompt,
            getattr(args, "critic_timeout", 300),
            logger,
            max_tokens=getattr(args, "critic_max_tokens", 16384),
            temperature=getattr(args, "critic_temperature", None),
            allow_token_growth=False,
        )

        if not _apply_critic_response(host_repo_dir, response, logger):
            logger.warning("Critic response could not be applied; stopping critic loop")
            break

        best_patch = _capture_current_patch(host_repo_dir, base_commit, logger) or best_patch

    metadata["critic_failed"] = not passed
    return best_patch
