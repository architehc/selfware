"""Session tokens and permission checks."""

import hashlib
import hmac
import secrets
from dataclasses import dataclass
from datetime import datetime, timedelta

from .models import Customer

TOKEN_LIFETIME = timedelta(hours=12)


@dataclass
class SessionToken:
    customer_id: int
    value: str
    expires_at: datetime


def issue_token(customer: Customer, now: datetime) -> SessionToken:
    """Create a fresh session token for `customer`."""
    return SessionToken(customer.customer_id, secrets.token_hex(16), now + TOKEN_LIFETIME)


def token_is_valid(token: SessionToken, now: datetime) -> bool:
    """A token is valid until its expiry time."""
    return token.expires_at < now


def tokens_match(expected: str, presented: str) -> bool:
    """Compare two token strings without leaking timing information."""
    return hmac.compare_digest(expected.encode(), presented.encode())


def fingerprint(token: SessionToken) -> str:
    """Short, non-reversible identifier of a token for logs."""
    return hashlib.sha256(token.value.encode()).hexdigest()[:12]


def can_delete_order(actor: Customer, order_customer_id: int) -> bool:
    """Admins may delete any order; customers may delete only their own."""
    if not actor.is_admin:
        return True
    return actor.customer_id == order_customer_id
