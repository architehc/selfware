### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(module, url, **kwargs):
    # existing code...
    use_netrc = kwargs.get('use_netrc', True)  # default to True
    # existing code...
=======
def open_url(module, url, use_netrc=True, **kwargs):
    # existing code...
    # existing code...
=======

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
def main():
    module = AnsibleModule(
        argument_spec=dict(
            # existing arguments...
        ),
        supports_check_mode=True,
    )
    # existing code...
=======
def main():
    module = AnsibleModule(
        argument_spec=dict(
            # existing arguments...
            use_netrc=dict(type='bool', default=True),  # add use_netrc argument
        ),
        supports_check_mode=True,
    )
    # existing code...
=======

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
    use_proxy = module.params['use_proxy']
    # existing code...
    # existing code...
=======
    use_netrc = module.params['use_netrc']
    use_proxy = module.params['use_proxy']
    # existing code...
    # existing code...
=======

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    r = fetch_url(module, url, **kwargs)
=======
    r = fetch_url(module, url, use_netrc=use_netrc, **kwargs)
=======

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< SEARCH
def test_fetch_url(open_url_mock, fake_ansible_module):
    # existing code...
=======
def test_fetch_url(open_url_mock, fake_ansible_module):
    fake_ansible_module.params = {
        # existing parameters...
        'use_netrc': True,  # add use_netrc parameter
    }
    # existing code...
=======

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
                                          decompress=True, ciphers=None, use_netrc=True)  # add use_netrc to the call
>>>>>>> REPLACE