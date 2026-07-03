### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
self.qt_version_changed = old_qt_version != qt_version
self.qutebrowser_version_changed = (old_qutebrowser_version != qutebrowser.__version__)
=======
self.qutebrowser_version_changed = self._compare_versions(old_qutebrowser_version, qutebrowser.__version__)
self.qt_version_changed = self._compare_versions(old_qt_version, qt_version)
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
def _compare_versions(self, old_version: Optional[str], new_version: str) -> VersionChange:
    """Compare two version strings and return the appropriate VersionChange."""
    if old_version is None:
        return VersionChange.unknown
    try:
        old_parts = [int(part) for part in old_version.split('.')]
        new_parts = [int(part) for part in new_version.split('.')]
    except ValueError:
        log.debug("Could not parse version strings: %s vs %s", old_version, new_version)
        return VersionChange.unknown

    if old_parts == new_parts:
        return VersionChange.equal
    if old_parts > new_parts:
        return VersionChange.downgrade
    if old_parts[0] == new_parts[0]:  # same major version
        if old_parts[1] == new_parts[1]:  # same minor version
            return VersionChange.patch
        return VersionChange.minor
    return VersionChange.major
=======
class VersionChange(enum.Enum):
    unknown = 0
    equal = 1
    downgrade = 2
    patch = 3
    minor = 4
    major = 5

    def matches_filter(self, filterstr: str) -> bool:
        """Return whether the version change matches a given changelog_after_upgrade filter value."""
        if filterstr == 'all':
            return True
        if filterstr == 'none':
            return False
        if self == VersionChange.equal:
            return filterstr == 'equal'
        if self == VersionChange.downgrade:
            return filterstr == 'downgrade'
        if self == VersionChange.patch:
            return filterstr in ('patch', 'minor', 'major')
        if self == VersionChange.minor:
            return filterstr in ('minor', 'major')
        if self == VersionChange.major:
            return filterstr == 'major'
        return False

    @staticmethod
    def from_string(version_string: str) -> 'VersionChange':
        """Create a VersionChange from a version string comparison."""
        if version_string == 'equal':
            return VersionChange.equal
        if version_string == 'downgrade':
            return VersionChange.downgrade
        if version_string == 'patch':
            return VersionChange.patch
        if version_string == 'minor':
            return VersionChange.minor
        if version_string == 'major':
            return VersionChange.major
        return VersionChange.unknown

    def __str__(self) -> str:
        return self.name.lower()

class StateConfig(configparser.ConfigParser):
    def __init__(self) -> None:
        super().__init__()
        self._filename = os.path.join(standarddir.data(), 'state')
        self.read(self._filename, encoding='utf-8')
        qt_version = qVersion()

        if 'general' in self:
            old_qt_version = self['general'].get('qt_version', None)
            old_qutebrowser_version = self['general'].get('version', None)
            self.qt_version_changed = old_qt_version != qt_version
            self.qutebrowser_version_changed = (old_qutebrowser_version != qutebrowser.__version__)
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

    def _set_changed_attributes(self) -> None:
        """Set attributes indicating whether the qt version or qutebrowser version has changed."""
        old_qt_version = self['general'].get('qt_version', None)
        old_qutebrowser_version = self['general'].get('version', None)

        self.qt_version_changed = self._compare_versions(old_qt_version, qVersion())
        self.qutebrowser_version_changed = self._compare_versions(old_qutebrowser_version, qutebrowser.__version__)

    def _compare_versions(self, old_version: Optional[str], new_version: str) -> VersionChange:
        """Compare two version strings and return the appropriate VersionChange."""
        if old_version is None:
            return VersionChange.unknown
        try:
            old_parts = [int(part) for part in old_version.split('.')]
            new_parts = [int(part) for part in new_version.split('.')]
        except ValueError:
            log.debug("Could not parse version strings: %s vs %s", old_version, new_version)
            return VersionChange.unknown

        if old_parts == new_parts:
            return VersionChange.equal
        if old_parts > new_parts:
            return VersionChange.downgrade
        if old_parts[0] == new_parts[0]:  # same major version
            if old_parts[1] == new_parts[1]:  # same minor version
                return VersionChange.patch
            return VersionChange.minor
        return VersionChange.major
>>>>>>> REPLACE

### FILE: qutebrowser/app.py
<<<<<<< SEARCH
    if args.temp_basedir:
        args.basedir = tempfile.mkdtemp(prefix='qutebrowser-basedir-')
    log.init.debug("Main process PID: {}".format(os.getpid()))
    log.init.debug("Initializing directories...")
    standarddir.init(args)
    utils.preload_resources()
    log.init.debug("Initializing config...")
    configinit.early_init(args)
    log.init.debug("Initializing application...")
    app = Application(args)
    objects.qapp = app
    app.setOrganizationName("qutebrowser")
    app.setApplicationName("qutebrowser")
    # Default DesktopFileName is org.qutebrowser.qutebrowser, set in `get_argparser()`
    app.setDesktopFileName(args.desktop_file_name)
    app.setApplicationVersion(qutebrowser.__version__)
    if args.version:
        print(version.version_info())
        sys.exit(usertypes.Exit.ok)
    quitter.init(args)
    crashsignal.init(q_app=app, args=args, quitter=quitter.instance)
    try:
        server = ipc.send_or_listen(args)
    except ipc.Error:
        # ipc.send_or_listen already displays the error message for us.
        # We didn't really initialize much so far, so we just quit hard.
        sys.exit(usertypes.Exit.err_ipc)
    if server is None:
        if args.backend is not None:
            log.init.warning("Backend from the running instance will be used")
        sys.exit(usertypes.Exit.ok)
    else:
        quitter.instance.shutting_down.connect(server.shutdown)
        server.got_args.connect(lambda args, target_arg, cwd:
                                process_pos_args(args, cwd=cwd, via_ipc=True,
                                                 target_arg=target_arg))
    init(args=args)
    ret = qt_mainloop()
    return ret
=======
    if args.temp_basedir:
        args.basedir = tempfile.mkdtemp(prefix='qutebrowser-basedir-')
    log.init.debug("Main process PID: {}".format(os.getpid()))
    log.init.debug("Initializing directories...")
    standarddir.init(args)
    utils.preload_resources()
    log.init.debug("Initializing config...")
    configinit.early_init(args)
    log.init.debug("Initializing application...")
    app = Application(args)
    objects.qapp = app
    app.setOrganizationName("qutebrowser")
    app.setApplicationName("qutebrowser")
    # Default DesktopFileName is org.qutebrowser.qutebrowser, set in `get_argparser()`
    app.setDesktopFileName(args.desktop_file_name)
    app.setApplicationVersion(qutebrowser.__version__)
    if args.version:
        print(version.version_info())
        sys.exit(usertypes.Exit.ok)
    quitter.init(args)
    crashsignal.init(q_app=app, args=args, quitter=quitter.instance)
    try:
        server = ipc.send_or_listen(args)
    except ipc.Error:
        # ipc.send_or_listen already displays the error message for us.
        # We didn't really initialize much so far, so we just quit hard.
        sys.exit(usertypes.Exit.err_ipc)
    if server is None:
        if args.backend is not None:
            log.init.warning("Backend from the running instance will be used")
        sys.exit(usertypes.Exit.ok)
    else:
        quitter.instance.shutting_down.connect(server.shutdown)
        server.got_args.connect(lambda args, target_arg, cwd:
                                process_pos_args(args, cwd=cwd, via_ipc=True,
                                                 target_arg=target_arg))
    init(args=args)
    if config.ENABLE_CHANGELOG_AFTER_UPGRADE:
        log.init.info("Showing changelog after upgrade")
        message.show_message("Changelog", doc.changelog.get_changelog_text())
    ret = qt_mainloop()
    return ret
>>>>>>> REPLACE