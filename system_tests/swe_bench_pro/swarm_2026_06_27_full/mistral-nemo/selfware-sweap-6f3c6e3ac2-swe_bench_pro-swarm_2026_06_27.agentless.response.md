### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
    def get_statement_values(self, property: str) -> list[str]:
        statements = self.statements.get(property)
        if not statements:
            return []

        values = []
        for statement in statements:
            value = statement.get('value')
            if value and 'content' in value:
                values.append(value['content'])

        return values
=======
    def get_statement_values(self, property: str) -> list[str]:
        statements = self.statements.get(property)
        if not statements:
            return []

        values = []
        for statement in statements:
            value = statement.get('value')
            if value and 'content' in value and isinstance(value['content'], str):
                values.append(value['content'])

        return values
>>>>>>> REPLACE