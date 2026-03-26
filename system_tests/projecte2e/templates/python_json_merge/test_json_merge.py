"""Tests for JSON merge utilities."""

import pytest
from json_merge import (
    merge_dicts,
    get_nested,
    set_nested,
    flatten_dict,
    remove_nulls,
)


def test_merge_dicts():
    base = {"a": 1, "b": {"c": 2, "d": 3}}
    override = {"b": {"d": 4, "e": 5}, "f": 6}
    expected = {"a": 1, "b": {"c": 2, "d": 4, "e": 5}, "f": 6}
    assert merge_dicts(base, override) == expected


def test_merge_dicts_empty():
    assert merge_dicts({}, {"a": 1}) == {"a": 1}
    assert merge_dicts({"a": 1}, {}) == {"a": 1}
    assert merge_dicts({}, {}) == {}


def test_get_nested():
    obj = {"user": {"name": "John", "address": {"city": "NYC"}}}
    assert get_nested(obj, "user.name") == "John"
    assert get_nested(obj, "user.address.city") == "NYC"
    assert get_nested(obj, "user.age") is None
    assert get_nested(obj, "user.age", 0) == 0


def test_set_nested():
    obj = {}
    set_nested(obj, "a.b.c", 1)
    assert obj == {"a": {"b": {"c": 1}}}

    obj2 = {"x": 1}
    set_nested(obj2, "y.z", 2)
    assert obj2 == {"x": 1, "y": {"z": 2}}


def test_flatten_dict():
    obj = {"a": 1, "b": {"c": 2, "d": {"e": 3}}}
    expected = {"a": 1, "b.c": 2, "b.d.e": 3}
    assert flatten_dict(obj) == expected


def test_flatten_dict_empty():
    assert flatten_dict({}) == {}


def test_remove_nulls():
    obj = {"a": 1, "b": None, "c": {"d": 2, "e": None, "f": {"g": None}}}
    expected = {"a": 1, "c": {"d": 2, "f": {}}}
    assert remove_nulls(obj) == expected


def test_remove_nulls_no_nulls():
    obj = {"a": 1, "b": 2}
    assert remove_nulls(obj) == {"a": 1, "b": 2}
