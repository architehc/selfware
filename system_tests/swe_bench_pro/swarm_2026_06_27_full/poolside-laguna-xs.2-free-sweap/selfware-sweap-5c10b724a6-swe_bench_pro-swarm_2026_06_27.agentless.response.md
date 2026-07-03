
### FILE: qutebrowser/config/configfiles.py
<<<<<<< SEARCH
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
from qutebrowser.config import (configexc, config, configdata, configutils,
                                configtypes)
from qutebrowser.keyinput import keyutils
from qutebrowser.utils import standarddir, utils, qtutils, log, urlmatch
=======
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
import logging
from enum import Enum
from typing import (TYPE_CHECKING, Any, Dict, Iterable, Iterator, List, Mapping,
                    MutableMapping, Optional, Tuple, cast)

import yaml
from PyQt5.QtCore import pyqtSignal, pyqtSlot, QObject, QSettings, qVersion

import qutebrowser
from qutebrowser.config import (configexc, config, configdata, configutils,
                                configtypes)
from qutebrowser.keyinput import keyutils
from qutebrowser.utils import standarddir, utils, qtutils, log, urlmatch


class VersionChange(Enum):

    """Represents the type of version change."""

    unknown = 0
    equal = 1
    downgrade = 2
    patch = 3
    minor = 4
    major = 5

    def matches_filter(self, filterstr: str) -> bool:
        """Check if this version change matches the given filter string."""
        if filterstr == 'never':
            return False
        if filterstr == 'all':
            return True
        return self.name == filterstr
>>>>>>> REPLACE
