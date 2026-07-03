### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
    self.qutebrowser_version_changed = (
        old_qutebrowser_version != qutebrowser.__version__)
=======
    self.qutebrowser_version_changed = (
        old_qutebrowser_version != qutebrowser.__version__
        and config.val.changelog_after_upgrade.matches_filter('major')
        or config.val.changelog_after_upgrade.matches_filter('minor')
    )
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
=======
class StateConfig(configparser.ConfigParser):
    def _set_changed_attributes(self):
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
        self.qutebrowser_version_changed = (
            old_qutebrowser_version != qutebrowser.__version__)
=======
        self.qutebrowser_version_changed = (
            old_qutebrowser_version != qutebrowser.__version__
            and config.val.changelog_after_upgrade.matches_filter('major')
            or config.val.changelog_after_upgrade.matches_filter('minor')
        )
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class VersionChange(Enum):
=======
class VersionChange(Enum):
    unknown = auto()
    equal = auto()
    downgrade = auto()
    patch = auto()
    minor = auto()
    major = auto()
    
    def matches_filter(self, filterstr: str) -> bool:
        return filterstr in (self.name, 'never')
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
=======
class StateConfig(configparser.ConfigParser):
    def _set_changed_attributes(self):
        old_qt_version = self['general'].get('qt_version', None)
        old_qutebrowser_version = self['general'].get('version', None)
        qt_version = qVersion()
        qutebrowser_version = qutebrowser.__version__
        
        self.qt_version_changed = old_qt_version != qt_version
        self.qutebrowser_version_changed = (
            old_qutebrowser_version != qutebrowser_version
            and config.val.changelog_after_upgrade.matches_filter('major')
            or config.val.changelog_after_upgrade.matches_filter('minor')
        )
        self['general']['qt_version'] = qt_version
        self['general']['version'] = qutebrowser_version
>>>>>>> REPLACE