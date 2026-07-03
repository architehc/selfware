### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
def vdebug(self: logging.Logger,
           msg: str,
           *args: Any,
           **kwargs: Any) -> None:
    """Log with a VDEBUG level.

    VDEBUG is used when a debug message is rather verbose, and probably of
    little use to the end user or for post-mortem debugging, i.e. the content
    probably won't change unless the code changes.
    """
    if self.isEnabledFor(VDEBUG_LEVEL):
        # pylint: disable=protected-access
        self._log(VDEBUG_LEVEL, msg, args, **kwargs)
        # pylint: enable=protected-access


logging.Logger.vdebug = vdebug  # type: ignore[attr-defined]
>>>>>>> REPLACE

### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
class RAMHandler(logging.Handler):

    """Logging handler which keeps the messages in a deque in RAM.

    Loosely based on logging.BufferingHandler which is unsuitable because it
    uses a simple list rather than a deque.

    Attributes:
        _data: A deque containing the logging records.
    """

    def __init__(self, capacity: int) -> None:
        super().__init__()
        self.html_formatter: Optional[HTMLFormatter] = None
        if capacity!= -1:
            self._data: MutableSequence[logging.LogRecord] = collections.deque(
                maxlen=capacity
            )
        else:
            self._data = collections.deque()

    def emit(self, record: logging.LogRecord) -> None:
        self._data.append(record)

    def dump_log(self, html: bool = False, level: str = 'vdebug',
                 logfilter: LogFilter = None) -> str:
        """Dump the complete formatted log data as string.

        FIXME: We should do all the HTML formatting via jinja2.
        (probably obsolete when moving to a widget for logging,
        https://github.com/qutebrowser/qutebrowser/issues/34

        Args:
            html: Produce HTML rather than plaintext output.
            level: The minimal loglevel to show.
            logfilter: A LogFilter instance used to filter log lines.
        """
        minlevel = LOG_LEVELS.get(level.upper(), VDEBUG_LEVEL)

        if logfilter is None:
            logfilter = LogFilter(set())

        if html:
            assert self.html_formatter is not None
            fmt = self.html_formatter.format
        else:
            fmt = self.format

        self.acquire()
        try:
            lines = [fmt(record)
                     for record in self._data
                     if record.levelno >= minlevel and
                     logfilter.filter(record)]
        finally:
            self.release()
        return '\n'.join(lines)

    def change_log_capacity(self, capacity: int) -> None:
        self._data = collections.deque(self._data, maxlen=capacity)
>>>>>>> REPLACE

### FILE: qutebrowser/browser/qtnetworkdownloads.py
<<<<<<< SEARCH
def _init_reply(self, reply):
        """Set a new reply and connect its signals.

        Args:
            reply: The QNetworkReply to handle.
        """
        self.done = False
        self.successful = False
        self._reply = reply
        reply.setReadBufferSize(16 * 1024 * 1024)  # 16 MB

        reply.downloadProgress.connect(self.stats.on_download_progress)
        reply.finished.connect(self._on_reply_finished)
        reply.errorOccurred.connect(self._on_reply_error)
        reply.readyRead.connect(self._on_ready_read)
        reply.metaDataChanged.connect(self._on_meta_data_changed)

        # WORKAROUND for Qt corrupting data loaded from cache:
        # https://bugreports.qt.io/browse/QTBUG-42757
        reply.setAttribute(QNetworkRequest.Attribute.RedirectionLimitAttribute,
                           self._MAX_REDIRECTS)
        req.setMaximumRedirectsAllowed(self._MAX_REDIRECTS)

        return self.get_request(req, **kwargs)
>>>>>>> REPLACE

### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
class LogFilter:

    """Filter log records based on a set of log names."""

    def __init__(self, names: Set[str]) -> None:
        self.names = names

    def filter(self, record: logging.LogRecord) -> bool:
        """Filter a log record.

        Args:
            record: The log record to filter.
        """
        if record.name.split('.')[0] in self.names:
            return not self.negated
        return self.negated
>>>>>>> REPLACE