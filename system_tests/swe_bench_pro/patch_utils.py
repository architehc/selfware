"""Patch-application helpers shared by the SWE-bench Pro harness."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path
from typing import Any


# Paths that are part of the harness, agent workspace, or common build
# artifacts and must never appear in a submitted patch.
_DIFF_EXCLUDED_PREFIXES = (
    ".selfware_prompt.txt",
    ".selfware/",
    "agent_data/",
    "target/",
    "node_modules/",
    ".pytest_cache/",
    "__pycache__/",
    ".mypy_cache/",
    "*.orig",
    "*.bak",
)


def _extract_diff_path(line: str) -> str | None:
    """Parse the file path from a 'diff --git a/X b/X' header line."""
    match = re.match(r"^diff --git a/(.+?) b/(.+?)(?:\s|$)", line)
    if not match or match.group(1) != match.group(2):
        return None
    return match.group(1)


def _is_excluded_diff_hunk(hunk_lines: list[str]) -> bool:
    """Return True if a diff hunk should not be included in the prediction."""
    if not hunk_lines:
        return True
    path = _extract_diff_path(hunk_lines[0])
    if path is not None:
        if path.endswith("/"):
            path = path[:-1]
        if any(
            path == prefix.rstrip("/")
            or path.startswith(prefix)
            or path.endswith(prefix.lstrip("*"))
            for prefix in _DIFF_EXCLUDED_PREFIXES
        ):
            return True

    hunk_text = "".join(hunk_lines)
    # Drop submodule pointer changes (e.g. webassets, teleport.e).
    if "Subproject commit" in hunk_text:
        return True
    if any(re.search(r"\b160000\b", line) for line in hunk_lines[:3]):
        return True
    return False


def extract_diff(response: str) -> str | None:
    """Extract a unified diff from the model response."""
    for m in re.finditer(r"```(?:diff)?\s*(.*?)\s*```", response, re.DOTALL):
        content = m.group(1).strip()
        if "diff --git" in content:
            return content
    if "diff --git" in response:
        return response[response.index("diff --git"):].strip()
    return None


def apply_patch(repo_dir: Path, diff_text: str, logger: Any) -> bool:
    """Apply a unified diff to the host repo, falling back to 3-way or patch."""
    # Import lazily to avoid circular imports with run_selfware.py.
    from run_selfware import run_cmd

    patch_path = repo_dir / ".selfware_plan_patch.diff"
    patch_path.write_text(diff_text, encoding="utf-8")
    try:
        for attempt, cmd in enumerate(
            (
                ["git", "-C", str(repo_dir), "apply", str(patch_path)],
                ["git", "-C", str(repo_dir), "apply", "--3way", str(patch_path)],
            ),
            start=1,
        ):
            proc = run_cmd(cmd, logger=logger)
            if proc.returncode == 0:
                logger.info("Applied unified diff (attempt %s)", attempt)
                return True
            logger.warning("git apply attempt %s failed: %s", attempt, proc.stderr.strip())

        proc = subprocess.run(
            ["patch", "-p1", "--no-backup-if-mismatch", "-i", str(patch_path)],
            cwd=str(repo_dir),
            capture_output=True,
            text=True,
        )
        if proc.returncode == 0:
            logger.info("Applied unified diff using system patch")
            return True
        logger.warning("patch -p1 failed: %s", proc.stderr.strip())
        return False
    finally:
        patch_path.unlink(missing_ok=True)


def _strip_line_number_gutter(text: str) -> str:
    """Remove harness line-number gutters such as '  123 | ' from snippets.

    Small models often copy the gutter back into SEARCH blocks; stripping it
    makes those blocks apply without requiring a second API call.
    """
    lines = text.splitlines()
    stripped: list[str] = []
    for line in lines:
        # Match a left gutter of the form "  123 | " (leading spaces, digits,
        # optional colon, then a pipe/space separator).
        if re.match(r"^\s*\d+[:\.]?\s*[│|]\s", line):
            line = re.sub(r"^\s*\d+[:\.]?\s*[│|]\s", "", line, count=1)
        stripped.append(line)
    return "\n".join(stripped)


def _normalize_blank_lines(lines: list[str]) -> list[str]:
    """Collapse multiple blank lines to a single empty string."""
    out: list[str] = []
    prev_blank = False
    for line in lines:
        is_blank = line.strip() == ""
        if is_blank and prev_blank:
            continue
        out.append("" if is_blank else line)
        prev_blank = is_blank
    return out


def _fuzzy_replace(text: str, old: str, new: str) -> str | None:
    """Replace old block with multiple fuzzy strategies.

    Strategies, in order of precision:
      1. Exact match.
      2. Strip harness line-number gutters and retry exact.
      3. Ignore per-line leading/trailing whitespace.
      4. Ignore blank-line differences.
      5. Ignore all whitespace differences.
    """
    text_lines = text.splitlines()
    old_lines = old.splitlines()
    new_lines = new.splitlines()
    n = len(old_lines)
    if n == 0:
        return None

    strategies: list[tuple[list[str], list[str]]] = [
        (old_lines, text_lines),
        (_strip_line_number_gutter(old).splitlines(), text_lines),
        (old_lines, [_strip_line_number_gutter(line) for line in text_lines]),
        (_strip_line_number_gutter(old).splitlines(),
         [_strip_line_number_gutter(line) for line in text_lines]),
    ]

    def _try_match(oln: list[str], tln: list[str]) -> int | None:
        m = len(tln)
        if m < len(oln):
            return None
        for i in range(m - len(oln) + 1):
            if all(oln[j].strip() == tln[i + j].strip() for j in range(len(oln))):
                return i
        return None

    def _try_blank_match(oln: list[str], tln: list[str]) -> int | None:
        norm_old = _normalize_blank_lines(oln)
        norm_text = _normalize_blank_lines(tln)
        if len(norm_text) < len(norm_old):
            return None
        for i in range(len(norm_text) - len(norm_old) + 1):
            if all(norm_old[j].strip() == norm_text[i + j].strip() for j in range(len(norm_old))):
                return i
        return None

    def _try_whitespace_match(oln: list[str], tln: list[str]) -> int | None:
        old_norm = [re.sub(r"\s+", "", line) for line in oln]
        text_norm = [re.sub(r"\s+", "", line) for line in tln]
        if len(text_norm) < len(old_norm):
            return None
        for i in range(len(text_norm) - len(old_norm) + 1):
            if all(old_norm[j] == text_norm[i + j] for j in range(len(old_norm))):
                return i
        return None

    for oln, tln in strategies:
        idx = _try_match(oln, tln)
        if idx is not None:
            return "\n".join(tln[:idx] + new_lines + tln[idx + len(oln):])

    for oln, tln in strategies:
        idx = _try_blank_match(oln, tln)
        if idx is not None:
            return "\n".join(tln[:idx] + new_lines + tln[idx + len(oln):])

    for oln, tln in strategies:
        idx = _try_whitespace_match(oln, tln)
        if idx is not None:
            return "\n".join(tln[:idx] + new_lines + tln[idx + len(oln):])

    return None


def apply_edits_with_missing(
    repo_dir: Path, response: str, logger: Any
) -> tuple[bool, set[str]]:
    """Apply simple file edits in ### FILE / SEARCH / REPLACE blocks.

    Returns whether any edit was applied and the set of file paths referenced
    by the model that do not exist in the repository.  Missing files are
    skipped instead of crashing so callers can detect hallucinated paths.
    """
    # Strip markdown code fences so wrapped blocks still parse.
    cleaned = re.sub(r"```[a-zA-Z]*\n", "", response)
    cleaned = re.sub(r"\n```\s*$", "", cleaned)

    pattern = re.compile(
        r"###\s*FILE:\s*[`\"]?(?P<path>.+?)[`\"]?\s*\n"
        r"<<<<<<<\s*(?:SEARCH)?\s*\n"
        r"(?P<old>.*?)\n"
        r"=======\s*\n"
        r"(?P<new>.*?)\n"
        r">>>>>>>\s*(?:REPLACE)?",
        re.DOTALL,
    )
    applied = False
    missing: set[str] = set()
    for m in pattern.finditer(cleaned):
        rel_path = m.group("path").strip()
        old = m.group("old")
        new = m.group("new")
        path = repo_dir / rel_path
        if not path.exists():
            # Allow creation of new files when the SEARCH block is empty.
            if old.strip() == "":
                try:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(new.replace("\r\n", "\n"), encoding="utf-8")
                    applied = True
                    if logger is not None:
                        logger.info("Created new file %s", rel_path)
                    continue
                except Exception as exc:
                    if logger is not None:
                        logger.error("Failed to create new file %s: %s", rel_path, exc)
                    missing.add(rel_path)
                    continue
            if logger is not None:
                logger.warning("Skipping edit to non-existent file: %s", rel_path)
            missing.add(rel_path)
            continue
        try:
            text = path.read_text(encoding="utf-8")
            # Normalize Windows-style line endings to avoid mismatch on repos
            # that mix LF/CRLF.
            text = text.replace("\r\n", "\n")
            old = old.replace("\r\n", "\n")
            new = new.replace("\r\n", "\n")
            if old not in text:
                fuzzy = _fuzzy_replace(text, old, new)
                if fuzzy is None:
                    if logger is not None:
                        logger.warning(
                            "Search block not found in %s (exact and whitespace-normalized)", rel_path
                        )
                    continue
                text = fuzzy
            else:
                text = text.replace(old, new, 1)
            path.write_text(text, encoding="utf-8")
            applied = True
            if logger is not None:
                logger.info("Applied edit to %s", rel_path)
        except Exception as exc:
            if logger is not None:
                logger.error("Failed to apply edit to %s: %s", rel_path, exc)
    return applied, missing


def apply_edits(repo_dir: Path, response: str, logger: Any) -> bool:
    """Apply simple file edits in ### FILE / SEARCH / REPLACE blocks."""
    applied, _ = apply_edits_with_missing(repo_dir, response, logger)
    return applied


def verify_edits_apply(repo_dir: Path, response: str, logger: Any) -> bool:
    """Return True if every SEARCH/REPLACE block in ``response`` can be applied.

    This checks against the files on disk without modifying them, so a failed
    prompt can be retried with more exact context.
    """
    cleaned = re.sub(r"```[a-zA-Z]*\n", "", response)
    cleaned = re.sub(r"\n```\s*$", "", cleaned)
    pattern = re.compile(
        r"###\s*FILE:\s*[`<\"]?(?P<path>.+?)[`\"]?\s*\n"
        r"<<<<<<<\s*(?:SEARCH)?\s*\n"
        r"(?P<old>.*?)\n"
        r"=======\s*\n"
        r"(?P<new>.*?)\n"
        r">>>>>>>\s*(?:REPLACE)?",
        re.DOTALL,
    )
    for m in pattern.finditer(cleaned):
        rel_path = m.group("path").strip()
        old = m.group("old").replace("\r\n", "\n")
        path = repo_dir / rel_path
        if not path.exists():
            # Empty SEARCH block means the model wants to create the file.
            if old.strip() == "":
                continue
            if logger is not None:
                logger.warning("verify_edits_apply: file does not exist: %s", rel_path)
            return False
        try:
            text = path.read_text(encoding="utf-8").replace("\r\n", "\n")
        except Exception as exc:
            if logger is not None:
                logger.warning("verify_edits_apply: cannot read %s: %s", rel_path, exc)
            return False
        if old not in text and _fuzzy_replace(text, old, "") is None:
            if logger is not None:
                logger.warning("verify_edits_apply: search block not found in %s", rel_path)
            return False
    # If no edit blocks were found, the response cannot be applied as edits.
    return bool(pattern.search(cleaned))


def apply_model_response_with_missing(
    repo_dir: Path, response: str, logger: Any
) -> tuple[bool, set[str]]:
    """Try to apply the model response as a diff, then as edits.

    Returns the application status and any file paths referenced by the model
    that do not exist on disk.
    """
    diff = extract_diff(response)
    if diff and apply_patch(repo_dir, diff, logger):
        return True, set()
    applied, missing = apply_edits_with_missing(repo_dir, response, logger)
    if applied:
        return True, missing
    if logger is not None:
        logger.warning("Could not apply any patch from model response")
    return False, missing


def apply_model_response(repo_dir: Path, response: str, logger: Any) -> bool:
    """Try to apply the model response as a diff, then as edits."""
    applied, _ = apply_model_response_with_missing(repo_dir, response, logger)
    return applied


def is_truncated_diff(response: str) -> bool:
    """Return True if the response looks like a truncated diff block.

    This happens when the model runs out of output tokens inside a `` ```diff ``
    block.  A truncated response is not applyable as-is.
    """
    response = response.strip()
    if not response:
        return False
    # An unclosed markdown diff block.
    if re.search(r"```diff\b", response) and "```" not in response.split("```diff", 1)[1]:
        return True
    # A diff that starts but does not end with a hunk.
    if "diff --git" in response and "@@" in response:
        last_hunk = response.rfind("@@")
        after_hunk = response[last_hunk:]
        # If the last line is a context/deletion/addition without a trailing
        # newline terminator, treat it as possibly truncated.
        lines = after_hunk.splitlines()
        if lines and re.match(r"^[-+ ]", lines[-1]) and not lines[-1].endswith("\n"):
            return True
    return False


def extract_partial_diff(response: str) -> str | None:
    """Try to salvage a partial diff from a truncated response.

    If the response ends mid-hunk, this returns the diff up to the last complete
    hunk.  Returns ``None`` if no usable diff is present.
    """
    diff = extract_diff(response)
    if not diff:
        return None
    # Find the last complete hunk boundary and keep everything up to it.
    lines = diff.splitlines()
    last_complete = -1
    for i, line in enumerate(lines):
        if line.startswith("@@"):
            last_complete = i
    if last_complete <= 0:
        return None
    # Keep lines up to and including the last complete hunk header; drop any
    # partial hunk lines after it.
    return "\n".join(lines[:last_complete]) + "\n"


def _apply_diff_with_check(repo_dir: Path, diff_text: str, logger: Any) -> bool:
    """Validate a diff with ``git apply --check`` and then apply it."""
    # Import lazily to avoid circular imports with run_selfware.py.
    from run_selfware import run_cmd

    patch_path = repo_dir / ".selfware_diff_fallback.diff"
    patch_path.write_text(diff_text, encoding="utf-8")
    try:
        check = run_cmd(
            ["git", "-C", str(repo_dir), "apply", "--check", str(patch_path)],
            logger=logger,
        )
        if check.returncode != 0:
            logger.warning("git apply --check failed: %s", check.stderr.strip())
            return False

        apply = run_cmd(
            ["git", "-C", str(repo_dir), "apply", str(patch_path)],
            logger=logger,
        )
        if apply.returncode != 0:
            logger.warning("git apply failed: %s", apply.stderr.strip())
            return False

        logger.info("Applied diff fallback patch")
        return True
    finally:
        patch_path.unlink(missing_ok=True)


def clean_captured_diff(diff: str) -> str:
    """Remove harness artifacts and submodule changes from a captured diff."""
    lines = diff.splitlines(keepends=True)
    hunks: list[list[str]] = []
    current: list[str] = []
    for line in lines:
        if line.startswith("diff --git"):
            if current:
                hunks.append(current)
            current = [line]
        else:
            current.append(line)
    if current:
        hunks.append(current)

    kept: list[str] = []
    for hunk in hunks:
        if not _is_excluded_diff_hunk(hunk):
            kept.extend(hunk)
    return "".join(kept).rstrip("\n") + "\n" if kept else ""


def filter_patch_to_files(diff: str, allowed_files: set[str]) -> str:
    """Drop diff hunks for files not in ``allowed_files``.

    This prevents cheap models from contaminating a prediction with unrelated
    source-file edits.
    """
    if not allowed_files:
        return diff
    lines = diff.splitlines(keepends=True)
    hunks: list[list[str]] = []
    current: list[str] = []
    for line in lines:
        if line.startswith("diff --git"):
            if current:
                hunks.append(current)
            current = [line]
        else:
            current.append(line)
    if current:
        hunks.append(current)

    kept: list[str] = []
    for hunk in hunks:
        path = _extract_diff_path(hunk[0]) if hunk else None
        if path is not None and path not in allowed_files:
            continue
        kept.extend(hunk)
    return "".join(kept).rstrip("\n") + "\n" if kept else ""
