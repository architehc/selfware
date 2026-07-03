
diff --git a/qutebrowser/content/blocking.py b/qutebrowser/content/blocking.py
index 1234567..abcdefg 100644
--- a/qutebrowser/content/blocking.py
+++ b/qutebrowser/content/blocking.py
@@ -1,3 +1,4 @@
+
 """Content blocking functionality."""
 
 import typing
@@ -20,6 +21,24 @@ class HostBlockHandler:
         self._blocked_hosts = blocked_hosts
         self._whitelist = whitelist
 
+    def _widen_hostname(self, host: str) -> typing.List[str]:
+        """Generate a sequence of hostnames from the request host.
+        
+        For a multi-label hostname like a.b.c, yields ["a.b.c", "b.c", "c"].
+        For a single-label hostname like foobarbaz, yields ["foobarbaz"].
+        For an empty string, yields nothing.
+        """
+        if not host:
+            return []
+        parts = host.split('.')
+        return ['.'.join(parts[i:]) for i in range(len(parts))]
+
     def is_blocked(self, url: str) -> bool:
+        """Check if a URL is blocked, considering parent domains and whitelist."""
+        # Parse URL to get host
+        parsed = urllib.parse.urlparse(url)
+        host = parsed.hostname or ''
+        
+        # Check whitelist first
+        if host in self._whitelist:
+            return False
+        
+        # Check each widened hostname against blocked hosts
+        for hostname in self._widen_hostname(host):
+            if hostname in self._blocked_hosts:
+                return True
+        return False
```
