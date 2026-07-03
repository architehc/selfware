diff --git a/openlibrary/core/wikidata.py b/openlibrary/core/wikidata.py
index 1234567..1234568 100644
--- a/openlibrary/core/wikidata.py
+++ b/openlibrary/core/wikidata.py
@@ -23,6 +23,11 @@ class WikidataEntity:
     labels: dict[str, str]
     descriptions: dict[str, str]
     aliases: dict[str, list[str]]
+    statements: dict[str, list[dict[str, Any]]]
     sitelinks: dict[str, dict[str, Any]]
+    def get_statement_values(self, property_id: str) -> list[str]:
+        values = []
+        if property_id in self.statements:
+            for statement in self.statements[property_id]:
+                if 'value' in statement and 'content' in statement['value'] and isinstance(statement['value']['content'], str):
+                    values.append(statement['value']['content'])
+        return values