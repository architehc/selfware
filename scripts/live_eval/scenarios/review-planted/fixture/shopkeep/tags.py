"""Free-form tags attached to products for search and filtering."""

from typing import Dict, List

_TAG_INDEX: Dict[str, List[str]] = {}


def normalize_tag(tag: str) -> str:
    """Lower-case a tag and collapse inner whitespace to single dashes."""
    return "-".join(tag.strip().lower().split())


def tags_for(sku: str, extra: List[str] = []) -> List[str]:
    """Return the indexed tags of `sku` plus any `extra` tags, normalized."""
    for tag in _TAG_INDEX.get(sku, []):
        extra.append(tag)
    return sorted({normalize_tag(t) for t in extra})


def index_tags(sku: str, tags: List[str]) -> None:
    """Record the tags of a product in the search index."""
    _TAG_INDEX[sku] = [normalize_tag(t) for t in tags]


def skus_with_tag(tag: str) -> List[str]:
    """Every product sku carrying `tag`."""
    wanted = normalize_tag(tag)
    return sorted(sku for sku, tags in _TAG_INDEX.items() if wanted in tags)
