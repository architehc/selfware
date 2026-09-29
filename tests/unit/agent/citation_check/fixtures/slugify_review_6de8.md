## Code Review: `slugify/` package

**Scope:** 6 files, 562 lines (tests/examples/generated excluded). All files read and verified.

---

### Findings

#### Low severity

**1. `slugify/_legacy.py:150`**

```python
excluded = [word.lower() for word in stopwords] if lowercase else stopwords
```

When `lowercase=False`, `excluded` is assigned the raw `stopwords` object. If the caller passes a generator (or any single-pass iterator), the `not in` checks on line 151 will exhaust it after the first word. Subsequent words in the split text will all pass the filter (since the generator is now empty), producing an incorrect slug. The fix is to materialize with `list(stopwords)` in both branches:

```python
excluded = [word.lower() for word in stopwords] if lowercase else list(stopwords)
```

---

**2. `slugify/special.py:18`**

```python
char_list.insert(0, upper_dict)
```

Each uppercase entry is inserted at index 0, so the final order of uppercase pairs is the **reverse** of the original lowercase list. For the fixed single-character tables (Cyrillic, German, Greek) this is functionally correct because there are no prefix overlaps. However, the in-place mutation means `_CYRILLIC` and the module-level `CYRILLIC` (which is `_CYRILLIC` after `add_uppercase_char` is called) share the same list object, and the reversal is a subtle ordering difference from the original. If future data introduces multi-character keys with prefix relationships, the reversed order could change match results.

---

### Areas with no findings

| File | Status |
|------|--------|
| `slugify/__main__.py` (112 lines) | Clean — straightforward argparse CLI dispatch |
| `slugify/slugify.py` (219 lines) | Clean — legacy/modern dispatch, entity decoding, transliteration, and truncation logic are well-structured |
| `slugify/__version__.py` (8 lines) | Clean — static metadata constants |
| `slugify/__init__.py` (10 lines) | Clean — simple re-exports |

---

### Summary

The package is well-organized with a clear separation between the modern pipeline (`slugify.py`), the frozen legacy path (`_legacy.py`), and special character tables (`special.py`). Two low-severity issues were found: a potential generator-exhaustion bug in the stopwords filter, and a reversed insertion order in the uppercase helper. Neither affects correctness for the shipped data, but the first could bite callers who pass a generator for `stopwords`.