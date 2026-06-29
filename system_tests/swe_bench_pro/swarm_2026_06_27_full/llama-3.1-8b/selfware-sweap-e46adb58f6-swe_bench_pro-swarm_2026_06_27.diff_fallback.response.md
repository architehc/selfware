diff --git a/lib/ansible/module_utils/urls.py b/lib/ansible/module_utils/urls.py
index 1a2f3d3..e4f5c6d 100644
--- a/lib/ansible/module_utils/urls.py
+++ b/lib/ansible/module_utils/urls.py
@@ -836,6 +836,7 @@ class RequestWithMethod(urllib_request.Request):
     '''
     Workaround for using DELETE/PUT/etc with urllib2
     Originally contained in library/net_infrastructure/dnsmadeeasy
     '''
+    use_netrc = True
     def __init__(self, url, method, data=None, headers=None, origin_req_host=None, unverifiable=True):
         if headers is None:
             headers = {}
@@ -846,6 +847,7 @@ class RequestWithMethod(urllib_request.Request):
         urllib_request.Request.__init__(self, url, data, headers, origin_req_host, unverifiable)
 
     def get_method(self):
+        if not self.use_netrc:
             return self._method
         else:
             return self._method
@@ -855,6 +857,7 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=n
     """This is a class factory that closes over the value of
     ``follow_redirects`` so that the RedirectHandler class has access to
     that value without having to use globals, and potentially cause problems
+    """
     def __init__(self, follow_redirects=None, validate_certs=True, ca_path=None, ciphers=None):
         self.follow_redirects = follow_redirects
         self.validate_certs = validate_certs
@@ -858,6 +861,7 @@ def RedirectHandlerFactory(follow_redirects=None, validate_certs=True, ca_path=n
         self.ca_path = ca_path
         self.ciphers = ciphers

diff --git a/lib/ansible/modules/apt_repository.py b/lib/ansible/modules/apt_repository.py
index 1234567..89abcde 100644
--- a/lib/ansible/modules/apt_repository.py
+++ b/lib/ansible/modules/apt_repository.py
@@ -438,6 +438,7 @@ class UbuntuSourcesList(SourcesList):
     def __init__(self, module):
         self.module = module
         self.codename = module.params['codename'] or distro.codename
+        self.use_netrc = module.params.get('use_netrc', True)
         super(UbuntuSourcesList, self).__init__(module)

         self.apt_key_bin = self.module.get_bin_path('apt-key', required=False)
@@ -441,6 +442,7 @@ class UbuntuSourcesList(SourcesList):
         self.gpg_bin = self.module.get_bin_path('gpg', required=False)
         if not self.apt_key_bin and not self.gpg_bin:
             self.module.fail_json(msg='Either apt-key or gpg binary is required, but neither could be found')

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -382,6 +382,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     """
     Download data from the url and store in a temporary file.
 
     Return (tempfile, info about the request)
@@ -390,6 +391,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     start = datetime.datetime.utcnow()
     rsp, info = fetch_url(module, url, use_proxy=use_proxy, force=force, last_mod_time=last_mod_time, timeout=timeout, headers=headers, method=method,
                           unredirected_headers=unredirected_headers, decompress=decompress, ciphers=ciphers)
     elapsed = (datetime.datetime.utcnow() - start).seconds

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith('OK')):
         module.fail_json(msg="Request failed", status_code=info['status'], response=info['msg'], url=url, dest=dest, elapsed=elapsed)

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -401,6 +404,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
 def fetch_url(module, url, use_proxy, force, last_mod_time, timeout, headers=None, method='GET', unredirected_headers=None, decompress=True, ciphers=None):
     return fn

diff --git a/lib/ansible/modules/get_url.py b/lib/ansible/modules/get_url.py
index 9012345..6789012 100644
--- a/lib/ansible/modules/get_url.py
+++ b/lib/ansible/modules/get_url.py
@@ -392,6 +393,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] == 304:
         module.exit_json(url=url, dest=dest, changed=False, msg=info.get('msg', ''), status_code=info['status'], elapsed=elapsed)

     # Exceptions in fetch_url may result in a status -1, the ensures a proper error to the user in all cases
@@ -399,6 +401,7 @@ def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, heade
     if info['status'] != 200 and not url.startswith('file:/') and not (url.startswith('ftp:/') and info.get('msg', '').startswith