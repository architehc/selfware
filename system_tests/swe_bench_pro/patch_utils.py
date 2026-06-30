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


def _trim_trailing_text(diff_text: str) -> str:
    r"""Return ``diff_text`` truncated after the last unified-diff hunk line.

    Keeps the trailing newline that belongs to the final `` ``, ``+``, ``-``,
    or ``\ No newline at end of file`` line so ``git apply`` does not see a
    malformed patch.
    """
    lines = diff_text.splitlines(keepends=True)
    last_hunk_idx = -1
    for i, line in enumerate(lines):
        if line.startswith((" ", "+", "-", "\\")):
            last_hunk_idx = i
    if last_hunk_idx >= 0:
        return "".join(lines[: last_hunk_idx + 1])
    return diff_text


def extract_diff(response: str) -> str | None:
    """Extract a unified diff from the model response.

    Preserves the trailing newline on the final hunk line; stripping it can
    corrupt an otherwise valid ``git apply`` patch.
    """
    for m in re.finditer(r"```(?:diff)?\s*(.*?)\s*```", response, re.DOTALL):
        content = m.group(1)
        if "diff --git" in content:
            return _trim_trailing_text(content[content.index("diff --git"):])
    if "diff --git" in response:
        return _trim_trailing_text(response[response.index("diff --git"):])
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
    makes those blocks apply without requiring a second API call.  Blank lines
    that are shown only as a gutter (e.g. ``  5 |``) are reduced to an empty
    string.

    Recognizes gutters like ``123 |``, ``123:``, ``123.`` and ``123 ``.
    """
    lines = text.splitlines()
    stripped: list[str] = []
    for line in lines:
        # Trailing spaces after the gutter are not meaningful for code.
        line = line.rstrip()
        # Match a left gutter of the form "  123 | " (leading spaces, digits,
        # optional colon/dot or pipe, then one separator space or end-of-line).
        # The alternation order treats space-only gutters as the fallback, so
        # indentation is preserved in content lines and plain text like
        # "123hello" is left untouched.
        line = re.sub(
            r"^\s*\d+(?:\s*(?:[:\.]|[│|])(?:\s|$)|\s+)",
            "",
            line,
            count=1,
        )
        stripped.append(line)
    return "\n".join(stripped)


def _normalize_edit_response(text: str) -> str:
    """Canonicalize sloppy SEARCH/REPLACE edit blocks before parsing.

    Normalization rules:

    * Convert CRLF to LF and trim trailing whitespace on every line.
    * Canonicalize file headers (``### file:``, ``### Path:``,
      ``### FILE``, ...) to ``### FILE:``.
    * Canonicalize markers to ``<<<<<<< SEARCH``, ``=======`` and
      ``>>>>>>> REPLACE`` (case-insensitive).
    * Strip line-number gutters from lines inside SEARCH/REPLACE blocks.
    """
    text = text.replace("\r\n", "\n")
    lines = text.splitlines()
    out: list[str] = []
    in_block = False

    for raw_line in lines:
        line = raw_line.rstrip()

        # Canonicalize file headers before entering a block.
        if not in_block and re.match(r"^###\s*(?:FILE|PATH)\b", line, re.IGNORECASE):
            line = re.sub(
                r"^###\s*(?:FILE|PATH)\b\s*:?",
                "### FILE:",
                line,
                count=1,
                flags=re.IGNORECASE,
            )
            out.append(line)
            continue

        # Start of a SEARCH/REPLACE block.
        if re.match(r"^<<<<<<<\s*(?:SEARCH)?\s*$", line, re.IGNORECASE):
            in_block = True
            out.append("<<<<<<< SEARCH")
            continue

        if not in_block:
            out.append(line)
            continue

        # Inside a SEARCH/REPLACE block.
        if re.match(r"^=======\s*$", line):
            out.append("=======")
            continue

        if re.match(r"^>>>>>>>\s*(?:REPLACE)?\s*$", line, re.IGNORECASE):
            in_block = False
            out.append(">>>>>>> REPLACE")
            continue

        # Strip common line-number gutters from code lines.
        line = _strip_line_number_gutter(line)
        out.append(line)

    return "\n".join(out)


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
    """Replace old block with fuzzy but indentation-safe strategies.

    Strategies, in order of precision:
      1. Exact full-line match.
      2. Strip harness line-number gutters and retry full-line match.
      3. Ignore per-line trailing whitespace (preserve indentation).
      4. Ignore blank-line differences (preserve indentation).

    We intentionally avoid a "strip all whitespace" fallback because it
    frequently matches blocks with mismatched indentation and then inserts the
    replacement text verbatim, corrupting the file (e.g., indenting a whole
    function body).
    """
    text_lines = text.splitlines()
    old_lines = _strip_line_number_gutter(old).splitlines()
    new_lines = _strip_line_number_gutter(new).splitlines()
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
        """Full-line equality (trailing-whitespace tolerant)."""
        m = len(tln)
        if m < len(oln):
            return None
        for i in range(m - len(oln) + 1):
            if all(oln[j].rstrip() == tln[i + j].rstrip() for j in range(len(oln))):
                return i
        return None

    def _try_blank_match(oln: list[str], tln: list[str]) -> int | None:
        """Ignore extra blank lines while preserving indentation."""
        norm_old = _normalize_blank_lines(oln)
        norm_text = _normalize_blank_lines(tln)
        if len(norm_text) < len(norm_old):
            return None
        for i in range(len(norm_text) - len(norm_old) + 1):
            if all(norm_old[j].rstrip() == norm_text[i + j].rstrip() for j in range(len(norm_old))):
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

    return None


def apply_edits_with_missing(
    repo_dir: Path, response: str, logger: Any
) -> tuple[bool, set[str], set[str]]:
    """Apply simple file edits in ### FILE / SEARCH / REPLACE blocks.

    Returns a tuple of:
      * ``applied`` - whether at least one edit was applied successfully.
      * ``missing`` - file paths referenced by the model that do not exist.
      * ``failed`` - existing files whose SEARCH block could not be matched
        (exactly or with fuzzy normalization).  Callers should treat a
        non-empty ``failed`` set the same as a missing file: the patch is
        incomplete and must be retried.
    """
    # Strip markdown code fences so wrapped blocks still parse.
    cleaned = re.sub(r"```[a-zA-Z]*\n", "", response)
    cleaned = re.sub(r"\n```\s*$", "", cleaned)

    # Tolerate sloppy formatting from small models before parsing the blocks.
    cleaned = _normalize_edit_response(cleaned)

    # Allow empty SEARCH/REPLACE blocks even when the model omits the blank
    # line between the marker and the next delimiter.  The optional \n? after
    # the captured text lets the parser accept both:
    #   <<<<<<< SEARCH\n=======\n  and  <<<<<<< SEARCH\n\n=======\n
    pattern = re.compile(
        r"###\s*FILE:\s*[`\"]?(?P<path>.+?)[`\"]?\s*\n"
        r"<<<<<<<\s*(?:SEARCH)?\s*\n"
        r"(?P<old>.*?)\n?"
        r"=======\s*\n"
        r"(?P<new>.*?)\n?"
        r">>>>>>>\s*(?:REPLACE)?",
        re.DOTALL,
    )
    def _has_patch_markers(t: str) -> bool:
        return any(
            line.startswith(("<<<<<<<", "=======", ">>>>>>>")) for line in t.splitlines()
        )

    applied = False
    missing: set[str] = set()
    failed: set[str] = set()
    for m in pattern.finditer(cleaned):
        rel_path = m.group("path").strip()
        old = _strip_line_number_gutter(m.group("old"))
        new = _strip_line_number_gutter(m.group("new"))
        path = repo_dir / rel_path
        if not path.exists():
            # Allow creation of new files when the SEARCH block is empty.
            if old.strip() == "":
                new_text = new.replace("\r\n", "\n")
                if _has_patch_markers(new_text):
                    if logger is not None:
                        logger.warning("Refusing to create %s: content contains patch markers", rel_path)
                    missing.add(rel_path)
                    continue
                try:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(new_text, encoding="utf-8")
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
                    failed.add(rel_path)
                    continue
                text = fuzzy
            else:
                text = text.replace(old, new, 1)
            # Reject edits that leave patch markers behind; they are almost always
            # malformed model output and would corrupt the file.
            if _has_patch_markers(text) and not _has_patch_markers(
                path.read_text(encoding="utf-8")
            ):
                if logger is not None:
                    logger.warning("Refusing edit to %s: result contains patch markers", rel_path)
                failed.add(rel_path)
                continue
            path.write_text(text, encoding="utf-8")
            applied = True
            if logger is not None:
                logger.info("Applied edit to %s", rel_path)
        except Exception as exc:
            if logger is not None:
                logger.error("Failed to apply edit to %s: %s", rel_path, exc)
            failed.add(rel_path)
    return applied, missing, failed


def apply_edits(repo_dir: Path, response: str, logger: Any) -> bool:
    """Apply simple file edits in ### FILE / SEARCH / REPLACE blocks.

    Returns ``True`` only when every parsed edit block was applied successfully.
    A single failed SEARCH/REPLACE block makes the whole response unapplyable.
    """
    applied, _, failed = apply_edits_with_missing(repo_dir, response, logger)
    return applied and not failed


def verify_edits_apply(repo_dir: Path, response: str, logger: Any) -> bool:
    """Return True if every SEARCH/REPLACE block in ``response`` can be applied.

    This checks against the files on disk without modifying them, so a failed
    prompt can be retried with more exact context.
    """
    cleaned = re.sub(r"```[a-zA-Z]*\n", "", response)
    cleaned = re.sub(r"\n```\s*$", "", cleaned)
    cleaned = _normalize_edit_response(cleaned)
    pattern = re.compile(
        r"###\s*FILE:\s*[`<\"]?(?P<path>.+?)[`\"]?\s*\n"
        r"<<<<<<<\s*(?:SEARCH)?\s*\n"
        r"(?P<old>.*?)\n?"
        r"=======\s*\n"
        r"(?P<new>.*?)\n?"
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

    Returns the application status and the set of file paths that could not be
    applied.  This includes missing files and files whose SEARCH block could not
    be matched (exactly or fuzzily).
    """
    diff = extract_diff(response)
    if diff and apply_patch(repo_dir, diff, logger):
        return True, set()
    applied, missing, failed = apply_edits_with_missing(repo_dir, response, logger)
    unapplied = missing | failed
    if applied and not unapplied:
        return True, set()
    if logger is not None:
        logger.warning(
            "Could not apply patch from model response (missing=%s, failed_search=%s)",
            sorted(missing),
            sorted(failed),
        )
    return False, unapplied


def apply_model_response(repo_dir: Path, response: str, logger: Any) -> bool:
    """Try to apply the model response as a diff, then as edits."""
    applied, _ = apply_model_response_with_missing(repo_dir, response, logger)
    return applied


def is_truncated_diff(response: str) -> bool:
    """Return True if the response looks like a truncated diff block.

    This happens when the model runs out of output tokens inside a `` ```diff ``
    block.  A truncated response is not applyable as-is.
    """
    original_ends_newline = response.endswith("\n")
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
        if lines and re.match(r"^[-+ ]", lines[-1]) and not original_ends_newline:
            return True
    return False


def extract_partial_diff(response: str) -> str | None:
    """Try to salvage a partial diff from a truncated response.

    If the response ends mid-hunk, this returns the diff up to the last complete
    hunk.  Complete hunks are kept; only a trailing hunk header with no body
    lines is dropped.  Returns ``None`` if no usable diff is present.
    """
    diff = extract_diff(response)
    if not diff:
        return None
    lines = diff.splitlines()
    hunk_starts = [i for i, line in enumerate(lines) if line.startswith("@@")]
    if not hunk_starts:
        return None

    # Determine the last hunk that actually has body lines.
    last_usable = -1
    for idx, start in enumerate(hunk_starts):
        end = hunk_starts[idx + 1] if idx + 1 < len(hunk_starts) else len(lines)
        body_lines = [
            line for line in lines[start + 1 : end] if line.startswith((" ", "-", "+"))
        ]
        if body_lines:
            last_usable = idx

    if last_usable < 0:
        return None

    keep_up_to = (
        hunk_starts[last_usable + 1]
        if last_usable + 1 < len(hunk_starts)
        else len(lines)
    )
    return "\n".join(lines[:keep_up_to]) + "\n"


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


def paths_from_patch(patch_text: str) -> set[str]:
    """Return all repo-relative target paths mentioned in a unified diff."""
    paths: set[str] = set()
    for line in patch_text.splitlines():
        if line.startswith("diff --git a/"):
            match = re.match(r"^diff --git a/(.+?) b/(.+?)(?:\s|$)", line)
            if match:
                paths.add(match.group(2))
    return paths


def filter_patch_excluding_paths(diff: str, excluded_paths: set[str]) -> str:
    """Drop diff hunks whose target path is in ``excluded_paths``."""
    if not excluded_paths:
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
        if path is not None and path in excluded_paths:
            continue
        kept.extend(hunk)
    return "".join(kept).rstrip("\n") + "\n" if kept else ""


# Path patterns that are never source files we want in a prediction.
_REJECTED_PATH_PATTERNS = (
    "_test.go", "_test.py", "_test.js", "_test.ts", "_test.tsx",
    "test_", "/tests/", "/test/",
    ".md", ".mdx",
    "Dockerfile", ".dockerignore", ".gitignore", ".github/",
    "docs/", "doc/", "documentation/",
    "Makefile",
    ".bak", ".orig", "~",
)

# Config / metadata files that are legitimate only when the official fix patch
# also edits them (passed via ``extra_allowed`` / ``official_fix_paths``).
_CONFIG_PATH_PATTERNS = (
    ".json", ".yaml", ".yml", ".toml", ".lock",
    "setup.py", "setup.cfg", "pyproject.toml", "package.json",
    "package-lock.json", "yarn.lock", "requirements.txt", "Pipfile",
)


def _path_matches_any(path: str, patterns: tuple[str, ...]) -> bool:
    """Return True when ``path`` matches any substring or suffix pattern."""
    lower = path.lower()
    return any(lower.endswith(pat) for pat in patterns) or any(pat in lower for pat in patterns)


def _is_rejected_file(path: str) -> bool:
    """Return True for tests, docs, and build artifacts that are never allowed."""
    return _path_matches_any(path, _REJECTED_PATH_PATTERNS)


def _is_config_or_metadata(path: str) -> bool:
    """Return True for package metadata and config files."""
    return _path_matches_any(path, _CONFIG_PATH_PATTERNS)


def _is_likely_source_file(path: str) -> bool:
    """Return True when ``path`` looks like a source file the model may edit."""
    return not _is_rejected_file(path) and not _is_config_or_metadata(path)


def filter_patch_to_source_files(
    diff: str,
    extra_allowed: set[str] | None = None,
    test_patch_paths: set[str] | None = None,
    official_fix_paths: set[str] | None = None,
) -> str:
    """Drop diff hunks for tests, docs, build artifacts, and unrelated configs.

    Keeps all source-file hunks plus any paths in ``extra_allowed`` or
    ``official_fix_paths``.  ``official_fix_paths`` is the set of paths touched
    by the official fix patch; if the official fix edits package metadata or
    config files (e.g. ``package.json``, ``pyproject.toml``, ``*.yaml``) the
    model is allowed to edit those same paths.

    Hunks whose target path appears in ``test_patch_paths`` are always dropped
    so the official benchmark test patch is never re-submitted as part of the
    prediction.
    """
    allowed = (extra_allowed or set()) | (official_fix_paths or set())
    test_patch_paths = test_patch_paths or set()
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
        if path is not None:
            if path in test_patch_paths:
                continue
            if _is_rejected_file(path):
                continue
            if path not in allowed and not _is_likely_source_file(path):
                continue
        kept.extend(hunk)
    return "".join(kept).rstrip("\n") + "\n" if kept else ""
