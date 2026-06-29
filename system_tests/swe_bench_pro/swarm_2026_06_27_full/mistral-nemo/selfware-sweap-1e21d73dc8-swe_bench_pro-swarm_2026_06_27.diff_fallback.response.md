diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 1234567..1234568 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -834,6 +834,10 @@ class RequestWithMethod(urllib_request.Request):
         unverifiable=True):
             self._method = method.upper()
             urllib_request.Request.__init__(self, url, data, headers, origin_req_host, unverifiable)
+            self.use_netrc = use_netrc if use_netrc is not None else True

         def get_method(self):
             if self._method:
                 return self._method
@@ -855,6 +859,10 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
             return handler

         return handler

+    def open(self, timeout=socket._GLOBAL_DEFAULT_TIMEOUT, use_netrc=None):
+        return urllib_request.Request.open(self, timeout=timeout, use_netrc=use_netrc if use_netrc is not None else True)
+
     def fetch_url(module, url, use_proxy=False, force=False, last_mod_time=None, timeout=10, headers=None, method='GET', unredirected_headers=None,
                  decompress=True, ciphers=None, use_netrc=None):
         """