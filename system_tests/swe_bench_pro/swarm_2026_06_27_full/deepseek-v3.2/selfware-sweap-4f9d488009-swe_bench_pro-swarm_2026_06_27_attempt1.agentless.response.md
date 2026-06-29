### FILE: qutebrowser/components/hostblock.py
<<<<<<< SEARCH
    def _is_blocked(self, request_url: QUrl, first_party_url: QUrl) -> bool:
        """Check whether the given request URL should be blocked.

        Args:
            request_url: The URL of the request to check.
            first_party_url: The URL of the page that initiated the request.

        Return:
            True if the request should be blocked, False otherwise.
        """
        if not self._enabled:
            return False

        # Check if content blocking is disabled for the first-party URL
        if not config.instance.get('content.blocking.enabled',
                                   url=first_party_url):
            return False

        # Check whitelist
        if self._is_whitelisted(request_url, first_party_url):
            return False

        # Check blocked hosts
        host = request_url.host()
        if host in self._blocked_hosts or host in self._config_blocked_hosts:
            return True

        return False
=======
    def _is_blocked(self, request_url: QUrl, first_party_url: QUrl) -> bool:
        """Check whether the given request URL should be blocked.

        Args:
            request_url: The URL of the request to check.
            first_party_url: The URL of the page that initiated the request.

        Return:
            True if the request should be blocked, False otherwise.
        """
        if not self._enabled:
            return False

        # Check if content blocking is disabled for the first-party URL
        if not config.instance.get('content.blocking.enabled',
                                   url=first_party_url):
            return False

        # Check whitelist
        if self._is_whitelisted(request_url, first_party_url):
            return False

        # Check blocked hosts with subdomain widening
        host = request_url.host()
        for candidate in urlutils.widened_hostnames(host):
            if (candidate in self._blocked_hosts or
                candidate in self._config_blocked_hosts):
                return True

        return False
>>>>>>> REPLACE

### FILE: qutebrowser/utils/urlutils.py
<<<<<<< SEARCH
def widened_hostnames(hostname: str) -> Iterator[str]:
    """Yield a sequence of hostnames for checking parent-domain blocking.

    For a multi-label hostname like a.b.c, the sequence is:
        a.b.c, b.c, c

    For a single-label hostname like foobarbaz, the sequence is:
        foobarbaz

    For an empty string, the sequence is empty.

    Edge forms are preserved:
        ".c" -> ".c", "c"
        "c." -> "c."
        ".c." -> ".c.", "c."

    Args:
        hostname: The hostname to widen.

    Yields:
        Successively wider hostname components.
    """
    if not hostname:
        return

    yield hostname

    # Strip one label at a time
    while '.' in hostname:
        # Find the first dot
        dot = hostname.find('.')
        if dot == -1:
            break
        # Strip up to and including the dot
        hostname = hostname[dot + 1:]
        if hostname:
            yield hostname
=======
def widened_hostnames(hostname: str) -> Iterator[str]:
    """Yield a sequence of hostnames for checking parent-domain blocking.

    For a multi-label hostname like a.b.c, the sequence is:
        a.b.c, b.c, c

    For a single-label hostname like foobarbaz, the sequence is:
        foobarbaz

    For an empty string, the sequence is empty.

    Edge forms are preserved:
        ".c" -> ".c", "c"
        "c." -> "c."
        ".c." -> ".c.", "c."

    Args:
        hostname: The hostname to widen.

    Yields:
        Successively wider hostname components.
    """
    if hostname is None:
        return

    if not hostname:
        return

    yield hostname

    # Strip one label at a time
    while '.' in hostname:
        # Find the first dot
        dot = hostname.find('.')
        if dot == -1:
            break
        # Strip up to and including the dot
        hostname = hostname[dot + 1:]
        if hostname:
            yield hostname
>>>>>>> REPLACE