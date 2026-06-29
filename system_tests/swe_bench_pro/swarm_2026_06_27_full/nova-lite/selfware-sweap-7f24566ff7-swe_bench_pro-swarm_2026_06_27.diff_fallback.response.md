diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 7a8944a..788a981 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -836,6 +836,7 @@ class RequestWithMethod(urllib_request.Request):
     def __init__(self, url, method, data=None, headers=None, origin_req_host=None, unverifiable=True, use_netrc=True):
         if headers is None:
             headers = {}
+        self.use_netrc = use_netrc
         self._method = method.upper()
         urllib_request.Request.__init__(self, url, data, headers, origin_req_host, unverifiable)

     def get_method(self):
@@ -848,6 +849,11 @@ class RequestWithMethod(urllib_request.Request):
             return self._method
         else:
             return urllib_request.Request.get_method(self)
+    
+    def use_netrc_credentials(self):
+        return self.use_netrc
+        
@@ -854,6 +860,10 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=No
     """This is a class factory that closes over the value of
     ``follow_redirects`` so that the RedirectHandler class has access to
     that value without having to use globals, and potentially cause problems
+    
+def open_url(module, url, method='GET', data=None, headers=None, origin_req_host=None, unverifiable=True, use_netrc=True, **kwargs):
+    req = RequestWithMethod(url, method, data, headers, origin_req_host, unverifiable, use_netrc)
+    return urllib_request.urlopen(req, **kwargs)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 889718a..8787888 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -379,6 +379,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, headers=None, tmp_dest='', method='GET', unredirected_headers=None,
     rsp, info = fetch_url(module, url, use_proxy=use_proxy, force=force, last_mod_time=last_mod_time, timeout=timeout, headers=headers, method=method,
                           unredirected_headers=unredirected_headers, decompress=decompress, ciphers=ciphers)
     elapsed = (datetime.datetime.utcnow() - start).seconds
+    use_netrc = module.params.get('use_netrc', True)
@@ -395,6 +396,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, headers=None, tmp_dest='', method='GET', unredirected_headers=None,
     if info['status'] == -1:
         module.fail_json(msg=info['msg'], url=url, dest=dest, elapsed=elapsed)
 
-    if info['status']!= 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
+    if info['status']!= 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')) and use_netrc:
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/uri.py b/lib/ansible/modules/uri.py
index 889718a..8787888 100644
--- a/lib/ansible/modules/uri.py
+++ b/lib/ansible/modules/uri.py
@@ -429,6 +429,7 @@ def run(self):
         method = self.params.get('method', 'GET')
         data = self.params.get('data', None)
         headers = self.params.get('headers', None)
+        use_netrc = self.params.get('use_netrc', True)
 
         # Fetch the URL
         result = self.uri(url, method, data, headers, use_netrc)
         self.module.exit_json(**result)