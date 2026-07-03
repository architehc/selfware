### FILE: tests/unit/utils/test_log.py
<<<<<<< SEARCH
from qutebrowser.utils import log
=======
from qutebrowser.utils.qtlog import log
>>>>>>> REPLACE

### FILE: qutebrowser/utils/log.py
<<<<<<< SEARCH
from qutebrowser.utils import qtlog
=======
from qutebrowser.utils import log as qtlog
>>>>>>> REPLACE

### FILE: qutebrowser/browser/qtnetworkdownloads.py
<<<<<<< SEARCH
from qutebrowser.browser.webkit.network import networkmanager
=======
from qutebrowser.browser.qt.network import networkmanager
>>>>>>> REPLACE

### FILE: scripts/asciidoc2html.py
<<<<<<< SEARCH
sys.path.insert(0, str(REPO_ROOT))
=======
sys.path.insert(0, str(REPO_ROOT / 'qutebrowser'))
>>>>>>> REPLACE

### FILE: scripts/asciidoc2html.py
<<<<<<< SEARCH
sys.path.insert(0, str(REPO_ROOT))
=======
sys.path.insert(0, str(REPO_ROOT / 'qutebrowser'))
>>>>>>> REPLACE