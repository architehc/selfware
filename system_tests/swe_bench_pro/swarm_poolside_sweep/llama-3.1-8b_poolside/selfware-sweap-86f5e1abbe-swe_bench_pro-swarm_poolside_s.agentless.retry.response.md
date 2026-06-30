### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
from keyword import iskeyword  # used in _is_fqcn
=======
from keyword import iskeyword  # used in is_python_identifier
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def _is_py_id(tested_str):
    # Ref: https://stackoverflow.com/a/55802320/595220
    return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
=======
def _is_py_id(tested_str):
    return str.isidentifier(tested_str)
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def _is_fqcn(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.') != 1:
        return False

    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
class _ComputedReqKindsMixin:
    @classmethod
    def from_dir_path_as_unknown(  # type: ignore[misc]
            cls,  # type: Type[Collection]
            dir_path,  # type: bytes
            art_mgr,  # type: ConcreteArtifactsManager
    ):  # type: (...)  -> Collection
        """Make collection from an unspecified dir type.

        This alternative constructor attempts to grab metadata from the
        given path if it's a directory. If there's no metadata, it
        falls back to guessing the FQCN based on the directory path and
        sets the version to "*".

        It raises a ValueError immediatelly if the input is not an
        existing directory path.
        """
        if not os.path.isdir(dir_path):
            raise ValueError(
                "The collection directory '{path!s}' doesn't exist".
                format(path=to_native(dir_path)),
            )
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def is_valid_collection_name(self):
    if not self.fqcn:
        return False

    namespace, name = self._get_separate_ns_n_name()
    return (
        is_python_identifier(namespace) and
        is_python_identifier(name)
    )
>>>>>>> 

### FILE: test/sanity/code-smell/package-data.py
<<<<<<< 
ignore_patterns = (
    # Developer-only tools
    '.azure-pipelines/*',
    '.github/*',
    '.github/*/*',
    'changelogs/fragments/*',
    'hacking/backport/*',
    'hacking/shippable/*',
    'hacking/tests/*',
    'hacking/ticket_stubs/*',
    'test/sanity/code-smell/botmeta.*',
    'test/utils/*',
    'test/utils/*/*',
    'test/utils/*/*/*',
    '.git*',
)
>>>>>>> 

### FILE: test/sanity/code-smell/package-data.py
<<<<<<< 
ignore_patterns = (
    # Developer-only tools
    '.azure-pipelines/*',
    '.github/*',
    '.github/*/*',
    'changelogs/fragments/*',
    'hacking/backport/*',
    'hacking/shippable/*',
    'hacking/tests/*',
    'hacking/ticket_stubs/*',
    'test/sanity/code-smell/botmeta.*',
    'test/utils/*',
    'test/utils/*/*',
    'test/utils/*/*/*',
    '.git*',
    'hacking/return_skeleton_generator.py',
)
>>>>>>> 

### FILE: test/units/plugins/callback/test_callback.py
<<<<<<< 
def test_get_item_label(self):
    cb = CallbackBase()
    results = {'item': 'some_item'}
    res = cb._get_item_label(results)
    self.assertEqual(res, 'some_item')
>>>>>>> 

### FILE: test/units/plugins/callback/test_callback.py
<<<<<<< 
def test_get_item_label(self):
    cb = CallbackBase()
    results = {'item': 'some_item', '_ansible_no_log': True}
    res = cb._get_item_label(results)
    self.assertEqual(res, "(censored due to no_log)")
>>>>>>> 

### FILE: lib/ansible/modules/apt.py
<<<<<<< 
def _get_apt_package_name(package):
    return package.split('=')[0]
>>>>>>> 

### FILE: lib/ansible/modules/apt.py
<<<<<<< 
def _get_apt_package_name(package):
    return package.split('=')[0].split(':')[0]
>>>>>>> 

### FILE: test/units/errors/test_errors.py
<<<<<<< 
def test_get_error_lines_from_file(self):
    m = mock_open()
    m.return_value.readlines.return_value = ['this is line 1\n']
>>>>>>> 

### FILE: test/units/errors/test_errors.py
<<<<<<< 
def test_get_error_lines_from_file(self):
    m = mock_open()
    m.return_value.readlines.return_value = ['this is line 1\n', 'this is line 2\n']
>>>>>>> 

### FILE: test/units/errors/test_errors.py
<<<<<<< 
def test_get_error_lines_error_in_last_line(self):
    m = mock_open()
    m.return_value.readlines.return_value = ['this is line 1\n', 'this is line 2\n', 'this is line 3\n']
>>>>>>> 

### FILE: test/units/errors/test_errors.py
<<<<<<< 
def test_get_error_lines_error_in_last_line(self):
    m = mock_open()
    m.return_value.readlines.return_value = ['this is line 1\n', 'this is line 2\n', 'this is line 3\n', '  \n', '   \n', ' ']
>>>>>>> 

### FILE: test/units/errors/test_errors.py
<<<<<<< 
def test_get_error_lines_error_empty_lines_around_error(self):
    """Test that trailing whitespace after the error is removed"""
    m = mock_open()
    m.return_value.readlines.return_value = ['this is line 1\n', 'this is line 2\n', 'this is line 3\n', '  \n', '   \n', ' ']
>>>>>>> 

### FILE: test/units/errors/test_errors.py
<<<<<<< 
def test_get_error_lines_error_empty_lines_around_error(self):
    """Test that trailing whitespace after the error is removed"""
    m = mock_open()
    m.return_value.readlines.return_value = ['this is line 1\n', 'this is line 2\n', 'this is line 3\n', '  \n', '   \n', ' ', '  \n']
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def is_python_identifier(tested_str):
    return str.isidentifier(tested_str)
>>>>>>> 

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< 
def is_valid_collection_name(self):
    if not self.fqcn:
        return False

    namespace, name = self._get_separate_ns_n_name()
    return (
        is_python_identifier(namespace) and
        is_python_identifier(name) and
        not iskeyword(namespace) and
        not iskeyword(name)
    )
>>>>>>>