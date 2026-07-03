### FILE: scripts/dictcli.py
<<<<<<< SEARCH
    def update(languages):
        """Update the given languages."""
        installed = [lang for lang in languages if lang.local_version is not None]
        for lang in installed:
            if lang.local_version < lang.remote_version:
                print('Upgrading {} from {} to {}'.format(
                    lang.code,
                    version_str(lang.local_version),
                    version_str(lang.remote_version)))
                install_lang(lang)
=======
    def update(languages):
        """Update the given languages."""
        installed = [lang for lang in languages if lang.local_version is not None]
        for lang in installed:
            if lang.local_version < lang.remote_version:
                print(f'Upgrading {lang.code} from {version_str(lang.local_version)} to {version_str(lang.remote_version)}')
                install_lang(lang)
>>>>>>> REPLACE

### FILE: scripts/link_pyqt.py
<<<<<<< SEARCH
    def link_pyqt(executable, venv_path):
        """Symlink the systemwide PyQt/sip into the venv.

        Args:
            executable: The python executable where the source files are present.
            venv_path: The path to the virtualenv site-packages.
        """
        try:
            get_lib_path(executable, 'PyQt5.sip')
        except Error:
            # There is no PyQt5.sip, so we need to copy the toplevel sip.
            sip_file = get_lib_path(executable, 'sip')
        else:
            # There is a PyQt5.sip, it'll get copied with the PyQt5 dir.
            sip_file = None

        sipconfig_file = get_lib_path(executable, 'sipconfig', required=False)
        pyqt_dir = os.path.dirname(get_lib_path(executable, 'PyQt5.QtCore'))

        for path in [sip_file, sipconfig_file, pyqt_dir]:
            if path is None:
                continue

            fn = os.path.basename(path)
            dest = os.path.join(venv_path, fn)

            if os.path.exists(dest):
                if needs_update(path, dest):
                    remove(dest)
                else:
                    continue

            copy_or_link(path, dest)
=======
    def link_pyqt(executable, venv_path):
        """Symlink the systemwide PyQt/sip into the venv.

        Args:
            executable: The python executable where the source files are present.
            venv_path: The path to the virtualenv site-packages.
        """
        try:
            get_lib_path(executable, 'PyQt5.sip')
        except Error:
            # There is no PyQt5.sip, so we need to copy the toplevel sip.
            sip_file = get_lib_path(executable, 'sip')
        else:
            # There is a PyQt5.sip, it'll get copied with the PyQt5 dir.
            sip_file = None

        sipconfig_file = get_lib_path(executable, 'sipconfig', required=False)
        pyqt_dir = os.path.dirname(get_lib_path(executable, 'PyQt5.QtCore'))

        for path in [sip_file, sipconfig_file, pyqt_dir]:
            if path is None:
                continue

            fn = os.path.basename(path)
            dest = os.path.join(venv_path, fn)

            if os.path.exists(dest):
                if needs_update(path, dest):
                    remove(dest)
                else:
                    continue

            copy_or_link(path, dest)
>>>>>>> REPLACE

### FILE: qutebrowser/commands/command.py
<<<<<<< SEARCH
    def _check_prerequisites(self, win_id):
        """Check if the command is permitted to run currently.

        Args:
            win_id: The window ID the command is run in.
        """
        from qutebrowser.keyinput import modeman
        mode_manager = modeman.instance(win_id)
        self.validate_mode(mode_manager.mode)

        if self.backend is not None and objects.backend != self.backend:
            raise cmdexc.PrerequisitesError(
                "{}: Only available with {} "
                "backend.".format(self.name, self.backend.name))

        if self.deprecated:
            message.warning(f'{self.name} is deprecated - {self.deprecated}')
=======
    def _check_prerequisites(self, win_id):
        """Check if the command is permitted to run currently.

        Args:
            win_id: The window ID the command is run in.
        """
        from qutebrowser.keyinput import modeman
        mode_manager = modeman.instance(win_id)
        self.validate_mode(mode_manager.mode)

        if self.backend is not None and objects.backend != self.backend:
            raise cmdexc.PrerequisitesError(
                f"{self.name}: Only available with {self.backend.name} backend.")

        if self.deprecated:
            message.warning(f"{self.name} is deprecated - {self.deprecated}")
>>>>>>> REPLACE

### FILE: scripts/mkvenv.py
<<<<<<< SEARCH
    def run_qt_smoke_test(venv_dir: pathlib.Path) -> None:
        """Make sure the Qt installation works."""
        utils.print_title("Running Qt smoke test")
        code = [
            'import sys',
            'from PyQt5.QtWidgets import QApplication',
            'from PyQt5.QtCore import qVersion, QT_VERSION_STR, PYQT_VERSION_STR',
            'print(f"Python: {sys.version}")',
            'print(f"qVersion: {qVersion()}")',
            'print(f"QT_VERSION_STR: {QT_VERSION_STR}")',
            'print(f"PYQT_VERSION_STR: {PYQT_VERSION_STR}")',
            'QApplication([])',
            'print("Qt seems to work properly!")',
            'print()',
        ]
        try:
            run_venv(
                venv_dir,
                'python', '-c', '; '.join(code),
                env={'QT_DEBUG_PLUGINS': '1'},
                capture_error=True
            )
        except Error as e:
            proc_e = e.__cause__
            assert isinstance(proc_e, subprocess.CalledProcessError), proc_e
            print(proc_e.stderr)
            raise Error(
                f"Smoke test failed with status {proc_e.returncode}. "
                "You might find additional information in the debug output above.")
=======
    def run_qt_smoke_test(venv_dir: pathlib.Path) -> None:
        """Make sure the Qt installation works."""
        utils.print_title("Running Qt smoke test")
        code = [
            'import sys',
            'from PyQt5.QtWidgets import QApplication',
            'from PyQt5.QtCore import qVersion, QT_VERSION_STR, PYQT_VERSION_STR',
            'print(f"Python: {sys.version}")',
            'print(f"qVersion: {qVersion()}")',
            'print(f"QT_VERSION_STR: {QT_VERSION_STR}")',
            'print(f"PYQT_VERSION_STR: {PYQT_VERSION_STR}")',
            'QApplication([])',
            'print("Qt seems to work properly!")',
            'print()',
        ]
        try:
            run_venv(
                venv_dir,
                'python', '-c', '; '.join(code),
                env={'QT_DEBUG_PLUGINS': '1'},
                capture_error=True
            )
        except Error as e:
            proc_e = e.__cause__
            assert isinstance(proc_e, subprocess.CalledProcessError), proc_e
            print(proc_e.stderr)
            raise Error(
                f"Smoke test failed with status {proc_e.returncode}. "
                "You might find additional information in the debug output above.")
>>>>>>> REPLACE