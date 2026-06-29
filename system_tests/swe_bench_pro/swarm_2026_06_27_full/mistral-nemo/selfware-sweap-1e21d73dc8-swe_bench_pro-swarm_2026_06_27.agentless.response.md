### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< SEARCH
    def test_fetch_url_params(open_url_mock, fake_ansible_module):
        fake_ansible_module.params = {
            'validate_certs': False,
            'url_username': 'user',
            'url_password': 'passwd',
            'http_agent': 'ansible-test',
            'force_basic_auth': True,
            'follow_redirects': 'all',
            'client_cert': 'client.pem',
            'client_key': 'client.key',
        }

        r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')

        dummy, kwargs = open_url_mock.call_args

        open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                              follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                              http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                              use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                              decompress=True, ciphers=None, use_netrc=True)
>>>>>>> REPLACE
        open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                              follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                              http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                              use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                              decompress=True, ciphers=None, use_netrc=False)