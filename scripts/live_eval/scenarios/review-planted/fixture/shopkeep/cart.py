"""Shopping carts that turn into orders."""

from typing import Dict, List

from .models import Order, OrderLine, Product


class Cart:
    """Quantities per sku for one customer."""

    def __init__(self, customer_id: int):
        self.customer_id = customer_id
        self._quantities: Dict[str, int] = {}

    def add(self, product: Product, quantity: int = 1) -> None:
        """Add `quantity` units of `product`; quantity must be positive."""
        if quantity <= 0:
            raise ValueError("quantity must be positive")
        self._quantities[product.sku] = self._quantities.get(product.sku, 0) + quantity

    def remove(self, sku: str) -> None:
        """Drop a sku from the cart entirely."""
        self._quantities.pop(sku, None)

    def skus(self) -> List[str]:
        """Skus currently in the cart, sorted."""
        return sorted(self._quantities)

    def checkout(self, order_id: int, catalog: Dict[str, Product]) -> Order:
        """Freeze the cart into an order at current catalog prices."""
        if not self._quantities:
            raise ValueError("cannot check out an empty cart")
        lines = [
            OrderLine(sku, qty, catalog[sku].price_cents)
            for sku, qty in sorted(self._quantities.items())
        ]
        self._quantities.clear()
        return Order(order_id=order_id, customer_id=self.customer_id, lines=lines)
