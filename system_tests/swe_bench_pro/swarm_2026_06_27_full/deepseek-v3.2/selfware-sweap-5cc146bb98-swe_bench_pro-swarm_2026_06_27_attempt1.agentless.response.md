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
# Removed Python 2 compatibility code for identifier checking.
# The function _is_py_id is no longer used.
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
def _is_fqcn(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.') != 1:
        return False

    # Use the new helper is_python_identifier which also checks for keywords
    from ansible.utils.collection_loader import is_python_identifier
    return all(
        is_python_identifier(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
>>>>>>> REPLACE

### FILE: lib/ansible/utils/collection_loader.py
<<<<<<< SEARCH
    @classmethod
    def is_valid_collection_name(cls, collection_name):
        # type: (str) -> bool
        """
        Validate a collection name.
        """
        if not isinstance(collection_name, string_types):
            return False

        if collection_name.count('.') != 1:
            return False

        namespace, name = collection_name.split('.')
        if not namespace or not name:
            return False

        # Check that namespace and name are valid Python identifiers
        # and not Python keywords.
        # FIXME: This should be using the same logic as _is_fqcn in dataclasses.py
        #        but currently that logic is broken for Python keywords.
        #        For now, we just check that they are non-empty and contain only
        #        alphanumeric characters and underscores.
        import keyword
        if keyword.iskeyword(namespace) or keyword.iskeyword(name):
            return False

        # Check that each segment is a valid Python identifier
        # Python 3 has str.isidentifier, but we need to support Python 2 as well.
        # We'll use a simple regex that matches the Python identifier spec.
        # This regex is taken from the Python documentation:
        #   identifier ::=  (letter|"_") (letter | digit | "_")*
        #   letter     ::=  lowercase | uppercase
        # We'll also need to consider Unicode letters in Python 3, but for now
        # we'll keep it simple and only allow ASCII.
        import re
        id_regex = re.compile(r'^[a-zA-Z_][a-zA-Z0-9_]*$')
        if not id_regex.match(namespace) or not id_regex.match(name):
            return False

        return True
=======
    @classmethod
    def is_valid_collection_name(cls, collection_name):
        # type: (str) -> bool
        """
        Validate a collection name.
        """
        if not isinstance(collection_name, string_types):
            return False

        if collection_name.count('.') != 1:
            return False

        namespace, name = collection_name.split('.')
        if not namespace or not name:
            return False

        # Use the new helper function to check that each segment is a valid Python identifier
        # and not a Python keyword.
        from ansible.utils.collection_loader import is_python_identifier
        return is_python_identifier(namespace) and is_python_identifier(name)
>>>>>>> REPLACE

### FILE: lib/ansible/utils/collection_loader.py
<<<<<<< SEARCH
# empty line
=======
def is_python_identifier(identifier):
    # type: (str) -> bool
    """
    Return True if the given string is a valid Python identifier (including
    that it is not a Python keyword).
    """
    import keyword
    import re

    # First, check if it's a keyword
    if keyword.iskeyword(identifier):
        return False

    # Use str.isidentifier if available (Python 3)
    try:
        if hasattr(str, 'isidentifier'):
            return str.isidentifier(identifier)
    except AttributeError:
        pass

    # Fallback for Python 2: use regex that matches Python identifier spec.
    # This regex is taken from the Python documentation:
    #   identifier ::=  (letter|"_") (letter | digit | "_")*
    #   letter     ::=  lowercase | uppercase
    # We'll also need to consider Unicode letters in Python 3, but for now
    # we'll keep it simple and only allow ASCII.
    id_regex = re.compile(r'^[a-zA-Z_][a-zA-Z0-9_]*$')
    return bool(id_regex.match(identifier))
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
            elif req_name is not None and _is_fqcn(req_name):
                req_type = 'galaxy'
=======
            elif req_name is not None and _is_fqcn(req_name):
                req_type = 'galaxy'
>>>>>>> REPLACE