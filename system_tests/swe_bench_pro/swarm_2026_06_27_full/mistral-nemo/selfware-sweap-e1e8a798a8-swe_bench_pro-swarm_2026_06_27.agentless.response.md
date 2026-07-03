### FILE: qutebrowser/commands/command.py
<<<<<<< SEARCH
        if self._count is None:
            assert param.default is not inspect.Parameter.empty
            self._count = param.default
        else:
            self._count += 1
=======
        if self._count is None:
            self._count = 1
        else:
            self._count += 1
>>>>>>> REPLACE