"""Command-line entry point: print a price list."""

import argparse
import sys
from typing import List, Optional

from .pricing import PriceBook

DEMO_PRICES = {"MUG-01": 1200, "TEE-02": 2500, "CAP-03": 1800}


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="shopkeep", description="Print a price list.")
    parser.add_argument("--currency", default="EUR", help="currency code (default EUR)")
    return parser


def main(argv: Optional[List[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    book = PriceBook(DEMO_PRICES)
    if args.currency not in book.rates():
        print(f"unknown currency: {args.currency}", file=sys.stderr)
        return 2
    for sku in sorted(DEMO_PRICES):
        cents = book.price(sku, args.currency)
        print(f"{sku}\t{cents / 100:.2f} {args.currency}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
