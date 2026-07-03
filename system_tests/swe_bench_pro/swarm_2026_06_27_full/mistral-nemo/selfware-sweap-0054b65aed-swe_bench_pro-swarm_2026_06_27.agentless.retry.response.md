### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
    def _set_changed_attributes(self) -> None:
        # ...
        self.qutebrowser_version_changed = VersionChange.unknown
        # ...
>>>>>>> REPLACE
        self.qutebrowser_version_changed = VersionChange.unknown
        # ...
        if not self._old_qutebrowser_version:
            self._old_qutebrowser_version = self['general'].get('version', None)
        if self._old_qutebrowser_version is not None:
            try:
                old_version = utils.parse_version(self._old_qutebrowser_version)
                new_version = utils.parse_version(qutebrowser.__version__)
                if old_version == new_version:
                    self.qutebrowser_version_changed = VersionChange.equal
                elif old_version < new_version:
                    self.qutebrowser_version_changed = VersionChange.downgrade
                elif old_version > new_version:
                    self.qutebrowser_version_changed = VersionChange.patch
                elif old_version.major == new_version.major and old_version.minor < new_version.minor:
                    self.qutebrowser_version_changed = VersionChange.minor
                elif old_version.major < new_version.major:
                    self.qutebrowser_version_changed = VersionChange.major
            except ValueError:
                log.init.warning("Could not parse old qutebrowser version: {}"
                                 .format(self._old_qutebrowser_version))
                self.qutebrowser_version_changed = VersionChange.unknown
        else:
            self.qutebrowser_version_changed = VersionChange.unknown
<<<<<<