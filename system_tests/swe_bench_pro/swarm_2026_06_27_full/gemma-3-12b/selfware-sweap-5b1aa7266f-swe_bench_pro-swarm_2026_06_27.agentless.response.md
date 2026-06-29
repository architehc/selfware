### FILE: qutebrowser/utils/urlutils.py
<<<<<<< SEARCH
def widened_hostnames(host: str) -> List[str]:
    """Return a sequence of hostnames to check against, starting with the
    given host and stripping labels from the right.
    """
    if not host:
        return []
    parts = host.split('.')
    for i in range(len(parts)):
        yield '.'.join(parts[i:])
>>>>>>> REPLACE
### FILE: qutebrowser/components/hostblock.py
<<<<<<< SEARCH
    def _is_blocked(self, url: QUrl) -> bool:
        """Check if the given URL is blocked."""
        host = url.host()
        if not host:
            return False

        # Check if the host is in the blocked hosts set.
        if host in self._blocked_hosts:
            return True

        # Check if any parent domain is in the blocked hosts set.
        for parent_host in urlutils.widened_hostnames(host):
            if parent_host in self._blocked_hosts:
                return True

        return False
>>>>>>> REPLACE