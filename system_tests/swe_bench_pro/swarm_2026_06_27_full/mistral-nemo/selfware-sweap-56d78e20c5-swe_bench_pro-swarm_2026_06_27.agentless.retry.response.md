### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
        ('VDEBUG', logging.VDEBUG),  # type: ignore[attr-defined]
        ('DEBUG', logging.DEBUG),
        ('INFO', logging.INFO),
        ('WARNING', logging.WARNING),
        ('ERROR', logging.ERROR),
        ('CRITICAL', logging.CRITICAL),
    LOG_LEVELS = {
        'VDEBUG': logging.VDEBUG,  # type: ignore[attr-defined]
        'DEBUG': logging.DEBUG,
        'INFO': logging.INFO,
        'WARNING': logging.WARNING,
        'ERROR': logging.ERROR,
        'CRITICAL': logging.CRITICAL,
    },
REPLACE
        ('VDEBUG', 9),
        ('DEBUG', logging.DEBUG),
        ('INFO', logging.INFO),
        ('WARNING', logging.WARNING),
        ('ERROR', logging.ERROR),
        ('CRITICAL', logging.CRITICAL),
    LOG_LEVELS = {
        'VDEBUG': 9,
        'DEBUG': logging.DEBUG,
        'INFO': logging.INFO,
        'WARNING': logging.WARNING,
        'ERROR': logging.ERROR,
        'CRITICAL': logging.CRITICAL,
    },
>>>>>>> REPLACE