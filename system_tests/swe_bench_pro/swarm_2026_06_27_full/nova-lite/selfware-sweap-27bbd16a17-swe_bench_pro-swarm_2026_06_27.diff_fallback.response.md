diff --git a/qutebrowser/config/configfiles.py b/qutebrowser/config/configfiles.py
index 1234567..8765432 100644
--- a/qutebrowser/config/configfiles.py
+++ b/qutebrowser/config/configfiles.py
@@ -59,6 +59,30 @@
     super().__init__()
     self._filename = os.path.join(standarddir.data(),'state')
     self.read(self._filename, encoding='utf-8')
+
+    from enum import Enum
+
+    class VersionChange(Enum):
+        unknown = 0
+        equal = 1
+        downgrade = 2
+        patch = 3
+        minor = 4
+        major = 5
+
+        def matches_filter(self, filterstr: str) -> bool:
+            return self.name == filterstr
+
+    qt_version = qVersion()
+
+    if 'general' in self:
+        old_qt_version = self['general'].get('qt_version', None)
+        old_qutebrowser_version = self['general'].get('version', None)
+
+        self._set_changed_attributes(old_qt_version, qt_version, old_qutebrowser_version)
+
+    else:
+        self.qt_version_changed = False
+        self.qutebrowser_version_changed = False
+
     for sect in ['general', 'geometry', 'inspector']:
         try:
             self.add_section(sect)
@@ -70,7 +94,7 @@
             self.qt_version_changed = (
                 old_qutebrowser_version!= qutebrowser.__version__)
-        else:
+        elif old_qutebrowser_version is None:
             self.qt_version_changed = False
             self.qutebrowser_version_changed = False
@@ -77,6 +101,21 @@
     for sect in ['general', 'geometry', 'inspector']:
         try:
             self.add_section(sect)
+
+    def _set_changed_attributes(self, old_qt_version, qt_version, old_qutebrowser_version):
+        try:
+            if old_qt_version!= qt_version:
+                self.qt_version_changed = self.VersionChange.patch if old_qt_version is None else self.VersionChange.minor
+            else:
+                self.qt_version_changed = self.VersionChange.equal
+
+            if old_qutebrowser_version is None:
+                self.qutebrowser_version_changed = self.VersionChange.major
+            elif old_qutebrowser_version == qutebrowser.__version__:
+                self.qutebrowser_version_changed = self.VersionChange.equal
+            elif old_qutebrowser_version < qutebrowser.__version__:
+                self.qutebrowser_version_changed = self.VersionChange.patch if old_qutebrowser_version.split('.')[1] < qutebrowser.__version__.split('.')[1] else self.VersionChange.minor
+            else:
+                self.qutebrowser_version_changed = self.VersionChange.downgrade
+        except Exception as e:
+            log.init.warning(f"Could not parse version: {e}")
+            self.qutebrowser_version_changed = self.VersionChange.unknown

diff --git a/qutebrowser/app.py b/qutebrowser/app.py
index 1234567..8765432 100644
--- a/qutebrowser/app.py
+++ b/qutebrowser/app.py
@@ -386,7 +386,15 @@
     if not configfiles.state.qutebrowser_version_changed:
         return
     if not config.val.changelog_after_upgrade:
-        log.init.debug("Showing changelog is disabled")
+        if configfiles.state.qutebrowser_version_changed.matches_filter('major') or \
+           configfiles.state.qutebrowser_version_changed.matches_filter('minor'):
+            log.init.debug("Showing changelog is disabled")
+            return
+        else:
+            log.init.info("Changelog is shown due to a patch update, consider configuring changelog_after_upgrade setting.")
+
+    try:
+        changelog = utils.read_file('html/doc/changelog.html')
+    except OSError as e:
         log.init.warning(f"Not showing changelog due to {e}")
         return
@@ -399,6 +407,7 @@
     if f'id="v{version}"' not in changelog:
         log.init.warning("Not showing changelog (anchor not found)")