
### FILE: qutebrowser/utils/urlutils.py
<<<<<<< SEARCH
def _get_host(url: QUrl) -> str:
    """Get the host from a URL, handling qute:// URLs."""
    if url.scheme() == 'qute':
        return ''
    return url.host()
=======
def widened_hostnames(hostname: Optional[str]) -> Iterator[str]:
    """Generate a sequence of hostnames by progressively removing labels.

    For a multi-label hostname like a.b.c, yields ["a.b.c", "b.c", "c"].
    For a single-label hostname like foobarbaz, yields ["foobarbaz"].
    For an empty string or None, yields nothing.
    """
    if not hostname:
        return
    yield hostname
    parts = hostname.split('.')
    while len(parts) > 1:
        parts = parts[1:]
        yield '.'.join(parts)


def _get_host(url: QUrl) -> str:
    """Get the host from a URL, handling qute:// URLs."""
    if url.scheme() == 'qute':
        return ''
    return url.host()
>>>>>>> REPLACE

### FILE: qutebrowser/components/hostblock.py
<<<<<<< SEARCH
    def _is_blocked(self, url: QUrl) -> bool:
        """Check if a URL is blocked."""
        if not self._is_enabled_for_url(url):
            return False

        host = url.host()
        if not host:
            return False

        return host in self._blocked_hosts or host in self._config_blocked_hosts
=======
    def _is_blocked(self, url: QUrl) -> bool:
        """Check if a URL is blocked."""
        if not self._is_enabled_for_url(url):
            return False

        host = url.host()
        if not host:
            return False

        # Check whitelist first
        if self._is_whitelisted(url):
            return False

        # Check each widened hostname against blocked sets
        for hostname in urlutils.widened_hostnames(host):
            if hostname in self._blocked_hosts or hostname in self._config_blocked_hosts:
                return True

        return False
>>>>>>> REPLACE
