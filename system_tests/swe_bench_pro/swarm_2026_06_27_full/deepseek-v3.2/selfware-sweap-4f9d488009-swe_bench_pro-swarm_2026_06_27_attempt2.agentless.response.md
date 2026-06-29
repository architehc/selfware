### FILE: qutebrowser/components/hostblock.py
<<<<<<< SEARCH
    def _is_blocked(self, request_url):
        """Check if the given request URL is blocked.

        Args:
            request_url: The QUrl to check.

        Return:
            True if the request should be blocked, False otherwise.
        """
        if not self._enabled:
            return False

        # Check if content blocking is disabled for this URL
        first_party_url = self._current_first_party_url
        if first_party_url.isValid():
            if not config.instance.get('content.blocking.enabled',
                                       url=first_party_url):
                return False

        # Check whitelist
        if self._is_whitelisted(request_url):
            return False

        host = request_url.host()
        if not host:
            return False

        # Check exact host match
        if host in self._blocked_hosts:
            return True
        if host in self._config_blocked_hosts:
            return True

        return False
=======
    def _is_blocked(self, request_url):
        """Check if the given request URL is blocked.

        Args:
            request_url: The QUrl to check.

        Return:
            True if the request should be blocked, False otherwise.
        """
        if not self._enabled:
            return False

        # Check if content blocking is disabled for this URL
        first_party_url = self._current_first_party_url
        if first_party_url.isValid():
            if not config.instance.get('content.blocking.enabled',
                                       url=first_party_url):
                return False

        # Check whitelist
        if self._is_whitelisted(request_url):
            return False

        host = request_url.host()
        if not host:
            return False

        # Check each parent domain
        for candidate in urlutils.widened_hostnames(host):
            if candidate in self._blocked_hosts:
                return True
            if candidate in self._config_blocked_hosts:
                return True

        return False
>>>>>>> REPLACE

### FILE: qutebrowser/utils/urlutils.py
<<<<<<< SEARCH
def widened_hostnames(hostname):
    """Yield a hostname and its parent domains by stripping one label at a time.

    For example, for "a.b.c" yields "a.b.c", "b.c", "c".
    For a single label like "foobarbaz" yields only "foobarbaz".
    For an empty string yields nothing.
    Edge forms are preserved: ".c" yields ".c", "c"; "c." yields "c."; ".c." yields ".c.", "c.".

    Args:
        hostname: The hostname as a string, may be empty.

    Yields:
        Strings of successive parent domains.
    """
    if hostname is None or hostname == '':
        return

    yield hostname

    # Strip one label at a time
    parts = hostname.split('.')
    for i in range(1, len(parts)):
        parent = '.'.join(parts[i:])
        if parent:
            yield parent
=======
def widened_hostnames(hostname):
    """Yield a hostname and its parent domains by stripping one label at a time.

    For example, for "a.b.c" yields "a.b.c", "b.c", "c".
    For a single label like "foobarbaz" yields only "foobarbaz".
    For an empty string yields nothing.
    Edge forms are preserved: ".c" yields ".c", "c"; "c." yields "c."; ".c." yields ".c.", "c.".

    Args:
        hostname: The hostname as a string, may be empty.

    Yields:
        Strings of successive parent domains.
    """
    if hostname is None or hostname == '':
        return

    yield hostname

    # Strip one label at a time, but keep trailing dot if present
    has_trailing_dot = hostname.endswith('.')
    parts = hostname.rstrip('.').split('.')
    for i in range(1, len(parts)):
        parent = '.'.join(parts[i:])
        if has_trailing_dot:
            parent += '.'
        if parent:
            yield parent
>>>>>>> REPLACE