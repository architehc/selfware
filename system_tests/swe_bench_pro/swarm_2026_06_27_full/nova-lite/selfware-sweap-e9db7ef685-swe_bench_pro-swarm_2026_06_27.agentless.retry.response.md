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
        Get the list of valid values contained in the statements for the given property.
        Preserves order, skips invalid entries, and returns an empty list when no usable values exist.
        """
        values = []
        if property_id in self.statements:
            for statement in self.statements[property_id]:
                value = statement.get('value', {})
                content = value.get('content', '')
                if isinstance(content, str) and content:
                    values.append(content)
        return values
>>>>>>> REPLACE