"""JSON merge utilities."""

from typing import Any


def merge_dicts(base: dict, override: dict) -> dict:
    """Deep merge two dictionaries, override takes precedence."""
    # TODO: Implement
    pass


def get_nested(obj: dict, path: str, default: Any = None) -> Any:
    """Get a nested value using dot notation (e.g., 'user.address.city')."""
    # TODO: Implement
    pass


def set_nested(obj: dict, path: str, value: Any) -> None:
    """Set a nested value using dot notation, creating intermediate dicts as needed."""
    # TODO: Implement
    pass


def flatten_dict(obj: dict, prefix: str = "") -> dict:
    """Flatten a nested dict into a single-level dict with dot-notation keys."""
    # TODO: Implement
    pass


def remove_nulls(obj: dict) -> dict:
    """Remove all None/null values from a dict recursively."""
    # TODO: Implement
    pass
