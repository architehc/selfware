I'll start by understanding the issue and the requirements.

The issue is that the changelog appears after all upgrades (patch, minor, major) regardless of type, and there's no configuration to control this. The expected behavior is that the changelog should appear only after meaningful upgrades (minor or major releases), and users should be able to configure this behavior.

Requirements:
1. In `qutebrowser/config/configfiles.py`, introduce a new enumeration class `VersionChange` with values: `unknown`, `equal`, `downgrade`, `patch`, `minor`, `major`.
2. The `VersionChange` enum should provide a `matches_filter(filterstr: str) -> bool` method that returns whether the version change matches a given `changelog_after_upgrade` filter value.
3. In `qutebrowser/config/configfiles.py`, the `StateConfig` class should determine version changes via a new private method `_set_changed_attributes`.
4. Define `StateConfig._set_changed_attributes` to set `qt_version_changed`/`qutebrowser_version_changed` attributes.
5. In `_set_changed_attributes`, set `self.qutebrowser_version_changed` to a `VersionChange` value by comparing the old stored version against the current `qutebrowser.__version__`. Distinguish between:
   - `equal` (same version)
   - `downgrade` (new version lower than old)
   - `patch` (only patch number differs)
   - `minor` (same major, different minor)
   - `major` (different major version)
   - `unknown` (unparsable or missing version)
6. In `StateConfig._set_changed_attributes`, if the old version cannot be parsed, log a warning and set `self.qutebrowser_version_changed` to `VersionChange.unknown`.

Looking at the provided source snippets:

1. `qutebrowser/config/configfiles.py` lines 59-83 show the `__init__` method of `StateConfig`. Currently, it sets `self.qt_version_changed` and `self.qutebrowser_version_changed` as booleans based on simple inequality checks.

2. `tests/unit/config/test_configfiles.py` lines 165-189 show tests for `qt_version_changed` and `qutebrowser_version_changed`. These tests currently expect boolean values.

3. `qutebrowser/app.py` lines 377-401 show where the changelog is displayed. It checks `configfiles.state.qutebrowser_version_changed` (currently a boolean) and `config.val.changelog_after_upgrade`.

4. `doc/changelog.asciidoc` and `doc/help/settings.asciidoc` mention the `changelog_after_upgrade` setting.

The problem is that currently `qutebrowser_version_changed` is a boolean, but we need it to be a `VersionChange` enum that can distinguish between different types of version changes. Also, the `changelog_after_upgrade` setting needs to be able to filter based on the `VersionChange` enum.

Let me first check what the current `changelog_after_upgrade` setting looks like. From the settings documentation, it seems to be a boolean setting. But according to the requirements, it should be a filter that can match against `VersionChange` values.

Let me look at the actual implementation in `configfiles.py` to understand the current structure better.