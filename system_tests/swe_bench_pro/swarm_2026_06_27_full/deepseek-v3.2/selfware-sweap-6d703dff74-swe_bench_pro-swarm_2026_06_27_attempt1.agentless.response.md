### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
@contextlib.contextmanager
def hide_qt_warning(pattern: str, logger: str = 'qt') -> Iterator[None]:
    """Hide Qt warnings matching the given regex."""
    log_filter = QtWarningFilter(pattern)
    logger_obj = logging.getLogger(logger)
    logger_obj.addFilter(log_filter)
    try:
        yield
    finally:
        logger_obj.removeFilter(log_filter)
=======
@contextlib.contextmanager
def hide_qt_warning(pattern: str, logger: str = 'qt') -> Iterator[None]:
    """Hide Qt warnings matching the given regex."""
    from qutebrowser.utils import qtlog
    yield from qtlog.hide_qt_warning(pattern, logger)
>>>>>>> REPLACE

### FILE: qutebrowser/utils/qtlog.py
<<<<<<< SEARCH
"""Tests for qutebrowser.utils.qtlog."""
=======
"""Tests for qutebrowser.utils.qtlog."""

import contextlib
import logging
from typing import Iterator
>>>>>>> REPLACE

### FILE: qutebrowser/utils/qtlog.py
<<<<<<< SEARCH
class TestHideQtWarning:

    """Tests for hide_qt_warning/QtWarningFilter."""

    @pytest.fixture
    def qt_logger(self):
        return logging.getLogger('qt-tests')

    def test_unfiltered(self, qt_logger, caplog):
        with qtlog.hide_qt_warning("World", 'qt-tests'):
            with caplog.at_level(logging.WARNING, 'qt-tests'):
                qt_logger.warning("Hello World")
        assert len(caplog.records) == 1
        record = caplog.records[0]
        assert record.levelname == 'WARNING'
        assert record.message == "Hello World"

    @pytest.mark.parametrize('line', [
        "Hello",  # exact match
        "Hello World",  # match at start of line
        "  Hello World  ",  # match with spaces
    ])
    def test_filtered(self, qt_logger, caplog, line):
        with qtlog.hide_qt_warning("Hello", 'qt-tests'):
            with caplog.at_level(logging.WARNING, 'qt-tests'):
                qt_logger.warning(line)
        assert not caplog.records
=======
class QtWarningFilter(logging.Filter):

    """Filter to filter Qt warnings.

    Attributes:
        _pattern: The start of the message.
    """

    def __init__(self, pattern: str) -> None:
        super().__init__()
        self._pattern = pattern

    def filter(self, record: logging.LogRecord) -> bool:
        """Determine if the specified record is to be logged."""
        do_log = not record.msg.strip().startswith(self._pattern)
        return do_log


@contextlib.contextmanager
def hide_qt_warning(pattern: str, logger: str = 'qt') -> Iterator[None]:
    """Hide Qt warnings matching the given regex."""
    log_filter = QtWarningFilter(pattern)
    logger_obj = logging.getLogger(logger)
    logger_obj.addFilter(log_filter)
    try:
        yield
    finally:
        logger_obj.removeFilter(log_filter)


class TestHideQtWarning:

    """Tests for hide_qt_warning/QtWarningFilter."""

    @pytest.fixture
    def qt_logger(self):
        return logging.getLogger('qt-tests')

    def test_unfiltered(self, qt_logger, caplog):
        with hide_qt_warning("World", 'qt-tests'):
            with caplog.at_level(logging.WARNING, 'qt-tests'):
                qt_logger.warning("Hello World")
        assert len(caplog.records) == 1
        record = caplog.records[0]
        assert record.levelname == 'WARNING'
        assert record.message == "Hello World"

    @pytest.mark.parametrize('line', [
        "Hello",  # exact match
        "Hello World",  # match at start of line
        "  Hello World  ",  # match with spaces
    ])
    def test_filtered(self, qt_logger, caplog, line):
        with hide_qt_warning("Hello", 'qt-tests'):
            with caplog.at_level(logging.WARNING, 'qt-tests'):
                qt_logger.warning(line)
        assert not caplog.records
>>>>>>> REPLACE