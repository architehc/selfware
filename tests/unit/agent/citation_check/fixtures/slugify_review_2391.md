## Review: `slugify/` package

All six files in the reading plan have been read (562 lines total). Below are the verified findings grouped by severity.

---

### Medium

**`slugify/_legacy.py:150`**
```python
excluded = [word.lower() for word in stopwords] if lowercase else stopwords
```
When `lowercase=False` and `stopwords` is a one-shot generator (e.g. `iter([...])`), the list comprehension on the left side of the ternary is skipped, so `excluded` is bound to the generator itself. The subsequent membership check (`if word in excluded` on line 151) consumes the generator on the first call; after that the generator is exhausted and every remaining word passes the filter unconditionally. This silently produces wrong output for multi-word inputs when a caller supplies a generator with `lowercase=False`.

---

### Low

**`slugify/_legacy.py:21`**
```python
HEX_PATTERN = re.compile(r'&#x([\da-fA-F]+);')
```
The character class for the prefix is the literal `x` only, so the uppercase form `&#X41;` is not matched. The modern path (`slugify/slugify.py`) uses `[xX]` in its equivalent pattern. In the legacy path this means uppercase-prefixed hex character references pass through undecoded, producing a slightly different slug than the modern path would for the same input. (The module is frozen, so this is a known discrepancy rather than a fixable bug.)

**`slugify/__main__.py:53`**
```python
if args.input_string and args.stdin:
```
`args.input_string` is a list (from `nargs='*'`). An empty list `[]` is falsy, so the mutual-exclusivity guard is correctly skipped when only `--stdin` is supplied. However, if the user passes a single empty-string positional argument (`slugify "" --stdin`), `args.input_string` becomes `['']`—a truthy list—while `args.stdin` is `True`, and the parser correctly errors. The subtle point: because `['']` is truthy but the *content* is empty, the check fires even though conceptually "no input string was given." This is a minor inconsistency in how "was a positional argument supplied?" is determined versus how the actual value is used downstream.

---

### Areas with no findings

- **`slugify/slugify.py`** (219 lines) – modern pipeline, entity decoding, normalization, transliteration dispatch, and `smart_truncate` logic all appear internally consistent. No issues found.
- **`slugify/special.py`** (55 lines) – replacement-pair tables and the `add_uppercase` helper are straightforward data; no issues found.
- **`slugify/__version__.py`** (8 lines) – static metadata constants; no issues found.
- **`slugify/__init__.py`** (10 lines) – re-export shim; no issues found.

---

**Summary:** One medium-severity generator-exhaustion bug in the legacy stopword filter, and two low-severity observations (a case-sensitivity gap in the legacy hex pattern, and a minor truthiness nuance in the CLI mutual-exclusivity check). The modern path, special-replacement tables, version metadata, and package init are clean.