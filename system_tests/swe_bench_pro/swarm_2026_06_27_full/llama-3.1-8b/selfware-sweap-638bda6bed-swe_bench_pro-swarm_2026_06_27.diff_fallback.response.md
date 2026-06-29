diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index 3b3d3f2..e5c4f5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -128,8 +128,8 @@
 def _is_fqcn(tested_str):
     # FIXME: port this to AnsibleCollectionRef.is_valid_collection_name
     if tested_str.count('.') != 1:
-        return False
-
     return all(
-        not iskeyword(ns_or_name) and _is_py_id(ns_or_name)
+        is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
     )

@@ -141,3 +141,13 @@
 class _ComputedReqKindsMixin:
     @classmethod
     def _get_computed_req_kinds(cls, collection_ref):
+def is_python_identifier(s):
+    return s.isidentifier() and not iskeyword(s)
+
+def is_valid_collection_name(namespace, name):
+    return is_python_identifier(namespace) and is_python_identifier(name)
+
+def _is_py_id(s):
+    return is_python_identifier(s)
+
+def _is_collection_namespace_dir(collection_dir):
+    return is_valid_collection_name(*collection_dir.split('/'))
+
+def _is_collection_dir(collection_dir):
+    return is_valid_collection_name(*collection_dir.split('/'))

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index e5c4f5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency_resolution/dataclasses.py b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
index f6b3e5c..f6b3e5c 100644
--- a/lib/ansible/galaxy/dependency_resolution/dataclasses.py
+++ b/lib/ansible/galaxy/dependency_resolution/dataclasses.py
@@ -134,7 +134,7 @@
     return all(
         is_python_identifier(ns_or_name)
         for ns_or_name in tested_str.split('.')
-    )
+    )

diff --git a/lib/ansible/galaxy/dependency