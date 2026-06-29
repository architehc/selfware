### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    # ...
    def _set_changed_attributes(self):
        # ...
        self.qutebrowser_version_changed = VersionChange.unknown
        # ...
=======
class StateConfig(configparser.ConfigParser):
    # ...

    def _set_changed_attributes(self):
        # ...
        old_version = self._old_version
        if old_version is None:
            self.qutebrowser_version_changed = VersionChange.unknown
            return

        try:
            old_parts = list(map(int, old_version.split('.')))
            new_parts = list(map(int, qutebrowser.__version__.split('.')))
        except ValueError:
            log.init.warning("Unable to parse old version %s", old_version)
            self.qutebrowser_version_changed = VersionChange.unknown
            return

        self.qutebrowser_version_changed = VersionChange(
            old_parts, new_parts)
>>>>>>> REPLACE

### FILE: tests/unit/config/test_configfiles.py
<<<<<<< SEARCH
def test_qutebrowser_version_changed(state_writer, monkeypatch,
        old_version, expected):
    # ...
    state = configfiles.StateConfig()
    state.init_save_manager(fake_save_manager)
    state._old_version = old_version
    state._set_changed_attributes()
    assert state.qutebrowser_version_changed == expected
    # ...
=======
def test_qutebrowser_version_changed(state_writer, monkeypatch,
        old_version, expected):
    # ...
    state = configfiles.StateConfig()
    state.init_save_manager(fake_save_manager)
    state._old_version = old_version
    state._set_changed_attributes()
    assert state.qutebrowser_version_changed == expected
    # ...

    # Add new tests for the new VersionChange enum values
    tests = [
        ('1.14.1', '2.0.0', configfiles.VersionChange.major),
        ('1.14.1', '2.1.0', configfiles.VersionChange.major),
        ('1.14.1', '2.0.1', configfiles.VersionChange.major),
        ('1.14.1', '2.1.1', configfiles.VersionChange.major),
        ('2.1.1', '1.14.1', configfiles.VersionChange.downgrade),
        ('2.0.0', '1.14.1', configfiles.VersionChange.downgrade),
    ]
    for old_version, new_version, expected in tests:
        state._old_version = old_version
        state._new_version = new_version
        state._set_changed_attributes()
        assert state.qutebrowser_version_changed == expected
>>>>>>> REPLACE