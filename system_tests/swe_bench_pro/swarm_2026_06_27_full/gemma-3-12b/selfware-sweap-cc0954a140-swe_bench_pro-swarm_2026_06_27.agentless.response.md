### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
    def get_statement_values(self, property_id: str) -> list[str]:
        """
        Returns a list of string values for a given property ID.
        """
        values = []
        if property_id in self.statements:
            for statement in self.statements[property_id]:
                if 'value' in statement and 'content' in statement['value']:
                    content = statement['value']['content']
                    if isinstance(content, str) and content:
                        values.append(content)
        return values
=======
    def get_statement_values(self, property_id: str) -> list[str]:
        """
        Returns a list of string values for a given property ID.
        """
        values = []
        if property_id in self.statements:
            for statement in self.statements[property_id]:
                if 'value' in statement and 'content' in statement['value']:
                    content = statement['value']['content']
                    if isinstance(content, str) and content:
                        values.append(content)
        return values
>>>>>>> REPLACE