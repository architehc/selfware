
### FILE: qutebrowser/utils/qtlog.py
<<<<<<< SEARCH
=======
# Copyright 2014-2021 Florian Bru <The Compiler> <mail@qutebrowser.org>
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

"""Utilities related to Qt logging."""

import contextlib
import logging
from typing import Generator


class QtWarningFilter(logging.Filter):

    """A logging filter to suppress Qt warnings matching a pattern."""

    def __init__(self, pattern: str) -> None:
        """Initialize the filter.

        Args:
            pattern: The pattern to match at the start of warning messages.
        """
        self._pattern = pattern

    def filter(self, record: logging.LogRecord) -> bool:
        """Return False if the message starts with the pattern, True otherwise.

        Args:
            record: The log record to filter.

        Returns:
            False if the message should be suppressed, True otherwise.
        """
        message = record.getMessage().lstrip()
        return not message.startswith(self._pattern)


@contextlib.contextmanager
def hide_qt_warning(pattern: str, *logger_names: str) -> Generator[None, None, None]:
    """Context manager to hide Qt warnings matching a pattern.

    Args:
        pattern: The pattern to match at the start of warning messages.
        *logger_names: Names of loggers to apply the filter to.
    """
    filters = [QtWarningFilter(pattern) for _ in logger_names]
    for logger_name, log_filter in zip(logger_names, filters):
        logging.getLogger(logger_name).addFilter(log_filter)
    try:
        yield
    finally:
        for logger_name, log_filter in zip(logger_names, filters):
            logging.getLogger(logger_name).removeFilter(log_filter)
>>>>>>> REPLACE

### FILE: qutebrowser/browser/qtnetworkdownloads.py
<<<<<<< SEARCH
from qutebrowser.utils import message, usertypes, log, urlutils, utils, debug, objreg
=======
from qutebrowser.utils import message, usertypes, log, urlutils, utils, debug, objreg
from qutebrowser.utils import qtlog
>>>>>>> REPLACE
>>>>>>> REPLACE

### FILE: qutebrowser/browser/qtnetworkdownloads.py
<<<<<<< SEARCH
        with log.hide_qt_warning('QNetworkReplyImplPrivate::error: Internal '
                                 'problem, this method must only be called '
                                 'once.'):
=======
        with qtlog.hide_qt_warning('QNetworkReplyImplPrivate::error: Internal '
                                   'problem, this method must only be called '
                                   'once.'):
>>>>>>> REPLACE
