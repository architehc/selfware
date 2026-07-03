diff --git a/qutebrowser/config/configfiles.py b/qutebrowser/config/configfiles.py
index 3a4a5b4..e4f6c7d 100644
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
@@ -66,6 +74,23 @@ class StateConfig:
             self.qt_version_changed = old_qt_version!= qt_version
             self.qutebrowser_version_changed = (
                 old_qutebrowser_version!= qutebrowser.__version__)
         else:
             self.qt_version_changed = False
             self.qutebrowser_version_changed = False

+        from qutebrowser.utils import version
+
+        self.qutebrowser_version_changed = self._set_changed_attributes(
+            old_qutebrowser_version, qutebrowser.__version__)
+
+    def _set_changed_attributes(self, old_version, new_version):
+        from qutebrowser.utils import version
+
+        try:
+            old_version = version.parse(old_version)
+            new_version = version.parse(new_version)
+        except version.InvalidVersion:
+            log.init.warning("Cannot parse old version, assuming unknown")
+            self.qutebrowser_version_changed = VersionChange.unknown
+            return
+
+        if old_version == new_version:
+            self.qutebrowser_version_changed = VersionChange.equal
+        elif new_version < old_version:
+            self.qutebrowser_version_changed = VersionChange.downgrade
+        elif new_version.micro!= old_version.micro:
+            self.qutebrowser_version_changed = VersionChange.patch
+        elif new_version.minor!= old_version.minor:
+            self.qutebrowser_version_changed = VersionChange.minor
+        else:
+            self.qutebrowser_version_changed = VersionChange.major

     for sect in ['general', 'geometry', 'inspector']:
         try:
@@ -77,6 +102,7 @@ class StateConfig:
         deleted_keys = [
             'qt_version',
             'version',
+            'changelog_after_upgrade',
         ]

         for key in deleted_keys:
             if key in self:
                 del self[key]

diff --git a/qutebrowser/app.py b/qutebrowser/app.py
index 3a4a5b4..e4f6c7d 100644
--- a/qutebrowser/app.py
+++ b/qutebrowser/app.py
@@ -377,7 +377,7 @@ def show_changelog():
         general_sect['session-warning-shown'] = '1'

     for state, condition, url in pages:
-        if general_sect.get(state)!= '1' and condition:
+        if general_sect.get(state)!= '1' and condition and configfiles.state.qutebrowser_version_changed:
             tabbed_browser.tabopen(QUrl(url), background=False)
             general_sect[state] = '1'

@@ -386,7 +386,7 @@ def show_changelog():
     if not configfiles.state.qutebrowser_version_changed:
         return
     if not config.val.changelog_after_upgrade:
         log.init.debug("Showing changelog is disabled")
-        return
+        return
     try:
         changelog = utils.read_file('html/doc/changelog.html')
     except OSError as e:
@@ -400,7 +400,7 @@ def show_changelog():
         log.init.warning("Not showing changelog (anchor not found)")