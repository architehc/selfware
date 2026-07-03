### FILE: qutebrowser/components/hostblock.py
<<<<<<< SEARCH
def _is_blocked(self, url):
    host = url.host()
    if not self._blocking_enabled(url):
        return False
    if self._is_whitelisted(url):
        return False
    if self._is_blocked_host(host):
        return True
    return False

def _is_blocked_host(self, host):
    for blocked_host in self._blocked_hosts:
        if host == blocked_host:
            return True
    return False
=======
def _is_blocked(self, url):
    host = url.host()
    if not self._blocking_enabled(url):
        return False
    if self._is_whitelisted(url):
        return False
    if self._is_blocked_host(host):
        return True
    return False

def _is_blocked_host(self, host):
    widened_hosts = self._widen_hostname(host)
    for blocked_host in self._blocked_hosts:
        if blocked_host in widened_hosts:
            return True
    return False

def _widen_hostname(self, host):
    parts = host.split('.')
    widened_hosts = []
    while parts:
        widened_hosts.append('.'.join(parts))
        parts.pop(0)
    return widened_hosts
>>>>>>> REPLACE