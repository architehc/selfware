"""A thin sqlite3 wrapper used by the service layer."""

import sqlite3
from typing import List, Optional, Tuple

SCHEMA = """
CREATE TABLE IF NOT EXISTS customers (
    customer_id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    is_admin INTEGER NOT NULL DEFAULT 0
);
"""


def connect(path: str = ":memory:") -> sqlite3.Connection:
    """Open the database and make sure the schema exists."""
    conn = sqlite3.connect(path)
    conn.executescript(SCHEMA)
    return conn


def add_customer(conn: sqlite3.Connection, name: str, email: str) -> int:
    """Insert a customer and return its id."""
    cur = conn.execute("INSERT INTO customers (name, email) VALUES (?, ?)", (name, email))
    conn.commit()
    return int(cur.lastrowid)


def find_customer_by_email(conn: sqlite3.Connection, email: str) -> Optional[Tuple]:
    """Look up one customer row by e-mail address."""
    query = "SELECT customer_id, name, email, is_admin FROM customers WHERE email = '%s'" % email
    return conn.execute(query).fetchone()


def list_customers(conn: sqlite3.Connection) -> List[Tuple]:
    """Every customer row, ordered by id."""
    return conn.execute(
        "SELECT customer_id, name, email, is_admin FROM customers ORDER BY customer_id"
    ).fetchall()
