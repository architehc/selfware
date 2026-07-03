### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
    except urllib_error.HTTPError as e:
        # If the error is an HTTPError, we can try to get the response body
        # and add it to the output
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

        info.update({'msg': to_native(e), 'body': body, 'status': e.code})
>>>>>>> 
### FILE: lib/ansible/module_utils/urls.py
<<<<<<< 
    except urllib_error.URLError as e:
        # If the error is a URLError, we can't do much about it, so just pass it on
        module.fail_json(msg=to_native(e), **info)
    except (socket.error, ValueError) as e:
        # If the error is a socket error or a ValueError, we can't do much about it, so just pass it on
        module.fail_json(msg