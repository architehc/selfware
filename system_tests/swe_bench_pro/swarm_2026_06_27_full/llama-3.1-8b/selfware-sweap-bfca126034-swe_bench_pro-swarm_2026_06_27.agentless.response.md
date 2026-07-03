### FILE: scripts/hostblock.py
<<<<<<< SEARCH
def _is_blocked(self, url):
    """Check if a URL is blocked."""
    if not self._config.content.blocking.enabled:
        return False
    if self._config.content.blocking.method == "hosts":
        return self._is_blocked_hosts(url)
    return False
>>>>>>> REPLACE

### FILE: scripts/hostblock.py
<<<<<<< SEARCH
def _is_blocked_hosts(self, url):
    """Check if a URL is blocked by the hosts-based blocking method."""
    host = url.host()
    if host in self._blocked_hosts:
        return True
    # Check each parent domain
    for label in reversed(host.split('.')):
        host = '.'.join(host.split('.')[:-1])
        if host in self._blocked_hosts:
            return True
    return False
>>>>>>> REPLACE

### FILE: scripts/hostblock.py
<<<<<<< SEARCH
def _is_blocked_hosts(self, url):
    """Check if a URL is blocked by the hosts-based blocking method."""
    host = url.host()
    if host in self._blocked_hosts:
        return True
    # Check each parent domain
    for label in reversed(host.split('.')):
        host = '.'.join(host.split('.')[:-1])
        if host in self._blocked_hosts:
            return True
    return False
>>>>>>> REPLACE

### FILE: scripts/hostblock.py
<<<<<<< SEARCH
def _is_blocked_hosts(self, url):
    """Check if a URL is blocked by the hosts-based blocking method."""
    host = url.host()
    if host in self._blocked_hosts:
        return True
    # Check each parent domain
    for label in reversed(host.split('.')):
        host = '.'.join(host.split('.')[:-1])
        if host in self._blocked_hosts:
            return True
    return False
>>>>>>> REPLACE

### FILE: scripts/hostblock.py
<<<<<<< SEARCH
def _is_blocked_hosts(self, url):
    """Check if a URL is blocked by the hosts-based blocking method."""
    host = url.host()
    if host in self._blocked_hosts:
        return True
    # Check each parent domain
    for label in reversed(host.split('.')):
        host = '.'.join(host.split('.')[:-1])
        if host in self._blocked_hosts:
            return True
    return False
>>>>>>> REPLACE

### FILE: scripts/hostblock.py
<<<<<<< SEARCH
def _is_blocked_hosts(self, url):
    """Check if a URL is blocked by the hosts-based blocking method."""
    host = url.host()
    if host in self._blocked_hosts:
        return True
    # Check each parent domain
    for label in reversed(host.split('.')):
        host = '.'.join(host.split('.')[:-1])
        if host in self._blocked_hosts:
            return True
    return False
>>>>>>> REPLACE

### FILE: scripts/hostblock.py
<<<<<<< SEARCH
def _is_blocked_hosts(self, url):
    """Check if a URL is blocked by the hosts-based blocking method."""
    host = url.host()
    if host in self._blocked_hosts:
        return True
    # Check each parent domain
    for label in reversed(host.split('.')):
        host = '.'.join(host.split('.')[:-1])
        if host in self._blocked_hosts:
            return True
    return False
>>>>>>> REPLACE