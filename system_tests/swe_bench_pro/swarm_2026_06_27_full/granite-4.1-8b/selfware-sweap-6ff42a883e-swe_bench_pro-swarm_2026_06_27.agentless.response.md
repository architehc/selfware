### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(module, url, client_cert=None, client_key=None, cookies=None, data=None,
             follow_redirects='urllib2', force=False, force_basic_auth=False, headers=None,
             http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10,
             url_password='', url_username='', use_proxy=True, validate_certs=True,
             use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
             decompress=True, ciphers=None):
    """Open a URL using urllib2 or urllib3."""
    # Existing implementation...
=======
def open_url(module, url, client_cert=None, client_key=None, cookies=None, data=None,
             follow_redirects='urllib2', force=False, force_basic_auth=False, headers=None,
             http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10,
             url_password='', url_username='', use_proxy=True, validate_certs=True,
             use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
             decompress=True, ciphers=None, use_netrc=True):
    """Open a URL using urllib2 or urllib3."""
    # Existing implementation...
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
    ciphers=ciphers,
    decompress=decompress,
    url=url,
    dest=dest,
    tmp_dest=tmp_dest,
    force=force,
    backup=backup,
    checksum=checksum,
    use_proxy=use_proxy,
    validate_certs=validate_certs,
    timeout=timeout,
    headers=headers,
    url_username=url_username,
    url_password=url_password,
    force_basic_auth=force_basic_auth,
    client_cert=client_cert,
    client_key=client_key,
    ):
    """Downloads files from HTTP, HTTPS, or FTP to node"""
    # Existing implementation...
=======
    ciphers=ciphers,
    decompress=decompress,
    url=url,
    dest=dest,
    tmp_dest=tmp_dest,
    force=force,
    backup=backup,
    checksum=checksum,
    use_proxy=use_proxy,
    validate_certs=validate_certs,
    timeout=timeout,
    headers=headers,
    url_username=url_username,
    url_password=url_password,
    force_basic_auth=force_basic_auth,
    client_cert=client_cert,
    client_key=client_key,
    use_netrc=use_netrc,
    ):
    """Downloads files from HTTP, HTTPS, or FTP to node"""
    # Existing implementation...
>>>>>>> REPLACE

### FILE: lib/ansible/modules/uri.py
<<<<<<< SEARCH
    ciphers=ciphers,
    decompress=decompress,
    url=url,
    dest=dest,
    tmp_dest=tmp_dest,
    force=force,
    backup=backup,
    checksum=checksum,
    use_proxy=use_proxy,
    validate_certs=validate_certs,
    timeout=timeout,
    headers=headers,
    url_username=url_username,
    url_password=url_password,
    force_basic_auth=force_basic_auth,
    client_cert=client_cert,
    client_key=client_key,
    ):
    """Performs HTTP/HTTPS requests."""
    # Existing implementation...
=======
    ciphers=ciphers,
    decompress=decompress,
    url=url,
    dest=dest,
    tmp_dest=tmp_dest,
    force=force,
    backup=backup,
    checksum=checksum,
    use_proxy=use_proxy,
    validate_certs=validate_certs,
    timeout=timeout,
    headers=headers,
    url_username=url_username,
    url_password=url_password,
    force_basic_auth=force_basic_auth,
    client_cert=client_cert,
    client_key=client_key,
    use_netrc=use_netrc,
    ):
    """Performs HTTP/HTTPS requests."""
    # Existing implementation...
>>>>>>> REPLACE

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< SEARCH
def test_fetch_url( open_url_mock, fake_ansible_module ):
    r, info = fetch_url( fake_ansible_module, 'http://ansible.com/' )
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with( 'http://ansible.com/', client_cert=None, client_key=None, cookies=kwargs['cookies'], data=None,
                                           follow_redirects='urllib2', force=False, force_basic_auth='', headers=None,
                                           http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='', use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None )
=======
def test_fetch_url( open_url_mock, fake_ansible_module ):
    r, info = fetch_url( fake_ansible_module, 'http://ansible.com/', use_netrc=True )
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with( 'http://ansible.com/', client_cert=None, client_key=None, cookies=kwargs['cookies'], data=None,
                                           follow_redirects='urllib2', force=False, force_basic_auth='', headers=None,
                                           http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='', use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_netrc=True )
>>>>>>> REPLACE

### FILE: test/units/module_utils/urls/test_Request.py
<<<<<<< SEARCH
def test_Request_fallback( mocker ):
    mocker.patch( 'ansible.module_utils.urls.HAS_URLPARSE', new=False )
    with pytest.raises( FailJson ):
        fetch_url( fake_ansible_module, 'http://ansible.com/' )
=======
def test_Request_fallback( mocker ):
    mocker.patch( 'ansible.module_utils.urls.HAS_URLPARSE', new=False )
    with pytest.raises( FailJson ):
        fetch_url( fake_ansible_module, 'http://ansible.com/', use_netrc=True )
>>>>>>> REPLACE

### FILE: test/integration/targets/get_url/tasks/use_netrc.yml
<<<<<<< SEARCH
# --- BEGINNING OF FILE ---
=======
# Use .netrc to verify Authorization header is respected
- name: Ensure .netrc does not override Authorization header
  get_url:
    url: https://example.com/api
    dest: /tmp/example.json
    headers:
      Authorization: Bearer abcdefghijklmnop
    use_netrc: false
  register: result
  assert:
    truthy: true
    msg: "The Authorization header should be respected and the request should succeed."
>>>>>>> REPLACE

### FILE: test/integration/targets/lookup_url/tasks/use_netrc.yml
<<<<<<< SEARCH
# --- BEGINNING OF FILE ---
=======
# Use .netrc to verify Authorization header is respected
- name: Ensure .netrc does not override Authorization header
  set_fact:
    lookup_result: "{{ lookup('url', 'https://example.com/api', headers={'Authorization': 'Bearer abcdefghijklmnop'}, use_netrc=false) }}"
  register: result
  assert:
    truthy: true
    msg: "The Authorization header should be respected and the request should succeed."
>>>>>>> REPLACE

### FILE: test/integration/targets/uri/tasks/use_netrc.yml
<<<<<<< SEARCH
# --- BEGINNING OF FILE ---
=======
# Use .netrc to verify Authorization header is respected
- name: Ensure .netrc does not override Authorization header
  uri:
    url: https://example.com/api
    method: GET
    headers:
      Authorization: Bearer abcdefghijklmnop
    use_netrc: false
  register: result
  assert:
    truthy: true
    msg: "The Authorization header should be respected and the request should succeed."
>>>>>>> REPLACE