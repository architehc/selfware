
### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
from keyword import iskeyword  # used in _is_fqcn

try:
    from typing import TYPE_CHECKING
except ImportError:
    TYPE_CHECKING = False
=======
from keyword import iskeyword

try:
    from typing import TYPE_CHECKING
except ImportError:
    TYPE_CHECKING = False
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
try:  # NOTE: py3/py2 compat
    # FIXME: put somewhere into compat
    # py2 mypy can't deal with try/excepts
    _is_py_id = str.isidentifier  # type: ignore[attr-defined]
except AttributeError:  # Python 2
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    from re import match as _match_pattern
    from tokenize import Name as _VALID_IDENTIFIER_REGEX
    _valid_identifier_string_regex = ''.join((_VALID_IDENTIFIER_REGEX, r'\Z'))

    def _is_py_id(tested_str):
        # Ref: https://stackoverflow.com/a/55802320/595220
        return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
=======
def is_python_identifier(tested_str):
    """Check if a string is a valid Python identifier."""
    return tested_str.isidentifier() and not iskeyword(tested_str)
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_fqcn(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.') != 1:
        return False

    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
=======
def is_valid_collection_name(collection_name):
    """Check if a collection name is valid.

    A valid collection name has exactly two parts separated by a dot,
    where both parts are valid Python identifiers and not Python keywords.
    """
    if collection_name.count('.') != 1:
        return False

    namespace, name = collection_name.split('.')
    return is_python_identifier(namespace) and is_python_identifier(name)
>>>>>>> REPLACE
