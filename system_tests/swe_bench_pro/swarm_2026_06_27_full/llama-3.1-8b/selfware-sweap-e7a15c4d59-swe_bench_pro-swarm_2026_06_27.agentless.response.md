### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    #...
    def _set_changed_attributes(self) -> None:
        #...
        self.qutebrowser_version_changed = (
            old_qutebrowser_version!= qutebrowser.__version__)
    #...
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    #...
    def _set_changed_attributes(self) -> None:
        #...
        self.qutebrowser_version_changed = (
            old_qutebrowser_version!= qutebrowser.__version__)
        #...
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    #...
    def _set_changed_attributes(self) -> None:
        #...
        if old_qutebrowser_version is None:
            self.qutebrowser_version_changed = VersionChange.unknown
        else:
            self.qutebrowser_version_changed = (
                old_qutebrowser_version!= qutebrowser.__version__)
    #...
>>>>>>> REPLACE