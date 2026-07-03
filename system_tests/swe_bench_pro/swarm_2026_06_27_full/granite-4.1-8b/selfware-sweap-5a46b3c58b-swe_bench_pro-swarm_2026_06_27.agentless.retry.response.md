### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
if tested_str.count('.') != 1:
    return False

return all(
    # FIXME: keywords and identifiers are different in different Pythons
    not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
    for ns_or_name in tested_str.split('.')
)
=======
if tested_str.count('.') != 1:
    return False

return all(
    not iskeyword(ns_or_name) and is_valid_python_identifier(ns_or_name)
    for ns_or_name in tested_str.split('.')
)
>>>>>>> REPLACE

### FILE: lib/ansible/errors/__init__.py
<<<<<<< SEARCH
from ansible.errors.yaml_strings import (
    YAML_COMMON_DICT_ERROR,
    YAML_COMMON_LEADING_TAB_ERROR,
    YAML_COMMON_PARTIALLY_QUOTED_LINE_ERROR,
    YAML_COMMON_UNBALANCED_QUOTES_ERROR,
    YAML_COMMON_UNQUOTED_COLON_ERROR,
    YAML_COMMON_UNQUOTED_VARIABLE_ERROR,
    YAML_POSITION_DETAILS,
    YAML_AND_