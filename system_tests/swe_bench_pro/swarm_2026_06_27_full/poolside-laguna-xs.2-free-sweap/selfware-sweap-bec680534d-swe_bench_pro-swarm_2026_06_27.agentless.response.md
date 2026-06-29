
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def fetch_url(module, url, data=None, method=None, timeout=10, use_proxy=True, force=False, last_mod_time=None,
            component=None, headers=None, cookies=None, client_cert=None, client_key=None, follow_redirects=None,
            force_basic_auth=False, url_username=None, url_password=None, validate_certs=True, unredirected_headers=None,
            unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_gssapi=False):
=======
def fetch_url(module, url, data=None, method=None, timeout=10, use_proxy=True, force=False, last_mod_time=None,
            component=None, headers=None, cookies=None, client_cert=None, client_key=None, follow_redirects=None,
            force_basic_auth=False, url_username=None, url_password=None, validate_certs=True, unredirected_headers=None,
            unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_gssapi=False,
            use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
            decompress=decompress, ciphers=ciphers, use_gssapi=use_gssapi)
=======
            decompress=decompress, ciphers=ciphers, use_gssapi=use_gssapi, use_netrc=use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(url, data=None, method=None, timeout=10, use_proxy=True, force=False, last_mod_time=None,
             component=None, headers=None, cookies=None, client_cert=None, client_key=None, follow_redirects=None,
             force_basic_auth=False, url_username=None, url_password=None, validate_certs=True, unredirected_headers=None,
             unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_gssapi=False):
=======
def open_url(url, data=None, method=None, timeout=10, use_proxy=True, force=False, last_mod_time=None,
             component=None, headers=None, cookies=None, client_cert=None, client_key=None, follow_redirects=None,
             force_basic_auth=False, url_username=None, url_password=None, validate_certs=True, unredirected_headers=None,
             unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_gssapi=False,
             use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
            decompress=decompress, ciphers=ciphers, use_gssapi=use_gssapi)
=======
            decompress=decompress, ciphers=ciphers, use_gssapi=use_gssapi, use_netrc=use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
class Request(object):
    def __init__(self, url, data=None, headers=None, unredirected_headers=None,
                 timeout=5, use_proxy=True, proxy_ssl_verification=True,
                 proxy_ssl_server_hostname=True, ca_path=None, unredirected_headers_for_opener=None,
                 follow_redirects=True, force_basic_auth=False, url_username=None, url_password=None,
                 client_cert=None, client_key=None, force_ipv6=False, decompress=True, ciphers=None,
                 unix_socket=None, ca_path=None, use_gssapi=False):
=======
class Request(object):
    def __init__(self, url, data=None, headers=None, unredirected_headers=None,
                 timeout=5, use_proxy=True, proxy_ssl_verification=True,
                 proxy_ssl_server_hostname=True, ca_path=None, unredirected_headers_for_opener=None,
                 follow_redirects=True, force_basic_auth=False, url_username=None, url_password=None,
                 client_cert=None, client_key=None, force_ipv6=False, decompress=True, ciphers=None,
                 unix_socket=None, ca_path=None, use_gssapi=False, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
        self.unredirected_headers_for_opener = unredirected_headers_for_opener
        self.follow_redirects = follow_redirects
        self.force_basic_auth = force_basic_auth
        self.url_username = url_username
        self.url_password = url_password
        self.client_cert = client_cert
        self.client_key = client_key
        self.force_ipv6 = force_ipv6
        self.decompress = decompress
        self.ciphers = ciphers
        self.unix_socket = unix_socket
        self.use_gssapi = use_gssapi
=======
        self.unredirected_headers_for_opener = unredirected_headers_for_opener
        self.follow_redirects = follow_redirects
        self.force_basic_auth = force_basic_auth
        self.url_username = url_username
        self.url_password = url_password
        self.client_cert = client_cert
        self.client_key = client_key
        self.force_ipv6 = force_ipv6
        self.decompress = decompress
        self.ciphers = ciphers
        self.unix_socket = unix_socket
        self.use_gssapi = use_gssapi
        self.use_netrc = use_netrc
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    def open(self, http, req, timeout=5, use_proxy=True, proxy_ssl_verification=True,
             proxy_ssl_server_hostname=True, ca_path=None, unredirected_headers_for_opener=None,
             follow_redirects=True, force_basic_auth=False, url_username=None, url_password=None,
             client_cert=None, client_key=None, force_ipv6=False, decompress=True, ciphers=None,
             unix_socket=None, ca_path=None, use_gssapi=False):
=======
    def open(self, http, req, timeout=5, use_proxy=True, proxy_ssl_verification=True,
             proxy_ssl_server_hostname=True, ca_path=None, unredirected_headers_for_opener=None,
             follow_redirects=True, force_basic_auth=False, url_username=None, url_password=None,
             client_cert=None, client_key=None, force_ipv6=False, decompress=True, ciphers=None,
             unix_socket=None, ca_path=None, use_gssapi=False, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
            if self.use_gssapi:
                # Use GSSAPI authentication
                self._setup_gssapi(http, req)
=======
            if self.use_netrc:
                # Use .netrc for authentication
                self._setup_netrc(http, req)
            if self.use_gssapi:
                # Use GSSAPI authentication
                self._setup_gssapi(http, req)
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
            dest=dict(type='path', required=True),
            tmp_dest=dict(type='path'),
            force=dict(type='bool', default=False),
            backup=dict(type='bool', default=False),
            checksum=dict(type='str', default=''),
            use_proxy=dict(type='bool', default=True),
            validate_certs=dict(type='bool', default=True),
            timeout=dict(type='int', default=10),
            headers=dict(type='dict'),
            url_username=dict(type='str', aliases=['username']),
            url_password=dict(type='str', aliases=['password']),
            force_basic_auth=dict(type='bool', default=False),
            client_cert=dict(type='path'),
            client_key=dict(type='path'),
=======
            dest=dict(type='path', required=True),
            tmp_dest=dict(type='path'),
            force=dict(type='bool', default=False),
            backup=dict(type='bool', default=False),
            checksum=dict(type='str', default=''),
            use_proxy=dict(type='bool', default=True),
            validate_certs=dict(type='bool', default=True),
            timeout=dict(type='int', default=10),
            headers=dict(type='dict'),
            url_username=dict(type='str', aliases=['username']),
            url_password=dict(type='str', aliases=['password']),
            force_basic_auth=dict(type='bool', default=False),
            client_cert=dict(type='path'),
            client_key=dict(type='path'),
            use_netrc=dict(type='bool', default=True),
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
            client_cert=module.params.get('client_cert'),
            client_key=module.params.get('client_key'),
=======
            client_cert=module.params.get('client_cert'),
            client_key=module.params.get('client_key'),
            use_netrc=module.params.get('use_netrc'),
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
            client_cert=module.params.get('client_cert'),
            client_key=module.params.get('client_key'),
=======
            client_cert=module.params.get('client_cert'),
            client_key=module.params.get('client_key'),
            use_netrc=module.params.get('use_netrc'),
>>>>>>> REPLACE
