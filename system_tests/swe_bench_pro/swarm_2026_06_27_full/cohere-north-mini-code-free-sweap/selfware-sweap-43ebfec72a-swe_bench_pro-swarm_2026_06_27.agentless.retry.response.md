```python
### FILE: openlibrary/core/wikidata.py
<<<<<<< SEARCH
    def get_wikipedia_link(self, language: str = 'en') -> tuple[str, str] | None:
        """
        Get the Wikipedia URL and language for a given language code.
        Falls back to English if requested language is unavailable.
        """
        requested_wiki = f'{language}wiki'
        english_wiki = 'enwiki'

        if requested_wiki in self.sitelinks:
            return self.sitelinks[requested_wiki]['url'], language
        elif english_wiki in self.sitelinks:
            return self.sitelinks[english_wiki]['url'], 'en'
        return None

    @classmethod
    def from_dict(cls, response: dict, updated: datetime):
        return cls(
            **response,
            _updated=updated,
        )

    def to_wikidata_api_json_format(self) -> str:
        """
        Transforms the dataclass a JSON string like we get from the Wikidata API.
        This is used for storing the json in the database.
        """
        entity_dict = {
            'id': self.id,
            'type': self.type,
            'labels': self.labels,
            'descriptions': self.descriptions,
            'aliases': self.aliases,
            'statements': self.statements,
            'sitelinks': self.sitelinks,
        }
        return json.dumps(entity_dict)
=======
    def get_wikipedia_link(self, language: str = 'en') -> tuple[str, str] | None:
        """
        Get the Wikipedia URL and language for a given language code.
        Falls back to English if requested language is unavailable.
        """
        requested_wiki = f'{language}wiki'
        english_wiki = 'enwiki'

        if requested_wiki in self.sitelinks:
            return self.sitelinks[requested_wiki]['url'], language
        elif english_wiki in self.sitelinks:
            return self.sitelinks[english_wiki]['url'], 'en'
        return None

    def get_statement_values(self, property_id: str) -> list[str]:
        """
        Get the list of valid string values for a given property ID from the statements.
        Preserves order, skips invalid entries, returns empty list if property absent or no valid values.
        """
        if property_id not in self.statements:
            return []
        
        values = []
        for statement in self.statements[property_id]:
            # Each statement is a dict that may contain a 'value' key
            if 'value' not in statement:
                continue
            value = statement['value']
            # The value may be a dict with a 'content' key
            if not isinstance(value, dict) or 'content' not in value:
                continue
            content = value['content']
            # Only include non-empty strings
            if isinstance(content, str) and content:
                values.append(content)
        return values

    @classmethod
    def from_dict(cls, response: dict, updated: datetime):
        return cls(
            **response,
            _updated=updated,
        )

    def to_wikidata_api_json_format(self) -> str:
        """
        Transforms the dataclass a JSON string like we get from the Wikidata API.
        This is used for storing the json in the database.
        """
        entity_dict = {
            'id': self.id,
            'type': self.type,
            'labels': self.labels,
            'descriptions': self.descriptions,
            'aliases': self.aliases,
            'statements': self.statements,
            'sitelinks': self.sitelinks,
        }
        return json.dumps(entity_dict)
>>>>>>> REPLACE
```