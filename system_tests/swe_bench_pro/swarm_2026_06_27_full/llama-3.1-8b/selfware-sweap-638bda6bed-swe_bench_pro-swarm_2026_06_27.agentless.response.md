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

### FILE: lib/ansible/errors/__init__.py
<<<<<<< SEARCH
from keyword import iskeyword  # used in _is_fqcn
>>>>>>> REPLACE

### FILE: lib/ansible/modules/apt.py
<<<<<<< SEARCH
from ansible.module_utils._text import to_native, to_text
from ansible.module_utils.common._collections_compat import Sequence
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def is_valid_collection_name(fqcn):
    namespace, name = fqcn.split('.')
    return is_python_identifier(namespace) and is_python_identifier(name) and not iskeyword(namespace) and not iskeyword(name)
>>>>>>> REPLACE