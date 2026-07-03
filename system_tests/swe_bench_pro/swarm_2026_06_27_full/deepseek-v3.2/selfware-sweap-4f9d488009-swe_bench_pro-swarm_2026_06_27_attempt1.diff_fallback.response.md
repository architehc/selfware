diff --git a/qutebrowser/components/hostblock.py b/qutebrowser/components/hostblock.py
index 1a2b3c4..5d6e7f9 100644
--- a/qutebrowser/components/hostblock.py
+++ b/qutebrowser/components/hostblock.py
@@ -1,5 +1,6 @@
 # vim: ft=python fileencoding=utf-8 sts=4 sw=4 et:
 
+import urllib.parse
 from typing import Optional, Set
 
 from qutebrowser.config import config
@@ -7,6 +8,7 @@ from qutebrowser.utils import message, urlmatch, usertypes
 from qutebrowser.misc import objects
 
 
+
 class HostBlocker:
 
     """Check whether a request should be blocked based on its host.
@@ -39,6 +41,28 @@ class HostBlocker:
         self._blocked_hosts: Set[str] = set()
         self._allowed_hosts: Set[str] = set()
 
+    def _hostname_sequence(self, hostname: str) -> list[str]:
+        """Generate the sequence of hostnames to check for blocking.
+
+        For a multi-label hostname like a.b.c, the sequence is
+        ["a.b.c", "b.c", "c"].
+        For a single-label hostname like foobarbaz, the sequence is
+        ["foobarbaz"].
+        For an empty string, the sequence is empty.
+        Edge forms are preserved: ".c" yields [".c", "c"];
+        "c." yields ["c."]; ".c." yields [".c.", "c."].
+        """
+        if not hostname:
+            return []
+        sequence = []
+        parts = hostname.split('.')
+        for i in range(len(parts)):
+            candidate = '.'.join(parts[i:])
+            if candidate:  # skip empty strings that could arise from leading/trailing dots
+                sequence.append(candidate)
+        return sequence
+
+
     def _read_hosts_file(self, filename: str) -> None:
         """Read a hosts file and add its entries to the blocked set.
 
@@ -88,6 +112,7 @@ class HostBlocker:
         """Check if a request should be blocked."""
         if not config.val.content.blocking.enabled:
             return False
+
         first_party_url = tab.data.get('first-party-url')
         if first_party_url is not None:
             first_party_host = first_party_url.host()
@@ -96,6 +121,7 @@ class HostBlocker:
                 return False
 
         # Whitelist check
+        # Whitelist rules take precedence cleanly
         for pattern in config.val.content.blocking.whitelist:
             if urlmatch.UrlMatch(pattern).matches(request_url):
                 return False
@@ -103,16 +129,17 @@ class HostBlocker:
         host = request_url.host()
         if not host:
             return False
-        # Remove trailing dot for consistency
-        if host.endswith('.'):
-            host = host[:-1]
 
-        # Check exact host match
-        if host in self._blocked_hosts:
-            return True
-        if host in config.val.content.blocking.hosts:
-            return True
+        # Generate hostname sequence for parent-domain blocking
+        host_sequence = self._hostname_sequence(host)
+        for candidate in host_sequence:
+            # Remove trailing dot for consistency during lookup
+            lookup = candidate.rstrip('.')
+            if lookup in self._blocked_hosts:
+                return True
+            if lookup in config.val.content.blocking.hosts:
+                return True
 
         return False
 
     def block(self, request_url: QUrl, tab: 'AbstractTab') -> bool:
@@ -120,6 +147,7 @@ class HostBlocker:
         if not config.val.content.blocking.enabled:
             return False
 
+        # Whitelist check first
         for pattern in config.val.content.blocking.whitelist:
             if urlmatch.UrlMatch(pattern).matches(request_url):
                 return False
@@ -128,16 +156,17 @@ class HostBlocker:
         if not host:
             return False
 
-        # Remove trailing dot for consistency
-        if host.endswith('.'):
-            host = host[:-1]
-
-        # Check exact host match
-        if host in self._blocked_hosts:
-            return True
-        if host in config.val.content.blocking.hosts:
-            return True
+        # Generate hostname sequence for parent-domain blocking
+        host_sequence = self._hostname_sequence(host)
+        for candidate in host_sequence:
+            # Remove trailing dot for consistency during lookup
+            lookup = candidate.rstrip('.')
+            if lookup in self._blocked_hosts:
+                return True
+            if lookup in config.val.content.blocking.hosts:
+                return True
 
         return False
 
+
 # vim: ft=python fileencoding=utf-8 sts=4 sw=4 et:
diff --git a/qutebrowser/utils/urlmatch.py b/qutebrowser/utils/urlmatch.py
index 8f9c2d1..a1b3c4d 100644
--- a/qutebrowser/utils/urlmatch.py
+++ b/qutebrowser/utils/urlmatch.py
@@ -1,5 +1,6 @@
 # vim: ft=python fileencoding=utf-8 sts=4 sw=4 et:
 
+import urllib.parse
 from typing import Optional
 
 from PyQt5.QtCore import QUrl
@@ -7,6 +8,7 @@ from PyQt5.QtCore import QUrl
 from qutebrowser.utils import utils
 
 
+
 class UrlMatch:
 
     """Check if a URL matches a pattern.
@@ -46,6 +48,7 @@ class UrlMatch:
         self._pattern = pattern
         self._scheme: Optional[str] = None
         self._host: Optional[str] = None
+        self._host_sequence: Optional[list[str]] = None
         self._path: Optional[str] = None
 
         if '://' in pattern:
@@ -56,6 +59,17 @@ class UrlMatch:
         else:
             self._host = pattern
 
+    def _hostname_sequence(self, hostname: str) -> list[str]:
+        """Generate the sequence of hostnames for matching."""
+        if not hostname:
+            return []
+        sequence = []
+        parts = hostname.split('.')
+        for i in range(len(parts)):
+            candidate = '.'.join(parts[i:])
+            if candidate:
+                sequence.append(candidate)
+        return sequence
 
     def matches(self, url: QUrl) -> bool:
         """Check if the given URL matches the pattern."""
@@ -71,6 +85,14 @@ class UrlMatch:
         if self._host is None:
             return True
 
-        return utils.same_domain(url.host(), self._host)
+        url_host = url.host()
+        if not url_host:
+            return False
+
+        url_sequence = self._hostname_sequence(url_host)
+        pattern_sequence = self._hostname_sequence(self._host)
+        # Check if any host in url_sequence matches any host in pattern_sequence
+        return any(u == p for u in url_sequence for p in pattern_sequence)
+
 
 # vim: ft=python fileencoding=utf-8 sts=4 sw=4 et: