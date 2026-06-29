### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
from qutebrowser.utils import qtlog
=======
# from qutebrowser.utils import qtlog
>>>>>>> REPLACE

### FILE: qutebrowser/utils/qtlog.py
<<<<<<< SEARCH
# This file does not exist yet
=======
"""Qt-specific logging utilities."""

import logging
import contextlib

logger = logging.getLogger('qt-tests')

@contextlib.contextmanager
def hide_qt_warning(filter_string: str, logger_name: str) -> None:
    """Context manager to hide Qt warnings containing the filter string.

    Args:
        filter_string: The string to filter out from warnings.
        logger_name: The name of the logger to apply the filter to.
    """
    original_filter = logger.filters[0] if logger.filters else None
    try:
        logger.addFilter(QtWarningFilter(filter_string))
        yield
    finally:
        if original_filter:
            logger.filters = [original_filter]
        else:
            logger.filters = []

class QtWarningFilter(logging.Filter):
    """Filter for Qt warnings based on a filter string."""

    def __init__(self, filter_string: str) -> None:
        self.filter_string = filter_string

    def filter(self, record: logging.LogRecord) -> bool:
        """Filter log records based on the filter string.

        Args:
            record: The log record to filter.

        Returns:
            True if the record should be logged, False otherwise.
        """
        if record.levelno == logging.WARNING and record.getMessage().find(self.filter_string)!= -1:
            return False
        return True
>>>>>>> REPLACE

### FILE: tests/unit/utils/test_qtlog.py
<<<<<<< SEARCH
from qutebrowser import qutebrowser
from qutebrowser.utils import log, qtlog
=======
from qutebrowser.utils import qtlog
>>>>>>> REPLACE

### FILE: qutebrowser/browser/qtnetworkdownloads.py
<<<<<<< SEARCH
from qutebrowser.utils import log
=======
from qutebrowser.utils import qtlog
>>>>>>> REPLACE