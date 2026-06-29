diff --git a/openlibrary/core/wikidata.py b/openlibrary/core/wikidata.py
index 9a3a3f1..f5a5f5a 100644
--- a/openlibrary/core/wikidata.py
+++ b/openlibrary/core/wikidata.py
@@ -34,6 +34,7 @@
 
     id: str
     type: str
     labels: dict[str, str]
     descriptions: dict[str, str]
     aliases: dict[str, list[str]]
+    statements: dict[str, dict[str, dict]]
 
 @dataclass
 class WikidataEntity:
@@ -108,6 +109,13 @@
     response = requests.get(f'{WIKIDATA_API_URL}{id}')
     if response.status_code == 200:
         entity = WikidataEntity.from_dict(
-             response=response.json(), updated=datetime.now()
+             response=response.json(),
+             statements=response.json()['entities'][id].get('statements', {}),
+             updated=datetime.now()
         )
         _add_to_cache(entity)
         return entity
@@ -136,6 +144,13 @@
     response = list(
         db.get_db().query(
             'select * from wikidata where id IN ($ids)',
             vars={'ids': ids},
         )
     )
     return [
-         WikidataEntity.from_dict(response=r.data, updated=r.updated) for r in response
+         WikidataEntity.from_dict(
+             response=r.data,
+             statements=r.data.get('statements', {}),
+             updated=r.updated
+         ) for r in response
     ]
 
 def _get_from_cache(id: str) -> WikidataEntity | None:
@@ -166,6 +185,13 @@
     return None
     # Responses documented here https://doc.wikimedia.org/Wikibase/master/js/rest-api/
 
 def get_statement_values(self, property_id: str) -> list[str]:
+    """Return a list of string values for the given property."""
+    return [
+        value['value']['content']
+        for statement in self.statements.get(property_id, [])
+        if 'value' in statement and 'content' in statement['value'] and isinstance(statement['value']['content'], str)
+    ]
+
 def __repr__(self):
     return "<WikidataEntity: %s>" % repr(self.id)