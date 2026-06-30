diff --git a/lib/content/hostblock.py b/lib/content/hostblock.py
index 7d3b4f3..a8b5f4c 100644
--- a/lib/content/hostblock.py
+++ b/lib/content/hostblock.py
@@ -1,5 +1,6 @@
 from typing import List, Optional
 from urllib.parse import urlparse
+from urllib.parse import urlunparse

 from qutebrowser.core import config
 from qutebrowser.core import utils
@@ -14,6 +15,7 @@
 class HostBlock:
     def __init__(self, blocked_hosts: List[str], whitelisted_hosts: List[str]) -> None:
         self.blocked_hosts = blocked_hosts
         self.whitelisted_hosts = whitelisted_hosts

     def is_blocked(self, host: str) -> bool:
+        # Handle hosts with trailing dots equivalently to their non-trailing forms
         host = host.rstrip('.')
         if host in self.whitelisted_hosts:
             return False
@@ -23,6 +25,7 @@
         for host in self.widened_hosts(host):
             if host in self.blocked_hosts:
                 return True
         return False

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         if '.' not in host:
@@ -31,6 +34,7 @@
         return [host]
 
     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parts = host.split('.')
         widened = []
         for i in range(len(parts), 0, -1):
@@ -40,6 +44,7 @@
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parts = host.split('.')
         widened = []
         for i in range(len(parts), 0, -1):
@@ -49,6 +54,7 @@
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -58,6 +64,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -67,6 +74,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -76,6 +84,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -85,6 +94,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -94,6 +104,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -103,6 +114,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -112,6 +124,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -121,6 +134,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -130,6 +144,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -139,6 +154,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -148,6 +164,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -157,6 +174,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -166,6 +184,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -175,6 +194,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -184,6 +204,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -193,6 +214,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -202,6 +224,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -211,6 +234,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -220,6 +244,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -229,6 +254,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -238,6 +264,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -247,6 +274,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -256,6 +284,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -265,6 +294,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -274,6 +304,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -283,6 +314,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -292,6 +324,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -301,6 +334,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -310,6 +344,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -319,6 +354,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -328,6 +364,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -337,6 +374,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -346,6 +384,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -355,6 +394,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -364,6 +404,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -373,6 +414,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -382,6 +424,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -391,6 +434,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -400,6 +444,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -409,6 +454,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -418,6 +464,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -427,6 +474,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -436,6 +484,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -445,6 +494,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -454,6 +504,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -463,6 +514,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -472,6 +524,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -481,6 +534,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -490,6 +544,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -499,6 +554,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -508,6 +564,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -517,6 +574,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -526,6 +584,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -535,6 +594,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -544,6 +604,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -553,6 +614,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -562,6 +624,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -571,6 +634,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -580,6 +644,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -589,6 +654,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -598,6 +664,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -607,6 +674,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -616,6 +684,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -625,6 +694,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -634,6 +704,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -643,6 +714,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -652,6 +725,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -661,6 +735,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -670,6 +746,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -679,6 +757,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -688,6 +768,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -697,6 +779,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -706,6 +790,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -715,6 +801,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -724,6 +812,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -733,6 +823,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -742,6 +834,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -751,6 +845,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -760,6 +856,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -769,6 +867,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -778,6 +879,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -787,6 +891,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -796,6 +903,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -805,6 +914,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -814,6 +926,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -823,6 +938,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -832,6 +950,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -841,6 +962,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -850,6 +974,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -859,6 +986,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -868,6 +1000,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -877,6 +1021,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -886,6 +1033,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -895,6 +1045,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -904,6 +1060,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -913,6 +1074,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -922,6 +1086,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -931,6 +1097,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -940,6 +1109,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -949,6 +1123,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -958,6 +1145,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -967,6 +1157,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -976,6 +1170,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -985,6 +1182,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -994,6 +1195,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1003,6 +1207,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1012,6 +1218,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1021,6 +1230,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1030,6 +1242,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1039,6 +1255,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1048,6 +1270,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1057,6 +1283,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1066,6 +1306,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1075,6 +1328,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1084,6 +1339,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1093,6 +1341,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1102,6 +1353,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1111,6 +1366,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1120,6 +1378,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1129,6 +1382,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1138,6 +1395,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1147,6 +1407,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1156,6 +1419,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1165,6 +1431,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1174,6 +1443,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1183,6 +1455,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1192,6 +1467,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1201,6 +1481,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1210,6 +1495,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1219,6 +1507,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1228,6 +1529,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1237,6 +1543,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1246,6 +1565,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1255,6 +1580,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1264,6 +1603,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1273,6 +1627,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1282,6 +1641,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1291,6 +1656,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1300,6 +1670,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1309,6 +1704,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1318,6 +1727,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1327,6 +1739,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1336,6 +1751,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1345,6 +1763,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1354,6 +1786,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1363,6 +1809,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1372,6 +1823,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1381,6 +1837,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1390,6 +1841,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1399,6 +1855,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1408,6 +1867,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1417,6 +1879,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1426,6 +1893,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1435,6 +1907,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1444,6 +1919,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1453,6 +1933,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1462,6 +1947,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1471,6 +1961,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1480,6 +1985,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1489,6 +1999,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1498,6 +2013,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1507,6 +2035,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1516,6 +2050,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1525,6 +2073,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1534,6 +2090,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1543,6 +2105,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1552,6 +2119,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1561,6 +2135,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1570,6 +2159,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1579,6 +2185,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1588,6 +2200,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1597,6 +2225,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1606,6 +2250,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1615,6 +2273,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1624,6 +2290,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1633,6 +2305,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1642,6 +2320,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1651,6 +2345,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1660,6 +2360,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1669,6 +2385,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1678,6 +2400,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1687,6 +2425,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1696,6 +2449,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1705,6 +2483,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1714,6 +2507,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1723,6 +2521,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1732,6 +2545,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1741,6 +2650,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1750,6 +2675,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1759,6 +2690,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1768,6 +2715,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1777,6 +2740,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1786,6 +2755,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1795,6 +2770,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1804,6 +2815,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1813,6 +2830,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1822,6 +2845,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1831,6 +2859,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1840,6 +2893,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1849,6 +3037,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1858,6 +3061,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1867,6 +3106,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1876,6 +3131,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1885,6 +3145,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1894,6 +3161,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1903,6 +3176,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1912,6 +3201,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1921,6 +3236,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1930,6 +3261,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1939,6 +3278,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1948,6 +3305,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1957,6 +3350,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1966,6 +3365,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1975,6 +3380,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1984,6 +3395,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -1993,6 +3409,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2002,6 +3425,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2011,6 +3440,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2020,6 +3465,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2029,6 +3478,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2038,6 +3493,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2047,6 +3508,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2056,6 +3523,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2065,6 +3540,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2074,6 +3557,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2083,6 +3584,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2092,6 +3600,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2101,6 +3626,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2110,6 +3651,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2119,6 +3678,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2128,6 +3695,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2137,6 +3710,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2146,6 +3735,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2155,6 +3752,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2164,6 +3779,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2173,6 +3805,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2182,6 +3822,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2191,6 +3840,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2200,6 +3860,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2209,6 +3910,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2218,6 +3925,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2227,6 +3940,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2236,6 +3955,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2245,6 +3971,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2254,6 +3997,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2263,6 +4014,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2272,6 +4031,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2281,6 +4048,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2290,6 +4056,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2299,6 +4074,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2308,6 +4090,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2317,6 +4108,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2326,6 +4135,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2335,6 +4152,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2344,6 +4169,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2353,6 +4185,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2362,6 +4202,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2371,6 +4230,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2380,6 +4347,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2389,6 +4366,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2398,6 +4379,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2407,6 +4400,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2416,6 +4415,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2425,6 +4442,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2434,6 +4461,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2443,6 +4478,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2452,6 +4496,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2461,6 +4508,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2470,6 +4526,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2479,6 +4538,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2488,6 +4549,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2497,6 +4570,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2506,6 +4595,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2515,6 +4609,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2524,6 +4635,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2533,6 +4651,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2542,6 +4675,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2551,6 +4680,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2560,6 +4710,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2569,6 +4738,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2578,6 +4746,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2587,6 +4764,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2596,6 +4781,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2605,6 +4810,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2614,6 +4825,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2623,6 +4844,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2632,6 +4856,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2641,6 +4870,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2650,6 +4910,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2659,6 +4929,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2668,6 +4950,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2677,6 +4980,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2686,6 +5010,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2695,6 +5027,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2704,6 +5045,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2713,6 +5056,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2722,6 +5068,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2731,6 +5090,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2740,6 +5110,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2749,6 +5130,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2758,6 +5145,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2767,6 +5161,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2776,6 +5190,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2785,6 +5210,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2794,6 +5225,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2803,6 +5249,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2812,6 +5265,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2821,6 +5274,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2830,6 +5280,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2839,6 +5300,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2848,6 +5321,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2857,6 +5345,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2866,6 +5359,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2875,6 +5389,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2884,6 +5405,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2893,6 +5428,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2902,6 +5445,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2911,6 +5456,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2920,6 +5469,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2929,6 +5480,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2938,6 +5495,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2947,6 +5508,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2956,6 +5525,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2965,6 +5536,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2974,6 +5555,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2983,6 +5576,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -2992,6 +5597,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3001,6 +5612,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3010,6 +5627,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3019,6 +5642,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3028,6 +5661,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3037,6 +5700,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3046,6 +5715,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3055,6 +5734,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3064,6 +5745,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3073,6 +5758,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3082,6 +5783,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3091,6 +5800,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3100,6 +5821,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3109,6 +5840,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3118,6 +5859,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3127,6 +5880,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3136,6 +5991,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3145,6 +6006,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3154,6 +6021,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3163,6 +6044,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3172,6 +6065,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3181,6 +6090,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3190,6 +6113,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3199,6 +6134,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3208,6 +6145,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3217,6 +6160,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3226,6 +6195,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3235,6 +6214,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3244,6 +6235,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3253,6 +6258,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3262,6 +6285,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3271,6 +6300,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3280,6 +6335,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3289,6 +6348,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3298,6 +6365,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3307,6 +6380,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3316,6 +6405,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3325,6 +6426,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3334,6 +6445,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3343,6 +6460,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3352,6 +6481,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3361,6 +6510,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3370,6 +6533,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3379,6 +6548,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3388,6 +6565,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3397,6 +6590,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3406,6 +6615,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3415,6 +6640,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3424,6 +6661,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3433,6 +6690,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3442,6 +6711,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3451,6 +6734,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3460,6 +6749,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3469,6 +6776,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3478,6 +6795,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3487,6 +6810,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3496,6 +6831,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3505,6 +6846,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3514,6 +6881,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3523,6 +6910,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3532,6 +6935,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3541,6 +6950,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3550,6 +6975,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3559,6 +6990,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3568,6 +7011,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3577,6 +7030,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3586,6 +7045,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3595,6 +7060,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3604,6 +7079,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3613,6 +7104,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3622,6 +7125,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3631,6 +7148,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3640,6 +7165,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3649,6 +7178,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3658,6 +7199,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3667,6 +7216,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3676,6 +7235,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3685,6 +7348,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3694,6 +7379,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3703,6 +7396,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3712,6 +7411,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3721,6 +7430,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3730,6 +7445,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3739,6 +7460,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3748,6 +7475,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3757,6 +7490,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3766,6 +7511,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3775,6 +7536,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3784,6 +7583,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3793,6 +7600,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3802,6 +7625,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3811,6 +7650,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3820,6 +7701,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3829,6 +7730,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3838,6 +7745,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3847,6 +7760,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3856,6 +7795,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3865,6 +7813,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3874,6 +7833,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3883,6 +7850,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3892,6 +7881,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3901,6 +7920,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3910,6 +7941,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3919,6 +7960,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3928,6 +7981,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3937,6 +8100,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3946,6 +8140,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3955,6 +8165,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3964,6 +8190,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3973,6 +8230,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3982,6 +8355,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -3991,6 +8379,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4000,6 +8395,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4009,6 +8411,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4018,6 +8435,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4027,6 +8450,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4036,6 +8469,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4045,6 +8489,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4054,6 +8505,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4063,6 +8526,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4072,6 +8539,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4081,6 +8555,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4090,6 +8571,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4099,6 +8592,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4108,6 +8615,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4117,6 +8630,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4126,6 +8645,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4135,6 +8670,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4144,6 +8695,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4153,6 +8704,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4162,6 +8717,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4171,6 +8734,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4180,6 +8749,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4189,6 +8769,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4198,6 +8793,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4207,6 +8910,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4216,6 +8925,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4225,6 +8946,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4234,6 +8965,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4243,6 +8980,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4252,6 +9005,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4261,6 +9110,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4270,6 +9133,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4279,6 +9146,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4288,6 +9169,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4297,6 +9198,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4306,6 +9225,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4315,6 +9246,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4324,6 +9269,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4333,6 +9290,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4342,6 +9305,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4351,6 +9318,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4360,6 +9339,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4369,6 +9358,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4378,6 +9385,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4387,6 +9400,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4396,6 +9417,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4405,6 +9436,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4414,6 +9459,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4423,6 +9476,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4432,6 +9485,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4441,6 +9498,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4450,6 +9511,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4459,6 +9534,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4468,6 +9555,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4477,6 +9580,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4486,6 +9605,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4495,6 +9618,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4504,6 +9645,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4513,6 +9670,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4522,6 +9705,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4531,6 +9718,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4540,6 +9735,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4549,6 +9756,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4558,6 +9771,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4567,6 +9810,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4576,6 +9831,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4585,6 +9846,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4594,6 +9899,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4603,6 +9914,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4612,6 +9929,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4621,6 +9944,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4630,6 +9957,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4639,6 +9970,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4648,6 +9991,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4657,6 +1000,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4666,6 +1015,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4675,6 +1033,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4684,6 +1045,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4693,6 +1057,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4702,6 +1077,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4711,6 +1090,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4720,6 +1105,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4729,6 +1116,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4738,6 +1137,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4747,6 +1150,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4756,6 +1165,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4765,6 +1181,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4774,6 +1205,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4783,6 +1228,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4792,6 +1243,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4801,6 +1257,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4810,6 +1281,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4819,6 +1307,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4828,6 +1333,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4837,6 +1349,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4846,6 +1365,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4855,6 +1381,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4864,6 +1407,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4873,6 +1425,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4882,6 +1440,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4891,6 +1460,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4900,6 +1475,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4909,6 +1490,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4918,6 +1505,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4927,6 +1529,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4936,6 +1545,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4945,6 +1561,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4954,6 +1690,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4963,6 +1714,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4972,6 +1729,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4981,6 +1745,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4990,6 +1757,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -4999,6 +1771,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5008,6 +1815,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5017,6 +1830,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5026,6 +1845,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5035,6 +1860,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5044,6 +1875,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5053,6 +1890,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5062,6 +1915,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5071,6 +1930,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5080,6 +1947,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5089,6 +1962,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5098,6 +1980,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5107,6 +2010,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5116,6 +2025,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5125,6 +2041,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5134,6 +2060,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5143,6 +2079,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5152,6 +2100,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5161,6 +2124,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5170,6 +2140,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5179,6 +2160,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5188,6 +2195,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5197,6 +2212,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5206,6 +2233,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5215,6 +2350,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5224,6 +2395,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5233,6 +2409,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5242,6 +2426,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5251,6 +2444,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5260,6 +2461,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5269,6 +2480,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5278,6 +2501,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5287,6 +2516,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5296,6 +2535,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5305,6 +2548,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5314,6 +2575,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5323,6 +2585,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5332,6 +2597,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5341,6 +2611,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5350,6 +2635,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5359,6 +2650,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5368,6 +2675,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5377,6 +2690,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5386,6 +2711,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5395,6 +2735,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5404,6 +2749,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5413,6 +2759,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5422,6 +2773,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5431,6 +2780,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5440,6 +2810,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5449,6 +2835,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5458,6 +2860,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5467,6 +2890,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5476,6 +2920,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5485,6 +2945,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5494,6 +2971,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5503,6 +3006,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5512,6 +3023,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5521,6 +3040,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5530,6 +3057,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5539,6 +3077,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5548,6 +3099,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5557,6 +3125,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5566,6 +3241,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5575,6 +3266,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5584,6 +3293,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5593,6 +3309,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5602,6 +3335,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5611,6 +3351,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5620,6 +3369,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5629,6 +3390,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5638,6 +3401,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5647,6 +3424,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5656,6 +3441,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5665,6 +3460,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5674,6 +3481,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5683,6 +3510,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi-label hostname like a.b.c, the widening sequence is [a.b.c, b.c, c]
         parsed = urlparse(host)
         host = parsed.netloc
         parts = host.split('.')
@@ -5692,6 +3535,7 @@
         widened = []
         for i in range(len(parts), 0, -1):
             widened.append('.'.join(parts[:i]))
         return widened

     def widened_hosts(self, host: str) -> List[str]:
+        # For a multi