"""Persist orders as JSON documents on disk."""

import json
import os
from datetime import datetime
from typing import Dict, List, Optional

from .models import Order, OrderLine


def order_to_dict(order: Order) -> Dict:
    """A JSON-serializable view of an order."""
    return {
        "order_id": order.order_id,
        "customer_id": order.customer_id,
        "lines": [
            {"sku": l.sku, "quantity": l.quantity, "unit_price_cents": l.unit_price_cents}
            for l in order.lines
        ],
        "created_at": order.created_at.isoformat(),
        "cancelled_at": order.cancelled_at.isoformat() if order.cancelled_at else None,
    }


def _parse_time(value: Optional[str]) -> Optional[datetime]:
    return datetime.fromisoformat(value) if value else None


def order_from_dict(data: Dict) -> Order:
    """Rebuild an order from `order_to_dict` output."""
    lines: List[OrderLine] = [OrderLine(**line) for line in data["lines"]]
    return Order(
        order_id=data["order_id"],
        customer_id=data["customer_id"],
        lines=lines,
        created_at=datetime.fromisoformat(data["created_at"]),
        cancelled_at=_parse_time(data.get("cancelled_at")),
    )


def save_order(directory: str, order: Order) -> bool:
    """Write `order` to `<directory>/<order_id>.json`; True when it was written."""
    path = os.path.join(directory, f"{order.order_id}.json")
    try:
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(order_to_dict(order), handle)
    except OSError:
        pass
    return True


def load_order(directory: str, order_id: int) -> Order:
    """Read one order back from disk."""
    path = os.path.join(directory, f"{order_id}.json")
    with open(path, encoding="utf-8") as handle:
        return order_from_dict(json.load(handle))
