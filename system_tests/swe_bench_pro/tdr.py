"""Containerized Test-Driven Repair (TDR) loop for SWE-bench Pro.

This module runs the official SWE-bench Pro ``fail_to_pass``/``pass_to_pass``
tests inside the official Docker image and, if tests fail, asks a strong repair
model for a corrected patch, iterating up to ``N`` times.
"""

from __future__ import annotations

import json
import re
import shlex
import subprocess
import traceback
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

from patch_utils import _extract_diff_path


SWEBENCH_PRO_ROOT = Path("/tmp/SWE-bench_Pro-os")
CONTAINER_REPO_DIR = "/app"


def _load_list_field(value: Any) -> list[str]:
    """Normalize a HF dataset field that may be a JSON-encoded list or a real list."""
    import ast

    if isinstance(value, list):
        return [str(x) for x in value]
    if isinstance(value, str):
        value = value.strip()
        if not value:
            return []
        try:
            parsed = ast.literal_eval(value)
            if isinstance(parsed, list):
                return [str(x) for x in parsed]
        except (SyntaxError, ValueError):
            pass
        return [value]
    return []


def _ensure_container_running(
    image: str,
    container: str,
    logger: Any,
) -> bool:
    """Start a container if it is not already running."""
    from run_selfware import podman, start_container

    info = podman("inspect", container, "--format", "{{.State.Status}}", logger=logger)
    if info.returncode == 0 and info.stdout.strip() == "running":
        return True
    return start_container(image, container, logger, timeout=120)


def _copy_artifact_into(
    src: Path,
    dst: str,
    container: str,
    logger: Any,
) -> bool:
    """Copy a host file into a container path."""
    from run_selfware import copy_into_container

    return copy_into_container(src, dst, container, logger)


def _copy_artifact_out(
    container: str,
    src: str,
    dst: Path,
    logger: Any,
) -> bool:
    """Copy a file from a container path to the host."""
    from run_selfware import podman

    proc = podman("cp", f"{container}:{src}", str(dst), logger=logger)
    if proc.returncode != 0:
        logger.error("Failed to copy %s:%s to %s: %s", container, src, dst, proc.stderr.strip())
        return False
    return True


def _normalize_patch_for_dedup(patch: str) -> str:
    """Return a canonical form of a patch for near-duplicate detection."""
    return "\n".join(line.rstrip() for line in patch.splitlines() if line.strip())


def _read_ranked_file_excerpts(
    repo_dir: Path,
    ranked_files: list[str],
    max_lines: int = 300,
) -> dict[str, str]:
    """Read a bounded excerpt from each ranked source file."""
    excerpts: dict[str, str] = {}
    for rel in ranked_files:
        path = repo_dir / rel
        if not path.is_file():
            continue
        try:
            lines = path.read_text(encoding="utf-8", errors="ignore").splitlines()
            if len(lines) <= max_lines:
                excerpts[rel] = "\n".join(lines)
            else:
                half = max_lines // 2
                excerpts[rel] = "\n".join(
                    lines[:half]
                    + [f"\n... ({len(lines) - max_lines} lines omitted) ...\n"]
                    + lines[-half:]
                )
        except Exception:
            continue
    return excerpts


def _build_entryscript(instance: dict[str, Any]) -> str:
    """Build the container entry script that applies the patch and runs tests.

    The script is strict: if the patch is empty or cannot be applied, it writes
    an output.json marking the instance as failed instead of silently running
    tests on the unpatched base commit.
    """
    selected = _load_list_field(instance.get("selected_test_files_to_run", []))
    test_arg = " ".join(shlex.quote(t) for t in selected)
    before_cmd = instance.get("before_repo_set_cmd", "") or ""
    return (
        "#!/bin/bash\n"
        "set -uo pipefail\n"
        """trap 'if [ ! -f /workspace/output.json ]; then echo '"'"'{"tests": []}'"'"' > /workspace/output.json; fi' EXIT\n"""
        "cd /app\n"
        #
        # Reset defensively and capture the optional setup command status.
        #
        f"git reset --hard {instance['base_commit']} > /workspace/git_reset.log 2>&1\n"
        f"git checkout {instance['base_commit']} > /workspace/git_checkout.log 2>&1 || true\n"
        f"{before_cmd}\n"
        "before_status=$?\n"
        "echo \"before_repo_set_cmd exit code: $before_status\" >> /workspace/patch_apply.log\n"
        #
        # Empty-patch short-circuit.
        #
        "if [ ! -s /workspace/patch.diff ] || [ \"$(grep -v '^[[:space:]]*$' /workspace/patch.diff | wc -l)\" -eq 0 ]; then\n"
        "  echo '{\"tests\": []}' > /workspace/output.json\n"
        "  echo 'PATCH_EMPTY' > /workspace/patch_apply_status.txt\n"
        "  exit 0\n"
        "fi\n"
        #
        # Patch application with git apply, 3-way, and patch fallback.
        #
        "git apply -v /workspace/patch.diff > /workspace/patch_apply.log 2>&1\n"
        "apply_status=$?\n"
        "if [ $apply_status -ne 0 ]; then\n"
        "  git apply --3way -v /workspace/patch.diff >> /workspace/patch_apply.log 2>&1\n"
        "  apply_status=$?\n"
        "fi\n"
        "if [ $apply_status -ne 0 ]; then\n"
        "  patch -p1 --no-backup-if-mismatch -i /workspace/patch.diff >> /workspace/patch_apply.log 2>&1\n"
        "  apply_status=$?\n"
        "fi\n"
        "echo \"patch apply exit code: $apply_status\" >> /workspace/patch_apply.log\n"
        "echo $apply_status > /workspace/patch_apply_status.txt\n"
        "if [ $apply_status -ne 0 ]; then\n"
        "  echo '{\"tests\": []}' > /workspace/output.json\n"
        "  exit 0\n"
        "fi\n"
        #
        # No-op patch detection: applied but changed no files.
        #
        "changed_files=$(git diff --name-only HEAD)\n"
        "untracked_files=$(git ls-files --others --exclude-standard)\n"
        "if [ -z \"$changed_files\" ] && [ -z \"$untracked_files\" ]; then\n"
        "  echo '{\"tests\": []}' > /workspace/output.json\n"
        "  echo 'PATCH_NO_OP' > /workspace/patch_apply_status.txt\n"
        "  exit 0\n"
        "fi\n"
        #
        # Run tests and parse results.
        #
        # Clear stale artifacts from any previous iteration so a timeout or
        # crash cannot be mis-scored using old output.
        "rm -f /workspace/output.json /workspace/patch_apply_status.txt\n"
        f"bash /workspace/run_script.sh {test_arg} > /workspace/stdout.log 2> /workspace/stderr.log\n"
        "run_status=$?\n"
        "python /workspace/parser.py /workspace/stdout.log /workspace/stderr.log /workspace/output.json\n"
        "if [ ! -f /workspace/output.json ]; then\n"
        "  echo '{\"tests\": []}' > /workspace/output.json\n"
        "fi\n"
        "exit $run_status\n"
    )


def _score_test_output(
    output: dict[str, Any],
    instance: dict[str, Any],
) -> dict[str, Any]:
    """Compute fail/pass counts from the parsed parser.py output."""
    fail_to_pass = _load_list_field(instance.get("fail_to_pass", []))
    pass_to_pass = _load_list_field(instance.get("pass_to_pass", []))

    status_map: dict[str, str] = {}
    for test in output.get("tests", []):
        name = test.get("name", "")
        status = test.get("status", "")
        if name:
            status_map[name] = status.upper()

    fail_passed = 0
    failing_fail: list[tuple[str, str]] = []
    for t in fail_to_pass:
        status = status_map.get(t, "MISSING")
        if status == "PASSED":
            fail_passed += 1
        else:
            failing_fail.append((t, status))

    pass_passed = 0
    failing_pass: list[tuple[str, str]] = []
    for t in pass_to_pass:
        status = status_map.get(t, "MISSING")
        if status in ("PASSED", "SKIPPED"):
            pass_passed += 1
        else:
            failing_pass.append((t, status))

    total_score = fail_passed + pass_passed
    perfect = (
        fail_passed == len(fail_to_pass)
        and pass_passed == len(pass_to_pass)
        and len(fail_to_pass) + len(pass_to_pass) > 0
    )

    return {
        "status_map": status_map,
        "fail_passed": fail_passed,
        "fail_total": len(fail_to_pass),
        "pass_passed": pass_passed,
        "pass_total": len(pass_to_pass),
        "total_score": total_score,
        "perfect": perfect,
        "failing_fail": failing_fail,
        "failing_pass": failing_pass,
    }


def _compile_check_command(instance: dict[str, Any]) -> list[str] | None:
    """Return a container command that checks whether the patch compiles.

    Returns ``None`` when no language-specific compile check is implemented.
    """
    language = (instance.get("repo_language") or "").lower()
    if language == "go":
        return ["bash", "-c", "cd /app && go build ./... 2>&1"]
    if language in ("javascript", "typescript"):
        return ["bash", "-c", "cd /app && npm run build 2>&1"]
    return None


def _run_compile_check(
    container: str,
    instance: dict[str, Any],
    logger: Any,
    timeout: int = 180,
) -> str | None:
    """Run a compile check and return stderr/stdout on failure, or None on success."""
    from run_selfware import podman

    cmd = _compile_check_command(instance)
    if cmd is None:
        return None
    logger.info("Running compile check for %s", instance.get("instance_id"))
    try:
        proc = podman("exec", container, *cmd, timeout=timeout, logger=logger)
    except subprocess.TimeoutExpired:
        logger.warning("Compile check timed out after %ss", timeout)
        return "compile check timed out"
    if proc.returncode == 0:
        logger.info("Compile check passed")
        return None
    return proc.stdout + "\n" + proc.stderr


def _run_tests_once(
    image: str,
    container: str,
    instance: dict[str, Any],
    patch: str,
    args: Any,
    log_dir: Path,
    logger: Any,
    iteration: int,
) -> dict[str, Any]:
    """Run one evaluation iteration and return scoring metadata.

    This performs steps 1-7 of the TDR loop for a single patch.
    """
    from run_selfware import podman, pull_image

    instance_id = instance["instance_id"]
    result: dict[str, Any] = {
        "iteration": iteration,
        "total_score": -1,
        "perfect": False,
        "output_path": None,
        "stdout": "",
        "stderr": "",
        "error": None,
        "compile_error": None,
    }

    if not pull_image(image, logger):
        result["error"] = "failed to pull image"
        return result

    if not _ensure_container_running(image, container, logger):
        result["error"] = "failed to start container"
        return result

    mkdir = podman("exec", container, "mkdir", "-p", "/workspace", logger=logger)
    if mkdir.returncode != 0:
        logger.error("Failed to create /workspace in %s: %s", container, mkdir.stderr.strip())
        result["error"] = "failed to create /workspace"
        return result

    # Stage artifacts on the host.
    patch_file = log_dir / f"{container}.tdr.{iteration}.patch.diff"
    entryscript_file = log_dir / f"{container}.tdr.{iteration}.entryscript.sh"
    patch_file.write_text(patch, encoding="utf-8")
    entryscript_file.write_text(_build_entryscript(instance), encoding="utf-8")

    # Copy the patch and official evaluation files into the container.
    run_script_src = SWEBENCH_PRO_ROOT / "run_scripts" / instance_id / "run_script.sh"
    parser_src = SWEBENCH_PRO_ROOT / "run_scripts" / instance_id / "parser.py"
    if not run_script_src.exists() or not parser_src.exists():
        logger.error(
            "Missing official evaluation files for %s: %s / %s",
            instance_id,
            run_script_src,
            parser_src,
        )
        result["error"] = "missing evaluation files"
        return result

    ok = (
        _copy_artifact_into(patch_file, "/workspace/patch.diff", container, logger)
        and _copy_artifact_into(run_script_src, "/workspace/run_script.sh", container, logger)
        and _copy_artifact_into(parser_src, "/workspace/parser.py", container, logger)
        and _copy_artifact_into(entryscript_file, "/workspace/entryscript.sh", container, logger)
    )
    if not ok:
        result["error"] = "failed to copy artifacts into container"
        return result

    # Run the official evaluator entry script (applies the patch and runs tests).
    logger.info(
        "Running TDR test iteration %s for %s in %s (timeout=%ss)",
        iteration,
        instance_id,
        container,
        args.tdr_test_timeout,
    )
    try:
        proc = podman(
            "exec",
            container,
            "bash",
            "/workspace/entryscript.sh",
            timeout=args.tdr_test_timeout,
            logger=logger,
        )
        exec_rc = proc.returncode
    except subprocess.TimeoutExpired:
        logger.error(
            "TDR test iteration %s for %s timed out after %ss",
            iteration,
            instance_id,
            args.tdr_test_timeout,
        )
        exec_rc = -1
        result["error"] = "test execution timed out"

    output_file = log_dir / f"{container}.tdr.{iteration}.output.json"
    stdout_file = log_dir / f"{container}.tdr.{iteration}.stdout.log"
    stderr_file = log_dir / f"{container}.tdr.{iteration}.stderr.log"
    patch_apply_log = log_dir / f"{container}.tdr.{iteration}.patch_apply.log"
    patch_apply_status = log_dir / f"{container}.tdr.{iteration}.patch_apply_status.txt"

    _copy_artifact_out(container, "/workspace/output.json", output_file, logger)
    _copy_artifact_out(container, "/workspace/stdout.log", stdout_file, logger)
    _copy_artifact_out(container, "/workspace/stderr.log", stderr_file, logger)
    _copy_artifact_out(container, "/workspace/patch_apply.log", patch_apply_log, logger)
    _copy_artifact_out(container, "/workspace/patch_apply_status.txt", patch_apply_status, logger)

    stdout_text = stdout_file.read_text(encoding="utf-8", errors="ignore") if stdout_file.exists() else ""
    stderr_text = stderr_file.read_text(encoding="utf-8", errors="ignore") if stderr_file.exists() else ""
    result["stdout"] = stdout_text
    result["stderr"] = stderr_text

    # Compile check: only run when the patch was applied successfully so we
    # validate the patched code, not the base commit.
    patch_status_text = ""
    if patch_apply_status.exists():
        patch_status_text = patch_apply_status.read_text(encoding="utf-8").strip()
    if patch_status_text == "0":
        compile_error = _run_compile_check(
            container, instance, logger, timeout=getattr(args, "tdr_compile_timeout", 180)
        )
        if compile_error:
            logger.warning("Compile check failed for %s iteration %s", instance_id, iteration)
            result["compile_error"] = compile_error
            if result["error"] is None:
                result["error"] = "compile check failed"
    else:
        logger.info(
            "Skipping compile check for %s iteration %s because patch was not applied (status=%r)",
            instance_id,
            iteration,
            patch_status_text,
        )

    if not output_file.exists():
        logger.error("No output.json produced for %s iteration %s", instance_id, iteration)
        if result["error"] is None:
            result["error"] = "no output.json"
        return result

    try:
        output = json.loads(output_file.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        logger.error("Failed to parse output.json for %s iteration %s: %s", instance_id, iteration, exc)
        result["error"] = f"output.json parse error: {exc}"
        return result

    score = _score_test_output(output, instance)
    result.update(score)
    result["output_path"] = output_file
    logger.info(
        "TDR iteration %s for %s: fail %s/%s, pass %s/%s, total_score=%s, perfect=%s",
        iteration,
        instance_id,
        score["fail_passed"],
        score["fail_total"],
        score["pass_passed"],
        score["pass_total"],
        score["total_score"],
        score["perfect"],
    )
    return result


def _build_failure_summary(result: dict[str, Any]) -> str:
    """Build a concise summary of failing tests for the repair prompt."""
    lines: list[str] = []
    compile_error = result.get("compile_error")
    if compile_error:
        lines.append("COMPILE ERROR (fix this first; tests cannot run until it is resolved):")
        lines.append(compile_error[:2000])
        lines.append("")
    failing_fail = result.get("failing_fail", [])
    failing_pass = result.get("failing_pass", [])
    if failing_fail:
        lines.append("Failing fail-to-pass tests:")
        for name, status in failing_fail:
            lines.append(f"- {name} ({status})")
    if failing_pass:
        lines.append("Failing pass-to-pass tests:")
        for name, status in failing_pass:
            lines.append(f"- {name} ({status})")
    if not lines:
        lines.append("No individual test failures recorded.")
    return "\n".join(lines)


def _tail(text: str, max_chars: int = 1200) -> str:
    """Return the last ``max_chars`` characters of ``text``."""
    if len(text) <= max_chars:
        return text
    return "\n... (truncated) ...\n" + text[-max_chars:]


def _truncate_patch(current_patch: str, max_lines: int = 200) -> str:
    """Return a bounded view of the current patch to save prompt tokens."""
    lines = current_patch.splitlines()
    if len(lines) <= max_lines:
        return current_patch
    return "\n".join(
        lines[: max_lines // 2]
        + [f"\n... ({len(lines) - max_lines} patch lines omitted) ...\n"]
        + lines[-max_lines // 2 :]
    )


def _read_focused_excerpts(
    repo_dir: Path,
    ranked_files: list[str],
    failing_tests: list[tuple[str, str]],
    max_lines_per_file: int = 80,
) -> dict[str, str]:
    """Read focused excerpts around failing test names / likely identifiers.

    This avoids dumping huge files into the repair prompt and keeps the model
    focused on the code most likely to need changes.
    """
    from small_model_adapter import _tokenize_problem

    # Build a set of identifiers from failing test names (strip subtests).
    identifiers: set[str] = set()
    for name, _ in failing_tests:
        # "TestFoo/bar" -> "TestFoo"; also accept python dotted forms.
        root = name.split("/")[0].split(".")[-1].split("#")[0].strip()
        if root:
            identifiers.add(root)
            identifiers.update(_tokenize_problem(root))

    excerpts: dict[str, str] = {}
    for rel in ranked_files:
        path = repo_dir / rel
        if not path.is_file():
            continue
        try:
            lines = path.read_text(encoding="utf-8", errors="ignore").splitlines()
        except Exception:
            continue
        if not lines:
            continue

        # Find matching lines.
        match_lines: set[int] = set()
        for ident in identifiers:
            if not ident:
                continue
            rx = re.compile(r"\b" + re.escape(ident) + r"\b", re.IGNORECASE)
            for i, line in enumerate(lines):
                if rx.search(line):
                    match_lines.add(i)

        # If no direct match, fall back to first/last window so the file is not omitted.
        if not match_lines:
            half = max_lines_per_file // 2
            if len(lines) <= max_lines_per_file:
                selected = list(range(len(lines)))
            else:
                selected = list(range(half)) + list(range(len(lines) - half, len(lines)))
        else:
            window = max_lines_per_file // max(1, len(match_lines))
            window = max(15, min(window, max_lines_per_file // 2))
            selected: set[int] = set()
            for m in sorted(match_lines):
                selected.update(range(max(0, m - window), min(len(lines), m + window + 1)))
            selected = set(sorted(selected)[:max_lines_per_file])

        if not selected:
            continue
        rendered = [f"{i + 1:5d} | {lines[i]}" for i in sorted(selected)]
        excerpts[rel] = "\n".join(rendered)
    return excerpts


def _files_in_patch(patch: str) -> list[str]:
    """Return the file paths referenced in a unified diff."""
    files: list[str] = []
    seen: set[str] = set()
    for line in patch.splitlines():
        path = _extract_diff_path(line)
        if path and path not in seen:
            files.append(path)
            seen.add(path)
    return files


def _read_exact_source_files(
    repo_dir: Path,
    files: list[str],
    max_lines: int = 500,
    max_total_chars: int = 12000,
) -> str:
    """Read exact current contents of the given files for a repair prompt."""
    from small_model_adapter import _build_editable_manifest

    parts: list[str] = []
    total = 0
    for rel in files:
        path = repo_dir / rel
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        lines = text.splitlines()
        header = f"--- {rel} ---"
        if len(lines) <= max_lines:
            body = "\n".join(f"{i + 1:5d} | {line}" for i, line in enumerate(lines))
            part = f"{header}\nFULL FILE:\n{body}\n"
        else:
            half = max_lines // 2
            body_lines = (
                [f"{i + 1:5d} | {lines[i]}" for i in range(half)]
                + [f"\n... ({len(lines) - max_lines} lines omitted) ...\n"]
                + [f"{len(lines) - half + i + 1:5d} | {lines[-half + i]}" for i in range(half)]
            )
            body = "\n".join(body_lines)
            part = f"{header}\nEXCERPT:\n{body}\n"
        if total + len(part) > max_total_chars:
            part = part[: max_total_chars - total] + "\n... (truncated due to prompt budget) ...\n"
        parts.append(part)
        total += len(part)
        if total >= max_total_chars:
            break
    manifest = _build_editable_manifest(files, repo_dir, max_files=20)
    return "".join(parts) + "\nEditable files manifest (you may ONLY edit files listed here):\n" + manifest


def _build_repair_prompt(
    instance: dict[str, Any],
    current_patch: str,
    test_result: dict[str, Any],
    ranked_files: list[str] | None,
    repo_dir: Path,
    *,
    strict: bool = False,
) -> str:
    """Build a compact prompt asking the repair model to fix the remaining failures."""
    failing_fail = test_result.get("failing_fail", [])
    failing_pass = test_result.get("failing_pass", [])
    all_failing = failing_fail + failing_pass

    if strict:
        opener = (
            "You are a repair assistant. The previous repair patch could not be applied or was invalid. "
            "Return ONLY corrected SEARCH/REPLACE blocks that fix the remaining failures. "
            "No prose, no explanation, no markdown outside the blocks."
        )
    else:
        opener = (
            "You are a repair assistant. The initial patch below was produced for the issue, but some tests still fail. "
            "Produce a corrected unified diff that makes all tests pass."
        )

    sections = [
        opener,
        "",
    ]

    if strict:
        sections.extend([
            "CRITICAL FORMAT RULES:",
            "- Output ONLY SEARCH/REPLACE blocks.",
            "- Each block must start with ### FILE: <path> followed by <<<<<<< SEARCH, =======, and >>>>>>> REPLACE.",
            "- Do NOT write explanations or markdown fences around the whole patch.",
            "- The SEARCH text must match the EXACT current source lines below (including indentation).",
            "- Do NOT invent file paths. Only edit files listed in the Editable files manifest.",
            "- Files marked FULL FILE are shown completely. Files marked EXCERPT are truncated; only SEARCH for text inside the excerpt.",
            "- Line numbers in the left gutter are for reference ONLY.",
        ])
    else:
        sections.extend([
            "CRITICAL FORMAT RULES:",
            "- Output ONLY a single markdown code block containing a unified git diff.",
            "- Start your response with ```diff on its own line.",
            "- Do NOT write an explanation, analysis, or prose before the diff.",
            "- Do NOT wrap the diff in any other markdown or text.",
            "- The diff must apply cleanly to the current patched code with `git apply`.",
            "- Use correct `@@ -start,len +start,len @@` hunk headers.",
        ])

    sections.extend([
        "",
        "Problem statement:",
        instance.get("problem_statement", ""),
        "",
        "Requirements:",
        instance.get("requirements", ""),
        "",
    ])

    if strict:
        patch_files = _files_in_patch(current_patch)
        source_files = patch_files[:]
        if ranked_files:
            for rel in ranked_files:
                if rel not in source_files:
                    source_files.append(rel)
                if len(source_files) >= 6:
                    break
        exact_sources = _read_exact_source_files(repo_dir, source_files)
        sections.extend([
            "Current patch (for reference, the same changes are already applied to the files below):",
            "```diff",
            _truncate_patch(current_patch, max_lines=80),
            "```",
            "",
            "Exact current source files. Copy SEARCH text from these lines exactly:",
            exact_sources,
            "",
            "If the fix requires a file not listed above, respond with NO_PATCH and nothing else.",
        ])
    else:
        sections.extend([
            "Current patch (unified diff):",
            "```diff",
            _truncate_patch(current_patch, max_lines=200),
            "```",
        ])

    sections.extend([
        "",
        "Test failure summary:",
        _build_failure_summary(test_result),
        "",
        "Excerpt from test output:",
        "STDOUT:",
        _tail(test_result.get("stdout", "")),
        "",
        "STDERR:",
        _tail(test_result.get("stderr", "")),
    ])

    if not strict and ranked_files:
        excerpts = _read_focused_excerpts(
            repo_dir, ranked_files, all_failing, max_lines_per_file=80
        )
        if excerpts:
            sections.extend(["", "Relevant source files (focused excerpts):"])
            for rel, content in excerpts.items():
                sections.extend([f"\n--- {rel} ---\n", content])

    sections.extend([
        "",
        "Fix only source files. Do not modify tests, configs, docs, or unrelated code. Keep the patch minimal.",
    ])
    return "\n".join(sections)


def _looks_truncated(response: str) -> bool:
    """Return True if the response appears to end inside a diff code block."""
    diff_block = re.search(r"```diff\s*(.*)", response, re.DOTALL)
    if not diff_block:
        # No diff block started; treat as invalid so we can retry.
        return True
    after_start = response[diff_block.start():]
    # If there's a closing ``` after the start, it's complete.
    close_match = re.search(r"```", after_start[len("```diff"):])
    return close_match is None


def _strip_incomplete_diff(response: str) -> str:
    """If a diff block is unclosed, drop everything after the last hunk line.

    This keeps a partial response from causing git apply to misparse trailing
    prose.  Returns the original string if the diff block is properly closed.
    """
    if not _looks_truncated(response):
        return response
    # Find the last line that looks like a diff line.
    lines = response.splitlines()
    last_good = -1
    for i, line in enumerate(lines):
        if line.startswith(("diff --git", "--- ", "+++ ", "@@", "-", "+", " ")):
            last_good = i
    if last_good > 0:
        return "\n".join(lines[: last_good + 1]) + "\n```"
    return response


def _apply_repair_and_capture(
    repo_dir: Path,
    base_commit: str,
    current_patch: str,
    response: str,
    logger: Any,
) -> str | None:
    """Reset to base, apply the current patch, then apply the repair response.

    Returns the new unified diff, or ``None`` if it could not be captured.
    """
    from run_selfware import capture_patch_on_host, run_cmd
    from patch_utils import apply_model_response

    reset = run_cmd(["git", "-C", str(repo_dir), "reset", "--hard", base_commit], logger=logger)
    if reset.returncode != 0:
        logger.error("Failed to reset repo before repair: %s", reset.stderr.strip())
        return None
    checkout = run_cmd(["git", "-C", str(repo_dir), "checkout", base_commit], logger=logger)
    if checkout.returncode != 0:
        logger.warning("git checkout before repair failed: %s", checkout.stderr.strip())

    # Re-apply the patch we are repairing so the model can reason from a known state.
    if current_patch.strip():
        from patch_utils import apply_patch

        if not apply_patch(repo_dir, current_patch, logger):
            logger.warning("Could not re-apply current patch before repair")
            return None

    cleaned_response = _strip_incomplete_diff(response)
    applied = apply_model_response(repo_dir, cleaned_response, logger)
    if not applied:
        # Try to salvage a partial diff if the response was truncated mid-hunk.
        from patch_utils import extract_partial_diff, is_truncated_diff
        if is_truncated_diff(response):
            partial = extract_partial_diff(response)
            if partial:
                logger.info("Attempting to apply partial truncated diff for repair")
                applied = apply_model_response(repo_dir, partial, logger)
                if applied:
                    cleaned_response = partial
        if not applied:
            return None
    new_patch = capture_patch_on_host(repo_dir, logger, base_commit=base_commit)
    if not new_patch.strip():
        logger.warning("Repair response applied but produced an empty diff")
        return None
    return new_patch


def run_test_driven_repair(
    host_repo_dir: Path,
    container_name: str,
    image: str,
    instance: dict[str, Any],
    initial_patch: str,
    repair_config: dict[str, Any],
    args: Any,
    log_dir: Path,
    logger: Any,
    ranked_files: list[str] | None = None,
) -> str:
    """Run tests and repair iterations. Return the best patch (may be initial_patch)."""
    from run_selfware import (
        call_chat_endpoint,
        capture_patch_on_host,
        run_cmd,
        stop_and_remove_container,
    )

    instance_id = instance["instance_id"]
    base_commit = instance["base_commit"]
    best_patch = initial_patch
    best_score = -1
    current_patch = initial_patch

    try:
        for iteration in range(args.repair_iterations + 1):
            result = _run_tests_once(
                image,
                container_name,
                instance,
                current_patch,
                args,
                log_dir,
                logger,
                iteration,
            )

            if result.get("error") and result["total_score"] < 0:
                logger.error(
                    "TDR iteration %s for %s aborted: %s",
                    iteration,
                    instance_id,
                    result["error"],
                )
                if iteration == 0:
                    # Cannot repair if we could not even run the initial tests.
                    return initial_patch
                break

            if result["total_score"] > best_score:
                best_score = result["total_score"]
                best_patch = current_patch
                logger.info(
                    "New best patch for %s at iteration %s (score=%s)",
                    instance_id,
                    iteration,
                    best_score,
                )

            if result["perfect"]:
                logger.info("Perfect score reached for %s at iteration %s", instance_id, iteration)
                return best_patch

            # No more repair attempts after the last test iteration.
            if iteration == args.repair_iterations:
                break

            # Build and save the repair prompt.
            repair_prompt = _build_repair_prompt(
                instance,
                current_patch,
                result,
                ranked_files,
                host_repo_dir,
            )
            prompt_path = log_dir / f"{container_name}.tdr.{iteration}.repair.prompt.txt"
            prompt_path.write_text(repair_prompt, encoding="utf-8")

            logger.info(
                "Calling repair model for %s iteration %s (timeout=%ss, max_tokens=%s)",
                instance_id,
                iteration,
                args.repair_timeout,
                args.repair_max_tokens,
            )
            response = call_chat_endpoint(
                repair_config,
                repair_prompt,
                args.repair_timeout,
                logger,
                max_tokens=args.repair_max_tokens,
            )
            response_path = log_dir / f"{container_name}.tdr.{iteration}.repair.response.md"
            response_path.write_text(response, encoding="utf-8")

            new_patch = _apply_repair_and_capture(
                host_repo_dir,
                base_commit,
                current_patch,
                response,
                logger,
            )
            if new_patch is None:
                logger.warning(
                    "Repair iteration %s for %s did not produce an applicable patch; retrying once with strict prompt",
                    iteration,
                    instance_id,
                )
                if _looks_truncated(response):
                    logger.info("Repair response for %s appears truncated", instance_id)
                strict_prompt = _build_repair_prompt(
                    instance,
                    current_patch,
                    result,
                    ranked_files,
                    host_repo_dir,
                    strict=True,
                )
                strict_prompt_path = log_dir / f"{container_name}.tdr.{iteration}.repair.prompt.strict.txt"
                strict_prompt_path.write_text(strict_prompt, encoding="utf-8")
                retry_response = call_chat_endpoint(
                    repair_config,
                    strict_prompt,
                    args.repair_timeout,
                    logger,
                    max_tokens=args.repair_max_tokens,
                )
                retry_response_path = log_dir / f"{container_name}.tdr.{iteration}.repair.response.strict.md"
                retry_response_path.write_text(retry_response, encoding="utf-8")
                new_patch = _apply_repair_and_capture(
                    host_repo_dir,
                    base_commit,
                    current_patch,
                    retry_response,
                    logger,
                )
                if new_patch is None:
                    logger.warning(
                        "Strict repair retry for %s iteration %s also failed; keeping best",
                        instance_id,
                        iteration,
                    )
                    continue
            current_patch = new_patch
    except Exception as exc:
        logger.error("Unexpected error in TDR for %s: %s", instance_id, exc)
        logger.debug(traceback.format_exc())
    finally:
        if not getattr(args, "tdr_keep_container", False):
            stop_and_remove_container(container_name, logger)

    return best_patch


def _score_seed_patch(
    image: str,
    instance: dict[str, Any],
    patch: str,
    args: Any,
    log_dir: Path,
    logger: Any,
    container: str,
) -> dict[str, Any]:
    """Run a single seed patch through the evaluator and return its score."""
    result = _run_tests_once(
        image,
        container,
        instance,
        patch,
        args,
        log_dir,
        logger,
        iteration="seed",
    )
    return {"container": container, "patch": patch, **result}


def run_ensemble_seed_generation(
    host_repo_dir: Path,
    instance: dict[str, Any],
    models: str,
    args: Any,
    log_dir: Path,
    logger: Any,
    ranked_files: list[str] | None = None,
) -> str:
    """Generate seed patches from multiple models and return the best-scoring one.

    The ``models`` argument is a comma-separated list of OpenRouter profile names.
    Each profile's config is loaded from ``<config-dir>/openrouter_<profile>.toml``.
    """
    from run_selfware import (
        build_agentless_prompt,
        call_chat_endpoint,
        capture_patch_on_host,
        load_config,
        run_cmd,
        stop_and_remove_container,
    )
    from patch_utils import apply_model_response

    instance_id = instance["instance_id"]
    base_commit = instance["base_commit"]
    model_list = [m.strip() for m in models.split(",") if m.strip()]
    if not model_list:
        logger.warning("No ensemble models specified for %s", instance_id)
        return ""

    config_dir = Path(args.config_dir)
    seeds: list[dict[str, Any]] = []

    for profile in model_list:
        config_path = config_dir / f"openrouter_{profile}.toml"
        if not config_path.exists():
            logger.warning("Ensemble config not found for %s: %s", profile, config_path)
            continue
        try:
            seed_config = load_config(config_path)
        except Exception as exc:
            logger.warning("Failed to load ensemble config %s: %s", config_path, exc)
            continue

        # Reset the host repo before generating each seed.
        reset = run_cmd(
            ["git", "-C", str(host_repo_dir), "reset", "--hard", base_commit],
            logger=logger,
        )
        if reset.returncode != 0:
            logger.error("Failed to reset repo for ensemble seed %s: %s", profile, reset.stderr.strip())
            continue

        prompt = build_agentless_prompt(instance, host_repo_dir)
        prompt_path = log_dir / f"{instance_id}.ensemble.{profile}.prompt.txt"
        prompt_path.write_text(prompt, encoding="utf-8")
        logger.info(
            "Generating ensemble seed for %s from %s (timeout=%ss, max_tokens=%s)",
            instance_id,
            profile,
            args.ensemble_timeout,
            args.ensemble_max_tokens,
        )
        try:
            response = call_chat_endpoint(
                seed_config,
                prompt,
                args.ensemble_timeout,
                logger,
                max_tokens=args.ensemble_max_tokens,
            )
        except Exception as exc:
            logger.error("Ensemble API call failed for %s/%s: %s", instance_id, profile, exc)
            continue
        response_path = log_dir / f"{instance_id}.ensemble.{profile}.response.md"
        response_path.write_text(response, encoding="utf-8")

        applied = apply_model_response(host_repo_dir, response, logger)
        if not applied:
            logger.warning(
                "Ensemble seed %s for %s could not be applied; skipping",
                profile,
                instance_id,
            )
            # Reset so a partial/failed apply does not leak into the next seed.
            run_cmd(
                ["git", "-C", str(host_repo_dir), "reset", "--hard", base_commit],
                logger=logger,
            )
            continue
        patch = capture_patch_on_host(host_repo_dir, logger, base_commit=base_commit)
        if not patch.strip():
            logger.warning(
                "Ensemble seed %s for %s produced an empty diff; skipping",
                profile,
                instance_id,
            )
            continue
        seeds.append({"profile": profile, "patch": patch})
        logger.info(
            "Ensemble seed %s for %s produced a patch (%s chars)",
            profile,
            instance_id,
            len(patch),
        )

    if not seeds:
        logger.warning("No ensemble seeds produced applicable patches for %s", instance_id)
        return ""

    # Deduplicate near-identical seed patches before spending containers on scoring.
    seen_patches: dict[str, dict[str, Any]] = {}
    for seed in seeds:
        normalized = _normalize_patch_for_dedup(seed["patch"])
        if normalized in seen_patches:
            logger.info(
                "Skipping duplicate ensemble seed from %s (kept %s)",
                seed["profile"],
                seen_patches[normalized]["profile"],
            )
            continue
        seen_patches[normalized] = seed
    unique_seeds = list(seen_patches.values())
    if len(unique_seeds) < len(seeds):
        logger.info(
            "Ensemble deduplication for %s: %s seeds -> %s unique",
            instance_id,
            len(seeds),
            len(unique_seeds),
        )

    # Score each seed in its own container.
    image = f"docker.io/jefzda/sweap-images:{instance['dockerhub_tag']}"
    scored: list[dict[str, Any]] = []
    with ThreadPoolExecutor(max_workers=min(len(unique_seeds), 4)) as executor:
        future_to_seed = {}
        for idx, seed in enumerate(unique_seeds):
            container = f"{instance_id}-ensemble-seed-{idx}"
            container = re.sub(r"[^a-zA-Z0-9_.-]+", "_", container)[:60]
            future = executor.submit(
                _score_seed_patch,
                image,
                instance,
                seed["patch"],
                args,
                log_dir,
                logger,
                container,
            )
            future_to_seed[future] = (seed, container)

        for future in as_completed(future_to_seed):
            seed, container = future_to_seed[future]
            try:
                result = future.result()
                scored.append({**seed, **result})
            except Exception as exc:
                logger.error("Ensemble seed scoring failed for %s: %s", seed["profile"], exc)
            finally:
                stop_and_remove_container(container, logger)

    if not scored:
        logger.warning("Could not score any ensemble seeds for %s", instance_id)
        return ""

    # Only consider seeds whose scoring completed successfully.
    valid_scored = [s for s in scored if s.get("total_score", -1) >= 0]
    if not valid_scored:
        logger.warning("All ensemble seed scores failed for %s", instance_id)
        return ""

    # Prefer seeds that pass at least one fail-to-pass test.
    fail_passers = [s for s in valid_scored if s.get("fail_passed", 0) > 0]
    if fail_passers:
        best = max(
            fail_passers,
            key=lambda s: (s.get("total_score", -1), s.get("fail_passed", 0)),
        )
        logger.info(
            "Ensemble selected %s for %s (fail-to-pass passer; total_score=%s, fail=%s/%s, pass=%s/%s)",
            best["profile"],
            instance_id,
            best.get("total_score", -1),
            best.get("fail_passed", 0),
            best.get("fail_total", 0),
            best.get("pass_passed", 0),
            best.get("pass_total", 0),
        )
        return best.get("patch", "")

    # Fallback: no seed passed any fail-to-pass test, so return the highest
    # total_score seed rather than an empty patch.
    best = max(
        valid_scored,
        key=lambda s: (s.get("total_score", -1), s.get("fail_passed", 0)),
    )
    logger.info(
        "Ensemble fallback selected %s for %s (total_score=%s, fail=%s/%s, pass=%s/%s)",
        best["profile"],
        instance_id,
        best.get("total_score", -1),
        best.get("fail_passed", 0),
        best.get("fail_total", 0),
        best.get("pass_passed", 0),
        best.get("pass_total", 0),
    )
    return best.get("patch", "")
