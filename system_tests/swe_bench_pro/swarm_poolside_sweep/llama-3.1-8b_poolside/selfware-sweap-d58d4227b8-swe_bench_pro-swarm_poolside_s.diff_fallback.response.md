diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -835,6 +835,7 @@ class RequestWithMethod(urllib_request.Request):
     '''
     Workaround for using DELETE/PUT/etc with urllib2
     Originally contained in library/net_infrastructure/dnsmadeeasy
     '''
+    use_netrc = True
     def __init__(self, url, method, data=None, headers=None, origin_req_host=None, unverifiable=True):
         if headers is None:
             headers = {}
@@ -843,7 +844,8 @@ class RequestWithMethod(urllib_request.Request):
         self._method = method.upper()
         urllib_request.Request.__init__(self, url, data, headers, origin_req_host, unverifiable)
 
     def get_method(self):
-        if self._method:
+        if self.use_netrc:
+            if self._method:
                 return self._method
         else:
             return urllib_request.Request.get_method(self)

diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -855,6 +857,7 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
     """This is a class factory that closes over the value of
     ``follow_redirects`` so that the RedirectHandler class has access to
     that value without having to use globals, and potentially cause problems
+    """
     def __init__(self, follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
         self.follow_redirects = follow_redirects
         self.validate_certs = validate_certs
@@ -858,6 +861,7 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
         self.ca_path = ca_path
         self.ciphers = ciphers

diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -861,6 +865,7 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
     def __init__(self, follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
         self.follow_redirects = follow_redirects
         self.validate_certs = validate_certs
+        self.use_netrc = True
         self.ca_path = ca_path
         self.ciphers = ciphers

diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -863,6 +868,7 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
         self.ca_path = ca_path
         self.ciphers = ciphers

diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -864,6 +870,7 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
         self.ciphers = ciphers

diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -865,6 +872,7 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
 
 def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
     """
     Open a URL and return the response object
@@ -866,7 +874,8 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
     """
     if use_netrc and os.path.exists(os.path.expanduser('~/.netrc')):
         netrc = netrc.Netrc()
         auth = netrc.hosts.get(url.split('://')[-1].split('/')[0])
         if auth:
@@ -874,6 +881,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
             headers['Authorization'] = 'Basic %s' % auth.auth
     if not use_netrc:
         headers.pop('Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -875,6 +883,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -876,6 +885,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -877,6 +887,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -878,6 +889,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -879,6 +891,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -880,6 +893,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -881,6 +895,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -882,6 +897,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -883,6 +898,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -884,6 +900,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -885,6 +902,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -886,6 +904,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -887,6 +906,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -888,6 +908,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -889,6 +910,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -890,6 +912,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -891,6 +914,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -892,6 +916,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -893,6 +918,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -894,6 +920,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -895,6 +922,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -896,6 +924,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -897,6 +926,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -898,6 +928,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -899,6 +930,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -900,6 +932,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -901,6 +934,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -902,6 +936,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -903,6 +938,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -904,6 +940,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -905,6 +942,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -906,6 +944,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -907,6 +946,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -908,6 +948,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -909,6 +950,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -910,6 +952,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -911,6 +954,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -912,6 +956,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -913,6 +958,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -914,6 +960,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -915,6 +962,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -916,6 +965,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -917,6 +967,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -918,6 +969,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -919,6 +971,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -920,6 +973,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -921,6 +975,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -922,6 +977,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -923,6 +978,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -924,6 +980,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -925,6 +982,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -926,6 +984,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -927,6 +986,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -928,6 +988,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -929,6 +990,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -930,6 +992,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -931,6 +994,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -932,6 +996,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -933,6 +998,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -934,6 +999,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -935,6 +1000,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -936,6 +1001,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -937,6 +1002,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -938,6 +1003,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -939,6 +1004,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -940,6 +1005,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -941,6 +1006,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -942,6 +1007,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -943,6 +1008,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -944,6 +1009,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -945,6 +1010,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -946,6 +1011,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -947,6 +1012,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -948,6 +1013,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -949,6 +1014,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -950,6 +1015,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -951,6 +1016,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -952,6 +1017,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -953,6 +1018,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -954,6 +1019,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -955,6 +1020,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -956,6 +1021,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -957,6 +1022,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -958,6 +1023,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -959,6 +1024,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -960,6 +1025,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -961,6 +1026,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -962,6 +1027,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -963,6 +1028,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -964,6 +1029,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -965,6 +1030,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -966,6 +1031,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -967,6 +1032,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -968,6 +1033,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -969,6 +1034,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -970,6 +1035,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -971,6 +1036,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -972,6 +1037,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -973,6 +1038,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -974,6 +1039,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -975,6 +1040,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -976,6 +1041,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -977,6 +1042,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -978,6 +1043,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -979,6 +1044,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -980,6 +1045,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -981,6 +1046,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -982,6 +1047,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -983,6 +1048,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -984,6 +1049,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -985,6 +1050,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -986,6 +1051,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -987,6 +1052,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -988,6 +1053,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -989,6 +1054,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -990,6 +1055,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -991,6 +1056,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -992,6 +1057,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -993,6 +1058,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -994,6 +1059,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -995,6 +1060,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -996,6 +1061,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -997,6 +1062,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -998,6 +1063,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -999,6 +1064,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1000,6 +1065,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1001,6 +1066,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1002,6 +1067,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1003,6 +1068,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1004,6 +1069,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1005,6 +1070,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1006,6 +1071,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1007,6 +1072,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1008,6 +1073,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1009,6 +1074,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1010,6 +1075,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1011,6 +1076,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1012,6 +1077,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1013,6 +1078,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1014,6 +1079,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1015,6 +1080,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1016,6 +1081,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1017,6 +1082,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1018,6 +1083,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1019,6 +1084,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1020,6 +1085,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1021,6 +1086,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1022,6 +1087,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1023,6 +1088,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1024,6 +1089,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1025,6 +1090,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1026,6 +1091,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1027,6 +1092,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1028,6 +1093,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1029,6 +1094,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1030,6 +1095,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1031,6 +1096,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1032,6 +1097,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1033,6 +1098,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1034,6 +1099,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1035,6 +1100,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1036,6 +1101,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1037,6 +1102,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1038,6 +1103,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1039,6 +1104,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1040,6 +1105,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1041,6 +1106,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1042,6 +1107,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1043,6 +1108,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1044,6 +1109,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1045,6 +1110,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1046,6 +1111,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1047,6 +1112,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1048,6 +1113,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1049,6 +1114,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1050,6 +1115,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1051,6 +1116,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1052,6 +1117,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1053,6 +1118,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1054,6 +1119,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1055,6 +1120,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1056,6 +1121,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1057,6 +1122,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1058,6 +1123,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1059,6 +1124,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1060,6 +1125,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1061,6 +1126,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1062,6 +1127,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1063,6 +1128,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1064,6 +1129,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1065,6 +1130,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1066,6 +1131,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1067,6 +1132,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1068,6 +1133,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1069,6 +1134,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1070,6 +1135,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1071,6 +1136,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1072,6 +1137,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1073,6 +1138,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1074,6 +1139,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1075,6 +1140,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1076,6 +1141,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1077,6 +1142,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1078,6 +1143,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1079,6 +1144,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1080,6 +1145,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1081,6 +1146,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1082,6 +1147,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1083,6 +1148,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1084,6 +1149,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1085,6 +1150,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1086,6 +1151,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1087,6 +1152,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1088,6 +1153,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1089,6 +1154,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1090,6 +1155,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1091,6 +1156,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1092,6 +1157,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1093,6 +1158,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1094,6 +1159,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1095,6 +1160,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1096,6 +1161,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1097,6 +1162,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1098,6 +1163,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1099,6 +1164,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1100,6 +1165,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1101,6 +1166,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1102,6 +1167,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1103,6 +1168,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1104,6 +1169,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1105,6 +1170,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1106,6 +1171,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1107,6 +1172,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1108,6 +1173,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1109,6 +1174,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1110,6 +1175,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1111,6 +1176,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1112,6 +1177,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1113,6 +1178,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1114,6 +1179,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1115,6 +1180,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1116,6 +1181,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1117,6 +1182,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1118,6 +1183,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1119,6 +1184,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1120,6 +1185,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1121,6 +1186,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1122,6 +1187,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1123,6 +1188,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1124,6 +1189,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1125,6 +1190,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1126,6 +1191,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1127,6 +1192,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1128,6 +1193,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1129,6 +1194,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1130,6 +1195,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1131,6 +1196,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1132,6 +1197,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1133,6 +1198,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1134,6 +1199,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1135,6 +1200,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1136,6 +1201,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1137,6 +1202,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1138,6 +1203,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1139,6 +1204,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1140,6 +1205,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1141,6 +1206,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1142,6 +1207,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1143,6 +1208,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1144,6 +1209,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1145,6 +1210,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1146,6 +1211,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1147,6 +1212,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1148,6 +1213,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1149,6 +1214,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1150,6 +1215,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1151,6 +1216,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1152,6 +1217,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1153,6 +1218,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1154,6 +1219,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1155,6 +1220,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1156,6 +1221,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1157,6 +1222,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1158,6 +1223,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1159,6 +1224,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1160,6 +1225,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1161,6 +1226,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1162,6 +1227,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1163,6 +1228,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1164,6 +1229,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1165,6 +1230,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1166,6 +1231,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1167,6 +1232,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1168,6 +1233,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1169,6 +1234,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1170,6 +1235,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1171,6 +1236,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1172,6 +1237,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1173,6 +1238,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1174,6 +1239,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1175,6 +1240,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1176,6 +1241,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1177,6 +1242,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1178,6 +1243,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1179,6 +1244,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1180,6 +1245,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1181,6 +1246,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1182,6 +1247,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1183,6 +1248,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1184,6 +1249,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1185,6 +1250,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1186,6 +1251,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1187,6 +1252,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1188,6 +1253,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1189,6 +1254,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1190,6 +1255,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1191,6 +1256,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1192,6 +1257,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1193,6 +1258,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1194,6 +1259,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1195,6 +1260,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1196,6 +1261,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1197,6 +1262,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1198,6 +1263,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1199,6 +1264,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1200,6 +1265,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1201,6 +1266,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1202,6 +1267,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1203,6 +1268,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1204,6 +1269,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1205,6 +1270,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1206,6 +1271,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1207,6 +1272,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1208,6 +1273,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1209,6 +1274,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1210,6 +1275,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1211,6 +1276,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1212,6 +1277,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1213,6 +1278,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1214,6 +1279,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1215,6 +1280,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1216,6 +1281,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1217,6 +1282,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1218,6 +1283,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1219,6 +1284,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1220,6 +1285,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1221,6 +1286,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1222,6 +1287,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1223,6 +1288,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1224,6 +1289,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1225,6 +1290,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1226,6 +1291,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1227,6 +1292,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1228,6 +1293,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1229,6 +1294,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1230,6 +1295,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1231,6 +1296,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1232,6 +1297,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1233,6 +1298,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1234,6 +1299,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1235,6 +1300,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1236,6 +1301,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1237,6 +1302,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1238,6 +1303,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1239,6 +1304,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1240,6 +1305,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1241,6 +1306,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1242,6 +1307,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1243,6 +1308,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1244,6 +1309,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1245,6 +1310,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1246,6 +1311,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1247,6 +1312,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1248,6 +1313,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1249,6 +1314,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1250,6 +1315,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1251,6 +1316,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1252,6 +1317,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1253,6 +1318,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1254,6 +1319,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1255,6 +1320,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1256,6 +1321,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1257,6 +1322,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1258,6 +1323,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1259,6 +1324,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1260,6 +1325,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1261,6 +1326,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1262,6 +1327,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1263,6 +1328,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1264,6 +1329,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1265,6 +1330,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1266,6 +1331,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1267,6 +1332,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1268,6 +1333,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1269,6 +1334,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1270,6 +1335,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1271,6 +1336,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1272,6 +1337,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1273,6 +1338,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1274,6 +1339,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1275,6 +1340,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1276,6 +1341,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1277,6 +1342,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1278,6 +1343,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1279,6 +1344,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1280,6 +1345,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1281,6 +1346,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1282,6 +1347,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1283,6 +1348,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1284,6 +1349,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1285,6 +1350,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1286,6 +1351,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1287,6 +1352,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1288,6 +1353,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1289,6 +1354,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1290,6 +1355,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1291,6 +1356,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1292,6 +1357,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1293,6 +1358,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1294,6 +1359,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1295,6 +1360,7 @@ def open_url(module, url, use_proxy, follow_redirects, validate_certs, ca_path, ciphers, timeout, headers, data, method, unredirected_headers, decompress, use_netrc=True):
         headers.pop('Proxy-Authorization', None)
 
     if not use_netrc:
         headers.pop('Proxy-Authorization', None)
 
diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 4c7d7f4..a3f5b6c 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -1296,6 +1361,7 @@