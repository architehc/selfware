### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(url, client_cert=None, client_key=None, cookies=None, data=None, follow_redirects='urllib2', force=False, force_basic_auth='', headers=None, http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='', use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None):
=======
def open_url(url, client_cert=None, client_key=None, cookies=None, data=None, follow_redirects='urllib2', force=False, force_basic_auth='', headers=None, http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='', use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None, use_netrc=True):
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    # Lowercase keys, to conform to py2 behavior, so that py3 and py2 are predictable
    info.update(dict((k.lower(), v) for k, v in r.info().items()))
=======
    # Lowercase keys, to conform to py2 behavior, so that py3 and py2 are predictable
    info.update(dict((k.lower(), v) for k, v in r.info().items()))
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    # Don't be lossy, append header values for duplicate headers
    # In Py2 there is nothing that needs done, py2 does this for us
    if PY3:
        temp_headers = {}
        for name, value in r.headers.items():
            # The same as above, lower case keys to match py2 behavior, and create more consistent results
            name = name.lower()
            if name in temp_headers:
                temp_headers[name] = ', '.join((temp_headers[name], value))
            else:
                temp_headers[name] = value
        info.update(temp_headers)
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    cookie_list = []
    cookie_dict = dict()
    # Python sorts cookies in order of most specific (ie. longest) path first. See ``CookieJar._cookie_attrs``
    # Cookies with the same path are reversed from response order.
    # This code makes no assumptions about that, and accepts the order given by python
    for cookie in cookies:
        cookie_dict[cookie.name] = cookie.value
        cookie_list.append((cookie.name, cookie.value))
    info['cookies_string'] = '; '.join('%s=%s' % c for c in cookie_list)
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    info['cookies'] = cookie_dict
    # finally update the result with a message about the fetch
    info.update(dict(msg="OK (%s bytes)" % r.headers.get('Content-Length', 'unknown'), url=r.geturl(), status=r.code))
    return r, info
>>>>>>> REPLACE

### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
    except NoSSLError as e:
        distribution = get_distribution()
        if distribution is not None and distribution.lower() == 'redhat':
            module.fail_json(msg='%s. You can also install python-ssl from EPEL' % to_native(e), **info)
        else:
            module.fail_json(msg='%s' % to_native(e), **info)
    except (ConnectionError, ValueError) as e:
        module.fail_json(msg=to_native(e), **info)
    except MissingModuleError as e:
        module.fail_json(msg=to_text(e), exception=e.import_traceback)
    except urllib_error.HTTPError as e:
        r = e
        try:
            if e.fp is None:
                # Certain HTTPError objects may not have the ability to call ``.read()`` on Python 3
                # This is not handled gracefully in Python 3, and instead an exception is raised from
                # tempfile, due to ``urllib.response.addinfourl`` not being initialized
                raise AttributeError
            body = e.read()
        except AttributeError:
            body = ''
        else:
            e.close()
        
        # Try to add exception info to the output but don't fail if we can't
        try:
            # Lowercase keys, to conform to py2 behavior, so that py3 and py2 are predictable
            info.update(dict((k.lower(), v) for k, v in e.info().items()))
        except Exception:
            pass
>>>>>>> REPLACE

### FILE: lib/ansible/modules/apt_repository.py
<<<<<<< SEARCH
def fetch_url(module, url):
    return open_url(url, client_cert=None, client_key=None, cookies=None, data=None, follow_redirects='urllib2', force=False, force_basic_auth='', headers=None, http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='', use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None)
>>>>>>> REPLACE

### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
def fetch_url(module, url):
    return open_url(url, client_cert=None, client_key=None, cookies=None, data=None, follow_redirects='urllib2', force=False, force_basic_auth='', headers=None, http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='', use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None, decompress=True, ciphers=None)
>>>>>>> REPLACE

### FILE: test/integration/targets/get_url/tasks/use_netrc.yml
<<<<<<< SEARCH
- name: Test get_url with use_netrc
  get_url:
    url: https://example.com
    dest: /tmp
    use_netrc: yes
  register: result
  failed_when: result.status!= 200
  tags: use_netrc
>>>>>>> REPLACE

### FILE: test/integration/targets/lookup_url/tasks/use_netrc.yml
<<<<<<< SEARCH
- name: Test lookup_url with use_netrc
  lookup_url:
    url: https://example.com
    dest: /tmp
    use_netrc: yes
  register: result
  failed_when: result.status!= 200
  tags: use_netrc
>>>>>>> REPLACE

### FILE: test/integration/targets/uri/tasks/use_netrc.yml
<<<<<<< SEARCH
- name: Test uri with use_netrc
  uri:
    url: https://example.com
    dest: /tmp
    use_netrc: yes
  register: result
  failed_when: result.status!= 200
  tags: use_netrc
>>>>>>> REPLACE