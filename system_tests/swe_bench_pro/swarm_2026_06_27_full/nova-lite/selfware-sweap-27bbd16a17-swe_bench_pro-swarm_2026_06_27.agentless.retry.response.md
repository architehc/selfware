### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
if 'general' in self:
    old_qt_version = self['general'].get('qt_version', None)
    old_qutebrowser_version = self['general'].get('version', None)
    self.qt_version_changed = old_qt_version!= qt_version
    self.qutebrowser_version_changed = (
        old_qutebrowser_version!= qutebrowser.__version__)
else:
    self.qt_version_changed = False
    self.qutebrowser_version_changed = False
=======
if 'general' in self:
    old_qt_version = self['general'].get('qt_version', None)
    old_qutebrowser_version = self['general'].get('version', None)
    self.qt_version_changed = old_qt_version!= qt_version
    self.qutebrowser_version_changed = (
        old_qutebrowser_version!= qutebrowser.__version__)
else:
    self.qt_version_changed = False
    self.qutebrowser_version_changed = False
    self.version_change = VersionChange.unknown
=======

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
=======
class StateConfig(configparser.ConfigParser):
    class VersionChange(enum.Enum):
        unknown = enum.auto()
        equal = enum.auto()
        downgrade = enum.auto()
        patch = enum.auto()
        minor = enum.auto()
        major = enum.auto()
=======

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
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
=======
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
            self.version_change = self._set_changed_attributes(
                old_qt_version, qt_version, 'qt_version_changed')
            self.version_change = self._set_changed_attributes(
                old_qutebrowser_version, qutebrowser.__version__,
                'qutebrowser_version_changed')
        else:
            self.qt_version_changed = False
            self.qutebrowser_version_changed = False
            self.version_change = VersionChange.unknown
=======

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
    def _set_changed_attributes(self, old_version: Optional[str],
                                new_version: str,
                                attr_name: str) -> VersionChange:
        if old_version is None:
            return VersionChange.unknown
        try:
            old_version_parts = list(map(int, old_version.split('.')))
            new_version_parts = list(map(int, new_version.split('.')))
        except ValueError:
            log.warn(f'Could not parse version {old_version}')
            return VersionChange.unknown

        if old_version_parts[0]!= new_version_parts[0]:
            if old_version_parts[0] < new_version_parts[0]:
                return VersionChange.major
            elif old_version_parts[0] > new_version_parts[0]:
                return VersionChange.downgrade
            else:
                return VersionChange.unknown
        elif old_version_parts[1]!= new_version_parts[1]:
            return VersionChange.minor
        elif old_version_parts[2]!= new_version_parts[2]:
            return VersionChange.patch
        else:
            return VersionChange.equal
=======
    def _set_changed_attributes(self, old_version: Optional[str],
                                new_version: str,
                                attr_name: str) -> VersionChange:
        if old_version is None:
            return VersionChange.unknown
        try:
            old_version_parts = list(map(int, old_version.split('.')))
            new_version_parts = list(map(int, new_version.split('.')))
        except ValueError:
            log.warn(f'Could not parse version {old_version}')
            return VersionChange.unknown

        if old_version_parts[0]!= new_version_parts[0]:
            if old_version_parts[0] < new_version_parts[0]:
                setattr(self, attr_name, VersionChange.major)
            elif old_version_parts[0] > new_version_parts[0]:
                setattr(self, attr_name, VersionChange.downgrade)
            else:
                return VersionChange.unknown
        elif old_version_parts[1]!= new_version_parts[1]:
            setattr(self, attr_name, VersionChange.minor)
        elif old_version_parts[2]!= new_version_parts[2]:
            setattr(self, attr_name, VersionChange.patch)
        else:
            setattr(self, attr_name, VersionChange.equal)
        return getattr(self, attr_name)
=======

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
    def _save(self) -> None:
        """Save the state file to the configured location."""
        with open(self._filename, 'w', encoding='utf-8') as f:
            self.write(f)
=======
    def _save(self) -> None:
        """Save the state file to the configured location."""
        with open(self._filename, 'w', encoding='utf-8') as f:
            self.write(f)
        if self.qutebrowser_version_changed in (VersionChange.minor, VersionChange.major):
            message.show_changelog()
=======

### FILE: qutebrowser/app.py
<<<<<<< SEARCH
    if args.version:
        print(version.version_info())
        sys.exit(usertypes.Exit.ok)
=======
    if args.version:
        print(version.version_info())
        if config.val['changelog_after_upgrade'] == 'true':
            message.show_changelog()
        sys.exit(usertypes.Exit.ok)
=======
