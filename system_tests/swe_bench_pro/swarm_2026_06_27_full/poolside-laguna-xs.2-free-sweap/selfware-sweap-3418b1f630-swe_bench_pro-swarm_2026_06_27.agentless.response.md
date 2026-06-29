
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
        Get all valid string values from statements for a given property ID.
        
        Args:
            property_id: The Wikidata property identifier (e.g., 'P2038')
            
        Returns:
            A list of string values from the statements, preserving order.
            Returns an empty list if the property is absent or no valid values exist.
        """
        if property_id not in self.statements:
            return []
        
        values = []
        for statement in self.statements[property_id]:
            try:
                content = statement['value']['content']
                if isinstance(content, str) and content:
                    values.append(content)
            except (KeyError, TypeError):
                continue
        return values
>>>>>>> REPLACE
