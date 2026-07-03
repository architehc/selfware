### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
class WikidataEntity:
    """This is the model of the api response from WikiData plus the updated field
    https://www.wikidata.org/wiki/Wikidata:REST_API
    """
    id: str
    type: str
    labels: dict[str, str]
    descriptions: dict[str, str]
    aliases: dict[str, list[str]]
    statements: dict[str, dict]
    sitelinks: dict[str, dict]
    _updated: datetime  # This is when we fetched the data, not when the entity was changed in Wikidata
=======
class WikidataEntity:
    """This is the model of the api response from WikiData plus the updated field
    https://www.wikidata.org/wiki/Wikidata:REST_API
    """
    id: str
    type: str
    labels: dict[str, str]
    descriptions: dict[str, str]
    aliases: dict[str, list[str]]
    statements: dict[str, dict]
    sitelinks: dict[str, dict]
    _updated: datetime  # This is when we fetched the data, not when the entity was changed in Wikidata

    def get_statement_values(self, property_id: str) -> list[str]:
        """Return a list of valid values for a given property, preserving order."""
        values = []
        statements = self.statements.get(property_id, [])
        for stmt in statements:
            value_obj = stmt.get('value', {})
            content = value_obj.get('content')
            if isinstance(content, str) and content:
                values.append(content)
        return values
>>>>>>> REPLACE