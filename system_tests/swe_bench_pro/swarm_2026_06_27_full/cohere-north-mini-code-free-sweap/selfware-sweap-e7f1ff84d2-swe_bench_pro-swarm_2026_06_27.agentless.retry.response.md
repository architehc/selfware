### FILE: tests/unit/utils/test_log.py
<<<<<<< SEARCH
from qutebrowser.utils import log
=======
from qutebrowser.utils import log, qtlog
>>>>>>> REPLACE

### FILE: qutebrowser/browser/qtnetworkdownloads.py
<<<<<<< SEARCH
from qutebrowser.utils import message, usertypes, log, urlutils, utils, debug, objreg
=======
from qutebrowser.utils import message, usertypes, log, qtlog, urlutils, utils, debug, objreg
>>>>>>> REPLACE
<<<<<<< SEARCH
            with log.hide_qt_warning('QNetworkReplyImplPrivate::error: Internal '
                                     'problem, this method must only be called '
                                     'once.'):
=======
            with qtlog.hide_qt_warning('QNetworkReplyImplPrivate::error: Internal '
                                     'problem, this method must only be called '
                                     'once.'):
>>>>>>> REPLACE