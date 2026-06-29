### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    return []
>>>>>>> REPLACE
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    return []
>>>>>>> REPLACE
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    return []
>>>>>>> REPLACE
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    return []
>>>>>>> REPLACE
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    return []
>>>>>>> REPLACE
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    return []
>>>>>>> REPLACE
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    return []
>>>>>>> REPLACE
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    return []
>>>>>>> REPLACE
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, prop_id):
    if prop_id not in self.statements:
        return []
    return [s['value']['content'] for s in self.statements[prop_id] if 'value' in s and 'content' in s['value'] and s['value']['content']]
>>>>>>> REPLACE