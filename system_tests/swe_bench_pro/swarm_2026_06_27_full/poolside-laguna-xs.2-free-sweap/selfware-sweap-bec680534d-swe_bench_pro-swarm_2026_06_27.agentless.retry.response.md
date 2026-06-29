
Looking at this issue, I need to add a `use_netrc` parameter throughout the URL handling chain to prevent `.netrc` credentials from overriding user-specified Authorization headers. Let me trace through the code and create the necessary patches.

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    def __init__(self, url, data=None, headers=None, unredirected_headers=None,
                 timeout=10, method=None, redirect_interval=0, use_proxy=True,
                 force=False, force_basic_auth=False, url_username=None, url_password=None,
                 client_cert=None, client_key=None, ca_path=None, unredirected_headers=None,
                 decompress=True, ciphers=None):
=======
    def __init__(self, url, data=None, headers=None, unredirected_headers=None,
                 timeout=10, method=None, redirect_interval=0, use_proxy=True,
                 force=False, force_basic_auth=False, url_username=None, url_password=None,
                 client_cert=None, client_key=None, ca_path=None, unredirected_headers=None,
                 decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    def open(self, url, data=None, headers=None, unredirected_headers=None,
             timeout=10, method=None, redirect_interval=0, use_proxy=True,
             force=False, force_basic_auth=False, url_username=None, url_password=None,
             client_cert=None, client_key=None, ca_path=None, unredirected_headers=None,
             decompress=True, ciphers=None):
=======
    def open(self, url, data=None, headers=None, unredirected_headers=None,
             timeout=10, method=None, redirect_interval=0, use_proxy=True,
             force=False, force_basic_auth=False, url_username=None, url_password=None,
             client_cert=None, client_key=None, ca_path=None, unredirected_headers=None,
             decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(url, data=None, headers=None, unredirected_headers=None,
             timeout=10, method=None, redirect_interval=0, use_proxy=True,
             force=False, force_basic_auth=False, url_username=None, url_password=None,
             client_cert=None, client_key=None, ca_path=None, unredirected_headers=None,
             decompress=True, ciphers=None):
=======
def open_url(url, data=None, headers=None, unredirected_headers=None,
             timeout=10, method=None, redirect_interval=0, use_proxy=True,
             force=False, force_basic_auth=False, url_username=None, url_password=None,
             client_cert=None, client_key=None, ca_path=None, unredirected_headers=None,
             decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def fetch_url(module, url, data=None, headers=None, method=None, timeout=None,
              use_proxy=True, unredirected_headers=None, force=False,
              validate_certs=True, url_username=None, url_password=None,
              client_cert=None, client_key=None, ca_path=None,
              decompress=True, ciphers=None):
=======
def fetch_url(module, url, data=None, headers=None, method=None, timeout=None,
              use_proxy=True, unredirected_headers=None, force=False,
              validate_certs=True, url_username=None, url_password=None,
              client_cert=None, client_key=None, ca_path=None,
              decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    r, info = open_url(url, data=data, headers=headers, method=method, timeout=timeout,
                       use_proxy=use_proxy, unredirected_headers=unredirected_headers,
                       force=force, validate_certs=validate_certs,
                       url_username=url_username, url_password=url_password,
                       client_cert=client_cert, client_key=client_key,
                       ca_path=ca_path, decompress=decompress, ciphers=ciphers)
=======
    r, info = open_url(url, data=data, headers=headers, method=method, timeout=timeout,
                       use_proxy=use_proxy, unredirected_headers=unredirected_headers,
                       force=force, validate_certs=validate_certs,
                       url_username=url_username, url_password=url_password,
                       client_cert=client_cert, client_key=client_key,
                       ca_path=ca_path, decompress=decompress, ciphers=ciphers,
                       use_netrc=use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
  client_key:
    description:
      - PEM formatted file that contains your private key to be used for SSL client authentication.
=======
  client_key:
    description:
      - PEM formatted file that contains your private key to be used for SSL client authentication.
  use_netrc:
    description:
      - If C(no), .netrc credentials will not be used for authentication.
      - This allows explicit Authorization headers to take precedence over .netrc credentials.
    type: bool
    default: yes
    version_added: '2.15'
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
            client_cert=dict(type='path'),
            client_key=dict(type='path'),
=======
            client_cert=dict(type='path'),
            client_key=dict(type='path'),
            use_netrc=dict(type='bool', default=True),
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
            r, info = fetch_url(module, url, data=None, headers=headers, method='HEAD',
                                timeout=timeout, use_proxy=use_proxy,
                                unredirected_headers=unredirected_headers, force=force,
                                validate_certs=validate_certs, url_username=url_username,
                                url_password=url_password, client_cert=client_cert,
                                client_key=client_key, ca_path=ca_path,
                                decompress=decompress, ciphers=ciphers)
=======
            r, info = fetch_url(module, url, data=None, headers=headers, method='HEAD',
                                timeout=timeout, use_proxy=use_proxy,
                                unredirected_headers=unredirected_headers, force=force,
                                validate_certs=validate_certs, url_username=url_username,
                                url_password=url_password, client_cert=client_cert,
                                client_key=client_key, ca_path=ca_path,
                                decompress=decompress, ciphers=ciphers,
                                use_netrc=module.params.get('use_netrc', True))
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
            r, info = fetch_url(module, url, data=None, headers=headers, method='GET',
                                timeout=timeout, use_proxy=use_proxy,
                                unredirected_headers=unredirected_headers, force=force,
                                validate_certs=validate_certs, url_username=url_username,
                                url_password=url_password, client_cert=client_cert,
                                client_key=client_key, ca_path=ca_path,
                                decompress=decompress, ciphers=ciphers)
=======
            r, info = fetch_url(module, url, data=None, headers=headers, method='GET',
                                timeout=timeout, use_proxy=use_proxy,
                                unredirected_headers=unredirected_headers, force=force,
                                validate_certs=validate_certs, url_username=url_username,
                                url_password=url_password, client_cert=client_cert,
                                client_key=client_key, ca_path=ca_path,
                                decompress=decompress, ciphers=ciphers,
                                use_netrc=module.params.get('use_netrc', True))
>>>>>>> REPLACE

### FILE: lib/ansible/modules/apt_repository.py
<<<<<<< SEARCH
            filename=dict(type='str'),
            # This should not be needed, but exists as a failsafe
            install_python_apt=dict(type='bool', default=True),
            validate_certs=dict(type='bool', default=True),
            codename=dict(type='str'),
=======
            filename=dict(type='str'),
            # This should not be needed, but exists as a failsafe
            install_python_apt=dict(type='bool', default=True),
            validate_certs=dict(type='bool', default=True),
            codename=dict(type='str'),
            use_netrc=dict(type='bool', default=True),
>>>>>>> REPLACE

### FILE: lib/ansible/modules/apt_repository.py
<<<<<<< SEARCH
            r, info = fetch_url(module, url, data=None, headers=headers, method='GET',
                                timeout=timeout, use_proxy=use_proxy,
                                unredirected_headers=unredirected_headers, force=force,
                                validate_certs=validate_certs, url_username=url_username,
                                url_password=url_password, client_cert=client_cert,
                                client_key=client_key, ca_path=ca_path,
                                decompress=decompress, ciphers=ciphers)
=======
            r, info = fetch_url(module, url, data=None, headers=headers, method='GET',
                                timeout=timeout, use_proxy=use_proxy,
                                unredirected_headers=unredirected_headers, force=force,
                                validate_certs=validate_certs, url_username=url_username,
                                url_password=url_password, client_cert=client_cert,
                                client_key=client_key, ca_path=ca_path,
                                decompress=decompress, ciphers=ciphers,
                                use_netrc=params.get('use_netrc', True))
>>>>>>> REPLACE

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< SEARCH
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert=None, client_key=None, cookies=kwargs['cookies'], data=None,
                                          follow_redirects='urllib2', force=False, force_basic_auth='', headers=None,
                                          http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='',
                                          use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
=======
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert=None, client_key=None, cookies=kwargs['cookies'], data=None,
                                          follow_redirects='urllib2', force=False, force_basic_auth='', headers=None,
                                          http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='',
                                          use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None, use_netrc=True)
>>>>>>> REPLACE

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< SEARCH
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
=======
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None, use_netrc=True)
>>>>>>> REPLACE
