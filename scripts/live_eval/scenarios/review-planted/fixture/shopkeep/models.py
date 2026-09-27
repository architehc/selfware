"""Plain data types shared by the rest of the package."""

from dataclasses import dataclass, field
from datetime import datetime
from typing import List, Optional


@dataclass
class Customer:
    customer_id: int
    name: str
    email: str
    is_admin: bool = False


@dataclass
class Product:
    sku: str
    title: str
    price_cents: int
    stock: int = 0


@dataclass
class OrderLine:
    sku: str
    quantity: int
    unit_price_cents: int

    def total_cents(self) -> int:
        return self.quantity * self.unit_price_cents


@dataclass
class Order:
    order_id: int
    customer_id: int
    lines: List[OrderLine] = field(default_factory=list)
    created_at: datetime = field(default_factory=datetime.utcnow)
    cancelled_at: Optional[datetime] = None

    def total_cents(self) -> int:
        return sum(line.total_cents() for line in self.lines)

    @property
    def is_cancelled(self) -> bool:
        return self.cancelled_at is not None
