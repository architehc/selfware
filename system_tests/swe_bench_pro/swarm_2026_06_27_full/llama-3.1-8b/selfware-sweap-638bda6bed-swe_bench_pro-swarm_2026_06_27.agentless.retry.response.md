### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def _is_py_id(tested_str):
    # Ref: https://stackoverflow.com/a/55802320/595220
    return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
=======
def is_python_identifier(tested_str):
    return str.isidentifier(tested_str)
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def _is_fqcn(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.')!= 1:
        return False

    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def is_valid_collection_name(self):
    namespace, name = self._get_separate_ns_n_name()
    return (
        not iskeyword(namespace) and
        not iskeyword(name) and
        is_python_identifier(namespace) and
        is_python_identifier(name)
    )
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def _is_collection_dir(dir_path):
    return (
        _is_installed_collection_dir(dir_path) or
        _is_collection_src_dir(dir_path)
    )
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def _is_collection_src_dir(dir_path):
    b_dir_path = to_bytes(dir_path, errors='surrogate_or_strict')
    return os.path.isfile(os.path.join(b_dir_path, _GALAXY_YAML))
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def _is_installed_collection_dir(dir_path):
    b_dir_path = to_bytes(dir_path, errors='surrogate_or_strict')
    return os.path.isfile(os.path.join(b_dir_path, _MANIFEST_JSON))
>>>>>>> 

### FILE: lib/ansible/errors/__init__.py
<<<<<<< 
class AnsibleFileNotFound(AnsibleRuntimeError):
    ''' a file missing failure '''

    def __init__(self, message="", obj=None, show_content=True, suppress_extended_error=False, orig_exc=None, paths=None, file_name=None):

        self.file_name = file_name
        self.pat
>>>>>>> 

### FILE: lib/ansible/errors/__init__.py
<<<<<<< 
class AnsibleFileNotFound(AnsibleRuntimeError):
    ''' a file missing failure '''

    def __init__(self, message="", obj=None, show_content=True, suppress_extended_error=False, orig_exc=None, paths=None, file_name=None):

        self.file_name = file_name
        self.pattern = None
        self.paths = paths
        self.orig_exc = orig_exc
        self.message = message
        self.obj = obj
        self._show_content = show_content
        self._suppress_extended_error = suppress_extended_error
>>>>>>>