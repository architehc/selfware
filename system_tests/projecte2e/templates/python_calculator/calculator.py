from typing import Optional

def add(a: int, b: int) -> int:
    """Add two numbers."""
    return a + b

def subtract(a: int, b: int) -> int:
    """Subtract b from a."""
    return a - b

def multiply(a: int, b: int) -> int:
    """Multiply two numbers."""
    return a * b

def divide(a: int, b: int) -> Optional[float]:
    """Divide a by b. Returns None if b is 0."""
    if b == 0:
        return None
    return a / b
