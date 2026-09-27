import unittest

from shopkeep.cart import Cart
from shopkeep.models import Product


class CartTest(unittest.TestCase):
    def test_checkout_totals(self):
        catalog = {"A": Product("A", "Alpha", 150), "B": Product("B", "Beta", 200)}
        cart = Cart(customer_id=7)
        cart.add(catalog["A"], 2)
        cart.add(catalog["B"])
        order = cart.checkout(1, catalog)
        self.assertEqual(order.total_cents(), 500)
        self.assertEqual(cart.skus(), [])

    def test_rejects_non_positive_quantity(self):
        with self.assertRaises(ValueError):
            Cart(1).add(Product("A", "Alpha", 1), 0)


if __name__ == "__main__":
    unittest.main()
