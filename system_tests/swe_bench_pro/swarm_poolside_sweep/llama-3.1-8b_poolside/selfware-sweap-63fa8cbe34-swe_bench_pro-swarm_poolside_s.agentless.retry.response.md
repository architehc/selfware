### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    """The "state" file saving various application state."""
    def __init__(self) -> None:
        super().__init__()
        self._filename = os.path.join(standarddir.data(), 'state')
        self.read(self._filename, encoding='utf-8')
        qt_version = qVersion()

        # We handle this here, so we can avoid setting qt_version_changed if
        # the config is brand new, but can still set it when qt_version wasn't
        # there before...
        if 'general' in self:
            old_qt_version = self['general'].get('qt_version', None)
            old_qutebrowser_version = self['general'].get('version', None)
            self.qt_version_changed = old_qt_version!= qt_version
            self.qutebrowser_version_changed = (
                old_qutebrowser_version!= qutebrowser.__version__)
        else:
            self.qt_version_changed = False
            self.qutebrowser_version_changed = False

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
class StateConfig(configparser.ConfigParser):
    """The "state" file saving various application state."""
    def __init__(self) -> None:
        super().__init__()
        self._filename = os.path.join(standarddir.data(), 'state')
        self.read(self._filename, encoding='utf-8')
        qt_version = qVersion()

        # We handle this here, so we can avoid setting qt_version_changed if
        # the config is brand new, but can still set it when qt_version wasn't
        # there before...
        if 'general' in self:
            old_qt_version = self['general'].get('qt_version', None)
            old_qutebrowser_version = self['general'].get('version', None)
            self.qt_version_changed = old_qt_version!= qt_version
            self.qutebrowser_version_changed = (
                old_qutebrowser_version!= qutebrowser.__version__)
        else:
            self.qt_version_changed = False
            self.qutebrowser_version_changed = False

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

        self._set_changed_attributes()

    def _set_changed_attributes(self) -> None:
        """Set qt_version_changed/qutebrowser_version_changed attributes."""
        old_qutebrowser_version = self['general'].get('version', None)
        if old_qutebrowser_version is None:
            self.qutebrowser_version_changed = VersionChange.unknown
            return

        try:
            old_version = VersionChange(old_qutebrowser_version)
        except ValueError:
            log.init.warning("Failed to parse old version: {}".format(
                old_qutebrowser_version))
            self.qutebrowser_version_changed = VersionChange.unknown
            return

        self.qutebrowser_version_changed = old_version
>>>>>>> REPLACE