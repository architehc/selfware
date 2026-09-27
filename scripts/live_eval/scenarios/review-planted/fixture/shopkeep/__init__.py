"""shopkeep: a small order-management library."""

from .models import Customer, Order, OrderLine, Product
from .pricing import PriceBook
from .pagination import paginate

__all__ = ["Customer", "Order", "OrderLine", "Product", "PriceBook", "paginate"]
__version__ = "0.3.1"
