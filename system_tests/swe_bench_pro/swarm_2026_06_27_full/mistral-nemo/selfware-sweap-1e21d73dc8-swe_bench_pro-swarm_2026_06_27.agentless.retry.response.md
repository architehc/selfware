### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
from __future__ import (absolute_import, division, print_function)
__metaclass__ = type

import atexit
import base64
import email.mime.multipart
import email.mime.nonmultipart
import email.parser
import email.utils
import functools
import io
import mimetypes
import netrc
import os
import platform
import re
import socket
import sys
import tempfile
import traceback
import types

from contextlib import contextmanager

try:
    import gzip
    HAS_GZIP = True
    GZIP_IMP_ERR = None
except ImportError:
    # Python 2.4 (likely rhel5 which doesn't have tls1.1 support in its openssl)
    pass
else:
    GzipFile = gzip.GzipFile  # type: ignore[assignment,misc]

try:
    import email.policy
except ImportError:
    # Py2
    import email.generator

try:
    import httplib
except ImportError:
    # Python 3
    import http.client as httplib  # type: ignore[no-redef]

import ansible.module_utils.compat.typing as t
import ansible.module_utils.six.moves.http_cookiejar as cookiejar
import ansible.module_utils.six.moves.urllib.error as urllib_error

from ansible.module_utils.common.collections import Mapping, is_sequence
from ansible.module_utils.six import PY2, PY3, string_types
from ansible.module_utils.six.moves import cStringIO
from ansible.module_utils.basic import get_distribution, missing_required_lib
from ansible.module_utils._text import to_bytes, to_native, to_text

try:
    # python3
    import urllib.request as urllib_request
    from urllib.request import AbstractHTTPHandler, BaseHandler
except ImportError:
    # python2
    import urllib2 as urllib_request  # type: ignore[no-redef]
    from urllib2 import AbstractHTTPHandler, BaseHandler  # type: ignore[no-redef]

urllib_request.HTTPRedirectHandler.http_error_308 = urllib_request.HTTPRedirectHandler.http_error_307  # type: ignore[attr-defined]

try:
    from ansible.module_utils.six.moves.urllib.parse import urlparse, urlunparse, unquote
    HAS_URLPARSE = True
except Exception:
    HAS_URLPARSE = False

try:
    import ssl
    HAS_SSL = True
except Exception:
    HAS_SSL = False

try:
    # SNI Handling needs python2.7.9's SSLContext
    from ssl import create_default_context, SSLContext
    HAS_SSLCONTEXT = True
except ImportError:
    HAS_SSLCONTEXT = False

# SNI Handling for python < 2.7.9 with urllib3 support
HAS_URLLIB3_PYOPENSSLCONTEXT = False
HAS_URLLIB3_SSL_WRAP_SOCKET = False
if not HAS_SSLCONTEXT:
    try:
        # urllib3>=1.15
        try:
            from urllib3.contrib.pyopenssl import PyOpenSSLContext
        except Exception:
            from requests.packages.urllib3.contrib.pyopenssl import PyOpenSSLContext
        HAS_URLLIB3_PYOPENSSLCONTEXT = True
    except Exception:
        # urllib3<1.15,>=1.6
        try:
            try:
                from urllib3.contrib.pyopenssl import ssl_wrap_socket
            except Exception:
                from requests.packages.urllib3.contrib.pyopenssl import ssl_wrap_socket
            HAS_URLLIB3_SSL_WRAP_SOCKET = True
        except Exception:
            pass

# Select a protocol that includes all secure tls protocols
# Exclude insecure ssl protocols if possible

if HAS_SSL:
    # If we can't find extra tls methods, ssl.PROTOCOL_TLSv1 is sufficient
    PROTOCOL = ssl.PROTOCOL_TLSv1
if not HAS_SSLCONTEXT and HAS_SSL:
    try:
        import ctypes
        import ctypes.util
    except ImportError:
        # python 2.4 (likely rhel5 which doesn't have tls1.1 support in its openssl)
        pass
    else:
        libssl_name = ctypes.util.find_library('ssl')
        libssl = ctypes.CDLL(libssl_name)
        for method in ('TLSv1_1_method', 'TLSv1_2_method'):
            try:
                libssl[method]

... (truncated due to per-file budget) ...

### FILE: test/units/module_utils/urls/test_fetch_url.py
<<<<<<< SEARCH
from __future__ import absolute_import, division, print_function
__metaclass__ = type

import socket
import sys

from ansible.module_utils.six import StringIO
from ansible.module_utils.six.moves.http_cookiejar import Cookie
from ansible.module_utils.six.moves.http_client import HTTPMessage
from ansible.module_utils.six.moves.urllib.error import URLError
from ansible.module_utils.urls import fetch_url, urllib_error, ConnectionError, NoSSLError, httplib

import pytest
from units.compat.mock import MagicMock


class AnsibleModuleExit(Exception):
    def __init__(self, *args, **kwargs):
        self.args = args
        self.kwargs = kwargs


class ExitJson(AnsibleModuleExit):
    pass


class FailJson(AnsibleModuleExit):
    pass


@pytest.fixture
def open_url_mock(mocker):
    return mocker.patch('ansible.module_utils.urls.open_url')


@pytest.fixture
def fake_ansible_module():
    return FakeAnsibleModule()


class FakeAnsibleModule:
    def __init__(self):
        self.params = {}
        self.tmpdir = None

    def exit_json(self, *args, **kwargs):
        raise ExitJson(*args, **kwargs)

    def fail_json(self, *args, **kwargs):
        raise FailJson(*args, **kwargs)


def test_fetch_url_no_urlparse(mocker, fake_ansible_module):
    mocker.patch('ansible.module_utils.urls.HAS_URLPARSE', new=False)

    with pytest.raises(FailJson):
        fetch_url(fake_ansible_module, 'http://ansible.com/')


def test_fetch_url(open_url_mock, fake_ansible_module):
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')

    dummy, kwargs = open_url_mock.call_args

    open_url_mock.assert_called_once_with('http://ansible.com/', client_cert=None, client_key=None, cookies=kwargs['cookies'], data=None,
                                          follow_redirects='urllib2', force=False, force_basic_auth='', headers=None,
                                          http_agent='ansible-httpget', last_mod_time=None, method=None, timeout=10, url_password='', url_username='',
                                          use_proxy=True, validate_certs=True, use_gssapi=False, unix_socket=None, ca_path=None, unredirected_headers=None,
                                          decompress=True, ciphers=None)


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
                                          decompress=True, ciphers=None)


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

    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')

    assert info['cookies_string'] == 'Foo=bar; Baz=qux'

    # The key here has a `-` as opposed to what we see in the `uri` module that converts to `_`
    # Note: this is response order, which differs from cookies_string
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'

    # The key here has a `-` as opposed to what we see in the `uri` module that converts to `_`
    # Note: this is response order, which differs from cookies_string
    assert info['cookies'] == {'Baz': 'qux', 'Foo': 'bar'}

    # The key here has a `-` as opposed to what we see in the `uri` module that converts to `_`
    # Note: this is response order, which differs from cookies_string
    assert info['set-cookie'] == 'Foo=bar, Baz=qux'

    # The key here has a `-` as opposed to what we see in the `uri` module that converts to `_`
    # Note: this is response order, which differs from cookies_string
    assert info['cookies'] == {'Baz': 'qux', 'Foo': 'bar'}


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


def test_fetch_url_connectionerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = ConnectionError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')

    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1

    open_url_mock.side_effect = ValueError('TESTS')
    with pytest.raises(FailJson) as excinfo:
        fetch_url(fake_ansible_module, 'http://ansible.com/')

    assert excinfo.value.kwargs['msg'] == 'TESTS'
    assert 'http://ansible.com/' == excinfo.value.kwargs['url']
    assert excinfo.value.kwargs['status'] == -1


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


def test_fetch_url_urlerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = urllib_error.URLError('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Request failed: <urlopen error TESTS>',
                    'status': -1, 'url': 'http://ansible.com/'}


def test_fetch_url_socketerror(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = socket.error('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    assert info == {'msg': 'Connection failure: TESTS', 'status': -1, 'url': 'http://ansible.com/'}


def test_fetch_url_exception(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = Exception('TESTS')
    r, info = fetch_url(fake_ansible_module, 'http://ansible.com/')
    exception = info.pop('exception')
    assert info == {'msg': 'An unknown error occurred: TESTS', 'status': -1, 'url': 'http://ansible.com/'}
    assert "Exception: TESTS" in exception


def test_fetch_url_badstatusline(open_url_mock, fake_ansible_module):
    open_url_mock.side_effect = httplib.BadStatusLine('TESTS')
    r, inf
... (truncated due to per-file budget) ...

### FILE: lib/ansible/modules/apt_repository.py
<<<<<<< SEARCH
# encoding: utf-8

# Copyright: (c) 2012, Matt Wright <matt@nobien.net>
# Copyright: (c) 2013, Alexander Saltanov <asd@mokote.com>
# Copyright: (c) 2014, Rutger Spiertz <rutger@kumina.nl>

# GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)

from __future__ import absolute_import, division, print_function
__metaclass__ = type


DOCUMENTATION = '''
---
module: apt_repository
short_description: Add and remove APT repositories
description:
    - Add or remove an APT repositories in Ubuntu and Debian.
extends_documentation_fragment: action_common_attributes
attributes:
    check_mode:
        support: full
    diff_mode:
        support: full
    platform:
        platforms: debian
notes:
    - This module supports Debian Squeeze (version 6) as well as its successors and derivatives.
options:
    repo:
        description:
            - A source string for the repository.
        type: str
        required: true
    state:
        description:
            - A source string state.
        type: str
        choices: [ absent, present ]
        default: "present"
    mode:
        description:
            - The octal mode for newly created files in sources.list.d.
            - Default is what system uses (probably 0644).
        type: raw
        version_added: "1.6"
    update_cache:
        description:
            - Run the equivalent of C(apt-get update) when a change occurs.  Cache updates are run after making changes.
        type: bool
        default: "yes"
        aliases: [ update-cache ]
    update_cache_retries:
        description:
        - Amount of retries if the cache update fails. Also see I(update_cache_retry_max_delay).
        type: int
        default: 5
        version_added: '2.10'
    update_cache_retry_max_delay:
        description:
        - Use an exponential backoff delay for each retry (see I(update_cache_retries)) up to this max delay in seconds.
        type: int
        default: 12
        version_added: '2.10'
    validate_certs:
        description:
            - If C(no), SSL certificates for the target repo will not be validated. This should only be used
              on personally controlled sites using self-signed certificates.
        type: bool
        default: 'yes'
        version_added: '1.8'
    filename:
        description:
            - Sets the name of the source list file in sources.list.d.
              Defaults to a file name based on the repository source url.
              The .list extension will be automatically added.
        type: str
        version_added: '2.1'
    codename:
        description:
            - Override the distribution codename to use for PPA repositories.
              Should usually only be set when working with a PPA on
              a non-Ubuntu target (for example, Debian or Mint).
        type: str
        version_added: '2.3'
    install_python_apt:
        description:
            - Whether to automatically try to install the Python apt library or not, if it is not already installed.
              Runs C(apt-get install python-apt) for Python 2, and C(apt-get install python3-apt) for Python 3.
              Only works with the system Python 2 or Python 3. If you are using a Python on the remote that is not
               the system Python, set I(install_python_apt=false) and ensure that the Python apt library
               for your Python version is installed some other way.
        type: bool
        default: true
author:
- Alexander Saltanov (@sashka)
version_added: "0.7"
requirements:
   - python-apt (python 2)
   - python3-apt (python 3)
   - apt-key or gpg
'''

EXAMPLES = '''
- name: Add specified repository into sources list
  ansible.builtin.apt_repository:
    repo: deb http://archive.canonical.com/ubuntu hardy partner
    state: present

- name: Add specified repository into sources list using specified filename
  ansible.builtin.apt_repository:
    repo: deb http://dl.google.com/linux/chrome/deb/ stable main
    state: present
    filename: google-chrome

- name: Add source repository into sources list
  ansible.builtin.apt_repository:
    repo: deb-src http://archive.canonical.com/ubuntu hardy partner
    state: present

- name: Remove specified repository from sources list
  ansible.builtin.apt_repository:
    repo: deb http://archive.canonical.com/ubuntu hardy partner
    state: absent

- name: Add nginx stable repository from PPA and install its signing key on Ubuntu target
  ansible.builtin.apt_repository:
    repo: ppa:nginx/stable

- name: Add nginx stable repository from PPA and install its signing key on Debian target
  ansible.builtin.apt_repository:
    repo: 'ppa:nginx/stable'
    codename: trusty

- name: One way to avoid apt_key once it is removed from your distro
  block:
    - name: somerepo |no apt key
      ansible.builtin.get_url:
        url: https://download.example.com/linux/ubuntu/gpg
        dest: /etc/apt/trusted.gpg.d/somerepo.asc

    - name: somerepo | apt source
      ansible.builtin.apt_repository:
      repo: "deb [arch=amd64 signed-by=/etc/apt/trusted.gpg.d/somerepo.asc] https://download.example.com/linux/ubuntu {{ ansible_distribution_release }} stable"
      state: present
'''

RETURN = '''#'''

import copy
import glob
import json
import os
import re
import sys
import tempfile
import random
import time

from ansible.module_utils.basic import AnsibleModule
from ansible.module_utils.common.respawn import has_respawned, probe_interpreters_for_module, respawn_module
from ansible.module_utils._text import to_native, to_text

... (401 lines omitted) ...


Editable files manifest (you may ONLY edit files listed here):
- lib/ansible/module_utils/urls.py
- test/units/module_utils/urls/test_fetch_url.py
- lib/ansible/modules/apt_repository.py
- lib/ansible/modules/get_url.py
- test/sanity/code-smell/package-data.py
- (you may CREATE these new files if needed)
  - test/integration/targets/get_url/tasks/use_netrc.yml
  - test/integration/targets/lookup_url/tasks/use_netrc.yml
  - test/integration/targets/uri/tasks/use_netrc.yml

If the fix requires a file not listed above, respond with NO_PATCH and nothing else.

PATCH FORMAT — return one or more SEARCH/REPLACE blocks exactly like this:

### FILE: path/to/file.go
<<<<<<< SEARCH
old line(s) copied exactly from the file above
=======
new line(s) to replace them with
>>>>>>> REPLACE

CRITICAL RULES:
- Modify source files only. Do NOT edit tests, configs, docs, or unrelated code.
- Keep the patch minimal: no formatting, comment, or unrelated changes.
- Do NOT produce an empty patch.
- Prefer paths from the Editable files manifest, but you may edit other existing source files if the fix clearly requires it.
- The SEARCH text must match the source file EXACTLY (including indentation). Copy it verbatim from the file content shown above.
- Files marked FULL FILE above are shown completely. Copy SEARCH text from those exact lines.
- Do NOT include line numbers, '  123 | ', or '// --- lines X-Y ---' markers in SEARCH or REPLACE.
- Do NOT wrap the patch in ``` or any markdown fence.
- If the SEARCH text contains special characters, copy them exactly from the source.
- If you cannot match the exact text, choose a smaller SEARCH block that is fully visible rather than guessing.
- To CREATE a new file, use an empty SEARCH block:
  ### FILE: path/to/new_file.go
  <<<<<<< SEARCH
  =======
  <new file contents>
  >>>>>>> REPLACE