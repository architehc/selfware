### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
class WikidataEntity:
    def get_statement_values(self, property_id: str) -> list[str]:
        """Returns a list of valid values for a given property."""
        statements = self.statements.get(property_id, {})
        values = []
        for statement in statements:
            if 'value' in statement and 'content' in statement['value']:
                value = statement['value']['content']
                if isinstance(value, str) and value:
                    values.append(value)
        return values
>>>>>>> REPLACE