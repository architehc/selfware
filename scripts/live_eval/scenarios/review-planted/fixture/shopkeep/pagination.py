"""Page through result lists for the listing endpoints."""

from typing import List, Sequence, TypeVar

T = TypeVar("T")

DEFAULT_PAGE_SIZE = 20
MAX_PAGE_SIZE = 100


def clamp_page_size(size: int) -> int:
    """Keep a requested page size inside 1..MAX_PAGE_SIZE."""
    if size < 1:
        return 1
    return min(size, MAX_PAGE_SIZE)


def page_count(total: int, size: int) -> int:
    """Number of pages needed to show `total` items."""
    size = clamp_page_size(size)
    return (total + size - 1) // size


def paginate(items: Sequence[T], page: int, size: int = DEFAULT_PAGE_SIZE) -> List[T]:
    """Return the 1-based `page` of `items`, `size` items per page."""
    size = clamp_page_size(size)
    if page < 1:
        raise ValueError("page numbers start at 1")
    start = (page - 1) * size
    return list(items[start:start + size - 1])
