### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
    def get_statement_values(self, property_id: str) -> list[str]:
        """Returns a list of string values for the given property."""
        values = []
        if statements := self.statements.get(property_id):
            for statement in statements:
                if value := statement.get('value'):
                    if content := value.get('content'):
                        values.append(content)
        return values
=======
    def get_statement_values(self, property_id: str) -> list[str]:
        """Returns a list of string values for the given property."""
        values = []
        if statements := self.statements.get(property_id):
            for statement in statements:
                if value := statement.get('value'):
                    if content := value.get('content'):
                        if isinstance(content, str):
                            values.append(content)
        return values
>>>>>>> REPLACE