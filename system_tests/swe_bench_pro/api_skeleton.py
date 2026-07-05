#!/usr/bin/env python3
"""Current-checkout API skeleton extractor for SWE-bench Pro.

qutebrowser instances often fail because the model hallucinates outdated
module/class/function names.  This module extracts the *current* public API
signatures from the files likely to be touched and appends a compact block to
the prompt so the model sees the real names at the checked-out commit.
"""

from __future__ import annotations

import ast
import logging
from pathlib import Path
from typing import Any

from small_model_adapter import (
    _is_test_file,
    _load_list_field,
    _repo_is,
    rank_files_by_relevance,
)


DEFAULT_MAX_SKELETON_CHARS = 12_000
MAX_FILE_SIZE = 500_000
DOCSTRING_MAX_LEN = 120


def _resolve_safe_repo_path(repo_dir: Path, rel_path: str) -> Path | None:
    """Resolve ``rel_path`` against ``repo_dir`` and reject paths that escape.

    Candidate paths come from the instance and may contain ``../`` or absolute
    components. This helper treats any path whose resolved location is outside
    ``repo_dir`` as invalid, preventing directory-traversal reads.
    """
    try:
        repo_root = repo_dir.resolve()
        candidate = (repo_dir / rel_path).resolve()
        candidate.relative_to(repo_root)
    except (ValueError, RuntimeError, OSError):
        return None
    return candidate


def _is_private_name(name: str) -> bool:
    """Return True for ordinary private names but keep dunder special names."""
    if not name.startswith("_"):
        return False
    # Keep __future__, __init__, __all__, etc.
    if name.startswith("__") and name.endswith("__"):
        return False
    return True


def _format_import_line(stmt: ast.Import | ast.ImportFrom) -> str | None:
    """Reconstruct an import line, dropping private modules/names."""
    if isinstance(stmt, ast.Import):
        names: list[str] = []
        for alias in stmt.names:
            if _is_private_name(alias.name):
                continue
            if alias.asname:
                names.append(f"{alias.name} as {alias.asname}")
            else:
                names.append(alias.name)
        return "import " + ", ".join(names) if names else None

    if isinstance(stmt, ast.ImportFrom):
        module = stmt.module or ""
        if module and _is_private_name(module):
            return None
        names = []
        for alias in stmt.names:
            if alias.name == "*":
                names.append("*")
                continue
            if _is_private_name(alias.name):
                continue
            if alias.asname:
                names.append(f"{alias.name} as {alias.asname}")
            else:
                names.append(alias.name)
        if not names:
            return None
        level = "." * stmt.level
        mod_part = f"{level}{module}" if module else level
        return f"from {mod_part} import " + ", ".join(names)

    return None


def _format_function_signature(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    indent: str = "",
) -> str:
    """Return ``def name(args) -> annotation:`` without decorators or body."""
    prefix = "async def" if isinstance(node, ast.AsyncFunctionDef) else "def"
    args = ast.unparse(node.args)
    returns = f" -> {ast.unparse(node.returns)}" if node.returns else ""
    return f"{indent}{prefix} {node.name}({args}){returns}:"


def _format_class_signature(node: ast.ClassDef) -> str:
    """Return ``class Name(bases):``."""
    if node.bases:
        bases = ", ".join(ast.unparse(b) for b in node.bases)
        return f"class {node.name}({bases}):"
    return f"class {node.name}:"


def _extract_file_skeleton(
    repo_dir: Path,
    rel_path: str,
    keep_private: bool,
) -> str | None:
    """Build a compact API skeleton for a single Python file."""
    path = _resolve_safe_repo_path(repo_dir, rel_path)
    if path is None or not path.is_file() or path.stat().st_size > MAX_FILE_SIZE:
        return None
    if path.suffix.lower() != ".py":
        return None

    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return None

    try:
        tree = ast.parse(text, filename=str(path))
    except SyntaxError as exc:
        logging.getLogger("selfware-sweap").warning(
            "API skeleton parse error in %s: %s", rel_path, exc
        )
        return None

    lines: list[str] = [f"### {rel_path}"]

    docstring = ast.get_docstring(tree)
    if docstring:
        first = docstring.strip().splitlines()[0]
        if len(first) > DOCSTRING_MAX_LEN:
            first = first[: DOCSTRING_MAX_LEN - 3] + "..."
        lines.append(f'"""{first}"""')

    imports: list[str] = []
    body_lines: list[str] = []

    for stmt in tree.body:
        if isinstance(stmt, (ast.Import, ast.ImportFrom)):
            line = _format_import_line(stmt)
            if line:
                imports.append(line)
            continue

        if isinstance(stmt, ast.ClassDef):
            if not keep_private and _is_private_name(stmt.name):
                continue
            body_lines.append(_format_class_signature(stmt))
            for member in stmt.body:
                if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    if not keep_private and _is_private_name(member.name):
                        continue
                    body_lines.append(_format_function_signature(member, indent="    "))
            continue

        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if not keep_private and _is_private_name(stmt.name):
                continue
            body_lines.append(_format_function_signature(stmt))

    if imports:
        lines.extend(imports)
    if body_lines:
        lines.extend(body_lines)

    if len(lines) == 1:
        # Nothing but the header; skip the file.
        return None
    return "\n".join(lines) + "\n"


def extract_api_skeleton(
    repo_dir: Path,
    file_paths: list[str],
    *,
    max_total_chars: int | None = None,
) -> str:
    """Return a compact block of current public API signatures for ``file_paths``.

    For each Python file the block contains the first line of the module
    docstring, top-level imports, class definitions, and function/method
    signatures.  Private names (starting with a single underscore) are skipped
    unless the file is a test file.

    ``max_total_chars`` caps the total size of the returned block; once the
    budget is reached the remaining files are dropped or truncated.
    """
    repo_dir = Path(repo_dir)
    if max_total_chars is None:
        max_total_chars = DEFAULT_MAX_SKELETON_CHARS

    parts: list[str] = []
    total = 0
    for rel in file_paths:
        keep_private = _is_test_file(rel)
        skeleton = _extract_file_skeleton(repo_dir, rel, keep_private)
        if not skeleton:
            continue

        remaining = max_total_chars - total
        if remaining <= 0:
            break

        if len(skeleton) > remaining:
            # If this is the very first file, truncate it rather than omitting
            # the whole skeleton.
            if total == 0:
                cutoff = max(remaining - 30, 100)
                skeleton = skeleton[:cutoff] + "\n... (truncated) ...\n"
                parts.append(skeleton)
            else:
                parts.append("... (truncated due to API skeleton budget) ...\n")
            break

        parts.append(skeleton)
        total += len(skeleton)

    return "".join(parts)


def should_inject_api_skeleton(instance: dict[str, Any]) -> bool:
    """Return True when the instance belongs to a repo known to need API skeletons."""
    repo = instance.get("repo", "")
    return _repo_is(repo, "qutebrowser/qutebrowser")


def _candidate_files(
    repo_dir: Path,
    instance: dict[str, Any],
    top_k: int = 20,
) -> list[str]:
    """Build the ordered list of Python files whose API skeletons should be shown."""
    tests = _load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = _load_list_field(instance.get("fail_to_pass", []))

    search_text = instance.get("problem_statement", "") or ""
    requirements = instance.get("requirements", "") or ""
    if requirements:
        search_text = f"{search_text}\n{requirements}".strip()

    ranked = rank_files_by_relevance(
        repo_dir,
        search_text,
        test_names=tests + fail_to_pass,
        top_k=top_k,
    )

    candidates: list[str] = []
    seen: set[str] = set()
    repo_root = repo_dir.resolve()
    for raw in tests + fail_to_pass + ranked:
        # SWE-bench Pro sometimes stores pytest nodeids like
        # ``tests/unit/test_x.py::TestClass::test_method``.
        file_part = raw.split("::")[0]
        if not file_part.endswith(".py"):
            continue
        candidate_path = _resolve_safe_repo_path(repo_dir, file_part)
        if candidate_path is None or not candidate_path.is_file():
            continue
        safe_rel = candidate_path.relative_to(repo_root).as_posix()
        if safe_rel in seen:
            continue
        seen.add(safe_rel)
        candidates.append(safe_rel)

    return candidates


def inject_api_skeleton(
    host_repo_dir: Path,
    instance: dict[str, Any],
    prompt_text: str,
    logger: logging.Logger,
    *,
    max_total_chars: int | None = None,
) -> str:
    """Append a current API skeleton block to ``prompt_text`` when appropriate.

    The candidate file list is the union of ``selected_test_files_to_run``,
    ``fail_to_pass``, and the top-ranked files from ``rank_files_by_relevance``.
    The skeleton is capped by ``max_total_chars`` to keep the prompt within
    budget.
    """
    if not should_inject_api_skeleton(instance):
        return prompt_text

    candidates = _candidate_files(host_repo_dir, instance)
    if not candidates:
        logger.info(
            "API skeleton injection enabled for %s but no candidate Python files found",
            instance.get("instance_id", "?"),
        )
        return prompt_text

    skeleton = extract_api_skeleton(
        host_repo_dir,
        candidates,
        max_total_chars=max_total_chars,
    )
    if not skeleton.strip():
        return prompt_text

    logger.info(
        "Injecting API skeleton for %s (%s candidate files, %s chars)",
        instance.get("instance_id", "?"),
        len(candidates),
        len(skeleton),
    )
    return (
        f"{prompt_text}\n\n"
        "Current API skeleton (public signatures and imports of files likely to be touched):\n"
        f"{skeleton}"
    )
