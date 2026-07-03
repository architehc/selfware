diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index 118-142 (original) → 118-142 (modified)
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -128,19 +128,6 @@ def _is_fqcn(tested_str):
     # FIXME: port this to AnsibleCollectionRef.isValidCollectionName
     if tested_str.count('.') != 1:
         return False
 
-    def _is_py_id(s):
-        return s.isidentifier() and not keyword.iskeyword(s)
-
-    return all(
-        _is_py_id(ns_or_name)
-        for ns_or_name in tested_str.split('.')
-    )
-
- 
-class _ComputedReqKindsMixin:
- 
-     @classmethod
```