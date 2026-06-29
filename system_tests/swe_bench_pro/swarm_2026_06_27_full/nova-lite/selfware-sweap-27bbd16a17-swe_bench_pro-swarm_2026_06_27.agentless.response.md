### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class StateConfig(configparser.ConfigParser):
    def __init__(self) -> None:
        super().__init__()
        self._filename = os.path.join(standarddir.data(), 'state')
        self.read(self._filename, encoding='utf-8')
        qt_version = qVersion()

        self.qt_version_changed = old_qt_version!= qt_version
        self.qutebrowser_version_changed = (
            old_qutebrowser_version!= qutebrowser.__version__)
=======
class StateConfig(configparser.ConfigParser):
    def __init__(self) -> None:
        super().__init__()
        self._filename = os.path.join(standarddir.data(), 'state')
        self.read(self._filename, encoding='utf-8')
        qt_version = qVersion()

        self._set_changed_attributes('qt_version', qt_version, 'general')
        self._set_changed_attributes('version', qutebrowser.__version__, 'general')
=======

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
    def _set_changed_attributes(self, attr_name: str, current_version: str, section: str) -> None:
        old_version = self[section].get(attr_name, None)
        if old_version is None:
            self[section][attr_name] = current_version
            setattr(self, f'{attr_name}_changed', VersionChange.unknown)
            return

        try:
            version_change = VersionChange.from_versions(old_version, current_version)
        except ValueError:
            log.warning(f'Unable to parse old {attr_name}')
            setattr(self, f'{attr_name}_changed', VersionChange.unknown)
        else:
            setattr(self, f'{attr_name}_changed', version_change)
=======
def _parse_version(version_str: str) -> Tuple[int, int, int]:
    try:
        major, minor, patch = map(int, version_str.split('.'))
        return major, minor, patch
    except (ValueError, AttributeError):
        raise ValueError

class VersionChange(Enum):
    unknown = auto()
    equal = auto()
    downgrade = auto()
    patch = auto()
    minor = auto()
    major = auto()

    @classmethod
    def from_versions(cls, old_version: str, new_version: str) -> 'VersionChange':
        old_major, old_minor, old_patch = _parse_version(old_version)
        new_major, new_minor, new_patch = _parse_version(new_version)

        if old_version == new_version:
            return cls.equal
        if old_version > new_version:
            return cls.downgrade
        if old_major == new_major and old_minor == new_minor and old_patch!= new_patch:
            return cls.patch
        if old_major == new_major and old_minor!= new_minor:
            return cls.minor
        if old_major!= new_major:
            return cls.major
        raise ValueError

    def matches_filter(self, filterstr: str) -> bool:
        if filterstr == 'never':
            return False
        if filterstr == 'major' and self in (self.major, self.minor, self.patch):
            return True
        if filterstr == 'minor' and self in (self.minor, self.patch):
            return True
        if filterstr == 'patch' and self is self.patch:
            return True
        return False
=======

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
    def _set_changed_attributes(self, attr_name: str, current_version: str, section: str) -> None:
        old_version = self[section].get(attr_name, None)
        if old_version is None:
            self[section][attr_name] = current_version
            setattr(self, f'{attr_name}_changed', VersionChange.unknown)
            return

        try:
            version_change = VersionChange.from_versions(old_version, current_version)
        except ValueError:
            log.warning(f'Unable to parse old {attr_name}')
            setattr(self, f'{attr_name}_changed', VersionChange.unknown)
        else:
            setattr(self, f'{attr_name}_changed', version_change)
=======
def _parse_version(version_str: str) -> Tuple[int, int, int]:
    try:
        major, minor, patch = map(int, version_str.split('.'))
        return major, minor, patch
    except (ValueError, AttributeError):
        raise ValueError

class VersionChange(Enum):
    unknown = auto()
    equal = auto()
    downgrade = auto()
    patch = auto()
    minor = auto()
    major = auto()

    @classmethod
    def from_versions(cls, old_version: str, new_version: str) -> 'VersionChange':
        old_major, old_minor, old_patch = _parse_version(old_version)
        new_major, new_minor, new_patch = _parse_version(new_version)

        if old_version == new_version:
            return cls.equal
        if old_version > new_version:
            return cls.downgrade
        if old_major == new_major and old_minor == new_minor and old_patch!= new_patch:
            return cls.patch
        if old_major == new_major and old_minor!= new_minor:
            return cls.minor
        if old_major!= new_major:
            return cls.major
        raise ValueError

    def matches_filter(self, filterstr: str) -> bool:
        if filterstr == 'never':
            return False
        if filterstr == 'major' and self in (self.major, self.minor, self.patch):
            return True
        if filterstr == 'minor' and self in (self.minor, self.patch):
            return True
        if filterstr == 'patch' and self is self.patch:
            return True
        return False
=======

### FILE: qutebrowser/app.py
<<<<<<< SEARCH
if args.version:
    print(version.version_info())
    sys.exit(usertypes.Exit.ok)

    quitter.init(args)
    crashsignal.init(q_app=app, args=args, quitter=quitter.instance)
=======
if args.version:
    print(version.version_info())
    sys.exit(usertypes.Exit.ok)

    if state.qutebrowser_version_changed.matches_filter(config.val('changelog_after_upgrade', 'major')):
        print(version.changelog())
    sys.exit(usertypes.Exit.ok)

    quitter.init(args)
    crashsignal.init(q_app=app, args=args, quitter=quitter.instance)
=======
