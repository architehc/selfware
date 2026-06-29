diff --git a/qutebrowser/config/configfiles.py b/qutebrowser/config/configfiles.py
index 9f2c1d2e1b..5d3a7f4c8e 100644
--- a/qutebrowser/config/configfiles.py
+++ b/qutebrowser/config/configfiles.py
@@ -60,6 +60,12 @@ class StateConfig(Config):
         self.read(self._filename, encoding='utf-8')
         qt_version = qVersion()
 
+        # Define the VersionChange enumeration
+        class VersionChange(Enum):
+            unknown = 0
+            equal = 1
+            downgrade = 2
+            patch = 3
+            minor = 4
+            major = 5
+
         # We handle this here, so we can avoid setting qt_version_changed if
         # the config is brand new, but can still set it when qt_version wasn't
         # there before...
         if 'general' in self:
@@ -68,7 +74,16 @@ class StateConfig(Config):
             old_qutebrowser_version = self['general'].get('version', None)
             self.qt_version_changed = old_qt_version != qt_version
             self.qutebrowser_version_changed = (old_qutebrowser_version != qutebrowser.__version__)
+            # Determine the version change type
+            if old_qutebrowser_version is None:
+                self.qutebrowser_version_changed = VersionChange.unknown
+                logging.warning("Unable to parse old qutebrowser version; treating as unknown change.")
+                return
+            try:
+                old_major, old_minor, old_patch = map(int, old_qutebrowser_version.split('.'))
+                new_major, new_minor, new_patch = map(int, qutebrowser.__version__.split('.'))
+            except ValueError:
+                self.qutebrowser_version_changed = VersionChange.unknown
+                logging.warning("Unable to parse version strings; treating as unknown change.")
+                return
+            if old_major == new_major and old_minor == new_minor and old_patch == new_patch:
+                self.qutebrowser_version_changed = VersionChange.equal
+            elif old_major > new_major or (old_major == new_major and old_minor > new_minor) or (old_major == new_major and old_minor == new_minor and old_patch > new_patch):
+                self.qutebrowser_version_changed = VersionChange.downgrade
+            elif old_major == new_major:
+                if old_minor == new_minor:
+                    self.qutebrowser_version_changed = VersionChange.patch
+                else:
+                    self.qutebrowser_version_changed = VersionChange.minor
+            else:
+                self.qutebrowser_version_changed = VersionChange.major
+
         else:
             self.qt_version_changed = False
             self.qutebrowser_version_changed = False
 
@@ -83,6 +98,12 @@ class StateConfig(Config):
         # ... (rest of the class unchanged)
 
     # Add the matches_filter method to the VersionChange enum
+    def matches_filter(self, filterstr: str) -> bool:
+        """
+        Return whether the version change matches a given changelog_after_upgrade filter value.
+        """
+        if filterstr == 'major':
+            return self in (VersionChange.major, VersionChange.minor)
+        elif filterstr == 'minor':
+            return self in (VersionChange.minor, VersionChange.patch)
+        elif filterstr == 'patch':
+            return self == VersionChange.patch
+        elif filterstr == 'all':
+            return True
+        else:
+            return False
 
 # ... (rest of the file unchanged)
diff --git a/qutebrowser/app.py b/qutebrowser/app.py
index 3a1b2c3d4e5..6f7g8h9i0j 100644
--- a/qutebrowser/app.py
+++ b/qutebrowser/app.py
@@ -385,7 +385,7 @@ def main():
         # Show changelog on new releases
         if not configfiles.state.qutebrowser_version_changed:
             return
-        if not config.val.changelog_after_upgrade:
+        if not config.val.changelog_after_upgrade or \
+           not configfiles.state.qutebrowser_version_changed.matches_filter(config.val.changelog_after_upgrade):
             log.init.debug("Showing changelog is disabled")
             return
 
     try:
         changelog = utils.read_file('html/doc/changelog.html')
     except OSError as e:
         log.init.warning(f"Not showing changelog due to {e}")
         return
 
@@ -393,6 +393,7 @@ def main():
         if f'id="v{version}"' not in changelog:
             log.init.warning("Not showing changelog (anchor not found)")
 
+# ... (rest of the file unchanged)
```