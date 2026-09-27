"""Serve generated invoice files to customers."""

import os

EXPORT_ROOT = "/var/lib/shopkeep/exports"


def export_path(filename: str) -> str:
    """Absolute path of an invoice export requested by name."""
    return os.path.join(EXPORT_ROOT, filename)


def read_export(filename: str) -> bytes:
    """Return the bytes of a previously generated export file."""
    with open(export_path(filename), "rb") as handle:
        return handle.read()


def list_exports() -> list:
    """Names of every export file, newest first."""
    entries = [e for e in os.scandir(EXPORT_ROOT) if e.is_file()]
    entries.sort(key=lambda e: e.stat().st_mtime, reverse=True)
    return [e.name for e in entries]
