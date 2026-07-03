### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(url, client_cert=None, client_key=None, cookies=None, data=None, follow_redirects='urllib2', force=False, force_basic_auth='', headers=None, http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='', use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None):
=======
def open_url(url, client_cert=None, client_key=None, cookies=None, data=None, follow_redirects='urllib2', force=False, force_basic_auth='', headers=None, http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='', use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def fetch_url(module, url, method='GET', headers=None, data=None, timeout=10, status_forceload=10, force=False, use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None):
=======
def fetch_url(module, url, method='GET', headers=None, data=None, timeout=10, status_forceload=10, force=False, use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/modules/apt_repository.py
<<<<<<< SEARCH
def main():
    module = AnsibleModule(
        argument_spec=dict(
            repo=dict(type='str', required=True),
            state=dict(type='str', default='present', choices=['absent', 'present']),
            mode=dict(type='raw'),
            update_cache=dict(type='bool', default=True, aliases=['update-cache']),
            update_cache_retries=dict(type='int', default=5),
            update_cache_retry_max_delay=dict(type='int', default=12),
            filename=dict(type='str'),
            # This should not be needed, but exists as a failsafe
            install_python_apt=dict(type='bool', default=True),
            validate_certs=dict(type='bool', default=True),
            codename=dict(type='str'),
        ),
        supports_check_mode=True,
    )
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
def main():
    module = AnsibleModule(
        argument_spec=dict(
            url=dict(type='str', required=True),
            dest=dict(type='path', required=True),
            tmp_dest=dict(type='path'),
            force=dict(type='bool', default=False),
            backup=dict(type='bool', default=False),
            checksum=dict(type='str'),
            use_proxy=dict(type='bool', default=True),
            validate_certs=dict(type='bool', default=True),
            timeout=dict(type='int', default=10),
            headers=dict(type='dict'),
            url_username=dict(type='str'),
            url_password=dict(type='str'),
            force_basic_auth=dict(type='bool', default=False),
            client_cert=dict(type='path'),
            client_key=dict(type='path'),
        ),
        supports_check_mode=True,
    )
>>>>>>> REPLACE

### FILE: test/integration/targets/get_url/tasks/use_netrc.yml
<<<<<<< SEARCH
- name: Test that the uri module uses.netrc to overwrite Authorization header even if specified
  uri:
    url: https://example.com
    username: user
    password: pass
    use_netrc: yes
    status: 401
  register: result
  failed_when: result.status!= 401
>>>>>>> REPLACE

### FILE: test/integration/targets/lookup_url/tasks/use_netrc.yml
<<<<<<< SEARCH
- name: Test that the uri module uses.netrc to overwrite Authorization header even if specified
  uri:
    url: https://example.com
    username: user
    password: pass
    use_netrc: yes
    status: 401
  register: result
  failed_when: result.status!= 401
>>>>>>> REPLACE

### FILE: test/integration/targets/uri/tasks/use_netrc.yml
<<<<<<< SEARCH
- name: Test that the uri module uses.netrc to overwrite Authorization header even if specified
  uri:
    url: https://example.com
    username: user
    password: pass
    use_netrc: yes
    status: 401
  register: result
  failed_when: result.status!= 401
>>>>>>> REPLACE