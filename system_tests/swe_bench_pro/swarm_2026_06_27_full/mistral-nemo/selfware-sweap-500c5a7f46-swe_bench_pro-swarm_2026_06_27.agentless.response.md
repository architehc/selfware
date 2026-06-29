### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
# -*- coding: utf-8 -*-
# Copyright: (c) 2020-2021, Ansible Project
# GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
"""Dependency structs."""
# FIXME: add caching all over the place

from __future__ import (absolute_import, division, print_function)
__metaclass__ = type

import json
import os
from collections import namedtuple
from glob import iglob
from keyword import iskeyword  # used in _is_fqcn

try:
    from typing import TYPE_CHECKING
except ImportError:
    TYPE_CHECKING = False

if TYPE_CHECKING:
    from typing import Tuple, Type, TypeVar
    from ansible.galaxy.collection.concrete_artifact_manager import (
        ConcreteArtifactsManager,
    )
    Collection = TypeVar(
        'Collection',
        'Candidate', 'Requirement',
        '_ComputedReqKindsMixin',
    )

import yaml

from ansible.errors import AnsibleError
from ansible.galaxy.api import GalaxyAPI
from ansible.module_utils._text import to_bytes, to_native, to_text
from ansible.module_utils.six.moves.urllib.parse import urlparse
from ansible.module_utils.six import raise_from
from ansible.utils.display import Display


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


_ALLOW_CONCRETE_POINTER_IN_SOURCE = False  # NOTE: This is a feature flag
_GALAXY_YAML = b'galaxy.yml'
_MANIFEST_JSON = b'MANIFEST.json'


display = Display()


def _is_collection_src_dir(dir_path):
    b_dir_path = to_bytes(dir_path, errors='surrogate_or_strict')
    return os.path.isfile(os.path.join(b_dir_path, _GALAXY_YAML))


def _is_installed_collection_dir(dir_path):
    b_dir_path = to_bytes(dir_path, errors='surrogate_or_strict')
    return os.path.isfile(os.path.join(b_dir_path, _MANIFEST_JSON))


def _is_collection_dir(dir_path):
    return (
        _is_installed_collection_dir(dir_path) or
        _is_collection_src_dir(dir_path)
    )


def _find_collections_in_subdirs(dir_path):
    b_dir_path = to_bytes(dir_path, errors='surrogate_or_strict')

    subdir_glob_pattern = os.path.join(
        b_dir_path,
        # b'*',  # namespace is supposed to be top-level per spec
        b'*',  # collection name
    )

    for subdir in iglob(subdir_glob_pattern):
        if os.path.isfile(os.path.join(subdir, _MANIFEST_JSON)):
            yield subdir
        elif os.path.isfile(os.path.join(subdir, _GALAXY_YAML)):
            yield subdir


def _is_collection_namespace_dir(tested_str):
    return any(_find_collections_in_subdirs(tested_str))


def _is_file_path(tested_str):
    return os.path.isfile(to_bytes(tested_str, errors='surrogate_or_strict'))


def _is_http_url(tested_str):
    return urlparse(tested_str).scheme.lower() in {'http', 'https'}


def _is_git_url(tested_str):
    return tested_str.startswith(('git+', 'git@'))


def _is_concrete_artifact_pointer(tested_str):
    return any(
        predicate(tested_str)
        for predicate in (
            # NOTE: Maintain the checks to be sorted from light to heavy:
            _is_git_url,
            _is_http_url,
            _is_file_path,
            _is_collection_dir,
            _is_collection_namespace_dir,
        )
    )


def _is_fqcn(tested_str):
    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
    if tested_str.count('.') != 1:
        return False

    return all(
        # FIXME: keywords and identifiers are different in differnt Pythons
        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
        for ns_or_name in tested_str.split('.')
    )


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

        # TODO: cache this
        if _is_collection_src_dir(dir_path):
            return cls.from_collection_src_dir(dir_path, art_mgr)
        elif _is_installed_collection_dir(dir_path):
            return cls.from_installed_collection_dir(dir_path, art_mgr)
        else:
            raise ValueError(
                "The collection directory '{path!s}' doesn't contain "
                "either a {manifest_json!s} file or a {galaxy_yml!s} "
                "file.\nThe directory must be either an installed "
                "collection directory or a source collection "
                "directory, not both.".format(
                    path=to_native(dir_path),
                    manifest_json=to_native(_MANIFEST_JSON),
                    galaxy_yml=to_native(_GALAXY_YAML),
                )
            )

    @classmethod
    def from_collection_src_dir(  # type: ignore[misc]
            cls,  # type: Type[Collection]
            dir_path,  # type: bytes
            art_mgr,  # type: ConcreteArtifactsManager
    ):  # type: (...)  -> Collection
        """Make collection from a source collection directory.

        This constructor attempts to grab metadata from the
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

        # TODO: cache this
        if _is_collection_src_dir(dir_path):
            return cls.from_collection_src_dir(dir_path, art_mgr)
        elif _is_installed_collection_dir(dir_path):
            return cls.from_installed_collection_dir(dir_path, art_mgr)
        else:
            raise ValueError(
                "The collection directory '{path!s}' doesn't contain "
                "either a {manifest_json!s} file or a {galaxy_yml!s} "
                "file.\nThe directory must be either an installed "
                "collection directory or a source collection "
                "directory, not both.".format(
                    path=to_native(dir_path),
                    manifest_json=to_native(_MANIFEST_JSON),
                    galaxy_yml=to_native(_GALAXY_YAML),
                )
            )

    @classmethod
    def from_installed_collection_dir(  # type: ignore[misc]
            cls,  # type: Type[Collection]
            dir_path,  # type: bytes
            art_mgr,  # type: ConcreteArtifactsManager
    ):  # type: (...)  -> Collection
        """Make collection from an installed collection directory.

        This constructor attempts to grab metadata from the
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

        # TODO: cache this
        if _is_collection_src_dir(dir_path):
            return cls.from_collection_src_dir(dir_path, art_mgr)
        elif _is_installed_collection_dir(dir_path):
            return cls.from_installed_collection_dir(dir_path, art_mgr)
        else:
            raise ValueError(
                "The collection directory '{path!s}' doesn't contain "
                "either a {manifest_json!s} file or a {galaxy_yml!s} "
                "file.\nThe directory must be either an installed "
                "collection directory or a source collection "
                "directory, not both.".format(
                    path=to_native(dir_path),
                    manifest_json=to_native(_MANIFEST_JSON),
                    galaxy_yml=to_native(_GALAXY_YAML),
                )
            )

    @classmethod
    def from_collection_name_and_version(  # type: ignore[misc]
            cls,  # type: Type[Collection]
            name,  # type: str
            version,  # type: str
            art_mgr,  # type: ConcreteArtifactsManager
    ):  # type: (...)  -> Collection
        """Make collection from a name and version.

        This constructor attempts to grab metadata from the
        given path if it's a directory. If there's no metadata, it
        falls back to guessing the FQCN based on the directory path and
        sets the version to "*".

        It raises a ValueError immediatelly if the input is not an
        existing directory path.
        """
        if not isinstance(name, str):
            raise ValueError(
                "The collection name '{name!s}' is not a string".
                format(name=name),
            )

        if not isinstance(version, str):
            raise ValueError(
                "The collection version '{version!s}' is not a string".
                format(version=version),
            )

        # TODO: cache this
        if _is_collection_src_dir(dir_path):
            return cls.from_collection_src_dir(dir_path, art_mgr)
        elif _is_installed_collection_dir(dir_path):
            return cls.from_installed_collection_dir(dir_path, art_mgr)
        else:
            raise ValueError(
                "The collection directory '{path!s}' doesn't contain "
                "either a {manifest_json!s} file or a {galaxy_yml!s} "
                "file.\nThe directory must be either an installed "
                "collection directory or a source collection "
                "directory, not both.".format(
                    path=to_native(dir_path),
                    manifest_json=to_native(_MANIFEST_JSON),
                    galaxy_yml=to_native(_GALAXY_YAML),
                )
            )

    @classmethod
    def from_collection_name_and_version(  # type: ignore[misc]
            cls,  # type: Type[Collection]
            name,  # type: str
            version,  # type: str
            art_mgr,  # type: ConcreteArtifactsManager
    ):  # type: (...)  -> Collection
        """Make collection from a name and version.

        This constructor attempts to grab metadata from the
        given path if it's a directory. If there's no metadata, it
        falls back to guessing the FQCN based on the directory path and
        sets the version to "*".

        It raises a ValueError immedi