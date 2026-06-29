### FILE: lib/ansible/galaxy/dependency_resolution/dataclasses.py
<<<<<<< SEARCH
    def _is_collection_namespace_dir(tested_str):
        return any(_find_collections_in_subdirs(tested_str))
>>>>>>> REPLACE
    def _is_collection_namespace_dir(tested_str):
        return any(_find_collections_in_subdirs(tested_str)) and not any(t.iskeyword(tested_str.split('.')[0]) for t in tested_str.split('.'))
<<<<<<< SEARCH
    def _is_collection_dir(dir_path):
        return (
            _is_installed_collection_dir(dir_path) or
            _is_collection_src_dir(dir_path)
        )
>>>>>>> REPLACE
    def _is_collection_dir(dir_path):
        return (
            _is_installed_collection_dir(dir_path) or
            _is_collection_src_dir(dir_path) and not any(t.iskeyword(tested_str.split('.')[0]) for tested_str in _find_collections_in_subdirs(dir_path))
        )
<<<<<<< SEARCH
    def _is_file_path(tested_str):
        return os.path.isfile(to_bytes(tested_str, errors='surrogate_or_strict'))
>>>>>>> REPLACE
    def _is_file_path(tested_str):
        return os.path.isfile(to_bytes(tested_str, errors='surrogate_or_strict')) and not any(t.iskeyword(tested_str.split('.')[0]) for t in tested_str.split('/'))
<<<<<<< SEARCH
    def _is_http_url(tested_str):
        return urlparse(tested_str).scheme.lower() in {'http', 'https'}
>>>>>>> REPLACE
    def _is_http_url(tested_str):
        return urlparse(tested_str).scheme.lower() in {'http', 'https'} and not any(t.iskeyword(tested_str.split('/')[0]) for t in tested_str.split('/'))
<<<<<<< SEARCH
    def _is_git_url(tested_str):
        return tested_str.startswith(('git+', 'git@'))
>>>>>>> REPLACE
    def _is_git_url(tested_str):
        return tested_str.startswith(('git+', 'git@')) and not any(t.iskeyword(tested_str.split('/')[0]) for t in tested_str.split('/'))
<<<<<<< SEARCH
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
>>>>>>> REPLACE
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
        ) and not any(t.iskeyword(tested_str.split('.')[0]) for t in tested_str.split('/'))
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
>>>>>>> REPLACE
    def _is_fqcn(tested_str):
        # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
        if tested_str.count('.') != 1:
            return False

        return all(
            # FIXME: keywords and identifiers are different in differnt Pythons
            not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
            for ns_or_name in tested_str.split('.')
        ) and not any(t.iskeyword(tested_str.split('.')[0]) for t in tested_str.split('.'))

### FILE: lib/ansible/modules/apt.py
<<<<<<< SEARCH
    def _is_py_id(tested_str):
        # Ref: https://stackoverflow.com/a/55802320/595220
        return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
>>>>>>> REPLACE
    def _is_py_id(tested_str):
        # Ref: https://stackoverflow.com/a/55802320/595220
        return bool(_match_pattern(_valid_identifier_string_regex, tested_str)) and not any(t.iskeyword(tested_str) for t in tested_str.split('.'))

### FILE: lib/ansible/errors/__init__.py
<<<<<<< SEARCH
    def _get_extended_error(self):
        # ...
        if src_file not in ('<string>', '<unicode>') and self._show_content:
            (target_line, prev_line) = self._get_error_lines_from_file(src_file, line_number - 1)
            target_line = to_text(target_line)
            prev_line = to_text(prev_line)
            if target_line:
                stripped_line = target_line.replace(" ", "")
                # ...
>>>>>>> REPLACE
    def _get_extended_error(self):
        # ...
        if src_file not in ('<string>', '<unicode>') and self._show_content:
            (target_line, prev_line) = self._get_error_lines_from_file(src_file, line_number - 1)
            target_line = to_text(target_line)
            prev_line = to_text(prev_line)
            if target_line:
                stripped_line = target_line.replace(" ", "")
                # ...
                if any(t.iskeyword(tested_str.split('.')[0]) for t in target_line.split('.') if '.' in target_line):
                    return ("The error appears to be in '{src_file!s}': line {line_number!s}, column {col_number!s}, but may\nbe elsewhere in the "
                            "file depending on the exact syntax problem.\n\nThe offending line appears to be:\n\n{prev_line!s}\n{target_line!s}\n{arrow_line!s}\n\n"
                            "The collection name '{target_line.split('.')[0]}' is a Python reserved keyword and cannot be used as a collection name.\n".format(
                                src_file=src_file, line_number=line_number, col_number=col_number, prev_line=prev_line, target_line=target_line,
                                arrow_line=(" " * (col_number - 1)) + "^ here"))
                return ("The error appears to be in '{src_file!s}': line {line_number!s}, column {col_number!s}, but may\nbe elsewhere in the file depending on "
                        "the exact syntax problem.\n\nThe offending line appears to be:\n\n{prev_line!s}\n{target_line!s}\n{arrow_line!s}\n\n".format(
                            src_file=src_file, line_number=line_number, col_number=col_number, prev_line=prev_line, target_line=target_line,
                            arrow_line=(" " * (col_number - 1)) + "^ here"))

### FILE: test/units/plugins/callback/test_callback.py
<<<<<<< SEARCH
    def test_clean_results(self):
        cb = CallbackBase()
        result = {'item': 'some_item',
                  'invocation': 'foo --bar whatever [some_json]',
                  'a': 'a single a in result note letter a is in invocation',
                  'b': 'a single b in result note letter b is not in invocation',
                  'changed': True}

        expected_result = result.copy()
        cb._clean_results(result, 'ebug')
        self.assertEqual(result, expected_result)
>>>>>>> REPLACE
    def test_clean_results(self):
        cb = CallbackBase()
        result = {'item': 'some_item',
                  'invocation': 'foo --bar whatever [some_json]',
                  'a': 'a single a in result note letter a is in invocation',
                  'b': 'a single b in result note letter b is not in invocation',
                  'changed': True}

        expected_result = result.copy()
        cb._clean_results(result, 'ebug')
        self.assertEqual(result, expected_result)
        self.assertNotIn('invocation', result)
        self.assertNotIn('changed', result)

### FILE: test/units/errors/test_errors.py
<<<<<<< SEARCH
    def test_error_with_object(self):
        self.obj.ansible_pos = ('foo.yml', 1, 1)

        mock_method.return_value = ('this is line 1\n', '')
        e = AnsibleError(self.message, self.obj)

        self.assertEqual(
            e.message,
            ("This is the error message\n\nThe error appears to be in 'foo.yml': line 1, column 1, but may\nbe elsewhere in the file depending on the "
             "exact syntax problem.\n\nThe offending line appears to be:\n\n\nthis is line 1\n^ here\n")
        )
>>>>>>> REPLACE
    def test_error_with_object(self):
        self.obj.ansible_pos = ('foo.yml', 1, 1)

        mock_method.return_value = ('this is line 1\n', '')
        e = AnsibleError(self.message, self.obj)

        self.assertEqual(
            e.message,
            ("This is the error message\n\nThe error appears to be in 'foo.yml': line 1, column 1, but may\nbe elsewhere in the file depending on the "
             "exact syntax problem.\n\nThe offending line appears to be:\n\n\nthis is line 1\n^ here\n")
        )
        self.assertNotIn('SENTINEL', e.message)