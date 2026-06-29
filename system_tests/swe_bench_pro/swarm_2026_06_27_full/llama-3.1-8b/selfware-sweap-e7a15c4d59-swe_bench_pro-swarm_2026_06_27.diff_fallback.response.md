diff --git a/qutebrowser/config/configfiles.py b/qutebrowser/config/configfiles.py
index 5b7a5a2..d4f5b1c 100644
--- a/qutebrowser/config/configfiles.py
+++ b/qutebrowser/config/configfiles.py
@@ -59,6 +59,14 @@ class StateConfig:
         self._filename = os.path.join(standarddir.data(), 'state')
         self.read(self._filename, encoding='utf-8')
         qt_version = qVersion()

+        # We handle this here, so we can avoid setting qt_version_changed if
+        # the config is brand new, but can still set it when qt_version wasn't
+        # there before...
+        if 'general' in self:
+            old_qt_version = self['general'].get('qt_version', None)
+            old_qutebrowser_version = self['general'].get('version', None)
+
         if 'general' in self:
             old_qt_version = self['general'].get('qt_version', None)
             old_qutebrowser_version = self['general'].get('version', None)
@@ -66,6 +74,17 @@ class StateConfig:
             self.qt_version_changed = old_qt_version != qt_version
             self.qutebrowser_version_changed = (
                 old_qutebrowser_version != qutebrowser.__version__)
         else:
             self.qt_version_changed = False
             self.qutebrowser_version_changed = False

+        if old_qutebrowser_version is not None:
+            try:
+                old_version = parse_version(old_qutebrowser_version)
+            except ValueError:
+                log.init.warning("Failed to parse old version")
+                old_version = None
+        else:
+            old_version = None
+
         for sect in ['general', 'geometry', 'inspector']:
             try:
                 self.add_section(sect)
@@ -77,6 +96,24 @@ class StateConfig:
         deleted_keys = [
             'qt_version',
         ]

+class VersionChange(Enum):
+    unknown = 1
+    equal = 2
+    downgrade = 3
+    patch = 4
+    minor = 5
+    major = 6
+
+    def matches_filter(self, filterstr: str) -> bool:
+        if filterstr == 'major':
+            return self in [self.major, self.unknown]
+        elif filterstr == 'minor':
+            return self in [self.minor, self.unknown]
+        elif filterstr == 'patch':
+            return self in [self.patch, self.unknown]
+        elif filterstr == 'downgrade':
+            return self == self.downgrade
+        elif filterstr == 'equal':
+            return self == self.equal
+        return False
+
 def _set_changed_attributes(self):
     qt_version = qVersion()
     if 'general' in self:
@@ -83,6 +120,14 @@ def _set_changed_attributes(self):
         self.qt_version_changed = old_qt_version != qt_version
         self.qutebrowser_version_changed = (
             old_qutebrowser_version != qutebrowser.__version__)
     else:
         self.qt_version_changed = False
         self.qutebrowser_version_changed = False

+    if old_qutebrowser_version is not None:
+        try:
+            old_version = parse_version(old_qutebrowser_version)
+        except ValueError:
+            log.init.warning("Failed to parse old version")
+            old_version = None
+    else:
+        old_version = None
+
     for sect in ['general', 'geometry', 'inspector']:
         try:
             self.add_section(sect)
@@ -96,6 +141,14 @@ def _set_changed_attributes(self):
         deleted_keys = [
             'qt_version',
         ]

+    if old_qutebrowser_version is not None:
+        self.qutebrowser_version_changed = VersionChange(
+            self._compare_versions(old_qutebrowser_version, qutebrowser.__version__))
+    else:
+        self.qutebrowser_version_changed = VersionChange.unknown
+
 def _compare_versions(self, old_version, new_version):
     old_version_parts = list(map(int, old_version.split('.')))
     new_version_parts = list(map(int, new_version.split('.')))
@@ -104,6 +157,14 @@ def _compare_versions(self, old_version, new_version):
         return VersionChange.major
     elif old_version_parts[0] == new_version_parts[0] and old_version_parts[1] == new_version_parts[1]:
         return VersionChange.equal
     elif old_version_parts[0] == new_version_parts[0] and old_version_parts[1] < new_version_parts[1]:
+        if old_version_parts[2] == new_version_parts[2]:
+            return VersionChange.patch
+        else:
+            return VersionChange.minor
     elif old_version_parts[0] < new_version_parts[0]:
         return VersionChange.major
     else:
         return VersionChange.downgrade

diff --git a/qutebrowser/app.py b/qutebrowser/app.py
index 5b7a5a2..d4f5b1c 100644
--- a/qutebrowser/app.py
+++ b/qutebrowser/app.py
@@ -377,6 +377,7 @@ def _show_changelog(self):
     if not configfiles.state.qutebrowser_version_changed:
         return
     if not config.val.changelog_after_upgrade:
         log.init.debug("Showing changelog is disabled")
         return

     try:
         changelog = utils.read_file('html/doc/changelog.html')
@@ -386,7 +387,7 @@ def _show_changelog(self):
     if f'id="v{version}"' not in changelog:
         log.init.warning("Not showing changelog (anchor not found)")

-    if not configfiles.state.qutebrowser_version_changed:
+    if not configfiles.state.qutebrowser_version_changed.matches_filter('major'):
         return

     if not config.val.changelog_after_upgrade:
@@ -401,6 +402,7 @@ def _show_changelog(self):
     version = qutebrowser.__version__
     if f'id="v{version}"' not in changelog:
         log.init.warning("Not showing changelog (anchor not found)")