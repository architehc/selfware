### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    def __init__(self, *args, **kwargs):
        self.use_netrc = kwargs.pop('use_netrc', True)
        super(Request, self).__init__(*args, **kwargs)
=======
    def __init__(self, *args, **kwargs):
        self.use_netrc = kwargs.pop('use_netrc', False)
        super(Request, self).__init__(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    def open(self, *args, **kwargs):
        if self.use_netrc:
            kwargs.setdefault('netrc', True)
        return super(Request, self).open(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    def open_url(self, *args, **kwargs):
        if 'use_netrc' not in kwargs:
            kwargs['use_netrc'] = True
        return super(Request, self).open_url(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    def fetch_url(self, *args, **kwargs):
        if 'use_netrc' not in kwargs:
            kwargs['use_netrc'] = True
        return super(Request, self).fetch_url(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    def url_get(self, *args, **kwargs):
        if 'use_netrc' not in kwargs:
            kwargs['use_netrc'] = True
        return super(Request, self).url_get(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    def get_url(self, *args, **kwargs):
        if 'use_netrc' not in kwargs:
            kwargs['use_netrc'] = True
        return super(Request, self).get_url(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/modules/apt_repository.py
<<<<<<< 
    def __init__(self, *args, **kwargs):
        self.use_netrc = kwargs.pop('use_netrc', True)
        super(AptRepository, self).__init__(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/modules/apt_repository.py
<<<<<<< 
    def open_url(self, *args, **kwargs):
        if 'use_netrc' not in kwargs:
            kwargs['use_netrc'] = True
        return super(AptRepository, self).open_url(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/modules/get_url.py
<<<<<<< 
    def __init__(self, *args, **kwargs):
        self.use_netrc = kwargs.pop('use_netrc', True)
        super(GetUrl, self).__init__(*args, **kwargs)
>>>>>>> 

### FILE: lib/ansible/modules/get_url.py
<<<<<<< 
    def open_url(self, *args, **kwargs):
        if 'use_netrc' not in kwargs:
            kwargs['use_netrc'] = True
        return super(GetUrl, self).open_url(*args, **kwargs)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_urlparse(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(open_url_mock, fake_ansible_module):
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error unknown url error>', 'status': -1, 'url': 'http://ansible.com/'}
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': True}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error unknown url error>', 'status': -1, 'url': 'http://ansible.com/'}
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] === -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] === 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_nossl(open_url_mock, fake_ansible_module, mocker):
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='notredhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' not in excinfo.value.kwargs['msg']
    mocker.patch('ansible.module_utils.urls.get_distribution', return_value='redhat')
    open_url_mock.side_effect = NoSSLError
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'python-ssl' in excinfo.value.kwargs['msg']
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_httperror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.HTTPError(
        'http://ansible.com/',
        500,
        'Internal Server Error',
        {'Content-Type': 'application/json'},
        StringIO('TESTS')
    )
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'HTTP Error 500: Internal Server Error', 'body': 'TESTS',
                    'status': 500, 'url': 'http://ansible.com/', 'content-type': 'application/json'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Bad status line', 'status': -1, 'url': 'http://ansible.com/'}
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_no_netrc(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)
    fake_ansible_module.params = {'use_netrc': False}
    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
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
        'use_netrc': False
    }
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    dummy, kwargs = open_url_mock.call_args
    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert='client.pem', client_key='client.key', cookies=kwargs['cookies'], data=None,
                                          follow_redirects='all', force=False, force_basic_auth=True, headers=None,
                                          http_agent='ansible-test', last_mod_time=None, method=None, timeout=10, url_password='passwd', url_username='user',
                                          use_proxy=True, validate_certs=False, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)
>>>>>>> 

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< 
def test_fetch_url_cookies(mocker, fake_ansible_module):
    def make_cookies(*args, **kwargs):
        cookies = kwargs['cookies']
        r = MagicMock()
        try:
            r.headers = HTTPMessage()
            add_header = r.headers.add_header
        except TypeError:
            # PY2
            r.headers = HTTPMessage(StringIO())
            add_header = r.headers.addheader
        r.info.return_value = r.headers
        for name, value in (('Foo', 'bar'), ('Baz', 'qux')):
            cookie = Cookie(
                version=0,
                name=name,
                value=value,
                port=None,
                port_specified=False,
                domain="ansible.com",
                domain_specified=True,
                domain_initial_dot=False,
                path="/",
                path_specified=True,
                secure=False,
                expires=None,
                discard=False,
                comment=None,
                comment_url=None,
                rest=None
            )
            cookies.set_cookie(cookie)
            add_header('Set-Cookie', '%s=%s' % (name, value))

        return r

    mocker = mocker.patch('ansible.module_utils.urls.open_url', new=make_cookies)
    fake_ansible_module.params = {'use_netrc': False}
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'cookies': {'Baz': 'qux', 'Foo': 'bar'}}
    if sys.version_info < (3, 11):
        assert info['cookies_string'] == 'Baz=qux; Foo=bar'
    else:
        assert info['cookies_string'] == 'Foo=bar; Baz=qux'
    assert