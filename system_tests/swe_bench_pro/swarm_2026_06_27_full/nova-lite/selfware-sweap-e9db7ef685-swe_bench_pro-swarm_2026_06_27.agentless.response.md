### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
    def get_statement_values(self, property_id: str) -> list[str]:
=======
    def get_statement_values(self, property_id: str) -> list[str]:
        statements = self.statements.get(property_id, [])
        return [
            statement['value']['content']
            for statement in statements
            if 'value' in statement and 'content' in statement['value']
        ]
>>>>>>> REPLACE