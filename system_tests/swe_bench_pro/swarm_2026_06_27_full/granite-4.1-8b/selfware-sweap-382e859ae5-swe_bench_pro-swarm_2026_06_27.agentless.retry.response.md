### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
def get_statement_values(self, property_id: str) -> list[str]:
    """Iterate over the statement objects of the requested property and collect the string in `value.content`, preserving the original order and skipping items missing the expected nested fields or with non-string/empty `content`."""
    values = []
    if property_id in self.statements:
        for statement in self.statements[property_id]:
            value = statement.get('value', {})
            content = value.get('content')
            if isinstance(content, str) and content:
                values.append(content)
    return values
=======
def get_statement_values(self, property_id: str) -> list[str]:
    """Iterate over the statement objects of the requested property and collect the string in `value.content`, preserving the original order and skipping items missing the expected nested fields or with non-string/empty `content`."""
    values = []
    if property_id in self.statements:
        for statement in self.statements[property_id]:
            value = statement.get('value', {})
            content = value.get('content')
            if isinstance(content, str) and content:
                values.append(content)
    return values
>>>>>>> REPLACE