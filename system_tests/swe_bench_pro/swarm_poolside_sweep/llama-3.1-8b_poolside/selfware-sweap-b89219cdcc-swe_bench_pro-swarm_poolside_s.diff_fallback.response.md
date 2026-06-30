diff --git a/openlibrary/core/wikidata.py b/openlibrary/core/wikidata.py
index 3a3a4f3..d3d3d3d 100644
--- a/openlibrary/core/wikidata.py
+++ b/openlibrary/core/wikidata.py
@@ -29,6 +29,7 @@
 from openlibrary.core.helpers import private_collection_in
 from openlibrary.core.imports import ImportItem
 from openlibrary.core.observations import Observations
+from openlibrary.core.models import WikidataEntity

 logger = logging.getLogger("core.wikidata")

@@ -108,6 +109,7 @@
 def _get_from_web(id: str) -> WikidataEntity | None:
     response = requests.get(f'{WIKIDATA_API_URL}{id}')
     if response.status_code == 200:
+        entity = WikidataEntity.from_dict(response=response.json(), updated=datetime.now())
         entity = WikidataEntity.from_dict(
             response=response.json(), updated=datetime.now()
         )
         _add_to_cache(entity)
@@ -123,6 +125,7 @@
 def _get_from_cache_by_ids(ids: list[str]) -> list[WikidataEntity]:
     response = list(
         db.get_db().query(
@@ -132,6 +135,7 @@
     )
     return [
         WikidataEntity.from_dict(response=r.data, updated=r.updated) for r in response
     ]

+class WikidataEntity:
+    def get_statement_values(self, property_id: str) -> list[str]:
+        if property_id not in self.statements:
+            return []
+        statements = self.statements[property_id]
+        values = []
+        for statement in statements:
+            if 'value' in statement and 'content' in statement['value']:
+                values.append(statement['value']['content'])
+        return values

diff --git a/openlibrary/core/models.py b/openlibrary/core/models.py
index 3a3a4f3..d3d3d3d 100644
--- a/openlibrary/core/models.py
+++ b/openlibrary/core/models.py
@@ -32,6 +32,7 @@
 from openlibrary.catalog import add_book
 from openlibrary.core.booknotes import Booknotes
 from openlibrary.core.bookshelves import Bookshelves
+from openlibrary.core.wikidata import WikidataEntity

 from openlibrary.config import load_config
 from openlibrary.core.helpers import private_collection_in
@@ -769,6 +770,7 @@
 class Author:
     def url(self, suffix="", **params):
         return self.get_url(suffix, **params)

     def get_url_suffix(self):
         return self.name or "unnamed"

     def wikidata(
         self, bust_cache: bool = False, fetch_missing: bool = False
     ) -> WikidataEntity | None:
         return None
         if wd_id := self.remote_ids.get("wikidata"):
             return get_wikidata_entity(
                 qid=wd_id, bust_cache=bust_cache, fetch_missing=fetch_missing
             )
         return None

     def __repr__(self):
         return "<Author: %s>" % repr(self.key)