import unittest

from shopkeep.pagination import clamp_page_size, page_count


class PaginationTest(unittest.TestCase):
    def test_page_count(self):
        self.assertEqual(page_count(0, 10), 0)
        self.assertEqual(page_count(21, 10), 3)

    def test_clamp(self):
        self.assertEqual(clamp_page_size(0), 1)
        self.assertEqual(clamp_page_size(1000), 100)


if __name__ == "__main__":
    unittest.main()
