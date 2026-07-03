### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
def _set_changed_attributes(self):
    """Set attributes indicating whether the qt_version or qutebrowser_version has changed."""
    qt_version = qVersion()
    old_qt_version = self['general'].get('qt_version', None)
    old_qutebrowser_version = self['general'].get('version', None)
    self.qt_version_changed = old_qt_version != qt_version
    self.qutebrowser_version_changed = old_qutebrowser_version != qutebrowser.__version__
=======
def _set_changed_attributes(self):
    """Set attributes indicating whether the qt_version or qutebrowser_version has changed."""
    qt_version = qVersion()
    old_qt_version = self['general'].get('qt_version', None)
    old_qutebrowser_version = self['general'].get('version', None)
    self.qt_version_changed = old_qt_version != qt_version

    # Determine the version change using the VersionChange enum
    if old_qutebrowser_version is None or old_qutebrowser_version == qutebrowser.__version__:
        self.qutebrowser_version_changed = configfiles.VersionChange.equal
    elif old_qutebrowser_version > qutebrowser.__version__:
        self.qutebrowser_version_changed = configfiles.VersionChange.downgrade
    elif old_qutebrowser_version.split('.')[0] == qutebrowser.__version__.split('.')[0]:
        if old_qutebrowser_version.split('.')[1] == qutebrowser.__version__.split('.')[1]:
            self.qutebrowser_version_changed = configfiles.VersionChange.patch
        else:
            self.qutebrowser_version_changed = configfiles.VersionChange.minor
    else:
        self.qutebrowser_version_changed = configfiles.VersionChange.major

    if self.qutebrowser_version_changed == configfiles.VersionChange.unknown:
        log.warning("Unable to parse old version %s", old_qutebrowser_version)
=======
>>>>>>> REPLACE

### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
class VersionChange(enum.Enum):
    """Enumeration for version changes."""
    unknown = 0
    equal = 1
    downgrade = 2
    patch = 3
    minor = 4
    major = 5
=======
class VersionChange(enum.Enum):
    """Enumeration for version changes."""
    unknown = 0
    equal = 1
    downgrade = 2
    patch = 3
    minor = 4
    major = 5

    @classmethod
    def matches_filter(cls, filterstr: str) -> bool:
        """Return whether the version change matches a given changelog_after_upgrade filter value."""
        if filterstr == 'never':
            return False
        if filterstr == 'major' and cls == cls.major:
            return True
        if filterstr == 'minor' and cls == cls.minor:
            return True
        if filterstr == 'patch' and cls == cls.patch:
            return True
        if filterstr == 'always':
            return True
        return False
>>>>>>> REPLACE

### FILE: qutebrowser/app.py
<<<<<<< SEARCH
    if server is None:
        if args.backend is not None:
            log.init.warning("Backend from the running instance will be used")
        sys.exit(usertypes.Exit.ok)
    else:
        quitter.instance.shutting_down.connect(server.shutdown)
        server.got_args.connect(lambda args, target_arg, cwd:
                                process_pos_args(args, cwd=cwd, via_ipc=True,
                                                 target_arg=target_arg))
=======
    if server is None:
        if args.backend is not None:
            log.init.warning("Backend from the running instance will be used")
        sys.exit(usertypes.Exit.ok)
    else:
        quitter.instance.shutting_down.connect(server.shutdown)
        server.got_args.connect(lambda args, target_arg, cwd:
                                process_pos_args(args, cwd=cwd, via_ipc=True,
                                                 target_arg=target_arg))

    # Show changelog after upgrade if configured
    if config.val['changelog_after_upgrade'] == 'always' and state.qutebrowser_version_changed != configfiles.VersionChange.equal:
        message.show_message("Changelog", qutebrowser.app.versions.changelog())
>>>>>>> REPLACE

### FILE: doc/changelog.asciidoc
<<<<<<< SEARCH
* New `changelog_after_upgrade` setting.
=======
* New `changelog_after_upgrade` setting which controls whether a changelog is shown after qutebrowser is upgraded. By default, it is set to `never`, but can be configured to `always` or `major`/`minor`/`patch` to show the changelog only for certain types of upgrades.
>>>>>>> REPLACE

### FILE: doc/help/settings.asciidoc
<<<<<<< SEARCH
|<<changelog_after_upgrade,changelog_after_upgrade>>|Whether to show a changelog after qutebrowser was upgraded.
=======
|<<changelog_after_upgrade,changelog_after_upgrade>>|Whether to show a changelog after qutebrowser was upgraded. Possible values are `never`, `always`, `major`, `minor`, and `patch`.|
>>>>>>> REPLACE