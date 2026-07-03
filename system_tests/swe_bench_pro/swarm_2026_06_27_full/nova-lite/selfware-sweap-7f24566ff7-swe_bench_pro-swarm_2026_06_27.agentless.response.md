### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(url, **kwargs):
    """Open a URL and return a file-like object."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def fetch_url(module, url, **kwargs):
    """Fetch a URL and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def url_get(module, url, **kwargs):
    """Get a URL and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def get_url(module, url, **kwargs):
    """Get a URL and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def fetch_url_multipart(module, url, **kwargs):
    """Fetch a URL and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def fetch_url_smart(module, url, **kwargs):
    """Fetch a URL and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def fetch_url_async(module, url, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def fetch_url_async_smart(module, url, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def fetch_url_async_multipart(module, url, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def fetch_url_async_smart_multipart(module, url, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
def fetch_url_async_smart_multipart_no_wait(module, url, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
    if 'use_netrc' not in kwargs:
        kwargs['use_netrc'] = True
   ...
=======
def open_url(url, use_netrc=True, **kwargs):
    """Open a URL and return a file-like object."""
   ...
def fetch_url(module, url, use_netrc=True, **kwargs):
    """Fetch a URL and return a tuple of (response, info)."""
   ...
def url_get(module, url, use_netrc=True, **kwargs):
    """Get a URL and return a tuple of (response, info)."""
   ...
def get_url(module, url, use_netrc=True, **kwargs):
    """Get a URL and return a tuple of (response, info)."""
   ...
def fetch_url_multipart(module, url, use_netrc=True, **kwargs):
    """Fetch a URL and return a tuple of (response, info)."""
   ...
def fetch_url_smart(module, url, use_netrc=True, **kwargs):
    """Fetch a URL and return a tuple of (response, info)."""
   ...
def fetch_url_async(module, url, use_netrc=True, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
   ...
def fetch_url_async_smart(module, url, use_netrc=True, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
   ...
def fetch_url_async_multipart(module, url, use_netrc=True, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
   ...
def fetch_url_async_smart_multipart(module, url, use_netrc=True, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
   ...
def fetch_url_async_smart_multipart_no_wait(module, url, use_netrc=True, **kwargs):
    """Fetch a URL asynchronously and return a tuple of (response, info)."""
   ...
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
def main():
    module = AnsibleModule(
        argument_spec=dict(
           ...
        ),
        supports_check_mode=True,
    )
   ...
    result = get_url(module, url, **params)
   ...
=======
def main():
    module = AnsibleModule(
        argument_spec=dict(
            use_netrc=dict(type='bool', default=True),
           ...
        ),
        supports_check_mode=True,
    )
   ...
    result = get_url(module, url, use_netrc=module.params.get('use_netrc', True), **params)
   ...
>>>>>>> REPLACE

### FILE: lib/ansible/modules/uri.py
<<<<<<< SEARCH
def run_uri(module, uri, **kwargs):
   ...
    result = get_url(module, uri, **kwargs)
   ...
=======
def run_uri(module, uri, **kwargs):
   ...
    result = get_url(module, uri, use_netrc=module.params.get('use_netrc', True), **kwargs)
   ...
>>>>>>> REPLACE

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< SEARCH
def test_fetch_url(open_url_mock, fake_ansible_module):
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
   ...
def test_fetch_url_params(open_url_mock, fake_ansible_module):
   ...
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
   ...
=======
def test_fetch_url(open_url_mock, fake_ansible_module):
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/', use_netrc=True)
   ...
def test_fetch_url_params(open_url_mock, fake_ansible_module):
   ...
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/', use_netrc=True)
   ...
>>>>>>> REPLACE