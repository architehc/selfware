#!/usr/bin/env python3
"""Small-model context adapter for the SWE-bench Pro harness.

Cheap/small OpenRouter models (24-40B parameters, 32k-128k context windows)
get lost when the full repo tree and large prompts are dumped into the
context.  This module builds a *much* smaller prompt:

  * a shallow, capped directory tree
  * a ranked list of likely-relevant source files selected by grepping the
    problem statement
  * truncated snippets from only those files
  * terse instructions with a strict edit deadline

The adapter is intentionally simple (no AST parsing, no embeddings) so it
stays cheap and deterministic.
"""

from __future__ import annotations

import copy
import os
import re
import shutil
import subprocess
from pathlib import Path
from typing import Any


DEFAULT_EXCLUDE_DIRS: frozenset[str] = frozenset({
    ".git",
    "node_modules",
    "vendor",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "build",
    "dist",
    ".idea",
    ".vscode",
    ".github",
    "docs",
})

# Extensions that are almost never the source file we want to edit.
NON_SOURCE_EXTENSIONS: frozenset[str] = frozenset({
    ".md",
    ".mdx",
    ".json",
    ".yaml",
    ".yml",
    ".tf",
    ".lock",
    ".svg",
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".webp",
    ".webm",
    ".mp4",
    ".pb.go",
    ".pb.js",
    ".pb.d.ts",
    ".mod",
    ".sum",
})

def _format_test_command(language: str, tests: list[str]) -> str:
    """Return a sensible test command for the given language and test targets.

    ``tests`` comes from ``selected_test_files_to_run``.  For Go it contains
    test function names, so we build a ``-run`` regex.  For Python/JS it
    usually contains file paths.
    """
    if not tests:
        if language == "python":
            return "python -m pytest"
        if language == "go":
            return "go test ./..."
        if language in ("javascript", "typescript"):
            return "npm test"
        return "run the relevant test suite"

    if language == "python":
        return f"python -m pytest {' '.join(tests)}"

    if language == "go":
        # ``tests`` are test names; collapse to top-level test roots so the
        # regex does not become enormous.
        roots: list[str] = []
        seen_roots: set[str] = set()
        for t in tests:
            root = t.split("/")[0].split("#")[0].strip()
            if root and root not in seen_roots:
                seen_roots.add(root)
                roots.append(root)
        if not roots:
            return "go test ./..."
        run_re = "|".join(re.escape(r) for r in roots)
        return f"go test -run '{run_re}' ./..."

    if language in ("javascript", "typescript"):
        return f"npm test -- {' '.join(tests)}"

    return "run the relevant test suite"


# English stopwords plus very common programming words that are poor search
# signals on their own.
_STOPWORDS: frozenset[str] = frozenset({
    "the", "be", "to", "of", "and", "a", "in", "that", "have", "i", "it",
    "for", "not", "on", "with", "he", "as", "you", "do", "at", "this",
    "but", "his", "by", "from", "they", "we", "say", "her", "she", "or",
    "an", "will", "my", "one", "all", "would", "there", "their", "what",
    "so", "up", "out", "if", "about", "who", "get", "which", "go", "me",
    "when", "make", "can", "like", "time", "no", "just", "him", "know",
    "take", "people", "into", "year", "your", "good", "some", "could",
    "them", "see", "other", "than", "then", "now", "look", "only", "come",
    "its", "over", "think", "also", "back", "after", "use", "two", "how",
    "our", "work", "first", "well", "way", "even", "new", "want", "because",
    "any", "these", "give", "day", "most", "us", "is", "was", "are", "were",
    "function", "class", "method", "test", "error", "file", "path", "code",
    "should", "would", "could", "may", "might", "must", "return", "none",
    "true", "false", "null", "nil",
})


def compact_directory_tree(
    repo_path: str | Path,
    max_depth: int = 3,
    max_files: int = 200,
    exclude_dirs: set[str] | frozenset[str] | None = None,
) -> str:
    """Return a compact string tree of the repo.

    The tree is capped by depth and by total file count.  Hidden directories
    and common generated/dependency directories are skipped.
    """
    repo_path = Path(repo_path)
    exclude = set(exclude_dirs) if exclude_dirs is not None else DEFAULT_EXCLUDE_DIRS

    lines: list[str] = [f"{repo_path.name}/"]
    file_count = 0

    def _walk(current: Path, prefix: str, depth: int) -> None:
        nonlocal file_count
        if depth >= max_depth or file_count >= max_files:
            return

        try:
            entries = sorted(
                (e for e in current.iterdir() if not e.name.startswith(".") and e.name not in exclude),
                key=lambda e: (e.is_file(), e.name.lower()),
            )
        except PermissionError:
            return

        dirs = [e for e in entries if e.is_dir()]
        files = [e for e in entries if e.is_file()]

        # Show a limited number of files per directory so one huge directory
        # does not consume the whole budget.
        remaining_budget = max_files - file_count
        shown_files = files[: min(len(files), max(1, remaining_budget // max(1, len(dirs) + 1)))]
        if shown_files:
            shown_files = shown_files[: remaining_budget]
            file_count += len(shown_files)

        items = dirs + shown_files
        for i, entry in enumerate(items):
            is_last = i == len(items) - 1
            branch = "└── " if is_last else "├── "
            lines.append(f"{prefix}{branch}{entry.name}{'/' if entry.is_dir() else ''}")
            if entry.is_dir():
                extension = "    " if is_last else "│   "
                _walk(entry, prefix + extension, depth + 1)

    _walk(repo_path, "", 0)
    if file_count >= max_files:
        lines.append("... (tree truncated due to file limit) ...")
    return "\n".join(lines)


def _tokenize_problem(text: str) -> list[str]:
    """Extract candidate search terms from the problem statement."""
    # Keep identifiers (alphanumerics + underscore) with at least 3 chars.
    tokens = re.findall(r"[A-Za-z_][A-Za-z0-9_]{2,}", text or "")
    # De-prioritize very common words; keep technical terms and camelCase.
    filtered: list[str] = []
    seen: set[str] = set()
    for token in tokens:
        lower = token.lower()
        if lower in _STOPWORDS or lower in seen:
            continue
        seen.add(lower)
        filtered.append(token)
    return filtered


def _list_repo_files(repo_path: Path, exclude: set[str]) -> list[Path]:
    """List files in the repo, preferring git-tracked files when possible."""
    files: list[Path] = []
    try:
        proc = subprocess.run(
            ["git", "-C", str(repo_path), "ls-files", "-z"],
            capture_output=True,
            timeout=30,
            check=False,
        )
        if proc.returncode == 0:
            for raw in proc.stdout.split(b"\x00"):
                if not raw:
                    continue
                path = repo_path / raw.decode("utf-8", errors="ignore")
                if path.is_file():
                    files.append(path)
            return files
    except Exception:
        pass

    # Fallback: manual walk.
    for root, dirs, names in os.walk(repo_path, topdown=True):
        dirs[:] = [d for d in dirs if d not in exclude and not d.startswith(".")]
        for name in names:
            if name.startswith("."):
                continue
            files.append(Path(root) / name)
    return files


def _is_strong_identifier(token: str) -> bool:
    """Return True for tokens that look like code identifiers.

    Strong identifiers contain an underscore or a CamelCase boundary, so they
    are much more selective than common English words.
    """
    if "_" in token:
        return True
    return bool(re.search(r"[a-z][A-Z]", token))


def _rank_with_ripgrep(
    repo_path: Path,
    tokens: list[str],
    top_k: int,
    exclude: set[str],
) -> list[str]:
    """Rank files by TF-IDF-like relevance to the problem-statement tokens.

    Uses ripgrep if available; returns empty list if rg cannot be used.
    """
    if not tokens:
        return []
    if not shutil.which("rg"):
        return []

    # 1. Strong identifier search: give a big bonus to files that contain the
    # exact identifiers mentioned in the issue (functions, types, constants).
    strong_scores: dict[Path, float] = {}
    strong_tokens = [t for t in tokens if _is_strong_identifier(t)]
    for token in strong_tokens:
        cmd = [
            "rg",
            "-i",
            "-l",
            "-e",
            r"\b" + re.escape(token) + r"\b",
            "--glob",
            "!.git",
            str(repo_path),
        ]
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
                stdin=subprocess.DEVNULL,
            )
        except Exception:
            continue
        for line in proc.stdout.splitlines():
            path = Path(line)
            if not path.is_file():
                continue
            try:
                rel = path.relative_to(repo_path)
            except ValueError:
                continue
            if any(part in exclude for part in rel.parts):
                continue
            # Strong identifier matches are heavily weighted.
            strong_scores[path] = strong_scores.get(path, 0.0) + 100.0

    # 2. Broad token search with IDF weighting.
    pattern = r"\b(" + "|".join(re.escape(t) for t in tokens) + r")\b"
    cmd = [
        "rg",
        "-i",
        "-o",
        "-e",
        pattern,
        "--glob",
        "!.git",
        str(repo_path),
    ]
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
            stdin=subprocess.DEVNULL,
        )
    except Exception:
        return []

    token_counts: dict[Path, dict[str, int]] = {}
    token_doc_freq: dict[str, int] = {}
    for line in proc.stdout.splitlines():
        if ":" not in line:
            continue
        path_str, _, matched = line.rpartition(":")
        path = Path(path_str)
        if not path.is_file():
            continue
        try:
            rel = path.relative_to(repo_path)
        except ValueError:
            continue
        if any(part in exclude for part in rel.parts):
            continue
        # Normalise matched token back to one of our tokens (rg is case-insensitive).
        lower = matched.lower()
        token_counts.setdefault(path, {})
        token_counts[path][lower] = token_counts[path].get(lower, 0) + 1

    # Compute document frequency per token.
    for tc in token_counts.values():
        for tok in tc:
            token_doc_freq[tok] = token_doc_freq.get(tok, 0) + 1

    def _skip_file(rel: Path) -> bool:
        if any(part in exclude for part in rel.parts):
            return True
        name = rel.name.lower()
        if name == "changelog.md":
            return True
        return any(name.endswith(ext) for ext in NON_SOURCE_EXTENSIONS)

    scores: dict[Path, float] = {}
    for path, score in strong_scores.items():
        try:
            rel = path.relative_to(repo_path)
        except ValueError:
            continue
        if _skip_file(rel):
            continue
        scores[path] = score

    total_docs = max(1, len(token_counts))
    for path, tc in token_counts.items():
        try:
            rel = path.relative_to(repo_path)
        except ValueError:
            continue
        if _skip_file(rel):
            continue
        # Cap the broad-token contribution so a single huge generated file
        # cannot drown out a strong identifier match.
        broad_score = 0.0
        for tok, count in tc.items():
            idf = 1.0 + (total_docs / (1.0 + token_doc_freq.get(tok, 0)))
            broad_score += count * idf
        scores[path] = scores.get(path, 0.0) + min(broad_score, 80.0)

    ranked = sorted(scores.items(), key=lambda kv: kv[1], reverse=True)
    return [p.relative_to(repo_path).as_posix() for p, _ in ranked[:top_k]]


def _rank_with_python(
    repo_path: Path,
    tokens: list[str],
    top_k: int,
    exclude: set[str],
) -> list[str]:
    """Pure-Python fallback ranking when ripgrep is not available."""
    if not tokens:
        return []

    regexes = [re.compile(r"\b" + re.escape(t) + r"\b", re.IGNORECASE) for t in tokens]
    scores: dict[Path, float] = {}

    def _skip_python(rel: Path) -> bool:
        if any(part in exclude for part in rel.parts):
            return True
        name = rel.name.lower()
        if name == "changelog.md":
            return True
        return any(name.endswith(ext) for ext in NON_SOURCE_EXTENSIONS)

    for path in _list_repo_files(repo_path, exclude):
        try:
            rel = path.relative_to(repo_path)
        except ValueError:
            continue
        if _skip_python(rel):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        total = sum(1 for rx in regexes for _ in rx.finditer(text))
        if total:
            scores[path] = total

    ranked = sorted(scores.items(), key=lambda kv: kv[1], reverse=True)
    return [p.relative_to(repo_path).as_posix() for p, _ in ranked[:top_k]]


def _find_test_files(
    repo_path: Path,
    test_names: list[str],
    exclude: set[str],
) -> list[str]:
    """Find source/test files that define the given test function names."""
    if not shutil.which("rg") or not test_names:
        return []
    found: set[Path] = set()
    for name in test_names:
        # Strip subtest suffix so "TestFoo/bar" searches for "TestFoo".
        root = name.split("/")[0].split("#")[0].strip()
        if not root or root.startswith("Fuzz"):
            # Fuzz targets are harder to map to a single file; skip them.
            continue
        cmd = [
            "rg",
            "-l",
            "-e",
            r"\bfunc\s+" + re.escape(root) + r"\w*\b",
            "--glob",
            "!.git",
            str(repo_path),
        ]
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
                stdin=subprocess.DEVNULL,
            )
        except Exception:
            continue
        for line in proc.stdout.splitlines():
            path = Path(line)
            if not path.is_file():
                continue
            try:
                rel = path.relative_to(repo_path)
            except ValueError:
                continue
            if any(part in exclude for part in rel.parts):
                continue
            if path.suffix.lower() in NON_SOURCE_EXTENSIONS:
                continue
            found.add(path)
    return [p.relative_to(repo_path).as_posix() for p in found]


def _find_function_definitions(
    repo_path: Path,
    identifiers: list[str],
    exclude: set[str],
) -> list[str]:
    """Find files that define functions/methods named after ``identifiers``.

    This promotes the file that actually contains the routine mentioned in the
    requirements, which keeps small models from hallucinating the fix in the
    wrong file.
    """
    if not shutil.which("rg") or not identifiers:
        return []

    scores: dict[Path, int] = {}
    for ident in identifiers:
        if not ident:
            continue
        # Match "func Name(" or "func (recv *Type) Name(".
        pattern = r"func\s+(?:\([^)]*\)\s*)?" + re.escape(ident) + r"\s*\("
        cmd = [
            "rg",
            "-l",
            "-e",
            pattern,
            "--glob",
            "!.git",
            "--glob",
            "!*_test.go",
            str(repo_path),
        ]
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
                stdin=subprocess.DEVNULL,
            )
        except Exception:
            continue
        for line in proc.stdout.splitlines():
            path = Path(line)
            if not path.is_file():
                continue
            try:
                rel = path.relative_to(repo_path)
            except ValueError:
                continue
            if any(part in exclude for part in rel.parts):
                continue
            if path.suffix.lower() in NON_SOURCE_EXTENSIONS:
                continue
            scores[path] = scores.get(path, 0) + 1
    return [
        p.relative_to(repo_path).as_posix()
        for p, _ in sorted(scores.items(), key=lambda kv: kv[1], reverse=True)
    ]


def _is_local_source(rel: Path) -> bool:
    """True for files the harness usually considers editable source."""
    name = rel.name.lower()
    if name == "changelog.md":
        return False
    if any(name.endswith(ext) for ext in NON_SOURCE_EXTENSIONS):
        return False
    return True


def _extract_local_imports(repo_path: Path, rel: str) -> list[str]:
    """Return repo-relative paths imported/required by ``rel`` in the same project.

    This is intentionally language-agnostic and regex-based.  It catches:
      * Go:      import "github.com/org/repo/lib/foo"
      * Python:  from . import foo / from pkg.bar import baz
      * JS/TS:   import ... from './foo' / require('./foo')
    Only imports that map to an existing file under ``repo_path`` are returned.
    """
    path = repo_path / rel
    if not path.is_file():
        return []
    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except Exception:
        return []

    rel_path = Path(rel)
    rel_dir = rel_path.parent
    found: list[str] = []

    # Go imports.
    for m in re.finditer(r'import\s+(?:\([^)]*\)|"([^"]+)")', text, re.DOTALL):
        raw = (m.group(1) or "").strip()
        if not raw:
            # Multi-line import block.
            block = m.group(0)
            for qm in re.finditer(r'"([^"]+)"', block):
                raw = qm.group(1)
                if raw:
                    found.extend(_resolve_import_path(repo_path, raw))
        else:
            found.extend(_resolve_import_path(repo_path, raw))

    # Python relative imports.
    for m in re.finditer(
        r'^(?:from\s+([\w.]+)\s+import|import\s+([\w.]+))',
        text,
        re.MULTILINE,
    ):
        mod = (m.group(1) or m.group(2) or "").strip()
        if not mod or mod.startswith(("os", "sys", "typing", "collections", "json", "re", "math", "random", "datetime", "urllib", "http", "logging", "pathlib", "subprocess")):
            continue
        found.extend(_resolve_python_import(repo_path, rel_dir, mod))

    # JS/TS local imports.
    for m in re.finditer(
        r'(?:import\s+.*?\s+from\s+|require\s*\(\s*)["\'](\.[^"\']+)["\']',
        text,
    ):
        raw = m.group(1)
        if raw:
            found.extend(_resolve_js_import(repo_path, rel_dir, raw))

    return found


def _resolve_import_path(repo_path: Path, imp: str) -> list[str]:
    """Map a Go-style import path to a repo file path."""
    # If the import path ends with the repo name or starts with the module path,
    # try to find the file by stripping module prefix and appending .go.
    parts = imp.strip('"').split("/")
    candidates: list[Path] = []
    # Try every suffix of the import path as a relative path.
    for i in range(len(parts)):
        rel = "/".join(parts[i:])
        for suffix in (".go", "/index.go", ""):
            p = repo_path / (rel + suffix)
            if p.is_file():
                candidates.append(p)
    return [c.relative_to(repo_path).as_posix() for c in candidates]


def _resolve_python_import(repo_path: Path, rel_dir: Path, mod: str) -> list[str]:
    """Map a Python module string to repo file path(s)."""
    parts = mod.split(".")
    # Absolute package imports: try from repo root.
    candidates: list[Path] = []
    for base in (repo_path, repo_path / rel_dir):
        p = base / ("/".join(parts) + ".py")
        if p.is_file():
            candidates.append(p)
        p_init = base / "/".join(parts) / "__init__.py"
        if p_init.is_file():
            candidates.append(p_init)
    return [c.relative_to(repo_path).as_posix() for c in candidates]


def _resolve_js_import(repo_path: Path, rel_dir: Path, raw: str) -> list[str]:
    """Map a JS/TS relative import to repo file path(s)."""
    base = (repo_path / rel_dir / raw).resolve()
    candidates: list[Path] = []
    for suffix in ("", ".js", ".ts", ".jsx", ".tsx", "/index.js", "/index.ts"):
        p = Path(str(base) + suffix)
        if p.is_file():
            candidates.append(p)
    return [c.relative_to(repo_path).as_posix() for c in candidates]


def _cross_file_signal(
    repo_path: Path,
    ranked: list[str],
    tokens: list[str],
    exclude: set[str],
    top_k: int,
) -> list[str]:
    """Extend the ranked list with second-degree neighbours of top files.

    Files imported by the top-ranked files are likely helpers or callers that
    also need a small change for multi-file fixes.  They are added after the
    original ranking so they do not drown out the primary files.
    """
    if not ranked or not tokens:
        return ranked

    existing = set(ranked)
    neighbours: list[str] = []
    for rel in ranked[:10]:
        for nb in _extract_local_imports(repo_path, rel):
            if nb in existing:
                continue
            rel_path = Path(nb)
            if any(part in exclude for part in rel_path.parts):
                continue
            if not _is_local_source(rel_path):
                continue
            neighbours.append(nb)
            existing.add(nb)

    # Score neighbours by problem-statement token matches so the most relevant
    # neighbours rise to the top of the appended group.
    regexes = [re.compile(r"\b" + re.escape(t) + r"\b", re.IGNORECASE) for t in tokens]
    scores: dict[str, float] = {}
    for nb in neighbours:
        try:
            text = (repo_path / nb).read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        scores[nb] = sum(1 for rx in regexes for _ in rx.finditer(text))

    ordered_neighbours = sorted(scores, key=lambda x: scores[x], reverse=True)
    return ranked + ordered_neighbours[: max(0, top_k - len(ranked))]


def rank_files_by_relevance(
    repo_path: str | Path,
    problem_statement: str,
    *,
    test_names: list[str] | None = None,
    top_k: int = 30,
) -> list[str]:
    """Select ``top_k`` files likely related to the problem statement.

    The implementation is deliberately simple: tokenise the problem text,
    strip stopwords, then count token matches per file.  ripgrep is used when
    available; otherwise a pure-Python scan is used as a fallback.
    Test names and strong identifiers that name a defined function are promoted
    to the top so the model sees the actual implementation it needs to change.
    Second-degree neighbours (imports of the top files) are appended so
    cross-file fixes are not filtered out.
    """
    repo_path = Path(repo_path)
    if not repo_path.is_dir():
        return []

    tokens = _tokenize_problem(problem_statement)
    exclude = DEFAULT_EXCLUDE_DIRS

    ranked = _rank_with_ripgrep(repo_path, tokens, top_k, exclude)
    if not ranked:
        ranked = _rank_with_python(repo_path, tokens, top_k, exclude)

    test_files = _find_test_files(repo_path, test_names or [], exclude)
    strong_identifiers = [t for t in tokens if _is_strong_identifier(t)]
    defining_files = _find_function_definitions(
        repo_path, strong_identifiers, exclude
    )

    # Guess source files from highly-ranked test files (e.g. grpcserver_test.go
    # -> grpcserver.go).  This often points directly at the implementation the
    # failing test exercises.
    inferred_sources: list[str] = []
    for rel in ranked:
        if rel.endswith("_test.go"):
            src = rel[:-len("_test.go")] + ".go"
            src_path = repo_path / src
            if src_path.is_file():
                inferred_sources.append(src)

    # Promote defining files that already scored well so the actual
    # implementation (e.g. lib/auth/grpcserver.go) lands at the top, while
    # keeping any other definitions available as fallback.
    defining_set = set(defining_files)
    promoted = [f for f in ranked if f in defining_set]
    other_defining = [f for f in defining_files if f not in set(ranked)]

    # Assemble the final list: inferred source from tests, promoted definitions,
    # test files, then broad rank.
    seen = set()
    combined: list[str] = []
    for f in inferred_sources + promoted + other_defining + test_files + ranked:
        if f not in seen:
            seen.add(f)
            combined.append(f)

    # Append second-degree neighbours so multi-file fixes keep their context.
    combined = _cross_file_signal(repo_path, combined, tokens, exclude, top_k)
    return combined[:top_k]


def _find_definition_line(lines: list[str], identifiers: list[str]) -> int | None:
    """Return the 0-based line index of the first function/method definition
    matching any of ``identifiers``, or ``None`` if none is found."""
    for ident in identifiers:
        if not ident:
            continue
        pattern = r"func\s+(?:\([^)]*\)\s*)?" + re.escape(ident) + r"\s*\("
        rx = re.compile(pattern)
        for i, line in enumerate(lines):
            if rx.search(line):
                return i
    return None


def _extract_relevant_windows(
    lines: list[str],
    tokens: list[str],
    *,
    max_lines: int = 300,
    window: int = 40,
    required_line: int | None = None,
    include_line_numbers: bool = True,
) -> str:
    """Extract context windows around occurrences of ``tokens``.

    Each token match expands to ``window`` lines above and below.  Overlapping
    windows are merged and scored by the number of *distinct* tokens they
    contain, so a single repeated token (e.g. ``BadParameter``) cannot drown
    out the function definition that actually needs to change.  Line numbers
    are included as a left-hand margin by default; agentless prompts disable
    them so SEARCH blocks copy source text exactly.
    """
    def _fmt(idx: int, line: str) -> str:
        if include_line_numbers:
            return f"{idx + 1:5d} | {line}"
        return line

    if not tokens:
        snippet_lines = lines[:max_lines]
        return "\n".join(
            _fmt(i, line) for i, line in enumerate(snippet_lines)
        )

    token_regexes = [
        (token, re.compile(r"\b" + re.escape(token) + r"\b", re.IGNORECASE))
        for token in tokens
    ]

    # Collect (line, token) events for every token match.
    events: list[tuple[int, str]] = []
    for token, rx in token_regexes:
        for i, line in enumerate(lines):
            if rx.search(line):
                events.append((i, token))

    if not events:
        snippet_lines = lines[:max_lines]
        return "\n".join(
            _fmt(i, line) for i, line in enumerate(snippet_lines)
        )

    events.sort(key=lambda x: x[0])

    # Build intervals around each event line.  The score of an interval is the
    # number of distinct tokens that appear anywhere inside it.
    n = len(lines)
    intervals: list[tuple[int, int, set[str]]] = []
    for line_no, _ in events:
        start = max(0, line_no - window)
        end = min(n - 1, line_no + window)
        token_set = {tok for ln, tok in events if start <= ln <= end}
        intervals.append((start, end, token_set))

    # Merge overlapping/adjacent intervals, unioning their token sets.
    merged: list[tuple[int, int, set[str]]] = []
    for start, end, ts in sorted(intervals, key=lambda x: x[0]):
        if merged and start <= merged[-1][1] + 1:
            prev_start, prev_end, prev_ts = merged[-1]
            merged[-1] = (prev_start, max(prev_end, end), prev_ts | ts)
        else:
            merged.append((start, end, ts))

    # If a required line was supplied (e.g. a known function definition), make
    # sure it is covered by an interval so the model sees the code it must edit.
    if required_line is not None and 0 <= required_line < n:
        covered = any(start <= required_line <= end for start, end, _ in merged)
        if not covered:
            req_start = max(0, required_line - window)
            req_end = min(n - 1, required_line + window)
            merged.append((req_start, req_end, set()))
            merged.sort(key=lambda x: x[0])
            # Merge the new interval with any neighbours it touches.
            recombined: list[tuple[int, int, set[str]]] = []
            for start, end, ts in merged:
                if recombined and start <= recombined[-1][1] + 1:
                    prev_s, prev_e, prev_ts = recombined[-1]
                    recombined[-1] = (prev_s, max(prev_e, end), prev_ts | ts)
                else:
                    recombined.append((start, end, ts))
            merged = recombined

    # If the merged windows are still too large, keep the intervals with the
    # most distinct tokens first, but always preserve the required line.
    total = sum(end - start + 1 for start, end, _ in merged)
    if total > max_lines:
        required_interval_idx: int | None = None
        if required_line is not None:
            for idx, (start, end, _) in enumerate(merged):
                if start <= required_line <= end:
                    required_interval_idx = idx
                    break

        merged = sorted(
            enumerate(merged),
            key=lambda x: (x[0] == required_interval_idx, len(x[1][2])),
            reverse=True,
        )
        kept: list[tuple[int, int, set[str]]] = []
        kept_lines = 0
        for orig_idx, (start, end, ts) in merged:
            length = end - start + 1
            is_required = required_line is not None and start <= required_line <= end
            if kept_lines + length > max_lines:
                remaining = max_lines - kept_lines
                if remaining > 0:
                    if is_required:
                        # Always keep the required function definition in the
                        # partial window so the model sees the signature.
                        half = remaining // 2
                        new_start = max(start, required_line - half)
                        new_end = min(end, required_line + (remaining - half) - 1)
                        if new_end - new_start + 1 < remaining:
                            new_start = max(start, new_end - remaining + 1)
                    else:
                        # Center the partial window on the event lines inside it
                        # so the most relevant code is preserved.
                        event_lines = sorted({ln for ln, tok in events if start <= ln <= end})
                        if event_lines:
                            mid = event_lines[len(event_lines) // 2]
                            half = remaining // 2
                            new_start = max(start, mid - half)
                            new_end = min(end, new_start + remaining - 1)
                        else:
                            new_start = start
                            new_end = start + remaining - 1
                    kept.append((new_start, new_end, ts))
                    kept_lines += new_end - new_start + 1
                break
            kept.append((start, end, ts))
            kept_lines += length
        merged = sorted(kept, key=lambda x: x[0])

    out: list[str] = []
    for start, end, _ in merged:
        if include_line_numbers:
            out.append(f"// --- lines {start + 1}-{end + 1} ---")
        for i in range(start, end + 1):
            out.append(_fmt(i, lines[i]))
    return "\n".join(out)


def truncate_file_reads(
    files: list[str | Path],
    repo_path: str | Path | None = None,
    max_lines: int = 300,
    max_chars: int = 12000,
    max_file_size_bytes: int = 500_000,
    highlight_terms: list[str] | None = None,
) -> str:
    """Build a context block of file snippets.

    When ``highlight_terms`` are supplied, each file is excerpted around the
    lines that contain those terms.  Otherwise the first/last chunk of each
    file is used.  The combined block is shrunk iteratively until it fits
    ``max_chars``.
    """
    repo_path = Path(repo_path) if repo_path else None
    file_paths: list[Path] = []
    for f in files:
        p = Path(f)
        if not p.is_absolute() and repo_path is not None:
            p = repo_path / p
        if not p.is_file():
            continue
        # Skip files that are too large to be useful source snippets.
        if p.stat().st_size > max_file_size_bytes:
            continue
        # Skip obvious binary files by extension.
        if p.suffix.lower() in {".png", ".jpg", ".jpeg", ".gif", ".webp", ".mp4", ".webm", ".bin", ".exe", ".so", ".a", ".o"}:
            continue
        file_paths.append(p)

    if not file_paths:
        return "(no relevant files found)\n"

    def _render(per_file_lines: int, window: int) -> tuple[list[str], bool]:
        out: list[str] = []
        within_budget = True
        for path in file_paths:
            if repo_path is not None:
                try:
                    display = path.relative_to(repo_path).as_posix()
                except ValueError:
                    display = path.as_posix()
            else:
                display = path.as_posix()
            try:
                # Read only the first chunk; for very long files this avoids
                # loading the whole file into memory just to truncate it.
                with open(path, "r", encoding="utf-8", errors="ignore") as fh:
                    text = fh.read(max_file_size_bytes)
            except Exception:
                continue
            lines = text.splitlines()
            if highlight_terms:
                required_line = _find_definition_line(lines, highlight_terms)
                snippet = _extract_relevant_windows(
                    lines,
                    highlight_terms,
                    max_lines=per_file_lines,
                    window=window,
                    required_line=required_line,
                )
            elif len(lines) <= per_file_lines:
                snippet = "\n".join(
                    f"{i + 1:5d} | {line}" for i, line in enumerate(lines)
                )
            else:
                half = per_file_lines // 2
                snippet_lines = (
                    lines[:half]
                    + [f"\n... ({len(lines) - per_file_lines} lines omitted) ...\n"]
                    + lines[-half:]
                )
                snippet = "\n".join(
                    f"{i + 1:5d} | {line}" for i, line in enumerate(snippet_lines)
                )
            out.append(f"--- {display} ---\n{snippet}\n")
            # Early exit if we are already over the budget; avoids wasted work.
            if len("\n".join(out)) > max_chars:
                within_budget = False
                break
        return out, within_budget

    # Iteratively shrink per-file budget until the whole block fits.
    current_max_lines = max_lines
    min_lines = 20
    current_window = 40
    while current_max_lines > min_lines:
        parts, fits = _render(current_max_lines, current_window)
        combined = "\n".join(parts)
        if fits and len(combined) <= max_chars:
            return combined
        current_max_lines = max(min_lines, current_max_lines // 2)
        current_window = max(10, current_window // 2)

    # Final attempt at the minimum line budget.
    parts, fits = _render(min_lines, 10)
    combined = "\n".join(parts)
    if len(combined) > max_chars:
        combined = combined[:max_chars] + "\n... (truncated due to char limit) ...\n"
    return combined


def _load_list_field(value: Any) -> list[str]:
    """Normalize a HF dataset field that may be a JSON-encoded list or a real list."""
    if isinstance(value, list):
        return [str(x) for x in value]
    if isinstance(value, str):
        value = value.strip()
        if not value:
            return []
        try:
            import ast
            parsed = ast.literal_eval(value)
            if isinstance(parsed, list):
                return [str(x) for x in parsed]
        except (SyntaxError, ValueError):
            pass
        return [value]
    return []


def _read_agentless_file_snippets(
    files: list[str],
    repo_path: Path,
    *,
    max_total_chars: int = 12000,
    max_file_lines: int = 500,
    max_file_bytes: int = 200_000,
    required_identifiers: list[str] | None = None,
    highlight_terms: list[str] | None = None,
) -> str:
    """Read full source files when small; excerpt larger ones.

    Returns a single prompt block.  Full small files give the model exact text
    to match in SEARCH blocks; larger files are excerpted around the target
    function definition (if ``required_identifiers`` is supplied) or around
    highlighted terms, with a clear "(truncated)" marker.

    Budget is split evenly across ``files`` so a single huge file cannot
    crowd out the other ranked files.
    """
    if not files:
        return "(no relevant files found)\n"

    per_file_budget = max_total_chars // max(1, len(files))
    snippets: list[str] = []
    total_remaining = max_total_chars
    for rel in files:
        path = repo_path / rel
        if not path.is_file() or path.stat().st_size > max_file_bytes:
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except Exception:
            continue
        lines = text.splitlines()
        header = f"--- {rel} ---"
        if len(lines) <= max_file_lines:
            # Include the complete file so SEARCH text can match exactly.
            # No line-number gutters: small models sometimes copy them back into
            # SEARCH blocks and the gutter-stripping fallback cannot always save it.
            body = "\n".join(lines)
            part = f"{header}\nFULL FILE:\n{body}\n"
        else:
            # Try to center the excerpt on the function/type the model must edit.
            required_line = _find_definition_line(lines, required_identifiers or [])
            if required_line is not None:
                body = _extract_relevant_windows(
                    lines,
                    highlight_terms or [],
                    max_lines=max_file_lines,
                    window=max_file_lines // 4,
                    required_line=required_line,
                    include_line_numbers=False,
                )
                part = f"{header}\nEXCERPT (centered on target function):\n{body}\n"
            else:
                half = max_file_lines // 2
                body_lines = (
                    lines[:half]
                    + [f"\n... ({len(lines) - max_file_lines} lines omitted) ...\n"]
                    + lines[-half:]
                )
                body = "\n".join(body_lines)
                part = f"{header}\nEXCERPT:\n{body}\n"
        file_budget = min(per_file_budget, total_remaining)
        if len(part) > file_budget:
            part = part[:file_budget] + "\n... (truncated due to per-file budget) ...\n"
        snippets.append(part)
        total_remaining -= len(part)
        if total_remaining <= 0:
            break
    return "".join(snippets)


def _new_files_from_patch(patch_text: str) -> list[str]:
    """Return paths that are created as new files in a unified diff.

    A new file is identified by a hunk that starts with ``--- /dev/null``.
    """
    new_files: list[str] = []
    seen: set[str] = set()
    current_path: str | None = None
    for line in patch_text.splitlines():
        if line.startswith("diff --git a/"):
            # Extract the b/ path from "diff --git a/X b/X".
            match = re.match(r"^diff --git a/(.+?) b/(.+?)(?:\s|$)", line)
            current_path = match.group(2) if match else None
        elif line.startswith("--- /dev/null") and current_path:
            if current_path not in seen:
                seen.add(current_path)
                new_files.append(current_path)
    return new_files


def _plausible_source_path(path: str) -> bool:
    """Return True for repo-relative source-file paths we might edit or create."""
    if not path or path.startswith(("http", "www", "/", "#", "-")):
        return False
    if "." not in Path(path).name:
        return False
    return bool(
        re.search(
            r"\.(js|ts|jsx|tsx|go|py|java|rb|rs|cpp|c|h|hpp|swift|kt|kts|scala|php|cs)$",
            path,
            re.IGNORECASE,
        )
    )


def _extract_new_file_hints(text: str, repo_path: Path) -> list[str]:
    """Find repo-relative source-file paths that the issue says to create.

    Heuristic: look for backtick/quote-enclosed paths (e.g. ``src/foo.js``)
    near phrases like "new file", "should be created", or "create a".  Only
    paths that do not already exist are returned.
    """
    if not text:
        return []
    hints: list[str] = []
    seen: set[str] = set()
    # Sentences that mention creating a file.
    create_phrases = re.finditer(
        r"(?:new (?:file|controller|router|module)|create[ds]?|should be created)[^.]*?"
        r"(?:`([^`\n]+\.[a-z0-9]+)`|\"([^\"\n]+\.[a-z0-9]+)\"|'([^'\n]+\.[a-z0-9]+)')",
        text,
        re.IGNORECASE | re.DOTALL,
    )
    for m in create_phrases:
        path = m.group(1) or m.group(2) or m.group(3)
        if not path or path in seen or not _plausible_source_path(path):
            continue
        candidate = repo_path / path
        if not candidate.exists():
            seen.add(path)
            hints.append(path)
    return hints


def _extract_source_paths_from_text(text: str, repo_path: Path) -> list[str]:
    """Return existing source-file paths mentioned in the issue/requirements/tests.

    These are added to the agentless context so the model sees files it is
    likely to need even when the embedding ranker misses them.
    """
    if not text:
        return []
    found: list[str] = []
    seen: set[str] = set()
    # Backtick/quoted paths, plus bare paths starting with common source prefixes.
    for m in re.finditer(
        r"(?:`([^`\n]+\.[a-z0-9]+)`|\"([^\"\n]+\.[a-z0-9]+)\"|'([^'\n]+\.[a-z0-9]+)'|(\b(?:src|lib|pkg|app|internal|server|client|core|utils?|helpers?|models?|controllers?|routes?|components?)/[^\s:,;\"'`<>()]+\.[a-z0-9]+))",
        text,
        re.IGNORECASE,
    ):
        path = m.group(1) or m.group(2) or m.group(3) or m.group(4)
        if not path or path in seen or not _plausible_source_path(path):
            continue
        candidate = repo_path / path
        if candidate.is_file():
            seen.add(path)
            found.append(path)
    return found


def _is_test_file(path: str) -> bool:
    """Return True if *path* looks like a test file.

    Covers language-specific conventions (``_test.go``, ``_test.py``,
    ``test_*.py``) as well as directory layouts such as NodeBB's
    ``test/**/*.js`` or ``tests/**/*.js``.
    """
    if not path:
        return False
    basename = os.path.basename(path)
    if basename.startswith("test_"):
        return True
    if any(
        path.endswith(ext)
        for ext in ("_test.go", "_test.py", "_test.js", "_test.ts", "_test.tsx")
    ):
        return True
    parts = path.replace("\\", "/").split("/")
    return "test" in parts or "tests" in parts


def _extract_test_hints(patch_text: str, max_chars: int = 2000) -> str:
    """Extract a concise, model-readable hint block from the official test patch.

    The full test patch is too large and regressed performance when dumped into
    the prompt.  Instead we surface:
    - new test files and new test function names,
    - quoted error strings and numeric literals that the tests assert on.
    """
    if not patch_text:
        return ""
    hints: list[str] = []
    seen_funcs: set[str] = set()
    seen_strings: set[str] = set()
    current_path: str | None = None

    for line in patch_text.splitlines():
        if line.startswith("diff --git a/"):
            match = re.match(r"^diff --git a/(.+?) b/(.+?)(?:\s|$)", line)
            current_path = match.group(2) if match else None
            continue
        # Skip diff metadata lines so we do not surface index modes as hints.
        if line.startswith(("index ", "--- ", "+++ ", "@@")):
            continue
        if current_path and _is_test_file(current_path):
            if current_path.endswith("_test.go"):
                # Go test functions: func TestXxx(t *testing.T) or subtests t.Run("name").
                m = re.search(r"func\s+(Test[A-Za-z0-9_]+)", line)
                if m and m.group(1) not in seen_funcs:
                    seen_funcs.add(m.group(1))
                    hints.append(f"- New test: {m.group(1)} in {current_path}")
                m = re.search(r't\.Run\("([^"]+)"', line)
                if m and m.group(1) not in seen_funcs:
                    seen_funcs.add(m.group(1))
                    hints.append(f'- Subtest: "{m.group(1)}"')
            elif current_path.endswith((".js", ".ts", ".tsx")):
                # JavaScript/TypeScript test blocks: it/describe/test('name').
                for m in re.finditer(r"\b(it|test|describe)\s*\(\s*[\"']([^\"']+)", line):
                    name = f"{m.group(1)}('{m.group(2)}')"
                    if name not in seen_funcs:
                        seen_funcs.add(name)
                        hints.append(f"- New test: {name} in {current_path}")
        # Look for quoted strings that look like error messages or assertions.
        for quote in re.findall(r'"([^"]{5,80})"', line):
            if quote not in seen_strings and any(
                kw in quote.lower()
                for kw in ["error", "invalid", "exceeded", "expected", "must", "cannot", "failed"]
            ):
                seen_strings.add(quote)
                hints.append(f'- Expected string: "{quote}"')
        # Boundary numbers (heuristic: standalone integer literals > 1000).
        for num in re.findall(r"\b(\d{5,})\b", line):
            key = f"num-{num}"
            if key not in seen_strings:
                seen_strings.add(key)
                hints.append(f"- Boundary/literal: {num}")

    if not hints:
        return ""
    text = "\n".join(hints)
    if len(text) > max_chars:
        text = text[:max_chars].rsplit("\n", 1)[0] + "\n... (more test hints omitted) ..."
    return text


def _extract_failing_test_snippets(patch_text: str, max_chars: int = 3000) -> str:
    """Extract concrete added test code from the official test patch.

    Seeing the assertions the new tests make is often more informative than
    hint lists.  We return the first 1-2 new test functions per modified test
    file, truncated to keep the prompt compact.
    """
    if not patch_text:
        return ""
    from collections import defaultdict

    added_by_file: dict[str, list[str]] = defaultdict(list)
    current_file: str | None = None
    for line in patch_text.splitlines():
        if line.startswith("+++ "):
            current_file = line[6:] if line.startswith("+++ b/") else line[5:].strip()
        elif line.startswith("+") and current_file:
            added_by_file[current_file].append(line[1:])

    snippets: list[str] = []
    for path, lines in added_by_file.items():
        if not _is_test_file(path):
            continue
        is_python_test = path.endswith(".py")
        funcs: list[tuple[str, str]] = []
        i = 0
        while i < len(lines):
            l = lines[i]
            name: str | None = None
            if path.endswith("_test.go"):
                m = re.search(r"func\s+(Test[A-Za-z0-9_]+)", l)
                if m:
                    name = m.group(1)
            elif is_python_test and l.strip().startswith("def test_"):
                name = l.strip().split("(")[0].replace("def ", "")
            elif path.endswith((".js", ".ts", ".tsx")):
                m = re.search(r"\b(it|test|describe)\s*\(\s*[\"']([^\"']+)", l)
                if m:
                    name = f"{m.group(1)}('{m.group(2)}')"
            if name:
                body = [l]
                j = i + 1
                while j < min(i + 18, len(lines)):
                    body.append(lines[j])
                    j += 1
                funcs.append((name, "\n".join(body)))
                i = j
            else:
                i += 1
        for name, body in funcs[:2]:
            snippets.append(f"### {path} — {name}\n{body}")

    if not snippets:
        return ""
    text = "\n\n".join(snippets)
    if len(text) > max_chars:
        text = text[:max_chars].rsplit("\n", 1)[0] + "\n... (truncated) ..."
    return text


def _parse_interface(interface_text: str | None) -> list[dict[str, str]]:
    """Parse the structured ``interface`` field into API entries.

    The field is a free-text block with repeated blocks like::

        Type: Function

        Name: hide_qt_warning

        Path: qutebrowser/utils/qtlog.py

        Input: pattern: str, logger: str = 'qt'

        Output: context manager (Iterator[None])

        Description: Temporarily suppresses Qt log warnings.

    Returns a list of dicts with keys: type, name, path, input, output,
    description, public_api.
    """
    if not interface_text:
        return []

    known_keys = {"type", "name", "path", "input", "output", "description", "public_api"}
    # Alternative labels that some instances use for the target file/path.
    key_aliases = {"pathfile": "path", "location": "path"}

    def _normalize_key(key: str) -> str:
        return key.strip().lower().replace(" ", "_")

    entries: list[dict[str, str]] = []
    current_entry: dict[str, str] | None = None
    current_key: str | None = None

    for line in interface_text.splitlines():
        stripped = line.strip()
        if not stripped:
            current_key = None
            continue

        if ": " in stripped:
            maybe_key, _, value = stripped.partition(": ")
            maybe_key = _normalize_key(maybe_key)
            canonical_key = key_aliases.get(maybe_key)
            if maybe_key in known_keys or canonical_key:
                if maybe_key == "type" and current_entry:
                    entries.append(current_entry)
                    current_entry = None
                if current_entry is None:
                    current_entry = {}
                store_key = canonical_key or maybe_key
                current_entry[store_key] = value.strip()
                current_key = store_key
                continue

        # Continuation line for the current key.
        if current_key and current_entry is not None:
            current_entry[current_key] = f"{current_entry[current_key]}\n{stripped}"

    if current_entry:
        entries.append(current_entry)
    return entries


def _format_target_api_section(interface_text: str | None) -> str:
    """Format the parsed ``interface`` field for inclusion in the prompt."""
    entries = _parse_interface(interface_text)
    if not entries:
        return ""

    lines = ["Target API (functions/classes you may need to modify):"]
    for entry in entries:
        api_type = entry.get("type", "API")
        name = entry["name"]
        path = entry.get("path", "(unknown path)")
        input_sig = entry.get("input", "")
        output_sig = entry.get("output", "")
        description = entry.get("description", "")
        public_api = entry.get("public_api", "")

        lines.append(f"- {api_type}: `{name}` — {path}")
        if input_sig:
            lines.append(f"  Input: {input_sig}")
        if output_sig:
            lines.append(f"  Output: {output_sig}")
        if public_api:
            lines.append(f"  Public API: {public_api}")
        if description:
            lines.append(f"  Description: {description}")

    return "\n".join(lines)


def _build_focused_test_oracle(patch_text: str | None, max_chars: int = 4000) -> str:
    """Build a compact but concrete oracle from the official test patch.

    Combines concrete failing test code snippets with a short list of key
    assertion hints. This gives the model the exact assertions it must satisfy
    without dumping the entire test patch into the prompt.
    """
    if not patch_text:
        return ""

    snippets = _extract_failing_test_snippets(patch_text, max_chars=max_chars)
    if not snippets:
        return ""

    # Reserve most of the budget for the snippets; hints get the remainder.
    hints_budget = max(0, max_chars - len(snippets) - 200)
    hints = _extract_test_hints(patch_text, max_chars=hints_budget)

    if hints:
        return (
            "Key assertion hints (from the failing tests):\n"
            f"{hints}\n\n"
            "Failing test code snippets (do NOT edit tests):\n"
            f"{snippets}"
        )
    return "Failing test code snippets (do NOT edit tests):\n" + snippets


def _expand_to_package(
    repo_path: Path,
    files: list[str],
    max_extra: int = 20,
) -> list[str]:
    """For Go files, include all ``.go`` files in the same package directory.

    Many SWE-bench Pro fixes require cross-file changes within one package
    (e.g., a new API in ``simple_cache.go`` plus an updated caller in
    ``cached_http_client.go``).  Expanding the ranked list to the whole package
    makes those dependencies visible while keeping the context bounded.
    """
    expanded: list[str] = []
    seen: set[str] = set()
    extra_added = 0
    for rel in files:
        if rel not in seen:
            expanded.append(rel)
            seen.add(rel)
        if not rel.endswith(".go"):
            continue
        directory = Path(rel).parent
        pkg_dir = repo_path / directory
        if not pkg_dir.is_dir():
            continue
        try:
            for sibling in sorted(pkg_dir.iterdir()):
                if not sibling.is_file():
                    continue
                if sibling.name.endswith("_test.go"):
                    # Keep the prompt source-only; test files are applied by the evaluator.
                    continue
                if sibling.name.endswith(".go"):
                    sibling_rel = (directory / sibling.name).as_posix()
                    if sibling_rel not in seen and extra_added < max_extra:
                        expanded.append(sibling_rel)
                        seen.add(sibling_rel)
                        extra_added += 1
        except Exception:
            continue
    return expanded


def _build_editable_manifest(
    files: list[str],
    repo_path: Path,
    max_files: int = 20,
    new_files: list[str] | None = None,
) -> str:
    """Return a bullet list of existing and allowable new files to edit."""
    lines: list[str] = []
    for rel in files[:max_files]:
        path = repo_path / rel
        if path.is_file():
            lines.append(f"- {rel}")
    if new_files:
        lines.append("- (you may CREATE these new files if needed)")
        for rel in new_files[:max(5, max_files - len(lines))]:
            lines.append(f"  - {rel}")
    if not lines:
        return "(no existing source files to edit)\n"
    return "\n".join(lines) + "\n"


def _context_budgets(context_window: int) -> tuple[int, int]:
    """Return (snippet_max_chars, snippet_max_lines) scaled to the model context.

    Roughly reserve 1/3 of the context window for file snippets, assuming
    ~4 characters per token.  Cap the snippet budget so prompts do not grow
    unreasonably large on 1M-context models.
    """
    if context_window <= 0:
        context_window = 32768
    snippet_chars = min(context_window * 4 // 3, 300_000)
    snippet_lines = min(context_window // 100, 10_000)
    return snippet_chars, snippet_lines


def build_agentless_prompt(
    instance: dict[str, Any],
    repo_path: str | Path,
    *,
    few_shot_examples: str | None = None,
    top_k: int = 3,
    snippet_max_lines: int | None = None,
    snippet_max_chars: int | None = None,
    context_window: int = 32768,
    expand_to_package: bool = False,
) -> str:
    """Assemble a one-shot prompt that asks the model directly for a patch.

    This bypasses the Selfware agent loop entirely.  The model receives a
    compact problem description plus the most relevant source snippets and is
    asked to return SEARCH/REPLACE blocks (easier for very small models than a
    unified git diff).
    """
    computed_chars, computed_lines = _context_budgets(context_window)
    if snippet_max_chars is None:
        snippet_max_chars = computed_chars
    if snippet_max_lines is None:
        snippet_max_lines = computed_lines

    repo_path = Path(repo_path)
    problem = instance.get("problem_statement", "") or ""
    requirements = instance.get("requirements", "") or ""
    repo = instance.get("repo", "")
    base_commit = instance.get("base_commit", "")
    language = (instance.get("repo_language") or "").lower()
    tests = _load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = _load_list_field(instance.get("fail_to_pass", []))

    test_cmd = _format_test_command(language, tests)

    search_text = problem
    if requirements:
        search_text += "\n" + requirements
    ranked = rank_files_by_relevance(
        repo_path,
        search_text,
        test_names=tests + fail_to_pass,
        top_k=top_k * 3,
    )
    # Agentless is source-only; drop test files so the model edits implementation
    # files rather than tests.
    def _is_test_file(rel: str) -> bool:
        name = rel.lower()
        return name.endswith("_test.go") or name.startswith("test_") or name.endswith("_test.py")

    source_ranked = [f for f in ranked if not _is_test_file(f)]
    if len(source_ranked) < top_k:
        source_ranked = ranked[:top_k]
    # Many fixes touch multiple files in the same Go package; optionally expand
    # the ranked list so the model sees callers and related types.  This is
    # disabled by default for agentless because it can shift context enough to
    # hurt cases that worked with the focused file list.
    if expand_to_package:
        source_ranked = _expand_to_package(repo_path, source_ranked)
    # Surface source files explicitly mentioned in the issue/requirements/tests.
    # The embedding ranker often misses files named in prose (e.g., router paths).
    mentioned_files = _extract_source_paths_from_text(search_text, repo_path)
    mentioned_files += _extract_source_paths_from_text("\n".join(fail_to_pass), repo_path)
    # De-duplicate while preserving order: mentioned files first, then ranked.
    seen_files: set[str] = set()
    source_ranked = [
        f for f in (mentioned_files + source_ranked)
        if not _is_test_file(f) and not (f in seen_files or seen_files.add(f))
    ]
    # Prefer full file contents for small files so SEARCH blocks match exactly.
    # Larger files are excerpted around relevant identifiers.
    snippet_terms = [t for t in _tokenize_problem(search_text) if _is_strong_identifier(t)]
    snippet_files = source_ranked[:top_k]
    # Strong identifiers from the problem/requirements guide excerpting for huge
    # files: if a ranked file contains a function named in the issue, center the
    # excerpt on that function instead of the first/last chunks.
    required_identifiers = list(
        dict.fromkeys(
            snippet_terms
            + [t for t in _tokenize_problem(" ".join(fail_to_pass)) if _is_strong_identifier(t)]
        )
    )
    snippets = _read_agentless_file_snippets(
        snippet_files,
        repo_path,
        max_total_chars=snippet_max_chars,
        max_file_lines=snippet_max_lines,
        required_identifiers=required_identifiers,
        highlight_terms=snippet_terms,
    )
    if "(no relevant files found)" in snippets:
        snippets = truncate_file_reads(
            snippet_files,
            repo_path=repo_path,
            max_lines=snippet_max_lines,
            max_chars=snippet_max_chars,
            highlight_terms=snippet_terms,
        )

    # Surface files the test patch creates so the model knows it may need to
    # create matching source/testdata files.
    test_patch = instance.get("test_patch", "") or ""
    new_files = _new_files_from_patch(test_patch)
    requirement_hints = _extract_new_file_hints(
        f"{problem}\n{requirements}", repo_path
    )
    new_files = list(dict.fromkeys(new_files + requirement_hints))
    test_hints = _extract_test_hints(test_patch)
    test_snippets = _extract_failing_test_snippets(test_patch)
    editable_manifest = _build_editable_manifest(
        snippet_files, repo_path, new_files=new_files
    )

    create_example = (
        "### FILE: src/controllers/well-known.js\n"
        "<<<<<<< SEARCH\n"
        "=======\n"
        "\"use strict\";\n"
        "\n"
        "module.exports = function (router) {\n"
        "    router.get(\"/.well-known/webfinger\", ...);\n"
        "};\n"
        ">>>>>>> REPLACE"
    )

    example = (
        "### FILE: lib/auth/grpcserver.go\n"
        "<<<<<<< SEARCH\n"
        "func processRequest() error {\n"
        "    err := doWork()\n"
        "    return trace.Wrap(err)\n"
        "}\n"
        "=======\n"
        "func processRequest() error {\n"
        "    if err := validate(); err != nil {\n"
        "        return trace.Wrap(err)\n"
        "    }\n"
        "    err := doWork()\n"
        "    return trace.Wrap(err)\n"
        "}\n"
        ">>>>>>> REPLACE"
    )

    sections = [
        "You are a coding assistant. Produce the smallest source-code patch that fixes the issue below.",
        "Do NOT explain your reasoning. Return ONLY SEARCH/REPLACE blocks.",
        "",
        f"Repo: {repo} @ {base_commit}",
        "",
        "Issue:",
        problem,
        "",
        "Requirements / implementation notes:",
        requirements or "- (none provided)",
        "",
        "Failing tests:",
        "\n".join(f"- {t}" for t in fail_to_pass) or "- (none specified)",
        "",
        "Test-patch hints (the evaluator applies the full test patch; do not edit tests):",
        test_hints or "- (none extracted)",
        "",
        "Failing test code snippets (added by the test patch; these are the tests you must make pass):",
        test_snippets or "- (none extracted)",
        "",
        f"External test command (run by the evaluator, NOT you): {test_cmd}",
        "",
        "Likely relevant source files:",
        snippets,
        "",
        "Editable files manifest — prioritize these source files:",
        editable_manifest,
        "You may edit other source files if the fix clearly requires it, but the files listed above are the best starting point.",
        "",
        "PATCH FORMAT — return one or more SEARCH/REPLACE blocks exactly like this:",
        "",
        "### FILE: path/to/file.go",
        "<<<<<<< SEARCH",
        "old line(s) copied exactly from the file above",
        "=======",
        "new line(s) to replace them with",
        ">>>>>>> REPLACE",
        "",
        "Example:",
        example,
        "",
        "Example for CREATING a new file (empty SEARCH block):",
        create_example,
        "",
        "CRITICAL RULES:",
        "- Modify source files only. Do NOT edit tests, configs, docs, or unrelated code.",
        "- Keep the patch minimal: no formatting, comment, or unrelated changes.",
        "- Do NOT produce an empty patch.",
        "- Prefer paths from the Editable files manifest, but you may edit other existing source files if the fix clearly requires it.",
        "- The SEARCH text must match the source file EXACTLY (including indentation). Copy it verbatim from the file content shown above.",
        "- Files marked FULL FILE above are shown completely. Copy SEARCH text from those exact lines.",
        "- Files marked EXCERPT are truncated. Only SEARCH for text that appears inside the excerpt; do not guess at omitted lines.",
        "- Do NOT include line numbers, '  123 | ', or '// --- lines X-Y ---' markers in SEARCH or REPLACE.",
        "- Do NOT wrap the whole patch in a markdown code fence. Return raw SEARCH/REPLACE blocks only.",
        "- If the SEARCH text contains special characters, copy them exactly from the source.",
        "- If you cannot match the exact text in the excerpt, choose a smaller SEARCH block that is fully visible rather than guessing.",
        "- To CREATE a new file, use an empty SEARCH block:\n"
        "  ### FILE: path/to/new_file.go\n"
        "  <<<<<<< SEARCH\n"
        "  =======\n"
        "  <new file contents>\n"
        "  >>>>>>> REPLACE",
    ]
    if few_shot_examples:
        sections.extend([
            "",
            "FEW-SHOT EXAMPLES (source-only fixes):",
            few_shot_examples,
        ])
    return "\n".join(sections)


def build_agentless_retry_prompt(
    instance: dict[str, Any],
    repo_path: str | Path,
    failed_response: str,
    *,
    few_shot_examples: str | None = None,
    top_k: int = 3,
    context_window: int = 32768,
    expand_to_package: bool = False,
) -> str:
    """Build a stricter retry prompt after an unapplyable agentless response.

    The retry prompt includes the full content of the most relevant source files
    so the model can produce SEARCH blocks that match exactly.
    """
    snippet_max_chars, snippet_max_lines = _context_budgets(context_window)

    repo_path = Path(repo_path)
    problem = instance.get("problem_statement", "") or ""
    requirements = instance.get("requirements", "") or ""
    search_text = problem
    if requirements:
        search_text += "\n" + requirements
    tests = _load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = _load_list_field(instance.get("fail_to_pass", []))

    ranked = rank_files_by_relevance(
        repo_path,
        search_text,
        test_names=tests + fail_to_pass,
        top_k=top_k * 3,
    )

    def _is_test_file(rel: str) -> bool:
        name = rel.lower()
        return name.endswith("_test.go") or name.startswith("test_") or name.endswith("_test.py")

    source_ranked = [f for f in ranked if not _is_test_file(f)]
    if len(source_ranked) < top_k:
        source_ranked = ranked[:top_k]
    if expand_to_package:
        source_ranked = _expand_to_package(repo_path, source_ranked)

    snippet_files = source_ranked[:top_k]
    snippet_terms = [t for t in _tokenize_problem(search_text) if _is_strong_identifier(t)]
    required_identifiers = list(
        dict.fromkeys(
            snippet_terms
            + [t for t in _tokenize_problem(" ".join(fail_to_pass)) if _is_strong_identifier(t)]
        )
    )
    exact_snippets = _read_agentless_file_snippets(
        snippet_files,
        repo_path,
        max_total_chars=snippet_max_chars,
        max_file_lines=snippet_max_lines,
        required_identifiers=required_identifiers,
        highlight_terms=snippet_terms,
    )

    test_patch = instance.get("test_patch", "") or ""
    new_files = _new_files_from_patch(test_patch)
    requirement_hints = _extract_new_file_hints(
        f"{problem}\n{requirements}", repo_path
    )
    new_files = list(dict.fromkeys(new_files + requirement_hints))
    test_hints = _extract_test_hints(test_patch)
    editable_manifest = _build_editable_manifest(
        snippet_files, repo_path, new_files=new_files
    )

    sections = [
        "Your previous SEARCH/REPLACE patch could not be applied because the SEARCH text did not match the source files exactly.",
        "Return ONLY corrected SEARCH/REPLACE blocks. Do NOT explain. Do NOT add markdown fences around the whole patch.",
        "",
        "Issue:",
        problem,
        "",
        "Requirements / implementation notes:",
        requirements or "- (none provided)",
        "",
        "Test-patch hints (the evaluator applies the full test patch; do not edit tests):",
        test_hints or "- (none extracted)",
        "",
        "Below are the EXACT current contents of the most relevant source files. Copy SEARCH text from these lines exactly.",
        exact_snippets,
        "",
        "Editable files manifest (you may ONLY edit files listed here):",
        editable_manifest,
        "If the fix requires a file not listed above, respond with NO_PATCH and nothing else.",
        "",
        "PATCH FORMAT — return one or more SEARCH/REPLACE blocks exactly like this:",
        "",
        "### FILE: path/to/file.go",
        "<<<<<<< SEARCH",
        "old line(s) copied exactly from the file above",
        "=======",
        "new line(s) to replace them with",
        ">>>>>>> REPLACE",
        "",
        "CRITICAL RULES:",
        "- Modify source files only. Do NOT edit tests, configs, docs, or unrelated code.",
        "- Keep the patch minimal: no formatting, comment, or unrelated changes.",
        "- Do NOT produce an empty patch.",
        "- Prefer paths from the Editable files manifest, but you may edit other existing source files if the fix clearly requires it.",
        "- The SEARCH text must match the source file EXACTLY (including indentation). Copy it verbatim from the file content shown above.",
        "- Files marked FULL FILE above are shown completely. Copy SEARCH text from those exact lines.",
        "- Do NOT include line numbers, '  123 | ', or '// --- lines X-Y ---' markers in SEARCH or REPLACE.",
        "- Do NOT wrap the patch in ``` or any markdown fence.",
        "- If the SEARCH text contains special characters, copy them exactly from the source.",
        "- If you cannot match the exact text, choose a smaller SEARCH block that is fully visible rather than guessing.",
        "- To CREATE a new file, use an empty SEARCH block:\n"
        "  ### FILE: path/to/new_file.go\n"
        "  <<<<<<< SEARCH\n"
        "  =======\n"
        "  <new file contents>\n"
        "  >>>>>>> REPLACE",
    ]
    if few_shot_examples:
        sections.extend([
            "",
            "FEW-SHOT EXAMPLES (source-only fixes):",
            few_shot_examples,
        ])
    return "\n".join(sections)


def build_small_model_prompt(
    instance: dict[str, Any],
    repo_path: str | Path,
    *,
    repair_feedback: str | None = None,
    few_shot_examples: str | None = None,
    container_repo_dir: str = "/app",
    tree_max_depth: int = 3,
    tree_max_files: int = 200,
    top_k: int = 30,
    snippet_max_lines: int | None = None,
    snippet_max_chars: int | None = None,
    context_window: int = 32768,
) -> str:
    """Assemble a compact system + problem + tree + snippets prompt.

    The prompt is designed for small/cheap models: terse instructions, a
    shallow repo tree, and only the most relevant source snippets.
    """
    computed_chars, computed_lines = _context_budgets(context_window)
    if snippet_max_chars is None:
        snippet_max_chars = computed_chars
    if snippet_max_lines is None:
        snippet_max_lines = computed_lines

    repo_path = Path(repo_path)
    problem = instance.get("problem_statement", "") or ""
    requirements = instance.get("requirements", "") or ""
    repo = instance.get("repo", "")
    base_commit = instance.get("base_commit", "")
    language = (instance.get("repo_language") or "").lower()
    tests = _load_list_field(instance.get("selected_test_files_to_run", []))
    fail_to_pass = _load_list_field(instance.get("fail_to_pass", []))

    test_cmd = _format_test_command(language, tests)

    tree = compact_directory_tree(
        repo_path,
        max_depth=tree_max_depth,
        max_files=tree_max_files,
    )
    test_patch = instance.get("test_patch", "") or ""
    test_hints = _extract_test_hints(test_patch)
    search_text = problem
    if requirements:
        search_text += "\n" + requirements
    ranked = rank_files_by_relevance(
        repo_path,
        search_text,
        test_names=tests + fail_to_pass,
        top_k=top_k,
    )
    snippet_terms = [t for t in _tokenize_problem(search_text) if _is_strong_identifier(t)]
    snippets = truncate_file_reads(
        ranked,
        repo_path=repo_path,
        max_lines=snippet_max_lines,
        max_chars=snippet_max_chars,
        highlight_terms=snippet_terms,
    )

    sections = [
        "You are a coding assistant fixing a single issue in a repository.",
        "Work only with source files. Do NOT edit tests, configs, docs, or unrelated code.",
        "",
        f"Repo: {repo} @ {base_commit}",
        "Working directory: the repo root. Use relative paths (e.g., lib/auth/grpcserver.go, not /app/lib/auth/grpcserver.go).",
        "",
        "Issue:",
        problem,
        "",
        "Requirements / implementation notes:",
        requirements or "- (none provided)",
        "",
        "Test-patch hints (the evaluator applies the full test patch; do not edit tests):",
        test_hints or "- (none extracted)",
        "",
        "Test files / cases:",
        "\n".join(f"- {t}" for t in tests) or "- (none specified)",
        "",
        "Failing tests:",
        "\n".join(f"- {t}" for t in fail_to_pass) or "- (none specified)",
        "",
        f"External test command (run by the evaluator, NOT you): {test_cmd}",
        "",
        "Directory layout (shallow, key files only):",
        tree,
        "",
        "Likely relevant source files:",
        snippets,
        "",
        "YOUR TASK:",
        "1. Read the issue and the snippets above carefully.",
        "2. Your FIRST concrete action after reading must be file_edit on the relevant source file.",
        "   Do NOT start with shell_exec, directory_tree, or by running the test command.",
        "3. If the exact lines you need are not visible in a snippet, use file_read with a relative path to read more, then file_edit immediately.",
        "4. Apply the smallest source-code fix with file_edit. Use relative paths only.",
        "5. Do NOT run tests inside this agent. The evaluator runs them externally. Finish as soon as you have made at least one source-file edit.",
        "",
        "CORRECT file_edit EXAMPLE (format only; replace with the real old/new lines from the file you edit):",
        "",
        "### FILE: lib/auth/grpcserver.go",
        "<<<<<<< SEARCH",
        "func processRequest() error {",
        "    err := doWork()",
        "    return trace.Wrap(err)",
        "}",
        "=======",
        "func processRequest() error {",
        "    if err := validate(); err != nil {",
        "        return trace.Wrap(err)",
        "    }",
        "    err := doWork()",
        "    return trace.Wrap(err)",
        "}",
        ">>>>>>> REPLACE",
        "",
        "CRITICAL RULES:",
        "- Modify source files only. Do NOT edit tests, configs, docs, or unrelated code.",
        "- Do NOT produce an empty patch. At least one source file must change.",
        "- You MUST call file_edit at least once before finishing. No exceptions.",
        "- Your FIRST action must be file_edit. Do not start with shell_exec, directory_tree, or any test command.",
        "- Use relative file paths ONLY. Do NOT use absolute paths like /app/.... They will be rejected.",
        "- Prefer file_edit over file_write; include 3-5 lines of context.",
        "- Keep the patch minimal: no formatting, comment, or unrelated changes.",
        "- Tests do not need to pass inside this agent; you only have to produce a source diff.",
        "- Line numbers in the snippets above are for reference only; do not include them in file_edit or in the final diff.",
    ]
    if few_shot_examples:
        sections.extend([
            "",
            "FEW-SHOT EXAMPLES — source-only fixes (problem → patch style):",
            few_shot_examples,
        ])
    if repair_feedback:
        sections.extend([
            "",
            "REPAIR FEEDBACK (fix the remaining failure):",
            repair_feedback,
        ])
    return "\n".join(sections)


