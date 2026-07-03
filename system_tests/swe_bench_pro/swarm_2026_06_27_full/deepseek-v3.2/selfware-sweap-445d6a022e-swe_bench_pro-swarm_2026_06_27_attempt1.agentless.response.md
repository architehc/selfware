### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    def __init__(self, url, headers=None, use_proxy=True, force=False, timeout=10, validate_certs=True, url_username=None, url_password=None,
                 http_agent=None, force_basic_auth=False, follow_redirects='urllib2', client_cert=None, client_key=None, cookies=None,
                 use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None):
=======
    def __init__(self, url, headers=None, use_proxy=True, force=False, timeout=10, validate_certs=True, url_username=None, url_password=None,
                 http_agent=None, force_basic_auth=False, follow_redirects='urllib2', client_cert=None, client_key=None, cookies=None,
                 use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
        self.unredirected_headers = unredirected_headers
        self.decompress = decompress
        self.ciphers = ciphers
=======
        self.unredirected_headers = unredirected_headers
        self.decompress = decompress
        self.ciphers = ciphers
        self.use_netrc = use_netrc
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    def open(self, method, url, data=None, headers=None, use_proxy=None, force=None, timeout=None, validate_certs=None, url_username=None,
             url_password=None, http_agent=None, force_basic_auth=None, follow_redirects=None, client_cert=None, client_key=None, cookies=None,
             use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=None, ciphers=None):
=======
    def open(self, method, url, data=None, headers=None, use_proxy=None, force=None, timeout=None, validate_certs=None, url_username=None,
             url_password=None, http_agent=None, force_basic_auth=None, follow_redirects=None, client_cert=None, client_key=None, cookies=None,
             use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=None, ciphers=None, use_netrc=None):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
        ciphers = self._fallback(ciphers, self.ciphers)

        handlers = []
=======
        ciphers = self._fallback(ciphers, self.ciphers)
        use_netrc = self._fallback(use_netrc, self.use_netrc)

        handlers = []
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
            elif username and force_basic_auth:
                headers["Authorization"] = basic_auth_header(username, password)

            else:
                try:
                    rc = netrc.netrc(os.environ.get('NETRC'))
                    login = rc.authenticators(parsed.hostname)
                except IOError:
                    login = None

                if login:
                    username, _, password = login
                    if username and password:
                        headers["Authorization"] = basic_auth_header(username, password)
=======
            elif username and force_basic_auth:
                headers["Authorization"] = basic_auth_header(username, password)

            else:
                if use_netrc:
                    try:
                        rc = netrc.netrc(os.environ.get('NETRC'))
                        login = rc.authenticators(parsed.hostname)
                    except IOError:
                        login = None

                    if login:
                        username, _, password = login
                        if username and password:
                            headers["Authorization"] = basic_auth_header(username, password)
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(url, data=None, headers=None, method=None, use_proxy=True,
             force=False, timeout=10, validate_certs=True, url_username=None,
             url_password=None, http_agent=None, force_basic_auth=False,
             follow_redirects='urllib2', client_cert=None, client_key=None,
             cookies=None, use_gssapi=False, unix_socket=None, ca_path=None,
             unredirected_headers=None, decompress=True, ciphers=None):
=======
def open_url(url, data=None, headers=None, method=None, use_proxy=True,
             force=False, timeout=10, validate_certs=True, url_username=None,
             url_password=None, http_agent=None, force_basic_auth=False,
             follow_redirects='urllib2', client_cert=None, client_key=None,
             cookies=None, use_gssapi=False, unix_socket=None, ca_path=None,
             unredirected_headers=None, decompress=True, ciphers=None,
             use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    r = Request(url, data=data, headers=headers, method=method, use_proxy=use_proxy,
                force=force, timeout=timeout, validate_certs=validate_certs,
                url_username=url_username, url_password=url_password,
                http_agent=http_agent, force_basic_auth=force_basic_auth,
                follow_redirects=follow_redirects, client_cert=client_cert,
                client_key=client_key, cookies=cookies, use_gssapi=use_gssapi,
                unix_socket=unix_socket, ca_path=ca_path,
                unredirected_headers=unredirected_headers, decompress=decompress,
                ciphers=ciphers)
=======
    r = Request(url, data=data, headers=headers, method=method, use_proxy=use_proxy,
                force=force, timeout=timeout, validate_certs=validate_certs,
                url_username=url_username, url_password=url_password,
                http_agent=http_agent, force_basic_auth=force_basic_auth,
                follow_redirects=follow_redirects, client_cert=client_cert,
                client_key=client_key, cookies=cookies, use_gssapi=use_gssapi,
                unix_socket=unix_socket, ca_path=ca_path,
                unredirected_headers=unredirected_headers, decompress=decompress,
                ciphers=ciphers, use_netrc=use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def fetch_url(module, url, data=None, headers=None, method=None,
              use_proxy=True, force=False, last_mod_time=None, timeout=10,
              validate_certs=True, url_username=None, url_password=None,
              http_agent=None, force_basic_auth=False, follow_redirects='urllib2',
              client_cert=None, client_key=None, cookies=None, use_gssapi=False,
              unix_socket=None, ca_path=None, unredirected_headers=None,
              decompress=True, ciphers=None):
=======
def fetch_url(module, url, data=None, headers=None, method=None,
              use_proxy=True, force=False, last_mod_time=None, timeout=10,
              validate_certs=True, url_username=None, url_password=None,
              http_agent=None, force_basic_auth=False, follow_redirects='urllib2',
              client_cert=None, client_key=None, cookies=None, use_gssapi=False,
              unix_socket=None, ca_path=None, unredirected_headers=None,
              decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    r, info = open_url(url, data, headers, method, use_proxy, force,
                       timeout, validate_certs, url_username, url_password,
                       http_agent, force_basic_auth, follow_redirects,
                       client_cert, client_key, cookies, use_gssapi,
                       unix_socket, ca_path, unredirected_headers,
                       decompress, ciphers)
=======
    r, info = open_url(url, data, headers, method, use_proxy, force,
                       timeout, validate_certs, url_username, url_password,
                       http_agent, force_basic_auth, follow_redirects,
                       client_cert, client_key, cookies, use_gssapi,
                       unix_socket, ca_path, unredirected_headers,
                       decompress, ciphers, use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, headers=None, tmp_dest='', method='GET', unredirected_headers=None,
            decompress=True, ciphers=None):
=======
def url_get(module, url, dest, use_proxy, last_mod_time, force, timeout=10, headers=None, tmp_dest='', method='GET', unredirected_headers=None,
            decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
    rsp, info = fetch_url(module, url, use_proxy=use_proxy, force=force, last_mod_time=last_mod_time, timeout=timeout, headers=headers, method=method,
                          unredirected_headers=unredirected_headers, decompress=decompress, ciphers=ciphers)
=======
    rsp, info = fetch_url(module, url, use_proxy=use_proxy, force=force, last_mod_time=last_mod_time, timeout=timeout, headers=headers, method=method,
                          unredirected_headers=unredirected_headers, decompress=decompress, ciphers=ciphers, use_netrc=use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
        if is_url(checksum):
            checksum_url = checksum
            # download checksum file to checksum_tmpsrc
            checksum_tmpsrc, checksum_info = url_get(module, checksum_url, dest, use_proxy, last_mod_time, force, timeout, headers, tmp_dest,
                                                     unredirected_headers=unredirected_headers, ciphers=ciphers)
=======
        if is_url(checksum):
            checksum_url = checksum
            # download checksum file to checksum_tmpsrc
            checksum_tmpsrc, checksum_info = url_get(module, checksum_url, dest, use_proxy, last_mod_time, force, timeout, headers, tmp_dest,
                                                     unredirected_headers=unredirected_headers, ciphers=ciphers, use_netrc=use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
    tmpsrc, info = url_get(module, url, dest, use_proxy, last_mod_time, force, timeout, headers, tmp_dest, method,
                           unredirected_headers=unredirected_headers, decompress=decompress, ciphers=ciphers)
=======
    tmpsrc, info = url_get(module, url, dest, use_proxy, last_mod_time, force, timeout, headers, tmp_dest, method,
                           unredirected_headers=unredirected_headers, decompress=decompress, ciphers=ciphers, use_netrc=use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/modules/uri.py
<<<<<<< SEARCH
    argument_spec.update(
        url=dict(type='str', required=True),
        dest=dict(type='path', required=False),
        body=dict(type='raw'),
        body_format=dict(type='str', choices=['form-urlencoded', 'json', 'raw'], default='raw'),
        method=dict(type='str', default='GET'),
        return_content=dict(type='bool', default=False),
        status_code=dict(type='list', elements='int', default=[200]),
        headers=dict(type='dict'),
        timeout=dict(type='int', default=30),
        follow_redirects=dict(type='str', choices=['all', 'none', 'safe', 'urllib2', 'yes', 'no'], default='safe'),
        validate_certs=dict(type='bool', default=True),
        url_username=dict(type='str'),
        url_password=dict(type='str', no_log=True),
        force_basic_auth=dict(type='bool', default=False),
        client_cert=dict(type='path'),
        client_key=dict(type='path'),
        use_gssapi=dict(type='bool', default=False),
        use_proxy=dict(type='bool', default=True),
        cookie_file=dict(type='path'),
        cookies=dict(type='dict'),
        unix_socket=dict(type='path'),
        ca_path=dict(type='path'),
        unredirected_headers=dict(type='list', elements='str', default=[]),
        decompress=dict(type='bool', default=True),
        ciphers=dict(type='list', elements='str'),
        http_agent=dict(type='str', default='ansible-httpget'),
        follow=dict(type='bool', default=False, removed_in_version='2.14', removed_from_collection='ansible.builtin'),
    )
=======
    argument_spec.update(
        url=dict(type='str', required=True),
        dest=dict(type='path', required=False),
        body=dict(type='raw'),
        body_format=dict(type='str', choices=['form-urlencoded', 'json', 'raw'], default='raw'),
        method=dict(type='str', default='GET'),
        return_content=dict(type='bool', default=False),
        status_code=dict(type='list', elements='int', default=[200]),
        headers=dict(type='dict'),
        timeout=dict(type='int', default=30),
        follow_redirects=dict(type='str', choices=['all', 'none', 'safe', 'urllib2', 'yes', 'no'], default='safe'),
        validate_certs=dict(type='bool', default=True),
        url_username=dict(type='str'),
        url_password=dict(type='str', no_log=True),
        force_basic_auth=dict(type='bool', default=False),
        client_cert=dict(type='path'),
        client_key=dict(type='path'),
        use_gssapi=dict(type='bool', default=False),
        use_proxy=dict(type='bool', default=True),
        cookie_file=dict(type='path'),
        cookies=dict(type='dict'),
        unix_socket=dict(type='path'),
        ca_path=dict(type='path'),
        unredirected_headers=dict(type='list', elements='str', default=[]),
        decompress=dict(type='bool', default=True),
        ciphers=dict(type='list', elements='str'),
        http_agent=dict(type='str', default='ansible-httpget'),
        follow=dict(type='bool', default=False, removed_in_version='2.14', removed_from_collection='ansible.builtin'),
        use_netrc=dict(type='bool', default=True),
    )
>>>>>>> REPLACE

### FILE: lib/ansible/modules/uri.py
<<<<<<< SEARCH
    # fetch the requested data
    start = datetime.datetime.utcnow()
    response, info = fetch_url(module, url, data=body, headers=headers, method=method,
                               timeout=timeout, use_proxy=use_proxy,
                               validate_certs=validate_certs,
                               url_username=url_username, url_password=url_password,
                               force_basic_auth=force_basic_auth,
                               follow_redirects=follow_redirects,
                               client_cert=client_cert, client_key=client_key,
                               cookies=cookies, use_gssapi=use_gssapi,
                               unix_socket=unix_socket, ca_path=ca_path,
                               unredirected_headers=unredirected_headers,
                               decompress=decompress, ciphers=ciphers,
                               http_agent=http_agent)
=======
    # fetch the requested data
    start = datetime.datetime.utcnow()
    response, info = fetch_url(module, url, data=body, headers=headers, method=method,
                               timeout=timeout, use_proxy=use_proxy,
                               validate_certs=validate_certs,
                               url_username=url_username, url_password=url_password,
                               force_basic_auth=force_basic_auth,
                               follow_redirects=follow_redirects,
                               client_cert=client_cert, client_key=client_key,
                               cookies=cookies, use_gssapi=use_gssapi,
                               unix_socket=unix_socket, ca_path=ca_path,
                               unredirected_headers=unredirected_headers,
                               decompress=decompress, ciphers=ciphers,
                               http_agent=http_agent, use_netrc=use_netrc)
>>>>>>> REPLACE

### FILE: lib/ansible/plugins/lookup/url.py
<<<<<<< SEARCH
    def run(self, terms, variables=None, **kwargs):
=======
    def run(self, terms, variables=None, **kwargs):
        use_netrc = kwargs.get('use_netrc', True)
>>>>>>> REPLACE

### FILE: lib/ansible/plugins/lookup/url.py
<<<<<<< SEARCH
            response, info = fetch_url(self._templar, term, validate_certs=validate_certs, use_proxy=use_proxy,
                                       headers=headers, force=force, timeout=timeout, http_agent=http_agent,
                                       force_basic_auth=force_basic_auth, follow_redirects=follow_redirects,
                                       client_cert=client_cert, client_key=client_key, unix_socket=unix_socket,
                                       ca_path=ca_path, unredirected_headers=unredirected_headers,
                                       decompress=decompress, ciphers=ciphers)
=======
            response, info = fetch_url(self._templar, term, validate_certs=validate_certs, use_proxy=use_proxy,
                                       headers=headers, force=force, timeout=timeout, http_agent=http_agent,
                                       force_basic_auth=force_basic_auth, follow_redirects=follow_redirects,
                                       client_cert=client_cert, client_key=client_key, unix_socket=unix_socket,
                                       ca_path=ca_path, unredirected_headers=unredirected_headers,
                                       decompress=decompress, ciphers=ciphers, use_netrc=use_netrc)
>>>>>>> REPLACE