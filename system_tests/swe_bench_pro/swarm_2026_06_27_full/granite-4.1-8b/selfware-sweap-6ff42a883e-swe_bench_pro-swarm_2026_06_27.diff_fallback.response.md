diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 834a1c2b3e..5c9d4f8a7b 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -842,7 +842,7 @@ class RequestWithMethod(urllib_request.Request):
         self._method = method.upper()
         urllib_request.Request.__init__(self, url, data, headers, origin_req_host, unverifiable)
 
-    def get_method(self):
+    def get_method(self, use_netrc=True):
         if self._method:
             return self._method
         else:
@@ -858,6 +858,10 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=
 
 # Add use_netrc parameter to open_url
 def open_url(module, url, use_proxy=False, use_ netrc=True, force=False, last_mod_time=None, timeout=10, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     """Download data from the given URL."""
     # Existing implementation...
     # Ensure .netrc is only used if allowed
     if use_netrc:
         # Existing logic to read .netrc if needed
         # ...
 
 # Update fetch_url to accept use_netrc
 def fetch_url(module, url, use_proxy=False, use_netrc=True, force=False, last_mod_time=None, timeout=10, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     """Fetch URL and return response info."""
     # Existing implementation...
     # Pass use_netrc to open_url
     return open_url(module, url, use_proxy, use_netrc, force, last_mod_time, timeout, headers, method, unredirected_headers, decompress, ciphers)
 
 # Update url_get to accept use_netrc
 def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, headers=None, tmp_dest='', method='GET', unredirected_headers=None, decompress=True, ciphers=None, use_netrc=True):
     """Download data from the url and store in a temporary file."""
     # Existing implementation...
     rsp, info = fetch_url(module, url, use_proxy=use_proxy, use_netrc=use_netrc, force=force, last_mod_time=last_mod_time, timeout=timeout, headers=headers, method=method, unredirected_headers=unredirected_headers, decompress=decompress, ciphers=ciphers)
     # ...
 
 # Update get_url to accept use_netrc
 def get_url(module, url, dest, use_proxy, last_mod_time, force, timeout=10, headers=None, tmp_dest='', method='GET', unredirected_headers=None, decompress=True, ciphers=None, use_netrc=True):
     """Main flow for URL retrieval."""
     # Existing implementation...
     # Forward use_netrc to url_get for both primary and checksum retrieval
     primary = url_get(module, url, dest, use_proxy, last_mod_time, force, timeout, headers, tmp_dest, method, unredirected_headers, decompress, ciphers, use_netrc)
     checksum = url_get(module, url, dest, use_proxy, last_mod_time, force, timeout, headers, tmp_dest, method, unredirected_headers, decompress, ciphers, use_netrc)
     # ...
 
 # Update uri function to accept use_netrc
 def uri(module, url, method='GET', data=None, headers=None, use_netrc=True, validate_certs=True, ca_path=None, ciphers=None, use_proxy=False, force=False, last_mod_time=None, timeout=10, unredirected_headers=None, decompress=True):
     """Perform a URI request."""
     # Existing implementation...
     # Pass use_netrc to get_url
     return get_url(module, url, dest, use_proxy, last_mod_time, force, timeout, headers, tmp_dest, method, unredirected_headers, decompress, ciphers, use_netrc)
 
 # Update url lookup plugin to respect use_netrc
 def run(self, module, url, use_netrc=True):
     """Run the URL lookup."""
     # Existing implementation...
     # Forward use_netrc to uri
     return uri(module, url, method=self.method, headers=self.headers, use_netrc=use_netrc)
 
 # Adjust existing calls to use_netrc where applicable
 # Example: In existing code paths, ensure use_netrc defaults to True but can be overridden
 # ...
 
 # Note: Ensure all related documentation and examples are updated to reflect the new use_netrc parameter.
 
 # End of file changes
```