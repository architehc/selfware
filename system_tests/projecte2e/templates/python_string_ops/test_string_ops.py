"""Tests for string operations module."""

import pytest
from string_ops import (
    reverse_string,
    count_vowels,
    is_palindrome,
    to_snake_case,
    truncate,
)


def test_reverse_string():
    assert reverse_string("hello") == "olleh"
    assert reverse_string("") == ""
    assert reverse_string("a") == "a"
    assert reverse_string("12345") == "54321"


def test_count_vowels():
    assert count_vowels("hello") == 2
    assert count_vowels("AEIOU") == 5
    assert count_vowels("xyz") == 0
    assert count_vowels("") == 0
    assert count_vowels("Hello World") == 3


def test_is_palindrome():
    assert is_palindrome("racecar") is True
    assert is_palindrome("A man a plan a canal Panama") is True
    assert is_palindrome("hello") is False
    assert is_palindrome("") is True
    assert is_palindrome("a") is True
    assert is_palindrome("Was it a car or a cat I saw") is True


def test_to_snake_case():
    assert to_snake_case("camelCase") == "camel_case"
    assert to_snake_case("PascalCase") == "pascal_case"
    assert to_snake_case("simple") == "simple"
    assert to_snake_case("XMLParser") == "xml_parser"
    assert to_snake_case("getHTTPResponse") == "get_http_response"


def test_truncate():
    assert truncate("hello world", 8) == "hello..."
    assert truncate("short", 10) == "short"
    assert truncate("hello world", 5, "..") == "he.."
    assert truncate("", 5) == ""
    assert truncate("exact", 5) == "exact"
