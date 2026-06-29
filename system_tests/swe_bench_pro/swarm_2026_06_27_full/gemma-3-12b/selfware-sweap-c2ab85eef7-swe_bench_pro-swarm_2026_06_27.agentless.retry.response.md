### FILE: qutebrowser/browser/qtnetworkdownloads.py
<<<<<<< SEARCH
with log.hide_qt_warning('QNetworkReplyImplPrivate::error: Internal 
                                 'problem, this method must only be called 
                                 'once.'):
=======
with log.hide_qt_warning('QNetworkReplyImplPrivate::error: Internal '
                                 'problem, this method must only be called '
                                 'once.'):
>>>>>>> REPLACE

### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
class LogFilter:

    """Filter logging records based on a set of logger names.

    Attributes:
        names: A set of logger names to filter for.
        negated: Whether to negate the filter (i.e., filter out the
                 specified names).
        only_debug: Whether to only filter messages more important than
                    debug.
    """

    def __init__(self, names: Set[str], negated: bool = False,
                 only_debug: bool = False) -> None:
=======
class LogFilter:

    """Filter logging records based on a set of logger names.

    Attributes:
        names: A set of logger names to filter for.
        negated: Whether to negate the filter (i.e., filter out the
                 specified names).
        only_debug: Whether to only filter messages more important than
                    debug.
    """

    def __init__(self, names: Set[str], negated: bool = False,
                 only_debug: bool = False) -> None:
>>>>>>> REPLACE

### FILE: tests/unit/utils/test_log.py
<<<<<<< SEARCH
    @pytest.mark.parametrize('cli, conf, expected', [
        (None, 'info', logging.INFO),
        (None, 'warning', logging.WARNING),
        ('info', 'warning', logging.INFO),
        ('warning', 'info', logging.WARNING),
    ])
    def test_init_from_config_console(self, cli, conf, expected, args,
                                      config_stub):
        args.debug = False
        args.loglevel = cli
        log.init_log(args)

        config_stub.val.logging.level.console = conf
        log.init_from_config(config_stub.val)
        assert log.console_handler.level == expected

    @pytest.mark.parametrize('conf, expected', [
        ('vdebug', logging.VDEBUG),
        ('debug', logging.DEBUG),
        ('info', logging.INFO),
        ('critical', logging.CRITICAL),
    ])
    def test_init_from_config_ram(self, conf, expected, args, config_stub):
        args.debug = False
        log.init_log(args)

        config_stub.val.logging.level.ram = conf
        log.init_from_config(config_stub.val)
        assert log.ram_handler.level == expected

    def test_init_from_config_consistent_default(self, config_stub, empty_args):
        """Ensure config defaults are consistent with the builtin defaults."""
        log.init_log(empty_args)

        assert log.ram_handler.level == logging.DEBUG
        ass
=======
    @pytest.mark.parametrize('cli, conf, expected', [
        (None, 'info', logging.INFO),
        (None, 'warning', logging.WARNING),
        ('info', 'warning', logging.INFO),
        ('warning', 'info', logging.WARNING),
    ])
    def test_init_from_config_console(self, cli, conf, expected, args,
                                      config_stub):
        args.debug = False
        args.loglevel = cli
        log.init_log(args)

        config_stub.val.logging.level.console = conf
        log.init_from_config(config_stub.val)
        assert log.console_handler.level == expected

    @pytest.mark.parametrize('conf, expected', [
        ('vdebug', logging.VDEBUG),
        ('debug', logging.DEBUG),
        ('info', logging.INFO),
        ('critical', logging.CRITICAL),
    ])
    def test_init_from_config_ram(self, conf, expected, args, config_stub):
        args.debug = False
        log.init_log(args)

        config_stub.val.logging.level.ram = conf
        log.init_from_config(config_stub.val)
        assert log.ram_handler.level == expected

    def test_init_from_config_consistent_default(self, config_stub, empty_args):
        """Ensure config defaults are consistent with the builtin defaults."""
        log.init_log(empty_args)

        assert log.ram_handler.level == logging.DEBUG
        assert log.console_handler.level == logging.INFO
>>>>>>> REPLACE
