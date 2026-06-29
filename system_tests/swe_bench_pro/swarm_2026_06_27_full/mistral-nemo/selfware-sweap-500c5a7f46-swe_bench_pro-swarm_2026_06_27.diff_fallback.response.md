diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index 123456..789abc 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -36,11 +36,6 @@ from ansible.module_utils.six.moves.urllib.parse import urlparse
 from ansible.module_utils.six import raise_from
 from ansible.utils.display import Display

-try:  # NOTE: py3/py2 compat
-    # FIXME: put somewhere into compat
-    # py2 mypy can't deal with try/excepts
-    _is_py_id = str.isidentifier  # type: ignore[attr-defined]
-except AttributeError:  # Python 2
-    # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
-    from re import match as _match_pattern
-    from tokenize import Name as _VALID_IDENTIFIER_REGEX
-    _valid_identifier_string_regex = ''.join((_VALID_IDENTIFIER_REGEX, r'\Z'))
-
-    def _is_py_id(tested_str):
-        # Ref: https://stackoverflow.com/a/55802320/595220
-        return bool(_match_pattern(_valid_identifier_string_regex, tested_str))
-
 _ALLOW_CONCRETE_POINTER_IN_SOURCE = False  # NOTE: This is a feature flag

@@ -118,7 +113,11 @@ def _is_git_url(tested_str):
     return bool(urlparse(tested_str).netloc)

 _is_http_url = _is_git_url

-def _is_fqcn(tested_str):
+def is_python_identifier(tested_str):
+    return bool(str.isidentifier(tested_str))
+
+def is_valid_collection_name(namespace, name):
+    return is_python_identifier(namespace) and is_python_identifier(name)
+
+def _is_fqcn(tested_str):
     namespace, name = tested_str.split('.', 1)
     return is_valid_collection_name(namespace, name)

@@ -140,6 +140,7 @@ class _ComputedReqKindsMixin:
     @classmethod
     def from_dir_path_as_unknown(  # type: ignore[misc]
         cls,  # type: Type[Collection]
         dir_path,  # type: bytes
+        fqcn,  # type: Optional[str]
         **kwargs  # type: Any
     ) -> Collection:
         # ...