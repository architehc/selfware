### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_fqcn(tested_str):
    if tested_str.count('.')!= 1:
        return False

    return all(
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )
=======
def is_python_identifier(tested_str):
    return bool(re.match(r'^\w+$', tested_str))

def is_valid_collection_name(tested_str):
    namespace, name = tested_str.split('.', 1)
    return not is_python_identifier(namespace) and not is_python_identifier(name) and is_python_identifier(tested_str)
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_py_id(tested_str):
    # Ref: https://stackoverflow.com/a/55802320/595220
    return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
=======
from keyword import iskeyword
from re import match as _match_pattern
from tokenize import Name as _VALID_IDENTIFIER_REGEX
_valid_identifier_string_regex = ''.join((_VALID_IDENTIFIER_REGEX, r'\Z'))
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
try:  # NOTE: py3/py2 compat
    _is_py_id = str.isidentifier  # type: ignore[attr-defined]
except AttributeError:  # Python 2
    from re import match as _match_pattern
    from tokenize import Name as _VALID_IDENTIFIER_REGEX
    _valid_identifier_string_regex = ''.join((_VALID_IDENTIFIER_REGEX, r'\Z'))

    def _is_py_id(tested_str):
        return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
=======
# Removed legacy helper functions and Python 2/3 compatibility code
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_collection_src_dir(dir_path):
    b_dir_path = to_bytes(dir_path, errors='surrogate_or_strict')
    return os.path.isfile(os.path.join(b_dir_path, _GALAXY_YAML))
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_installed_collection_dir(dir_path):
    b_dir_path = to_bytes(dir_path, errors='surrogate_or_strict')
    return os.path.isfile(os.path.join(b_dir_path, _MANIFEST_JSON))
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_collection_dir(dir_path):
    return (
        _is_installed_collection_dir(dir_path) or
        _is_collection_src_dir(dir_path)
    )
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _find_collections_in_subdirs(dir_path):
    b_dir_path = to_bytes(dir_path, errors='surrogate_or_strict')

    subdir_glob_pattern = os.path.join(
        b_dir_path,
        b'*',  # collection name
    )

    for subdir in iglob(subdir_glob_pattern):
        if os.path.isfile(os.path.join(subdir, _MANIFEST_JSON)):
            yield subdir
        elif os.path.isfile(os.path.join(subdir, _GALAXY_YAML)):
            yield subdir
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_collection_namespace_dir(tested_str):
    return any(_find_collections_in_subdirs(tested_str))
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_file_path(tested_str):
    return os.path.isfile(to_bytes(tested_str, errors='surrogate_or_strict'))
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_http_url(tested_str):
    return urlparse(tested_str).scheme.lower() in {'http', 'https'}
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_git_url(tested_str):
    return tested_str.startswith(('git+', 'git@'))
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
def _is_concrete_artifact_pointer(tested_str):
    return any(
        predicate(tested_str)
        for predicate in (
            _is_git_url,
            _is_http_url,
            _is_file_path,
            _is_collection_dir,
            _is_collection_namespace_dir,
        )
    )
=======
# No changes in this function
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
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
=======
class _ComputedReqKindsMixin:

    @classmethod
    def from_dir_path_as_unknown(cls, dir_path, art_mgr):
        if not os.path.isdir(dir_path):
            raise ValueError(
                "The collection directory '{path!s}' doesn't exist".
                format(path=to_native(dir_path)),
            )
=======
>>>>>>> REPLACE

### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
        if _is_collection_namespace_dir(src_path):
            req_name = None  # No name for a virtual req or "namespace."?
            req_type = 'subdirs'
            req_source = src_path
        else:
            raise AnsibleError(  # NOTE: this is never supposed to be hit
                'Failed to automatically detect the collection '
                'requirement type.',
            )

        if req_type not in {'file', 'galaxy', 'git', 'url', 'dir', 'subdirs'}:
            raise AnsibleError(
                "The collection requirement entry key 'type' must be "
                'one of file, galaxy, git, dir, subdirs, or url.'
            )

        if req_type!= 'galaxy' and req_source is None:
            req_source, req_name = req_name, None

        if (
                req_type == 'galaxy' and
                isinstance(req_source, GalaxyAPI) and
                not _is_http_url(req_source.api_server)
        ):
            raise AnsibleError(
                "Collections requirement 'source' entry should contain "
                'a valid Galaxy API URL but it does not: {not_url!s} '
                'is not an HTTP URL.'.
                format(not_url=req_source.api_server),
            )

        tmp_inst_req = cls(req_name, req_version, req_source, req_type)

        if req_type not in {'galaxy', 'subdirs'} and req_name is None:
            req_name = art_mgr.get_direct_collection_fqcn(tmp_inst_req)  # TODO: fix the cache key in artifacts manager?

        if req_type not in {'galaxy', 'subdirs'} and req_version == '*':
            req_version = art_mgr.get_direct_collection_version(tmp_inst_req)

        return cls(
            req_name, req_version,
            req_source, req_type,
        )

    def __repr__(self):
        return (
            '<{self!s} of type {coll_type!r} from {src!s}>'.
            format(self=self, coll_type=self.type, src=self.src or 'Galaxy')
        )

    def __str__(self):
        return to_native(self.__unicode__())

    def __unicode__(self):
        if self.fqcn is None:
            return (
                u'"virtual collection Git repo"' if self.is_scm
                else u'"virtual collection namespace"'
            )

        return (
            u'{fqcn!s}:{ver!s}'.
            format(fqcn=to_text(self.fqcn), ver=to_text(self.ver))
        )

    def _get_separate_ns_n_name(self):  # FIXME: use LRU cache
        return self.fqcn.split('.')

    @property
    def namespace(self):
        if self.is_virtual:
            raise TypeError('Virtual collections do not have a namespace')

        return self._get_separate_ns_n_name()[0]

    @property
    def name(self):
>>>>>>> REPLACE
        if self.is_virtual:
            raise TypeError('Virtual collections do not have a namespace')

        return self._get_separate_ns_n_name()[1]
>>>>>>> REPLACE