"""Patch-application helpers shared by the SWE-bench Pro harness."""

from __future__ import annotations

import re
import shutil
import subprocess
from collections import defaultdict
from pathlib import Path
from typing import Any, NamedTuple


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


_SOURCE_CODE_EXTENSIONS = {
    ".py",
    ".go",
    ".js",
    ".ts",
    ".tsx",
    ".rs",
    ".java",
    ".c",
    ".cpp",
    ".h",
    ".hpp",
    ".swift",
    ".kt",
    ".kts",
    ".scala",
    ".rb",
    ".php",
    ".cs",
    ".fs",
    ".fsx",
    ".clj",
    ".cljs",
    ".erl",
    ".hrl",
    ".ex",
    ".exs",
    ".lua",
    ".pl",
    ".pm",
    ".sh",
    ".bash",
    ".zsh",
    ".ps1",
    ".sql",
}


_CONFIG_SUFFIXES = {".json", ".yaml", ".yml", ".toml", ".lock"}
_CONFIG_BASENAMES = {
    "setup.py",
    "setup.cfg",
    "pyproject.toml",
    "package.json",
    "package-lock.json",
    "yarn.lock",
    "requirements.txt",
    "pipfile",
    "tox.ini",
    "pytest.ini",
}


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

    Keeps the trailing newline that belongs to the final `` `` , ``+``, ``-``,
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


def _resolve_safe_path(repo_dir: Path, rel_path: str) -> Path | None:
    """Resolve ``rel_path`` under ``repo_dir`` and ensure it stays inside.

    Rejects absolute paths, parent references, and any target that resolves
    outside of ``repo_dir`` (including via symlinks).
    """
    rel = rel_path.strip()
    if not rel:
        return None
    if any(ch in rel for ch in "\r\n\0"):
        return None
    if len(rel) > 512:
        return None
    p = Path(rel)
    if p.is_absolute():
        return None
    if any(part == ".." for part in p.parts):
        return None
    try:
        base = repo_dir.resolve()
        target = (repo_dir / rel).resolve()
        if target == base:
            return None
        target.relative_to(base)
    except (OSError, RuntimeError, ValueError):
        return None
    return target


class _EditBlock(NamedTuple):
    rel_path: str
    old: str
    new: str


def _parse_edit_blocks(response: str) -> list[_EditBlock]:
    """Parse all SEARCH/REPLACE edit blocks from ``response``."""
    cleaned = re.sub(r"```[a-zA-Z]*\n", "", response)
    cleaned = re.sub(r"\n```\s*$", "", cleaned)
    cleaned = _normalize_edit_response(cleaned)
    pattern = re.compile(
        r"###\s*FILE:\s*[`\"]?(?P<path>[^\r\n]+?)[`\"]?\s*\n"
        r"<<<<<<<\s*(?:SEARCH)?\s*\n"
        r"(?P<old>.*?)\n?"
        r"=======\s*\n"
        r"(?P<new>.*?)\n?"
        r">>>>>>>\s*(?:REPLACE)?",
        re.DOTALL,
    )
    blocks: list[_EditBlock] = []
    for m in pattern.finditer(cleaned):
        blocks.append(_EditBlock(m.group("path").strip(), m.group("old"), m.group("new")))
    return blocks


def _has_patch_markers(t: str) -> bool:
    return any(
        line.startswith(("<<<<<<<", "=======", ">>>>>>>")) for line in t.splitlines()
    )


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


def _normalize_blank_lines_with_map(
    lines: list[str],
) -> tuple[list[str], list[int], list[int]]:
    """Collapse consecutive blanks while preserving original line mapping.

    Returns ``(normalized_lines, start_indices, lengths)`` where each
    normalized line maps back to the original line range it represents.
    """
    norm: list[str] = []
    starts: list[int] = []
    lengths: list[int] = []
    i = 0
    n = len(lines)
    while i < n:
        if lines[i].strip() == "":
            start = i
            while i < n and lines[i].strip() == "":
                i += 1
            norm.append("")
            starts.append(start)
            lengths.append(i - start)
        else:
            norm.append(lines[i])
            starts.append(i)
            lengths.append(1)
            i += 1
    return norm, starts, lengths


def _find_exact_line_range(
    text_lines: list[str], old_lines: list[str]
) -> tuple[int, int] | None:
    """Return [start, end) line indices of an exact full-line match."""
    n = len(old_lines)
    m = len(text_lines)
    if n == 0 or m < n:
        return None
    for i in range(m - n + 1):
        if all(old_lines[j].rstrip() == text_lines[i + j].rstrip() for j in range(n)):
            return i, i + n
    return None


def _find_blank_line_range(
    text_lines: list[str], old_lines: list[str]
) -> tuple[int, int] | None:
    """Return [start, end) line indices of a blank-line-tolerant match."""
    norm_old, _, _ = _normalize_blank_lines_with_map(old_lines)
    norm_text, starts, lengths = _normalize_blank_lines_with_map(text_lines)
    n = len(norm_old)
    m = len(norm_text)
    if n == 0 or m < n:
        return None
    for i in range(m - n + 1):
        if all(norm_old[j].rstrip() == norm_text[i + j].rstrip() for j in range(n)):
            start = starts[i]
            end = starts[i + n - 1] + lengths[i + n - 1]
            return start, end
    return None


def _line_range_to_char_range(
    text_lines: list[str], start: int, end: int
) -> tuple[int, int]:
    """Convert a [start, end) line range into character positions."""
    pos = 0
    start_char = 0
    end_char = 0
    for idx, line in enumerate(text_lines):
        if idx == start:
            start_char = pos
        if idx == end:
            end_char = pos
            break
        pos += len(line) + 1
    else:
        if end == len(text_lines):
            end_char = pos
    return start_char, end_char


def _find_block_char_range(text: str, old: str) -> tuple[int, int] | None:
    """Find the [start, end) character range of ``old`` in ``text``.

    Tries exact substring first, then full-line matches (with gutter and
    trailing-space tolerance), then blank-line-normalized matching.  All
    returned indices refer to ``text`` itself so callers can replace the
    matched region atomically.
    """
    old = old.replace("\r\n", "\n")
    if old == "":
        return 0, 0
    if old in text:
        start = text.index(old)
        return start, start + len(old)

    text_lines = text.splitlines()
    old_lines = _strip_line_number_gutter(old).splitlines()
    n = len(old_lines)
    if n == 0 or n > len(text_lines):
        return None

    text_lines_stripped = [_strip_line_number_gutter(line) for line in text_lines]
    old_lines_stripped = _strip_line_number_gutter(old).splitlines()

    strategies = [
        (old_lines, text_lines),
        (old_lines_stripped, text_lines),
        (old_lines, text_lines_stripped),
        (old_lines_stripped, text_lines_stripped),
    ]

    for oln, tln in strategies:
        rng = _find_exact_line_range(tln, oln)
        if rng is not None:
            return _line_range_to_char_range(text_lines, *rng)
        rng = _find_blank_line_range(tln, oln)
        if rng is not None:
            return _line_range_to_char_range(text_lines, *rng)
    return None


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
    rng = _find_block_char_range(text, old)
    if rng is None:
        return None
    start, end = rng
    return text[:start] + new + text[end:]


def apply_edits_with_missing(
    repo_dir: Path, response: str, logger: Any
) -> tuple[bool, set[str], set[str]]:
    """Apply simple file edits in ### FILE / SEARCH / REPLACE blocks.

    Returns a tuple of:
      * ``applied`` - whether at least one edit was applied successfully.
      * ``missing`` - file paths referenced by the model that do not exist.
      * ``failed`` - existing files whose SEARCH block could not be matched
        (exactly or with fuzzy normalization), or files with unsafe paths or
        overlapping edit blocks.  Callers should treat a non-empty ``failed``
        set the same as a missing file: the patch is incomplete and must be
        retried.
    """
    applied = False
    missing: set[str] = set()
    failed: set[str] = set()

    blocks = _parse_edit_blocks(response)
    if not blocks and re.search(r"###\s*(?:FILE|PATH)\b", response, re.IGNORECASE):
        if "<<<<<<<" in response or ">>>>>>>" in response:
            failed.add("<malformed edit block>")
            if logger is not None:
                logger.warning("Rejecting malformed SEARCH/REPLACE edit block")
            return False, missing, failed

    # Validate paths before doing any work.
    safe_blocks: list[tuple[_EditBlock, Path]] = []
    for block in blocks:
        safe_path = _resolve_safe_path(repo_dir, block.rel_path)
        if safe_path is None:
            if logger is not None:
                logger.warning("Rejecting unsafe path in edit block: %s", block.rel_path)
            failed.add(block.rel_path)
            continue
        safe_blocks.append((block, safe_path))

    # Group by resolved file path so multi-block edits can be verified atomically.
    grouped: dict[Path, list[_EditBlock]] = defaultdict(list)
    for block, safe_path in safe_blocks:
        grouped[safe_path].append(block)

    for safe_path, file_blocks in grouped.items():
        rel_path = file_blocks[0].rel_path

        if not safe_path.exists():
            create_blocks = [
                b for b in file_blocks if _strip_line_number_gutter(b.old).strip() == ""
            ]
            if not create_blocks:
                if logger is not None:
                    logger.warning("Skipping edit to non-existent file: %s", rel_path)
                missing.add(rel_path)
                continue
            if len(create_blocks) > 1 or len(file_blocks) > 1:
                if logger is not None:
                    logger.warning("Refusing ambiguous creation of %s", rel_path)
                failed.add(rel_path)
                continue
            new_text = create_blocks[0].new.replace("\r\n", "\n")
            if _has_patch_markers(new_text):
                if logger is not None:
                    logger.warning(
                        "Refusing to create %s: content contains patch markers", rel_path
                    )
                missing.add(rel_path)
                continue
            try:
                safe_path.parent.mkdir(parents=True, exist_ok=True)
                safe_path.write_text(new_text, encoding="utf-8")
                applied = True
                if logger is not None:
                    logger.info("Created new file %s", rel_path)
            except Exception as exc:
                if logger is not None:
                    logger.error("Failed to create new file %s: %s", rel_path, exc)
                missing.add(rel_path)
            continue

        try:
            text = safe_path.read_text(encoding="utf-8").replace("\r\n", "\n")
        except Exception as exc:
            if logger is not None:
                logger.error("Failed to read %s: %s", rel_path, exc)
            failed.add(rel_path)
            continue

        original_text = text
        original_has_markers = _has_patch_markers(original_text)

        replacements: list[tuple[int, int, str]] = []
        match_failed = False
        for block in file_blocks:
            old = _strip_line_number_gutter(block.old).replace("\r\n", "\n")
            new = _strip_line_number_gutter(block.new).replace("\r\n", "\n")
            rng = _find_block_char_range(text, old)
            if rng is None:
                if logger is not None:
                    logger.warning(
                        "Search block not found in %s (exact and whitespace-normalized)",
                        rel_path,
                    )
                failed.add(rel_path)
                match_failed = True
                break
            start, end = rng
            replacements.append((start, end, new))

        if match_failed:
            continue

        # Verify all matched regions are pairwise disjoint.
        replacements_sorted = sorted(replacements, key=lambda x: x[0])
        overlapped = False
        for i in range(1, len(replacements_sorted)):
            if replacements_sorted[i][0] < replacements_sorted[i - 1][1]:
                overlapped = True
                break
        if overlapped:
            if logger is not None:
                logger.warning("Overlapping SEARCH/REPLACE blocks in %s", rel_path)
            failed.add(rel_path)
            continue

        # Apply atomically from back to front so earlier indices stay valid.
        for start, end, new in reversed(replacements_sorted):
            text = text[:start] + new + text[end:]

        if not original_has_markers and _has_patch_markers(text):
            if logger is not None:
                logger.warning("Refusing edit to %s: result contains patch markers", rel_path)
            failed.add(rel_path)
            continue

        try:
            safe_path.write_text(text, encoding="utf-8")
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
    blocks = _parse_edit_blocks(response)

    # Validate paths before checking content.
    safe_blocks: list[tuple[_EditBlock, Path]] = []
    for block in blocks:
        safe_path = _resolve_safe_path(repo_dir, block.rel_path)
        if safe_path is None:
            if logger is not None:
                logger.warning("verify_edits_apply: unsafe path %s", block.rel_path)
            return False
        safe_blocks.append((block, safe_path))

    grouped: dict[Path, list[_EditBlock]] = defaultdict(list)
    for block, safe_path in safe_blocks:
        grouped[safe_path].append(block)

    for safe_path, file_blocks in grouped.items():
        rel_path = file_blocks[0].rel_path
        if not safe_path.exists():
            # Empty SEARCH block means the model wants to create the file.
            if all(_strip_line_number_gutter(b.old).strip() == "" for b in file_blocks):
                continue
            if logger is not None:
                logger.warning("verify_edits_apply: file does not exist: %s", rel_path)
            return False
        try:
            text = safe_path.read_text(encoding="utf-8").replace("\r\n", "\n")
        except Exception as exc:
            if logger is not None:
                logger.warning("verify_edits_apply: cannot read %s: %s", rel_path, exc)
            return False

        ranges: list[tuple[int, int]] = []
        for block in file_blocks:
            old = _strip_line_number_gutter(block.old).replace("\r\n", "\n")
            rng = _find_block_char_range(text, old)
            if rng is None:
                if logger is not None:
                    logger.warning(
                        "verify_edits_apply: search block not found in %s", rel_path
                    )
                return False
            ranges.append(rng)

        ranges_sorted = sorted(ranges, key=lambda x: x[0])
        for i in range(1, len(ranges_sorted)):
            if ranges_sorted[i][0] < ranges_sorted[i - 1][1]:
                if logger is not None:
                    logger.warning(
                        "verify_edits_apply: overlapping blocks in %s", rel_path
                    )
                return False

    # If no edit blocks were found, the response cannot be applied as edits.
    return bool(blocks)


def _diff_paths_are_safe(repo_dir: Path, diff_text: str) -> bool:
    """Return True when every path mentioned in ``diff_text`` stays under repo_dir."""
    for line in diff_text.splitlines():
        if line.startswith("diff --git"):
            path = _extract_diff_path(line)
            if path is None:
                return False
            if _resolve_safe_path(repo_dir, path) is None:
                return False
    return True


def _filter_edit_blocks_to_source_files(
    response: str,
    extra_allowed: set[str] | None = None,
    test_patch_paths: set[str] | None = None,
    official_fix_paths: set[str] | None = None,
) -> str:
    """Drop edit blocks for tests, docs, build artifacts, and unrelated configs."""
    allowed = (extra_allowed or set()) | (official_fix_paths or set())
    test_patch_paths = test_patch_paths or set()
    blocks = _parse_edit_blocks(response)
    kept: list[str] = []
    for block in blocks:
        path = block.rel_path
        if path in test_patch_paths:
            continue
        if _is_rejected_file(path):
            continue
        if path not in allowed and not _is_likely_source_file(path):
            continue
        kept.append(
            f"### FILE: {path}\n"
            f"<<<<<<< SEARCH\n"
            f"{block.old}\n"
            f"=======\n"
            f"{block.new}\n"
            f">>>>>>> REPLACE"
        )
    return "\n".join(kept)


def apply_model_response_with_missing(
    repo_dir: Path,
    response: str,
    logger: Any,
    *,
    extra_allowed: set[str] | None = None,
    test_patch_paths: set[str] | None = None,
    official_fix_paths: set[str] | None = None,
) -> tuple[bool, set[str]]:
    """Try to apply the model response as a diff, then as edits.

    Returns the application status and the set of file paths that could not be
    applied.  This includes missing files and files whose SEARCH block could not
    be matched (exactly or fuzzily).
    """
    diff = extract_diff(response)
    if diff:
        diff = filter_patch_to_source_files(
            diff,
            extra_allowed=extra_allowed,
            test_patch_paths=test_patch_paths,
            official_fix_paths=official_fix_paths,
        )
        if diff and _diff_paths_are_safe(repo_dir, diff) and apply_patch(repo_dir, diff, logger):
            return True, set()

    # Filter edit blocks to source files and validate paths before applying.
    filtered_response = _filter_edit_blocks_to_source_files(
        response,
        extra_allowed=extra_allowed,
        test_patch_paths=test_patch_paths,
        official_fix_paths=official_fix_paths,
    )
    applied, missing, failed = apply_edits_with_missing(repo_dir, filtered_response, logger)
    unapplied = missing | failed
    if applied:
        return True, unapplied
    if logger is not None:
        logger.warning(
            "Could not apply any patch from model response (missing=%s, failed_search=%s)",
            sorted(missing),
            sorted(failed),
        )
    return False, unapplied


def apply_model_response(
    repo_dir: Path,
    response: str,
    logger: Any,
    *,
    extra_allowed: set[str] | None = None,
    test_patch_paths: set[str] | None = None,
    official_fix_paths: set[str] | None = None,
) -> bool:
    """Try to apply the model response as a diff, then as edits."""
    applied, _ = apply_model_response_with_missing(
        repo_dir,
        response,
        logger,
        extra_allowed=extra_allowed,
        test_patch_paths=test_patch_paths,
        official_fix_paths=official_fix_paths,
    )
    return applied


def is_truncated_diff(response: str) -> bool:
    """Return True if the response looks like a truncated diff block.

    This happens when the model runs out of output tokens inside a `` ```diff ```
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
    """Validate a diff with ``git apply --check`` and then apply it.

    If ``git apply`` rejects the diff, fall back to the system's ``patch -p1``
    command, which is more tolerant of whitespace/offset drift.  This mirrors
    the fuzzy matching already used by the SEARCH/REPLACE applier.
    """
    # Import lazily to avoid circular imports with run_selfware.py.
    from run_selfware import run_cmd

    patch_path = repo_dir / ".selfware_diff_fallback.diff"
    patch_path.write_text(diff_text, encoding="utf-8")
    try:
        check = run_cmd(
            ["git", "-C", str(repo_dir), "apply", "--check", str(patch_path)],
            logger=logger,
        )
        if check.returncode == 0:
            apply = run_cmd(
                ["git", "-C", str(repo_dir), "apply", str(patch_path)],
                logger=logger,
            )
            if apply.returncode == 0:
                logger.info("Applied diff fallback patch")
                return True
            logger.warning("git apply failed: %s", apply.stderr.strip())
        else:
            logger.warning("git apply --check failed: %s", check.stderr.strip())

        if shutil.which("patch") is None:
            logger.warning("patch binary not available; cannot attempt tolerant diff fallback")
            return False

        logger.info("Attempting tolerant patch -p1 fallback")
        patch_proc = run_cmd(
            [
                "patch",
                "-p1",
                "--no-backup-if-mismatch",
                "--force",
                "-i",
                str(patch_path),
            ],
            cwd=repo_dir,
            logger=logger,
        )
        if patch_proc.returncode == 0:
            logger.info("Applied diff fallback patch with patch -p1")
            return True
        logger.warning("patch -p1 fallback failed: %s", patch_proc.stderr.strip())
        return False
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


def _is_rejected_file(path: str) -> bool:
    """Return True for tests, docs, and build artifacts that are never allowed."""
    p = Path(path)
    name = p.name.lower()
    suffix = p.suffix.lower()
    parts = [part.lower() for part in p.parts]

    # Directory components that mark tests or documentation.
    if any(part in ("tests", "test") for part in parts):
        return True
    if any(part in ("docs", "doc", "documentation") for part in parts):
        return True
    if ".github" in parts:
        return True

    # Common test-file basenames.  Restrict the ``test_`` prefix to root-level
    # files so legitimate helpers like ``src/test_helpers.py`` are preserved.
    if name.startswith("test_") and len(p.parts) == 1 and suffix in _SOURCE_CODE_EXTENSIONS:
        return True
    if name.endswith(("_test.go", "_test.py", "_test.js", "_test.ts", "_test.tsx")):
        return True

    # Documentation and build artifacts.
    if suffix in (".md", ".mdx"):
        return True
    if name in ("dockerfile", ".dockerignore", ".gitignore"):
        return True
    if name == "makefile":
        return True
    if suffix in (".bak", ".orig") or name.endswith("~"):
        return True

    return False


def _is_config_or_metadata(path: str) -> bool:
    """Return True for package metadata and config files."""
    p = Path(path)
    name = p.name.lower()
    suffix = p.suffix.lower()
    if suffix in _CONFIG_SUFFIXES:
        return True
    if name in _CONFIG_BASENAMES:
        return True
    return False


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
