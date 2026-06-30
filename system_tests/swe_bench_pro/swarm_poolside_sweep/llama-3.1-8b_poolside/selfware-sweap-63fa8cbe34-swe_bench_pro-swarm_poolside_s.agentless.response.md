### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    #...
    def __init__(self) -> None:
        #...
        if 'general' in self:
            old_qt_version = self['general'].get('qt_version', None)
            old_qutebrowser_version = self['general'].get('version', None)
            self.qt_version_changed = old_qt_version!= qVersion()
            self.qutebrowser_version_changed = (
                old_qutebrowser_version!= qutebrowser.__version__)
        else:
            self.qt_version_changed = False
            self.qutebrowser_version_changed = False
    #...
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    #...
    def _set_changed_attributes(self) -> None:
        #...
        self.qutebrowser_version_changed = (
            self['general'].get('version', None)!= qutebrowser.__version__)
    #...
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    #...
    def _set_changed_attributes(self) -> None:
        #...
        try:
            old_version = self['general'].get('version', None)
            self.qutebrowser_version_changed = (
                old_version!= qutebrowser.__version__)
        except configparser.NoSectionError:
            self.qutebrowser_version_changed = False
    #...
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    #...
    def _set_changed_attributes(self) -> None:
        #...
        try:
            old_version = self['general'].get('version', None)
            if old_version is None:
                self.qutebrowser_version_changed = configfiles.VersionChange.unknown
            elif old_version == qutebrowser.__version__:
                self.qutebrowser_version_changed = configfiles.VersionChange.equal
            elif qutebrowser.__version__ < old_version:
                self.qutebrowser_version_changed = configfiles.VersionChange.downgrade
            elif qutebrowser.__version__[0] == old_version[0] and \
                 qutebrowser.__version__[1] == old_version[1]:
                self.qutebrowser_version_changed = configfiles.VersionChange.patch
            elif qutebrowser.__version__[0] == old_version[0] and \
                 qutebrowser.__version__[1]!= old_version[1]:
                self.qutebrowser_version_changed = configfiles.VersionChange.minor
            else:
                self.qutebrowser_version_changed = configfiles.VersionChange.major
        except configparser.NoSectionError:
            self.qutebrowser_version_changed = False
    #...
>>>>>>> REPLACE