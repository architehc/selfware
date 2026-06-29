### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
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
from qutebrowser.config import (configexc, config, configdata, configutils,
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
=======
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
from enum import Enum

import yaml
from PyQt5.QtCore import pyqtSignal, pyqtSlot, QObject, QSettings, qVersion

import qutebrowser
from qutebrowser.config import (configexc, config, configdata, configutils,
                                configtypes)
from qutebrowser.keyinput import keyutils
from qutebrowser.utils import standarddir, utils, qtutils, log, urlmatch

if TYPE_CHECKING:
    from qutebrowser.misc import savemanager


# The StateConfig instance
state = cast('StateConfig', None)


_SettingsType = Dict[str, Dict[str, Any]]


class VersionChange(Enum):
    """Enum for version changes."""

    unknown = 'unknown'
    equal = 'equal'
    downgrade = 'downgrade'
    patch = 'patch'
    minor = 'minor'
    major = 'major'

    def matches_filter(self, filterstr: str) -> bool:
        """Return whether this version change matches the given filter.

        The filter string can be one of:
        - 'never': never show changelog
        - 'patch': show for patch, minor, major upgrades
        - 'minor': show for minor, major upgrades
        - 'major': show only for major upgrades
        """
        if filterstr == 'never':
            return False
        if filterstr == 'patch':
            return self in (VersionChange.patch, VersionChange.minor, VersionChange.major)
        if filterstr == 'minor':
            return self in (VersionChange.minor, VersionChange.major)
        if filterstr == 'major':
            return self == VersionChange.major
        # fallback: treat as 'never'
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
            self._set_changed_attributes(old_qutebrowser_version)
        else:
            self.qt_version_changed = False
            self.qutebrowser_version_changed = VersionChange.unknown

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

    def _set_changed_attributes(self, old_version: Optional[str]) -> None:
        """Set version changed attributes based on old version.

        Args:
            old_version: The old version string from the state file.
        """
        current = qutebrowser.__version__
        if old_version is None:
            self.qutebrowser_version_changed = VersionChange.unknown
            return

        if old_version == current:
            self.qutebrowser_version_changed = VersionChange.equal
            return

        try:
            old_parts = list(map(int, old_version.split('.')))
            cur_parts = list(map(int, current.split('.')))
        except ValueError:
            log.config.warning(f"Unable to parse old version {old_version}")
            self.qutebrowser_version_changed = VersionChange.unknown
            return

        # pad to length 3 (major.minor.patch) with zeros
        while len(old_parts) < 3:
            old_parts.append(0)
        while len(cur_parts) < 3:
            cur_parts.append(0)

        if old_parts[0] != cur_parts[0]:
            self.qutebrowser_version_changed = VersionChange.major
        elif old_parts[1] != cur_parts[1]:
            self.qutebrowser_version_changed = VersionChange.minor
        elif old_parts[2] != cur_parts[2]:
            self.qutebrowser_version_changed = VersionChange.patch
        else:
            # shouldn't happen because we already checked equality
            self.qutebrowser_version_changed = VersionChange.equal

        # detect downgrade
        if old_parts[0] > cur_parts[0]:
            self.qutebrowser_version_changed = VersionChange.downgrade
        elif old_parts[0] == cur_parts[0]:
            if old_parts[1] > cur_parts[1]:
                self.qutebrowser_version_changed = VersionChange.downgrade
            elif old_parts[1] == cur_parts[1]:
                if old_parts[2] > cur_parts[2]:
                    self.qutebrowser_version_changed = VersionChange.downgrade
>>>>>>> REPLACE