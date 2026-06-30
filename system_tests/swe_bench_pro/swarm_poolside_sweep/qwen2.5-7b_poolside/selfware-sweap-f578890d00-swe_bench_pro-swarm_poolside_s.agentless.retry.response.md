### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
    elif record.name.split('.')[0] in self.names:
        return not self.negated
    return self.negated
=======
    elif record.name.split('.')[0] in self.names:
        return not self.negated
    return self.negated

    def hide_qt_warning(self, warning_filter: str) -> None:
        """Hide Qt warnings matching the filter string.

        Args:
            warning_filter: The filter string to match against warning messages.
        """
        self.names.add(warning_filter)
>>>>>>> REPLACE

### FILE: tests/unit/utils/test_log.py
<<<<<<< SEARCH
    def test_logfilter_benchmark(self, logger, benchmark):
        record = self._make_record(logger, 'unfiltered')
        filters = set(log.LOGGER_NAMES)  # Extreme case
        logfilter = log.LogFilter(filters, negated=False)
        benchmark(lambda: logfilter.filter(record))
=======
    def test_logfilter_benchmark(self, logger, benchmark):
        record = self._make_record(logger, 'unfiltered')
        filters = set(log.LOGGER_NAMES)  # Extreme case
        logfilter = log.LogFilter(filters, negated=False)
        benchmark(lambda: logfilter.filter(record))

    def test_hide_qt_warning(self, logger):
        """Test hide_qt_warning function."""
        logfilter = log.LogFilter(set())
        logfilter.hide_qt_warning('qt')
        record = self._make_record(logger, 'qt.warning')
        assert not logfilter.filter(record)
>>>>>>> REPLACE

### FILE: qutebrowser/browser/qtnetworkdownloads.py
<<<<<<< SEARCH
        reply.downloadProgress.connect(self.stats.on_download_progress)
        reply.finished.connect(self._on_reply_finished)
        reply.errorOccurred.connect(self._on_reply_error)
        reply.readyRead.connect(self._on_ready_read)
        reply.metaDataChanged.connect(self._on_meta_data_changed)
=======
        reply.downloadProgress.connect(self.stats.on_download_progress)
        reply.finished.connect(self._on_reply_finished)
        reply.errorOccurred.connect(self._on_reply_error)
        reply.readyRead.connect(self._on_ready_read)
        reply.metaDataChanged.connect(self._on_meta_data_changed)

    def hide_qt_warning(self, warning_filter: str) -> None:
        """Hide Qt warnings matching the filter string.

        Args:
            warning_filter: The filter string to match against warning messages.
        """
        self._qt_warning_filter = warning_filter
>>>>>>> REPLACE