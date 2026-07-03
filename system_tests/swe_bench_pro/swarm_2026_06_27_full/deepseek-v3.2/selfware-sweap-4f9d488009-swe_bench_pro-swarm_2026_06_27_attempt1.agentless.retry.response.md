### FILE: qutebrowser/components/hostblock.py
<<<<<<< SEARCH
    def _is_blocked(self, request_url: QUrl, first_party_url: QUrl) -> bool:
        """Check whether the given request should be blocked.

        Args:
            request_url: The URL of the request to check.
            first_party_url: The URL of the page that triggered the request.

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
        if self._is_whitelisted(request_url):
            return False

        host = request_url.host()
        if not host:
            return False

        # Remove trailing dot for consistency
        if host.endswith('.'):
            host = host[:-1]

        # Check against blocked hosts
        blocked = (host in self._blocked_hosts or
                   host in self._config_blocked_hosts)
        return blocked
=======
    def _is_blocked(self, request_url: QUrl, first_party_url: QUrl) -> bool:
        """Check whether the given request should be blocked.

        Args:
            request_url: The URL of the request to check.
            first_party_url: The URL of the page that triggered the request.

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
        if self._is_whitelisted(request_url):
            return False

        host = request_url.host()
        if not host:
            return False

        # Remove trailing dot for consistency
        if host.endswith('.'):
            host = host[:-1]

        # Generate widening sequence: a.b.c -> ["a.b.c", "b.c", "c"]
        # For single-label host like "localhost" -> ["localhost"]
        # For empty string -> []
        def widen(h: str) -> list[str]:
            if not h:
                return []
            parts = h.split('.')
            return ['.'.join(parts[i:]) for i in range(len(parts))]

        for candidate in widen(host):
            if (candidate in self._blocked_hosts or
                candidate in self._config_blocked_hosts):
                return True

        return False
>>>>>>> REPLACE

### FILE: qutebrowser/components/hostblock.py
<<<<<<< SEARCH
    def _is_whitelisted(self, url: QUrl) -> bool:
        """Check whether the given URL is whitelisted.

        Args:
            url: The URL to check.

        Return:
            True if the URL is whitelisted, False otherwise.
        """
        host = url.host()
        if not host:
            return False

        # Remove trailing dot for consistency
        if host.endswith('.'):
            host = host[:-1]

        # Check against whitelisted hosts
        return host in self._whitelisted_hosts
=======
    def _is_whitelisted(self, url: QUrl) -> bool:
        """Check whether the given URL is whitelisted.

        Args:
            url: The URL to check.

        Return:
            True if the URL is whitelisted, False otherwise.
        """
        host = url.host()
        if not host:
            return False

        # Remove trailing dot for consistency
        if host.endswith('.'):
            host = host[:-1]

        # Generate widening sequence for whitelist as well, because whitelist
        # should also apply to subdomains if a parent domain is whitelisted.
        def widen(h: str) -> list[str]:
            if not h:
                return []
            parts = h.split('.')
            return ['.'.join(parts[i:]) for i in range(len(parts))]

        for candidate in widen(host):
            if candidate in self._whitelisted_hosts:
                return True

        return False
>>>>>>> REPLACE