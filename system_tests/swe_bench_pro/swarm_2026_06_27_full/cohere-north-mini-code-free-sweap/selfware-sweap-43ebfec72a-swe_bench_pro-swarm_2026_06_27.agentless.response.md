I'll start by examining the issue and the relevant files.

The issue is about incomplete retrieval of property statement values in Wikidata entities. The `WikidataEntity` class currently stores property statements as raw data structures without a dedicated method to extract their values. The expected behavior is to add a method `get_statement_values` that takes a property identifier and returns a list of string values, preserving order, skipping invalid entries, and returning an empty list when no usable values exist.

Looking at the test file `openlibrary/tests/core/test_wikidata.py`, we see the test `test_get_statement_values`. It expects the method to exist and behave as described.

Now, let's look at the `WikidataEntity` class in `openlibrary/core/wikidata.py`. The class currently has fields: `id`, `type`, `labels`, `descriptions`, `aliases`, `statements`, `sitelinks`, `_updated`. The `statements` field is a dict mapping property IDs to a list of structured statement objects. According to the test, each statement object has a `value` key with a `content` key containing the string value.

The test shows that `statements` is a dict where each value is a list of dicts like `{'value': {'content': 'Chris-Wiggins'}}`. However, the `WikidataEntity` class definition in `wikidata.py` shows `statements: dict[str, dict]`. That seems inconsistent: the test expects a list of dicts, but the type annotation says dict. Let's examine the actual code.

I'll open the wikidata.py file to see the exact definition.