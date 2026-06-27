"""Failure classification and escalation recovery for the SWE-bench Pro harness.

This module is consumed by ``run_selfware.py``.  It keeps the recovery logic
out of the main harness so the retry rules can be iterated on without touching
the core runner.
"""

from __future__ import annotations

import copy
import re
from pathlib import Path
from typing import Any

import tomli_w

from small_model_adapter import (
    _is_strong_identifier,
    _tokenize_problem,
    truncate_file_reads,
)


# Classification labels returned by classify_failure().
MAX_ITERATIONS = "max_iterations"
NO_EDIT = "no_edit"
JSON_PARSE_ERROR = "json_parse_error"
REPETITION_LOOP = "repetition_loop"
HALLUCINATED_TOOL = "hallucinated_tool"
TIMEOUT = "timeout"
UNKNOWN = "unknown"
EMPTY_PATCH = "empty_patch"

# Recovery mode label used by the harness after retries are exhausted.
DIFF_FALLBACK = "diff_fallback"

# Sentinel keys attached to the dict returned by escalation_config().  They are
# stripped out before the TOML is written for selfware because the binary does
# not understand them.
SYSTEM_MESSAGE_KEY = "__recovery_system_message"
PROMPT_SUFFIX_KEY = "__recovery_prompt_suffix"
AGENTLESS_MODE_KEY = "__recovery_agentless"


def _is_patch_empty(patch: str | None) -> bool:
    """Return True when a captured patch contains no actual diff content."""
    return not (patch or "").strip()


def classify_failure(stderr_path: str | Path) -> str:
    """Classify the terminal failure mode from a selfware stderr log.

    Returns one of:
      max_iterations, no_edit, empty_patch, json_parse_error, repetition_loop,
      hallucinated_tool, timeout, unknown.
    """
    text = _read_log(stderr_path)
    if not text:
        # An empty/missing log usually means the harness killed the process
        # before selfware could flush anything (most commonly a timeout).
        return TIMEOUT

    # Order matters: terminal / unambiguous patterns first.
    if _has(text, r"Agent failed:\s*Max iterations exceeded"):
        return MAX_ITERATIONS
    if _has(text, r"empty patch|no source changes|produced no patch"):
        return EMPTY_PATCH
    if _has(text, r"Agent reached step \d+ without editing any file"):
        return NO_EDIT
    if _has(text, r"Failed to parse response JSON") or _has(
        text, r"untagged enum MessageContent"
    ):
        return JSON_PARSE_ERROR
    if _has(text, r"Repetition loop detected:\s*\w+\s+called\s+3\s+times"):
        return REPETITION_LOOP
    if _has(text, r"computer_window"):
        return HALLUCINATED_TOOL
    if _has(text, r"timed out|TimeoutExpired"):
        return TIMEOUT

    return UNKNOWN


def should_retry(failure_class: str, attempt: int, max_retries: int = 2) -> bool:
    """Return True if the failure merits another escalated retry.

    Unknown failures, the diff-fallback terminal state, and exhausted budgets
    are not retried.
    """
    if failure_class in (UNKNOWN, DIFF_FALLBACK):
        return False
    return 1 <= attempt <= max_retries


def escalation_config(
    base_config: dict[str, Any], failure_class: str
) -> dict[str, Any]:
    """Build a recovery configuration for a given failure class.

    The returned dict is a copy of ``base_config`` with TOML-level overrides
    plus two sentinel keys that the harness strips before writing the TOML:

      - ``__recovery_system_message``: prepended to the user prompt as a
        strongly-labelled directive.  selfware does not expose an extra
        system-prompt config field, so this is the best proxy.
      - ``__recovery_prompt_suffix``: appended to the user prompt (examples,
        format reminders, etc.).
    """
    config = copy.deepcopy(base_config)
    extras: dict[str, str] = {SYSTEM_MESSAGE_KEY: "", PROMPT_SUFFIX_KEY: ""}
    agent = config.setdefault("agent", {})

    if failure_class == MAX_ITERATIONS:
        # Shorter runway, smaller tool catalog, lower temperature, and a strict
        # plan-then-edit-then-verify workflow injected into the prompt.
        agent["max_iterations"] = max(15, agent.get("max_iterations", 60) // 2)
        agent["edit_deadline_step"] = max(3, agent.get("edit_deadline_step", 8) // 2)
        agent["minimal_tool_catalog"] = True
        agent["disable_episodic_memory"] = True
        agent["require_verification_before_completion"] = False
        config["temperature"] = max(0.0, config.get("temperature", 0.1) - 0.05)
        extras[PROMPT_SUFFIX_KEY] = (
            "\n\nRECOVERY MODE (max iterations exceeded): "
            "You now have FEWER steps. Step 1: write one sentence describing the root cause. "
            "Step 2: make exactly ONE minimal file_edit. "
            "Step 3: run the test command. Do not explore, do not re-read files you already saw."
        )

    elif failure_class == NO_EDIT:
        # The model did not edit anything in the first attempt. Keep a tight but
        # feasible deadline and give a concrete first-step example.
        agent["max_no_edit_steps"] = max(2, agent.get("max_no_edit_steps", 3) - 1)
        agent["edit_deadline_step"] = max(5, agent.get("edit_deadline_step", 8) - 1)
        agent["require_edit_before_completion"] = True
        agent["require_verification_before_completion"] = False
        extras[SYSTEM_MESSAGE_KEY] = (
            "You must edit at least one source file before finishing. "
            "Step 1: read the issue. Step 2: read the most relevant source snippet. "
            "Step 3: call file_edit with the minimal fix. "
            "Do not finish until a source file has been changed."
        )
        extras[PROMPT_SUFFIX_KEY] = (
            "\n\nRECOVERY MODE (no edit detected): you must make a file_edit.\n"
            "EXAMPLE file_edit block (replace with the real old/new lines):\n"
            "### FILE: src/example.py\n"
            "<<<<<<< SEARCH\n"
            "    old_buggy_line()\n"
            "=======\n"
            "    fixed_line()\n"
            ">>>>>>> REPLACE\n\n"
            "Make your first file_edit by step 3. Do not finish without editing."
        )

    elif failure_class == JSON_PARSE_ERROR:
        # Force XML-style tool blocks, turn off streaming (which can fragment
        # the response), lower temperature/top_p, and simplify instructions.
        agent["native_function_calling"] = False
        agent["minimal_tool_catalog"] = True
        agent["streaming"] = False
        config["temperature"] = max(0.0, config.get("temperature", 0.1) - 0.05)
        extra_body = config.get("extra_body") or {}
        if not isinstance(extra_body, dict):
            extra_body = {}
        extra_body["top_p"] = 0.3
        config["extra_body"] = extra_body
        extras[SYSTEM_MESSAGE_KEY] = (
            "Use ONLY the XML tool format shown in the system prompt. "
            "Do not output raw JSON or markdown code fences. "
            "Every actionable response must be exactly one <tool>...</tool> block."
        )

    elif failure_class == REPETITION_LOOP:
        # Drop verification loops and add an explicit anti-repetition reminder.
        agent["require_verification_before_completion"] = False
        agent["max_no_edit_steps"] = 1
        extras[SYSTEM_MESSAGE_KEY] = (
            "Do not repeat the same tool call. "
            "If you already read a file, used directory_tree, or ran a check, use that result and move forward. "
            "Re-reading the same path or re-running the same command is forbidden."
        )

    elif failure_class == HALLUCINATED_TOOL:
        # Restrict the advertised catalog and explicitly ban the hallucinated tool.
        agent["minimal_tool_catalog"] = True
        extras[SYSTEM_MESSAGE_KEY] = (
            "Do not use computer_window or any window-related tool. "
            "Only use the tools listed in the prompt: file_read, file_edit/file_write, "
            "shell_exec, cargo_check, directory_tree, and think."
        )

    elif failure_class == EMPTY_PATCH:
        # The previous attempt produced no diff. Bypass the multi-turn agent
        # loop and force a direct SEARCH/REPLACE response.
        config["temperature"] = max(0.0, config.get("temperature", 0.1) - 0.05)
        agent["native_function_calling"] = False
        agent["minimal_tool_catalog"] = True
        agent["streaming"] = False
        extras[AGENTLESS_MODE_KEY] = True
        extras[SYSTEM_MESSAGE_KEY] = (
            "The previous attempt produced an empty patch. "
            "You must emit a concrete SEARCH/REPLACE block that changes at least one source file. "
            "Do not finish until `git diff` shows a non-empty patch."
        )
        extras[PROMPT_SUFFIX_KEY] = (
            "\n\nRECOVERY MODE (empty patch): the previous run produced no source changes. "
            "Reply with one or more SEARCH/REPLACE blocks using this exact format:\n"
            "### FILE: path/to/file.py\n"
            "<<<<<<< SEARCH\n"
            "old lines\n"
            "=======\n"
            "new lines\n"
            ">>>>>>> REPLACE\n\n"
            "At least one source file must change. Do not add explanations outside the block."
        )

    elif failure_class == TIMEOUT:
        # Shorter per-step timeout, fewer iterations, minimal catalog, and a
        # very aggressive workflow directive.
        agent["max_iterations"] = max(15, agent.get("max_iterations", 60) // 2)
        agent["step_timeout_secs"] = max(30, agent.get("step_timeout_secs", 180) // 2)
        agent["minimal_tool_catalog"] = True
        agent["require_verification_before_completion"] = False
        extras[SYSTEM_MESSAGE_KEY] = (
            "You are running out of time. Read only the issue and the most relevant file, "
            "make one minimal edit, then finish. Do not explore or run long commands."
        )

    return {**config, **extras}


def extract_recovery_extras(
    escalation_result: dict[str, Any],
) -> tuple[dict[str, Any], str, str]:
    """Split the escalation result into a clean TOML dict + prompt directives."""
    clean = copy.deepcopy(escalation_result)
    system_message = clean.pop(SYSTEM_MESSAGE_KEY, "") or ""
    prompt_suffix = clean.pop(PROMPT_SUFFIX_KEY, "") or ""
    return clean, system_message, prompt_suffix


def write_recovery_config(
    escalation_result: dict[str, Any],
    output_dir: Path,
    name: str,
) -> tuple[Path, str, str]:
    """Write the recovery TOML and return ``(path, system_message, prompt_suffix)``."""
    clean, system_message, prompt_suffix = extract_recovery_extras(escalation_result)
    output_dir.mkdir(parents=True, exist_ok=True)
    path = output_dir / f"{name}.recovery.toml"
    with open(path, "wb") as f:
        tomli_w.dump(clean, f)
    return path, system_message, prompt_suffix


def build_recovery_prompt(
    base_prompt: str,
    system_message: str,
    prompt_suffix: str,
) -> str:
    """Apply recovery directives to the user prompt.

    Because selfware does not expose an extra system-prompt config field, the
    ``system_message`` is embedded at the top of the user prompt as a
    strongly-labelled directive.
    """
    parts: list[str] = []
    if system_message:
        parts.append(f"[RECOVERY SYSTEM DIRECTIVE]\n{system_message.strip()}")
    parts.append(base_prompt)
    if prompt_suffix:
        parts.append(prompt_suffix.strip())
    return "\n\n".join(parts)


def build_diff_fallback_prompt(
    prompt_text: str,
    ranked_files: list[str],
    repo_path: str | Path,
) -> str:
    """Build a one-shot unified-diff prompt for small/fragile models.

    After the normal multi-turn recovery loop fails to produce a non-empty
    patch, this prompt bypasses the agent tooling entirely and asks the model
    to emit a single ``git apply``-compatible unified diff.
    """
    repo_path = Path(repo_path)
    issue = _extract_issue(prompt_text)
    snippet_terms = [t for t in _tokenize_problem(issue) if _is_strong_identifier(t)]
    snippets = truncate_file_reads(
        ranked_files[:5],
        repo_path=repo_path,
        max_lines=200,
        max_chars=8000,
        highlight_terms=snippet_terms,
    )

    example = (
        "diff --git a/lib/auth/grpcserver.go b/lib/auth/grpcserver.go\n"
        "index d569ceaa1e..e752edd8c4 100644\n"
        "--- a/lib/auth/grpcserver.go\n"
        "+++ b/lib/auth/grpcserver.go\n"
        "@@ -104,7 +104,7 @@ func readHeaderAndPayload(reader io.Reader) (*MessageHeader, []byte, error) {\n"
        " \n"
        "     // Max BSON document size is 16MB\n"
        "     // https://www.mongodb.com/docs/manual/reference/limits/#mongodb-limit-BSON-Document-Size\n"
        "-    if length-headerSizeBytes >= 16*1024*1024 {\n"
        "+    if length-headerSizeBytes >= 48*1024*1024 {\n"
        "         return nil, nil, trace.BadParameter(\"exceeded the maximum document size, got length: %d\", length)\n"
        "     }\n"
        " \n"
    )

    parts = [
        "[ONE-SHOT DIFF FALLBACK]",
        "",
        "The previous agent attempts did not produce a valid patch. "
        "Generate the fix directly as a single unified git diff.",
        "",
        "Issue:",
        issue,
        "",
        "Top relevant source files (use these as the ground truth; do not invent code that does not match the files below):",
        snippets,
        "",
        "YOUR TASK:",
        "Output exactly ONE unified git diff (starting with `diff --git a/... b/...`) "
        "that fixes the issue above.",
        "- Use 3 lines of context around each change and keep hunks small.",
        "- Modify source files only. Do NOT edit tests, configs, docs, or unrelated code.",
        "- Do NOT rewrite whole files or invent functions/types that are not already in the source snippets above.",
        "- Do not output explanations, markdown fences (```diff ... ```), or any text outside the diff.",
        "- The line numbers shown in the snippets are for reference only; do not include them in the diff.",
        "- The diff must apply cleanly with `git apply --check`.",
        "",
        "Example format:",
        example,
    ]
    return "\n".join(parts)


def _extract_issue(prompt_text: str) -> str:
    """Return the text under an 'Issue:' heading, or the whole prompt."""
    next_heading = (
        r"(?:Requirements|Test files|Failing tests|Run tests|MANDATORY WORKFLOW|"
        r"YOUR TASK|CRITICAL RULES|Directory layout|Likely relevant|EXAMPLES|"
        r"REPAIR FEEDBACK):"
    )
    match = re.search(
        rf"(?:^|\n)Issue:\s*\n(.*?)(?=\n\s*{next_heading}|\Z)",
        prompt_text,
        re.DOTALL,
    )
    if match:
        issue = match.group(1).strip()
        if issue:
            return issue
    return prompt_text.strip()


def _read_snippets(
    repo_path: Path,
    files: list[str],
    max_lines: int = 200,
    max_chars: int = 8000,
) -> str:
    """Read and truncate the top source files for the diff fallback prompt."""
    chunks: list[str] = []
    for rel in files:
        path = repo_path / rel
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        lines = text.splitlines()
        if len(lines) > max_lines:
            half = max_lines // 2
            text = "\n".join(
                lines[:half]
                + [f"\n... ({len(lines) - max_lines} lines omitted) ...\n"]
                + lines[-half:]
            )
        chunks.append(f"--- {rel} ---\n{text}\n")

    combined = "\n".join(chunks)
    if len(combined) > max_chars:
        combined = combined[:max_chars] + "\n... (truncated due to length) ...\n"
    if not combined.strip():
        return "(no source snippets available)\n"
    return combined


def _read_log(path: str | Path) -> str:
    try:
        return Path(path).read_text(encoding="utf-8", errors="ignore")
    except Exception:
        return ""


def _has(text: str, pattern: str) -> bool:
    return bool(re.search(pattern, text, re.IGNORECASE))
