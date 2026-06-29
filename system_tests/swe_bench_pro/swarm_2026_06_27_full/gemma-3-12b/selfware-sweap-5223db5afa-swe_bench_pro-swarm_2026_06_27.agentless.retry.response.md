### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH

# Copyright 2014-2021 Florian Bruhin (The Compiler) <mail@qutebrowser.org>
#
# This file is part of qutebrowser.
#
# qutebrowser is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# qutebrowser is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with qutebrowser.  If not, see <https://www.gnu.org/licenses/>.

"""Configuration files residing on disk."""

import pathlib
import types
import os.path
import sys
import textwrap
import traceback
import configparser
import contextlib
import re
from typing import (TYPE_CHECKING, Any, Dict, Iterable, Iterator, List, Mapping,
                    MutableMapping, Optional, cast)

import yaml
from PyQt5.QtCore import pyqtSignal, pyqtSlot, QObject, QSettings, qVersion

import qutebrowser
from qutebrowser.config import (config, configdata, configutils,
                                configtypes)
from qutebrowser.keyinput import keyutils
from qutebrowser.utils import standarddir, utils, qtutils, log, urlmatch

if TYPE_CHECKING:
    from qutebrowser.misc import savemanager


# The StateConfig instance
state = cast('StateConfig', None)


_SettingsType = Dict[str, Dict[str, Any]]


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
            self.qt_version_changed = old_qt_version != qt_version
            self.qutebrowser_version_changed = (
                old_qutebrowser_version != qutebrowser.__version__)
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

    def init_save_manager(self,
                          save_manager: 'savemanager.SaveManager') -> None:
        """Make sure the config gets saved properly.

        We do this outside of __init__ because the config gets created before
        the save_manager exists.
        """
        save_manager.add_saveable('state-config', self._save)

    def _save(self) -> None:
        """Save the state file to the configured location."""
        with open(self._filename, 'w', encoding='utf-8') as f:
            self.write(f)


class YamlConfig(QObject):

    """A config stored on disk as YAML file.

    Class attributes:
        VERSION: The current version number of the config file.
    """

    VERSION = 2
    changed = pyqtSignal()

    def __init__(self, parent: QObject = None) -> None:
        super().__init__(parent)
        self._filename = os.path.join(standarddir.config(auto=True),
                                      'autoconfig.yml')
        self._dirty = False

        self._values: Dict[str, configutils.Values] = {}
        for name, opt in configdata.DATA.items():
            self._values[name] = configutils.Values(opt)

    def init_save_manager(self,
                          save_manager: 'savemanager.SaveManager') -> None:
        """Make sure the config gets saved properly.

        We do this outside of __init__ because the config gets created before
        the save_manager exists.
        """
        save_manager.add_saveable('yaml-config', self._save, self.changed)

    def __iter__(self) -> Iterator[configutils.Values]:
        """Iterate over configutils.Values items."""
        yield from self._values.values()

    @pyqtSlot()
    def _mark_changed(self) -> None:
        """Mark the YAML config as changed."""
        self._dirty = True
        self.changed.emit()

    def _save(self) -> None:
        """Save the settings to the YAML file if they've changed."""
        if not self._dirty:
            return

        settings: _SettingsType = {}
        for name, values in sorted(self._values.items()):
            if not values:
                continue
            settings[name] = {}
            for scoped in values:
                key = ('global' if scoped.pattern is None
                       else str(scoped.pattern))
                settings[name][key] = scoped.value

=======
from enum import Enum

class VersionChange(Enum):
    unknown = "unknown"
    equal = "equal"
    downgrade = "downgrade"
    patch = "patch"
    minor = "minor"
    major = "major"

    def matches_filter(self, filterstr: str) -> bool:
        """Return whether the version change matches a given `changelog_after_upgrade` filter value."""
        if filterstr is None:
            return True
        if filterstr == "all":
            return True
        if filterstr == "major":
            return self == VersionChange.major
        if filterstr == "minor":
            return self == VersionChange.minor or self == VersionChange.major
        if filterstr == "patch":
            return self == VersionChange.patch or self == VersionChange.minor or self == VersionChange.major
        return False


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
            self.qt_version_changed = old_qt_version != qt_version
            self.qutebrowser_version_changed = (
                old_qutebrowser_version != qutebrowser.__version__)
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

    def init_save_manager(self,
                          save_manager: 'savemanager.SaveManager') -> None:
        """Make sure the config gets saved properly.

        We do this outside of __init__ because the config gets created before
        the save_manager exists.
        """
        save_manager.add_saveable('state-config', self._save)

    def _save(self) -> None:
        """Save the state file to the configured location."""
        with open(self._filename, 'w', encoding='utf-8') as f:
            self.write(f)

    def _set_changed_attributes(self):
        """Set `qt_version_changed`/`qutebrowser_version_changed` attributes."""
        old_qutebrowser_version = self['general'].get('version', None)
        current_version = qutebrowser.__version__
        try:
            if old_qutebrowser_version is None:
                self.qutebrowser_version_changed = True
            else:
                old_version = version.parse(old_qutebrowser_version)
                current_version_parsed = version.parse(current_version)
                if old_version == current_version_parsed:
                    self.qutebrowser_version_changed = VersionChange.equal
                elif old_version < current_version_parsed:
                    self.qutebrowser_version_changed = VersionChange.major
                elif old_version > current_version_parsed:
                    self.qutebrowser_version_changed = VersionChange.downgrade
                elif old_version.patch != current_version_parsed.patch:
                    self.qutebrowser_version_changed = VersionChange.patch
                elif old_version.minor != current_version_parsed.minor:
                    self.qutebrowser_version_changed = VersionChange.minor
                else:
                    self.qutebrowser_version_changed = VersionChange.equal
        except ValueError:
            log.warning("Could not parse old qutebrowser version.")
            self.qutebrowser_version_changed = VersionChange.unknown

        qt_version = qVersion()
        old_qt_version = self['general'].get('qt_version', None)
        self.qt_version_changed = old_qt_version != qt_version

    def load(self, *args, **kwargs):
        super().load(*args, **kwargs)
        self._set_changed_attributes()
>>>>>>> REPLACE
### FILE: tests/unit/config/test_configfiles.py
<<<<<<< SEARCH

# vim: ft=python fileencoding=utf-8 sts=4 sw=4 et:
# Copyright 2017-2021 Florian Bruhin (The Compiler) <mail@qutebrowser.org>

# This file is part of qutebrowser.
#
# qutebrowser is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# qutebrowser is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with qutebrowser.  If not, see <https://www.gnu.org/licenses/>.

"""Tests for qutebrowser.config.configfiles."""

import os
import sys
import unittest.mock
import textwrap

import pytest
from PyQt5.QtCore import QSettings

from qutebrowser.config import (config, configfiles, configexc, configdata,
                                configtypes)
from qutebrowser.utils import utils, usertypes, urlmatch, standarddir
from qutebrowser.keyinput import keyutils


@pytest.fixture(autouse=True)
def configdata_init():
    """Initialize configdata if needed."""
    if configdata.DATA is None:
        configdata.init()


class AutoConfigHelper:

    """A helper to easily create/validate autoconfig.yml files."""

    def __init__(self, config_tmpdir):
        self.fobj = config_tmpdir / 'autoconfig.yml'

    def write_toplevel(self, data):
        with self.fobj.open('w', encoding='utf-8') as f:
            utils.yaml_dump(data, f)

    def write(self, values):
        data = {'config_version': 2, 'settings': values}
        self.write_toplevel(data)

    def write_raw(self, text):
        self.fobj.write_text(text, encoding='utf-8')

    def read_toplevel(self):
        with self.fobj.open('r', encoding='utf-8') as f:
            data = utils.yaml_load(f)
            assert data['config_version'] == 2
            return data

    def read(self):
        return self.read_toplevel()['settings']

    def read_raw(self):
        return self.fobj.read_text('utf-8')


@pytest.fixture
def autoconfig(config_tmpdir):
    return AutoConfigHelper(config_tmpdir)


@pytest.mark.parametrize('old_data, insert, new_data', [
    (None,
     False,
     '[general]\n'
     'qt_version = 5.6.7\n'
     'version = 1.2.3\n'
     '\n'
     '[geometry]\n'
     '\n'
     '[inspector]\n'
     '\n'),
    ('[general]\n'
     'fooled = true',
     False,
     '[general]\n'
     'qt_version = 5.6.7\n'
     'version = 1.2.3\n'
     '\n'
     '[geometry]\n'
     '\n'
     '[inspector]\n'
     '\n'),
    ('[general]\n'
     'foobar = 42',
     False,
     '[general]\n'
     'foobar = 42\n'
     'qt_version = 5.6.7\n'
     'version = 1.2.3\n'
     '\n'
     '[geometry]\n'
     '\n'
     '[inspector]\n'
     '\n'),
    (None,
     True,
     '[general]\n'
     'qt_version = 5.6.7\n'
     'version = 1.2.3\n'
     'newval = 23\n'
     '\n'
     '[geometry]\n'
     '\n'
     '[inspector]\n'
     '\n'),
])
def test_state_config(fake_save_manager, data_tmpdir, monkeypatch,
                      old_data, insert, new_data):
    monkeypatch.setattr(configfiles.qutebrowser, '__version__', '1.2.3')
    monkeypatch.setattr(configfiles, 'qVersion', lambda: '5.6.7')

    statefile = data_tmpdir / 'state'
    if old_data is not None:
        statefile.write_text(old_data, 'utf-8')

    state = configfiles.StateConfig()
    state.init_save_manager(fake_save_manager)

    if insert:
        state['general']['newval'] = '23'
    if 'foobar' in (old_data or ''):
        assert state['general']['foobar'] == '42'

    state._save()

    assert statefile.read_text('utf-8') == new_data
    fake_save_manager.add_saveable('state-config', unittest.mock.ANY)


@pytest.mark.parametrize('old_version, new_version, changed', [
    (None, '5.12.1', False),
    ('5.12.1', '5.12.1', False),
    ('5.12.2', '5.12.1', True),
    ('5.12.1', '5.12.2', True),
    ('5.13.0', '5.12.2', True),
    ('5.12.2', '5.13.0', True),
])
def test_qt_version_changed(data_tmpdir, monkeypatch,
                            old_version, new_version, changed):
    monkeypatch.setattr(configfiles, 'qVersion', lambda: new_version)

    statefile = data_tmpdir / 'state'
    if old_version is not None:
        data = ('[general]\n'
                'qt_version = {}'.format(old_version))
        statefile.write_text(data, 'utf-8')

=======
import pytest
from enum import Enum

from qutebrowser.config import (config, configfiles, configexc, configdata,
                                configtypes)
from qutebrowser.utils import utils, usertypes, urlmatch, standarddir
from qutebrowser.keyinput import keyutils


@pytest.fixture(autouse=True)
def configdata_init():
    """Initialize configdata if needed."""
    if configdata.DATA is None:
        configdata.init()


class AutoConfigHelper:

    """A helper to easily create/validate autoconfig.yml files."""

    def __init__(self, config_tmpdir):
        self.fobj = config_tmpdir / 'autoconfig.yml'

    def write_toplevel(self, data):
        with self.fobj.open('w', encoding='utf-8') as f:
            utils.yaml_dump(data, f)

    def write(self, values):
        data = {'config_version': 2, 'settings': values}
        self.write_toplevel(data)

    def write_raw(self, text):
        self.fobj.write_text(text, encoding='utf-8')

    def read_toplevel(self):
        with self.fobj.open('r', encoding='utf-8') as f:
            data = utils.yaml_load(f)
            assert data['config_version'] == 2
            return data

    def read(self):
        return self.read_toplevel()['settings']

    def read_raw(self):
        return self.fobj.read_text('utf-8')


@pytest.fixture
def autoconfig(config_tmpdir):
    return AutoConfigHelper(config_tmpdir)


@pytest.mark.parametrize('old_data, insert, new_data', [
    (None,
     False,
     '[general]\n'
     'qt_version = 5.6.7\n'
     'version = 1.2.3\n'
     '\n'
     '[geometry]\n'
     '\n'
     '[inspector]\n'
     '\n'),
    ('[general]\n'
     'fooled = true',
     False,
     '[general]\n'
     'qt_version = 5.6.7\n'
     'version = 1.2.3\n'
     '\n'
     '[geometry]\n'
     '\n'
     '[inspector]\n'
     '\n'),
    ('[general]\n'
     'foobar = 42',
     False,
     '[general]\n'
     'foobar = 42\n'
     'qt_version = 5.6.7\n'
     'version = 1.2.3\n'
     '\n'
     '[geometry]\n'
     '\n'
     '[inspector]\n'
     '\n'),
    (None,
     True,
     '[general]\n'
     'qt_version = 5.6.7\n'
     'version = 1.2.3\n'
     'newval = 23\n'
     '\n'
     '[geometry]\n'
     '\n'
     '[inspector]\n'
     '\n'),
])
def test_state_config(fake_save_manager, data_tmpdir, monkeypatch,
                      old_data, insert, new_data):
    monkeypatch.setattr(configfiles.qutebrowser, '__version__', '1.2.3')
    monkeypatch.setattr(configfiles, 'qVersion', lambda: '5.6.7')

    statefile = data_tmpdir / 'state'
    if old_data is not None:
        statefile.write_text(old_data, 'utf-8')

    state = configfiles.StateConfig()
    state.init_save_manager(fake_save_manager)

    if insert:
        state['general']['newval'] = '23'
    if 'foobar' in (old_data or ''):
        assert state['general']['foobar'] == '42'

    state._save()

    assert statefile.read_text('utf-8') == new_data
    fake_save_manager.add_saveable('state-config', unittest.mock.ANY)


@pytest.mark.parametrize('old_version, new_version, changed', [
    (None, '5.12.1', False),
    ('5.12.1', '5.12.1', False),
    ('5.12.2', '5.12.1', True),
    ('5.12.1', '5.12.2', True),
    ('5.13.0', '5.12.2', True),
    ('5.12.2', '5.13.0', True),
])
def test_qt_version_changed(data_tmpdir, monkeypatch,
                            old_version, new_version, changed):
    monkeypatch.setattr(configfiles, 'qVersion', lambda: new_version)

    statefile = data_tmpdir / 'state'
    if old_version is not None:
        data = ('[general]\n'
                'qt_version = {}'.format(old_version))
        statefile.write_text(data, 'utf-8')

>>>>>>> REPLACE
### FILE: qutebrowser/app.py
<<<<<<< SEARCH

# vim: ft=python fileencoding=utf-8 sts=4 sw=4 et:

# Copyright 2014-2021 Florian Bruhin (The Compiler) <mail@qutebrowser.org>
#
# This file is part of qutebrowser.
#
# qutebrowser is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# qutebrowser is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with qutebrowser.  If not, see <https://www.gnu.org/licenses/>.

"""Initialization of qutebrowser and application-wide things.

The run() function will get called once early initialization (in
qutebrowser.py/earlyinit.py) is done. See the qutebrowser.py docstring for
details about early initialization.

As we need to access the config before the QApplication is created, we
initialize everything the config needs before the QApplication is created, and
then leave it in a partially initialized state (no saving, no config errors
shown yet).

We then set up the QApplication object and initialize a few more low-level
things.

After that, init() and _init_modules() take over and initialize the rest.

After all initialization is done, the qt_mainloop() function is called, which
blocks and spins the Qt mainloop.
"""

import os
import sys
import functools
import tempfile
import datetime
import argparse
from typing import Iterable, Optional, cast

from PyQt5.QtWidgets import QApplication, QWidget
from PyQt5.QtGui import QDesktopServices, QPixmap, QIcon
from PyQt5.QtCore import pyqtSlot, QUrl, QObject, QEvent, pyqtSignal, Qt

import qutebrowser
import qutebrowser.resources
from qutebrowser.commands import runners
from qutebrowser.config import (config, websettings, configfiles, configinit,
                                qtargs)
from qutebrowser.browser import (urlmarks, history, browsertab,
                                 qtnetworkdownloads, downloads, greasemonkey)
from qutebrowser.browser.network import proxy
from qutebrowser.browser.webkit import cookies, cache
from qutebrowser.browser.webkit.network import networkmanager
from qutebrowser.extensions import loader
from qutebrowser.keyinput import macros, eventfilter
from qutebrowser.mainwindow import mainwindow, prompt, windowundo
from qutebrowser.misc import (ipc, savemanager, sessions, crashsignal,
                              earlyinit, sql, cmdhistory, backendproblem,
                              objects, quitter)
from qutebrowser.utils import (log, version, message, utils, urlutils, objreg,
                               usertypes, standarddir, error, qtutils, debug)
# pylint: disable=unused-import
# We import those to run the cmdutils.register decorators.
from qutebrowser.mainwindow.statusbar import command
from qutebrowser.misc import utilcmds
# pylint: enable=unused-import


q_app = cast(QApplication, None)


def run(args):
    """Initialize everything and run the application."""
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
            log.init.warning(
                "Backend from the running instance will be used")
        sys.exit(usertypes.Exit.ok)
    else:
        quitter.instance.shutting_down.connect(server.shutdown)
        server.got_args.connect(lambda args, target_arg, cwd:
                                process_pos_args(args, cwd=cwd, via_ipc=True,
                                                 target_arg=target_arg))

    init(args=args)
    ret = qt_mainloop()
    return ret


def qt_mainloop():
    """Simple wrapper to get a nicer stack trace for segfaults.

    WARNING: misc/crashdialog.py checks the stacktrace for this function
    name, so if this is changed, it should be changed there as well!
    """
    return objects.qapp.exec()


def init(*, args: argparse.Namespace) -> None:
    """Initialize everything."""
    log.init.debug("Starting init...")

    crashsignal.crash_handler.init_faulthandler()

    objects.qapp.setQuitOnLastWindowClosed(False)
    quitter.instance.shutting_down.connect(QApplication.closeAllWindows)

    _init_icon()
    _init_pulseaudio()

    loader.init()
    loader.load_components()
    try:
        _init_modules(args=args)
    except (OSError, UnicodeDecodeError, browsertab.WebTabError) as e:
        error.handle_fatal_exc(e, "Error while initializing!",
                               no_err_windows=args.no_err_windows,
                               pre_text="Error while initializing")
        sys.exit(usertypes.Exit.err_init)


... (269 lines omitted) ...



# This is effectively a @config.change_filter
# However, logging is initialized too early to use that annotation
def _on_config_changed(name: str) -> None:
    if name.startswith('logging.'):
        log.init_from_config(config.val)


def _init_modules(*, args):
    """Initialize all 'modules' which need to be initialized.

    Args:
        args: The argparse namespace.
    """
    log.init.debug("Initializing logging from config...")
    log.init_from_config(config.val)
    config.instance.changed.connect(_on_config_changed)

    log.init.debug("Initializing save manager...")
    save_manager = savemanager.SaveManager(objects.qapp)
    objreg.register('save-manager', save_manager)
    quitter.instance.shutting_down.connect(save_manager.shutdown)
    configinit.late_init(save_manager)

    log.init.debug("Checking backend requirements...")
    backendproblem.init(args=args, save_manager=save_manager)

    log.init.debug("Initializing prompts...")
    prompt.init()

    log.init.debug("Initializing network...")
    networkmanager.init()

    log.init.debug("Initializing proxy...")
    proxy.init()
    quitter.instance.shutting_down.connect(proxy.shutdown)

    log.init.debug("Initializing downloads...")
    downloads.init()
    quitter.instance.shutting_down.connect(downloads.shutdown)

    with debug.log_time("init", "Initializing SQL/history"):
        try:
            log.init.debug("Initializing SQL...")
            sql.init(os.path.join(standarddir.data(), 'history.sqlite'))

            log.init.debug("Initializing web history...")
            history.init(objects.qapp)
        except sql.KnownError as e:
            error.handle_fatal_exc(e, 'Error initializing SQL',
                                   pre_text='Error initializing SQL',
                                   no_err_windows=args.no_err_windows)
            sys.exit(usertypes.Exit.err_init)

    log.init.debug("Initializing command history...")
    cmdhistory.init()
    log.init.debug("Initializing sessions...")
    sessions.init(objects.qapp)

    log.init.debug("Initializing websettings...")
    websettings.init(args)
    quitter.instance.shutting_down.connect(websettings.shutdown)

    if not args.no_err_windows:
        crashsignal.crash_handler.display_faulthandler()

    log.init.debug("Initializing quickmarks...")
    quickmark_manager = urlmarks.QuickmarkManager(objects.qapp)
    objreg.register('quickmark-manager', quickmark_manager)

    log.init.debug("Initializing bookmarks...")
    bookmark_manager = urlmarks.BookmarkManager(objects.qapp)
    objreg.register('bookmark-manager', bookmark_manager)

    log.init.debug("Initializing cookies...")
    cookies.init(objects.qapp)

    log.init.debug("Initializing cac
=======
from qutebrowser.config import configfiles

class Application(QApplication):
    def __init__(self, args):
        super().__init__(sys.argv)
        self.args = args

    def init(self, args):
        """Initialize everything."""
        log.init.debug("Starting init...")

        crashsignal.crash_handler.init_faulthandler()

        self.setQuitOnLastWindowClosed(False)
        quitter.instance.shutting_down.connect(self.closeAllWindows)

        _init_icon()
        _init_pulseaudio()

        loader.init()
        loader.load_components()
        try:
            _init_modules(args=args)
        except (OSError, UnicodeDecodeError, browsertab.WebTabError) as e:
            error.handle_fatal_exc(e, "Error while initializing!",
                                   no_err_windows=args.no_err_windows,
                                   pre_text="Error while initializing")
            sys.exit(usertypes.Exit.err_init)

        # Show changelog after upgrade if configured
        if config.get('changelog_after_upgrade') is not None:
            if config.get('changelog_after_upgrade') == 'all':
                configfiles.StateConfig()._set_changed_attributes()
                if state.qutebrowser_version_changed == VersionChange.major or \
                   state.qutebrowser_version_changed == VersionChange.minor or \
                   state.qutebrowser_version_changed == VersionChange.patch:
                    prompt.show_changelog()
>>>>>>> REPLACE
