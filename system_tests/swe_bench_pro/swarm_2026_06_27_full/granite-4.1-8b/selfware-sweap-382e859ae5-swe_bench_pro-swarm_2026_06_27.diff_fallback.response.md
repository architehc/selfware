diff --git a/openlibrary/core/models.py b/openlibrary/core/models.py
index 9f8c2e3..e3d5c1a 100644
--- a/openlibrary/core/models.py
+++ b/openlibrary/core/models.py
@@ -30,6 +30,7 @@ from openlibrary.core.wikidata import WikidataEntity, get_wikidata_entity
 from . import cache, waitinglist
 from urllib.parse import urlencode
 from pydantic import ValidationError
+from typing import List
 
 from .ia import get_metadata
 from .waitinglist import WaitingLoan
@@ -779,6 +780,15 @@ class WikidataEntity:
         return None
 
     def get_statement_values(self, property_id: str) -> List[str]:
         """Return a list of valid string values for the given property."""
         values = []
         statements = getattr(self, 'statements', {})
         if property_id not in statements:
             return values
         for stmt in statements[property_id]:
             value = stmt.get('value', {})
             content = value.get('content')
             if isinstance(content, str) and content:
                 values.append(content)
         return values
```