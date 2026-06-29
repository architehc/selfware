### FILE: openlibrary/core/models.py
<<<<<<< SEARCH
from openlibrary.core.wikidata import WikidataEntity, get_wikidata_entity
=======
from openlibrary.core.wikidata import WikidataEntity, get_wikidata_entity


def get_statement_values(self, property_id: str) -> list[str]:
    """
    Retrieves a list of string values associated with a given property identifier.

    Args:
        property_id: The identifier of the property.

    Returns:
        A list of string values associated with the property.
        Returns an empty list if the property is not found or if no valid values exist.
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