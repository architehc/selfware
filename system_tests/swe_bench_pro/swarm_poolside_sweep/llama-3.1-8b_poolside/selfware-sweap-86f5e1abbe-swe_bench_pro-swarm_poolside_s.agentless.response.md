### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_py_id(tested_str):
    # Ref: https://stackoverflow.com/a/55802320/595220
    return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
=======
def is_python_identifier(tested_str):
    return str.isidentifier(tested_str)
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_fqcn(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.')!= 1:
        return False

    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_collection_namespace_dir(tested_str):
    return any(_find_collections_in_subdirs(tested_str))
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_py_id(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    from re import match as _match_pattern
    from tokenize import Name as _VALID_IDENTIFIER_REGEX
    _valid_identifier_string_regex = ''.join((_VALID_IDENTIFIER_REGEX, r'\Z'))

    def _is_py_id(tested_str):
        # Ref: https://stackoverflow.com/a/55802320/595220
        return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_fqcn(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.')!= 1:
        return False

    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def is_valid_collection_name(fqcn):
    if not fqcn:
        return False

    namespace, name = fqcn.split('.')

    if not is_python_identifier(namespace):
        return False

    if not is_python_identifier(name):
        return False

    return not iskeyword(namespace) and not iskeyword(name)
>>>>>>> REPLACE

### FILE: lib/ansible/modules/apt.py
<<<<<<< SEARCH
def _is_py_id(tested_str):
    # Ref: https://stackoverflow.com/a/55802320/595220
    return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
>>>>>>> REPLACE

### FILE: lib/ansible/errors/__init__.py
<<<<<<< SEARCH
from __future__ import (absolute_import, division, print_function)
__metaclass__ = type

import re

from ansible.errors.yaml_strings import (
    YAML_COMMON_DICT_ERROR,
    YAML_COMMON_LEADING_TAB_ERROR,
    YAML_COMMON_PARTIALLY_QUOTED_LINE_ERROR,
    YAML_COMMON_UNBALANCED_QUOTES_ERROR,
    YAML_COMMON_UNQUOTED_COLON_ERROR,
    YAML_COMMON_UNQUOTED_VARIABLE_ERROR,
    YAML_POSITION_DETAILS,
    YAML_AND_SHORTHAND_ERROR,
)
from ansible.module_utils._text import to_native, to_text
from ansible.module_utils.common._collections_compat import Sequence
>>>>>>> REPLACE

### FILE: test/sanity/code-smell/package-data.py
<<<<<<< SEARCH
import contextlib
import fnmatch
import glob
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
>>>>>>> REPLACE

### FILE: test/units/errors/test_errors.py
<<<<<<< SEARCH
from __future__ import (absolute_import, division, print_function)
__metaclass__ = type

import contextlib
import fnmatch
import glob
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
>>>>>>> REPLACE