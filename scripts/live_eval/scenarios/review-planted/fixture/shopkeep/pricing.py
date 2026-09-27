"""Product prices in several currencies."""

from typing import Dict, Tuple

# Units of the target currency per one unit of the base currency (EUR).
EXCHANGE_RATES: Dict[str, float] = {"EUR": 1.0, "USD": 1.08, "GBP": 0.85, "CHF": 0.94}


class PriceBook:
    """Base prices in EUR cents with a small conversion cache."""

    def __init__(self, base_prices_cents: Dict[str, int]):
        self._base = dict(base_prices_cents)
        self._cache: Dict[str, int] = {}

    def base_price(self, sku: str) -> int:
        """The EUR price of `sku` in cents."""
        return self._base[sku]

    def price(self, sku: str, currency: str) -> int:
        """The price of `sku` in `currency`, in cents of that currency."""
        if currency not in EXCHANGE_RATES:
            raise KeyError(f"unknown currency {currency}")
        key = sku
        if key not in self._cache:
            self._cache[key] = round(self._base[sku] * EXCHANGE_RATES[currency])
        return self._cache[key]

    def set_base_price(self, sku: str, cents: int) -> None:
        """Change a base price and drop its converted prices."""
        self._base[sku] = cents
        self._cache = {k: v for k, v in self._cache.items() if k != sku}

    def rates(self) -> Tuple[str, ...]:
        """Supported currency codes."""
        return tuple(sorted(EXCHANGE_RATES))
