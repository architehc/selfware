### FILE: scripts/dictcli.py
<<<<<<< SEARCH
def install_lang(lang):
    """Install a single lang given by the argument."""
    lang_url = API_URL + lang.remote_filename + '?format=TEXT'
    if not os.path.isdir(spell.dictionary_dir()):
        msg = '{} does not exist, creating the directory'
        print(msg.format(spell.dictionary_dir()))
        os.makedirs(spell.dictionary_dir())
    print('Downloading {}'.format(lang_url))
    dest = os.path.join(spell.dictionary_dir(), lang.remote_filename)
    download_dictionary(lang_url, dest)
    print('Installed to {}.'.format(dest))
>>>>>>> REPLACE

### FILE: qutebrowser/api/config.py
<<<<<<< SEARCH
#: Simplified access to config values using attribute access.
#: For example, to access the ``content.javascript.enabled`` setting,
#: you can do::
#:
#:   if config.val.content.javascript.enabled:
#:      ...
#:
#: This also supports setting configuration values::
#:
#:   config.val.content.javascript.enabled = False
val = cast('config.ConfigContainer', None)
>>>>>>> REPLACE

### FILE: scripts/hostblock_blame.py
<<<<<<< SEARCH
def main():
    """Check by which hostblock list a host was blocked."""
    if len(sys.argv)!= 2:
        print("Usage: {} <host>".format(sys.argv[0]), file=sys.stderr)
        sys.exit(1)
>>>>>>> REPLACE

### FILE: qutebrowser/api/cmdutils.py
<<<<<<< SEARCH
class CommandError(cmdexc.Error):
    """Raised when a command encounters an error while running.

    If your command handler encounters an error and cannot continue, raise this
    exception with an appropriate error message::

        raise cmdexc.CommandError("Message")

    The message will then be shown in the qutebrowser status bar.
    """
>>>>>>> REPLACE

### FILE: scripts/mkvenv.py
<<<<<<< SEARCH
def install_qutebrowser(venv_dir: pathlib.Path) -> None:
    """Install qutebrowser itself as an editable install."""
    utils.print_title("Installing qutebrowser")
    pip_install(venv_dir, '-e', str(REPO_ROOT))
>>>>>>> REPLACE