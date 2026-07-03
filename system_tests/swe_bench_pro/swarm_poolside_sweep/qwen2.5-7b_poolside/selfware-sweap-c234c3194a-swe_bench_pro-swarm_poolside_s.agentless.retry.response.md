### FILE: scripts/dictcli.py
<<<<<<< SEARCH
    if not os.path.isdir(spell.dictionary_dir()):
        msg = '{} does not exist, creating the directory'
        print(msg.format(spell.dictionary_dir()))
        os.makedirs(spell.dictionary_dir())
    print('Downloading {}'.format(lang_url))
    dest = os.path.join(spell.dictionary_dir(), lang.remote_filename)
    download_dictionary(lang_url, dest)
    print('Installed to {}.'.format(dest))
=======
    if not os.path.isdir(spell.dictionary_dir()):
        msg = '{} does not exist, creating the directory'.format(spell.dictionary_dir())
        print(msg)
        os.makedirs(spell.dictionary_dir())
    print('Downloading {}'.format(lang_url))
    dest = os.path.join(spell.dictionary_dir(), lang.remote_filename)
    download_dictionary(lang_url, dest)
    print('Installed to {}'.format(dest))
>>>>>>> REPLACE

### FILE: qutebrowser/commands/command.py
<<<<<<< SEARCH
            except AttributeError:
                # typing.get_args was added in Python 3.8
                types = list(typ.__args__)
=======
            except AttributeError:
                # typing.get_args was added in Python 3.8
                types = list(typ.__args__)
                if not isinstance(types, list):
                    types = [types]
>>>>>>> REPLACE