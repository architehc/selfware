```diff
diff --git a/scripts/hostblock_blame.py b/scripts/hostblock_blame.py
index 897478f..897478f 100644
--- a/scripts/hostblock_blame.py
+++ b/scripts/hostblock_blame.py
@@ -23,6 +23,7 @@
 
 import sys
 import io
+import re
 import os

@@ -34,6 +35,36 @@ def is_blocked(hostname, blocked_hosts, whitelist):
     return False
 
 def widen_hostname(hostname):
+    """Generate a sequence of hostnames derived from the request host."""
+    labels = hostname.split('.')
+    widened = []
+    for i in range(len(labels)):
+        sub_labels = labels[i:]
+        sub_hostname = '.'.join(sub_labels)
+        if sub_hostname.endswith('.'):
+            sub_hostname = sub_hostname[:-1]
+        widened.append(sub_hostname)
+    return widened
+
+def is_subdomain(sub, parent):
+    """Check if a subdomain is a subdomain of a parent domain."""
+    sub_parts = sub.split('.')
+    parent_parts = parent.split('.')
+    if len(sub_parts) < len(parent_parts):
+        return False
+    for i in range(len(parent_parts)):
+        if sub_parts[-(i + 1)]!= parent_parts[-(i + 1)]:
+            return False
+    return True
+
+def process_host(host):
+    """Handle hosts with trailing dots equivalently to their non-trailing forms."""
+    if host.endswith('.'):
+        host = host[:-1]
+    if host.startswith('.'):
+        host = host[1:]
+    return host
+
 def is_blocked_by_host(hostname, blocked_hosts):
     """Check if a hostname is blocked by the hostblock list."""
     if hostname in blocked_hosts:
@@ -42,6 +83,23 @@ def is_blocked_by_host(hostname, blocked_hosts):
     return False
 
 def is_whitelisted(hostname, whitelist):
+    """Check if a hostname is whitelisted."""
+    for pattern in whitelist:
+        if re.match(pattern, hostname):
+            return True
+    return False
+
+def should_block_request(url, blocked_hosts, whitelist):
+    """Determine if a request should be blocked."""
+    host = process_host(url.hostname)
+    if is_whitelisted(host, whitelist):
+        return False
+    widened_host = widen_hostname(host)
+    for h in widened_host:
+        if is_blocked_by_host(h, blocked_hosts):
+            return True
+    return False
+
 def main():
     """Main function."""
     if len(sys.argv) < 2:
```