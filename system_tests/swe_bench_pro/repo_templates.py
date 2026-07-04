"""Per-repo prompt templates for the SWE-bench Pro harness.

Templates live in ``repo_templates/`` and are keyed by the sanitized repo
identifier (``owner/name`` becomes ``owner__name.md``).  Unknown repos return an
empty string so callers can append unconditionally.
"""

from pathlib import Path

TEMPLATE_DIR = Path(__file__).resolve().parent / "repo_templates"


def _sanitize_repo_name(repo: str) -> str:
    """Convert a repo identifier into a safe filename stem.

    Handles both ``owner/name`` and ``owner__name`` forms and rejects path
    traversal so loaded templates can only come from ``repo_templates/``.
    """
    repo = repo.strip().replace("\\", "/")
    parts = [p for p in repo.split("/") if p and p not in (".", "..")]
    return "__".join(parts)


def load_repo_template(repo: str) -> str:
    """Return the prompt suffix for ``repo``, or an empty string if unknown."""
    if not repo:
        return ""
    name = _sanitize_repo_name(repo)
    if not name:
        return ""
    path = TEMPLATE_DIR / f"{name}.md"
    try:
        if not path.is_file():
            return ""
        return path.read_text(encoding="utf-8").strip()
    except OSError:
        return ""
