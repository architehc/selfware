```
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
        info.update(dict((k.lower(), v) for k, v in r.info().items()))
=======
        info.update(dict((k.lower(), v) for k, v in r.info().items()))
        # Lowercase keys, to conform to py2 behavior, so that py3 and py2 are predictable

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
```

```
### FILE: lib/ansible/modules/get_url.py
<<<<<<< SEARCH
        use_netrc=True)
=======
        use_netrc=use_netrc)
>>>>>>> REPLACE
```

```
### FILE: lib/ansible/modules/apt_repository.py
<<<<<<< SEARCH
        use_netrc=True,
=======
        use_netrc=use_netrc,
>>>>>>> REPLACE
```

```
### FILE: lib/ansible/modules/uri.py
<<<<<<< SEARCH
        use_netrc=True,
=======
        use_netrc=use_netrc,
>>>>>>> REPLACE
```

```
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< SEARCH
def open_url(url, **kwargs):
    """
    Open a URL and return a response object.
    """
    use_netrc = kwargs.pop('use_netrc', True)
    try:
        # Python 3
        r = urllib_request.urlopen(url, **kwargs)
    except Exception as e:
        if isinstance(e, urllib_error.HTTPError):
            # Handle HTTP errors
            r = e
        else:
            raise ConnectionError(to_native(e))

    if use_netrc:
        try:
            netrc_data = netrc.netrc()
            auth = netrc_data.hosts.get(urlparse(url).hostname)
            if auth:
                username, password = auth
                r = urllib_request.urlopen(url,
                                           username=username,
                                           password=password,
                                           **kwargs)
        except Exception as e:
            # Ignore errors during .netrc lookup
            pass

    return r, r.info()
=======
def open_url(url, **kwargs):
    """
    Open a URL and return a response object.
    """
    use_netrc = kwargs.pop('use_netrc', True)
    try:
        # Python 3
        r = urllib_request.urlopen(url, **kwargs)
    except Exception as e:
        if isinstance(e, urllib_error.HTTPError):
            # Handle HTTP errors
            r = e
        else:
            raise ConnectionError(to_native(e))

    if use_netrc:
        try:
            netrc_data = netrc.netrc()
            auth = netrc_data.hosts.get(urlparse(url).hostname)
            if auth:
                username, password = auth
                r = urllib_request.urlopen(url,
                                           username=username,
                                           password=password,
                                           **kwargs)
        except Exception as e:
            # Ignore errors during .netrc lookup
            pass

    return r, r.info()
>>>>>>> REPLACE
```