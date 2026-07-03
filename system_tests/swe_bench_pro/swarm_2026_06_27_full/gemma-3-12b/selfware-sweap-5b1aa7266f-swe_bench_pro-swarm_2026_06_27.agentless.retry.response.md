### FILE: scripts/dictcli.py
<<<<<<< SEARCH
        return spell.version(self.remote_filename)
    @property
    def local_version(self):
        """Resolve the version of the local dictionary."""
        local_filename = self.local_filename
=======
        return spell.version(self.remote_filename)

    @property
    def local_version(self):
        """Resolve the version of the local dictionary."""
        local_filename = self.local_filename
        if local_filename is None:
            return None
>>>>>>> REPLACE
### FILE: scripts/hostblock_blame.py
<<<<<<< SEARCH
    except Error as e:
        proc_e = e.__cause__
        assert isinstance(proc_e, subprocess.CalledProcessError), proc_e
        print(proc_e.stderr)
        raise Error(
            f"Smoke test failed with status {proc_e.returncode}. "
            "You might find additional information in the debug output above.")
>>>>>>> REPLACE
### FILE: scripts/mkvenv.py
<<<<<<< SEARCH
        if args.oldconfig:
            output_format = 'search'
        else:
            output_format = 'search'
    if not bookmark_types:
        bookmark_types = ['bookmark', 'keyword']
    if not output_format:
        output_format = 'quickmark'

    import_function = {
=======
        if args.oldconfig:
            output_format = 'search'
        else:
            output_format = 'search'
    if not bookmark_types:
        bookmark_types = ['bookmark', 'keyword']
    if not output_format:
        output_format = 'quickmark'

    import_function = {
        'html': import_html_bookmarks,
        'mozilla': import_moz_places,
        'chrome': import_chrome,
    }
>>>>>>> REPLACE
### FILE: scripts/importer.py
<<<<<<< SEARCH
        'search': (
            "SELECT moz_keywords.keyword, "
            "    moz_bookmarks.title, "
            "    search_conv(moz_places.url) AS url "
            "FROM moz_keywords,moz_places,moz_bookmarks "
            "WHERE moz_places.id=moz_bookmarks.fk "
            "AND moz_places.id=moz_keywords.place_id "
            "AND moz_places.url LIKE '%!%s%' ESCAPE '!';"
        )  # bookmarks with keyword and %s substitution
    }
    out_template = {
=======
        'search': (
            "SELECT moz_keywords.keyword, "
            "    moz_bookmarks.title, "
            "    search_conv(moz_places.url) AS url "
            "FROM moz_keywords,moz_places,moz_bookmarks "
            "WHERE moz_places.id=moz_bookmarks.fk "
            "AND moz_places.id=moz_keywords.place_id "
            "AND moz_places.url LIKE '%!%s%' ESCAPE '!';"
        )  # bookmarks with keyword and %s substitution
    }
    out_template = {
        'bookmark': {
            'bookmark
>>>>>>> REPLACE