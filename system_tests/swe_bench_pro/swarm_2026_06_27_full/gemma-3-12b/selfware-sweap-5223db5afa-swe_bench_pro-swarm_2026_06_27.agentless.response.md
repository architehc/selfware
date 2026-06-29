### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
        self.qt_version_changed = old_qt_version != qt_version
        self.qutebrowser_version_changed = (
            old_qutebrowser_version != qutebrowser.__version__)

        for sect in ['general', 'geometry', 'inspector']:
            try:
                self.add_section(sect)
            except configparser.DuplicateSectionError:
                pass

        deleted_keys = [
            ('general', 'fooled'),
            ('general', 'backend-warning-shown'),
            ('general', 'old-qt-warning-shown'),
            ('geometry', 'inspector'),
        ]
        for sect, key in deleted_keys:
            self[sect].pop(key, None)

        self['general']['qt_version'] = qt_version
        self['general']['version'] = qutebrowser.__version__
=======
        self.qt_version_changed = old_qt_version != qt_version
        self.qutebrowser_version_changed = (
            old_qutebrowser_version != qutebrowser.__version__)

        for sect in ['general', 'geometry', 'inspector']:
            try:
                self.add_section(sect)
            except configparser.DuplicateSectionError:
                pass

        deleted_keys = [
            ('general', 'fooled'),
            ('general', 'backend-warning-shown'),
            ('general', 'old-qt-warning-shown'),
            ('geometry', 'inspector'),
        ]
        for sect, key in deleted_keys:
            self[sect].pop(key, None)

        self['general']['qt_version'] = qt_version
        self['general']['version'] = qutebrowser.__version__

    def _set_changed_attributes(self):
        """Set the qt_version_changed and qutebrowser_version_changed attributes."""
        old_qt_version = self['general'].get('qt_version', None)
        old_qutebrowser_version = self['general'].get('version', None)
        qt_version = qVersion()

        try:
            if old_qutebrowser_version is None:
                self.qutebrowser_version_changed = True
            else:
                self.qutebrowser_version_changed = (
                    VersionChange.matches_filter(
                        config.get('changelog_after_upgrade', 'never'))
                    and old_qutebrowser_version != qutebrowser.__version__)
        except Exception:
            log.warning("Could not compare qutebrowser versions.")
            self.qutebrowser_version_changed = VersionChange.unknown

        try:
            if old_qt_version is None:
                self.qt_version_changed = True
            else:
                self.qt_version_changed = (
                    VersionChange.matches_filter(
                        config.get('changelog_after_upgrade', 'never'))
                    and old_qt_version != qt_version)
        except Exception:
            log.warning("Could not compare Qt versions.")
            self.qt_version_changed = False

    VersionChange = Enum('VersionChange', ['unknown', 'equal', 'downgrade',
                                          'patch', 'minor', 'major'])

    @staticmethod
    def matches_filter(filterstr: str) -> bool:
        """Return whether the version change matches a given `changelog_after_upgrade`
        filter value.
        """
        if filterstr == 'never':
            return False
        if filterstr == 'major':
            return VersionChange.major in config.get('changelog_after_upgrade', 'never')
        if filterstr == 'minor':
            return VersionChange.minor in config.get('changelog_after_upgrade', 'never')
        if filterstr == 'patch':
            return VersionChange.patch in config.get('changelog_after_upgrade', 'never')
        return True
>>>>>>> REPLACE

### FILE: tests/unit/config/test_configfiles.py
<<<<<<< SEARCH
def test_qutebrowser_version_unparsable(state_writer, monkeypatch, caplog):
    state_writer('version', 'blabla')

    with caplog.at_level(logging.WARNING):
        state = configfiles.StateConfig()

    assert caplog.messages == ['Unable to parse old version blabla']
    assert state.qutebrowser_version_changed == configfiles.VersionChange.unknown


@pytest.mark.parametrize('value, filterstr, matches', [
    (configfiles.VersionChange.major, 'never', False),
    (configfiles.VersionChange.minor, 'never', False),
    (configfiles.VersionChange.patch, 'never', False),

    (configfiles.VersionChange.major, 'major', True),
    (configfiles.VersionChange.minor, 'major', False),
    (configfiles.VersionChange.patch, 'major', False),

    (configfiles.VersionChange.major, 'minor', False),
    (configfiles.VersionChange.minor, 'minor', True),
    (configfiles.VersionChange.patch, 'minor', False),

    (configfiles.VersionChange.major, 'patch', False),
    (configfiles.VersionChange.minor, 'patch', True),
    (configfiles.VersionChange.patch, 'patch', True),
])
def test_version_change_filter(state_writer, monkeypatch, value,
                               filterstr, matches):
    state_writer('version', '1.14.1')
    state = configfiles.StateConfig()
    assert state.matches_filter(filterstr) == matches
=======
def test_qutebrowser_version_unparsable(state_writer, monkeypatch, caplog):
    state_writer('version', 'blabla')

    with caplog.at_level(logging.WARNING):
        state = configfiles.StateConfig()

    assert caplog.messages == ['Unable to parse old version blabla']
    assert state.qutebrowser_version_changed == configfiles.VersionChange.unknown


@pytest.mark.parametrize('value, filterstr, matches', [
    (configfiles.VersionChange.major, 'never', False),
    (configfiles.VersionChange.minor, 'never', False),
    (configfiles.VersionChange.patch, 'never', False),

    (configfiles.VersionChange.major, 'major', True),
    (configfiles.VersionChange.minor, 'major', False),
    (configfiles.VersionChange.patch, 'major', False),

    (configfiles.VersionChange.major, 'minor', False),
    (configfiles.VersionChange.minor, 'minor', True),
    (configfiles.VersionChange.patch, 'minor', False),

    (configfiles.VersionChange.major, 'patch', False),
    (configfiles.VersionChange.minor, 'patch', True),
    (configfiles.VersionChange.patch, 'patch', True),
])
def test_version_change_filter(state_writer, monkeypatch, value,
                               filterstr, matches):
    state_writer('version', '1.14.1')
    state = configfiles.StateConfig()
    assert state.VersionChange.matches_filter(filterstr) == matches
>>>>>>> REPLACE
