### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
=======
    return all(
        not iskeyword(ns_or_name) and is_python_identifier(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
>>>>>>> REPLACE

### FILE: lib/ansible/errors/__init__.py
<<<<<<< SEARCH
def _is_py_id(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.') != 1:
        return False

    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
=======
def is_python_identifier(tested_str):
    return _is_py_id(tested_str)
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_py_id(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.') != 1:
        return False

    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
=======
def _is_py_id(tested_str):
    return is_python_identifier(tested_str)
>>>>>>> REPLACE