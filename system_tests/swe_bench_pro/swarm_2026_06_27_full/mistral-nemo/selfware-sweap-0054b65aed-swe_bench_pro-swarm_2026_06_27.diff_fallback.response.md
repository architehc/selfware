diff --git a/qutebrowser/config/configfiles.py b/qutebrowser/config/configfiles.py
index 123456..789abc 100644
--- a/qutebrowser/config/configfiles.py
+++ b/qutebrowser/config/configfiles.py
@@ -59,6 +59,12 @@ class StateConfig(configparser.ConfigParser):
         super().__init__()
         self._filename = os.path.join(standarddir.data(), 'state')
         self.read(self._filename, encoding='utf-8')
-        qt_version = qVersion()
+        qt_version = qVersion()
+        qutebrowser_version = qutebrowser.__version__
+        self.qt_version_changed = old_qt_version != qt_version
+        self.qutebrowser_version_changed = old_qutebrowser_version != qutebrowser_version
+        self._version_change = self._determine_version_change(old_qutebrowser_version, qutebrowser_version)
+        self._changelog_filter = self._determine_changelog_filter()
         self._qt_version = qt_version
         self._qutebrowser_version = qutebrowser_version

@@ -71,6 +77,10 @@ class StateConfig(configparser.ConfigParser):
         self.qutebrowser_version_changed = (
                 old_qutebrowser_version != qutebrowser.__version__)
         self._version_change = self._determine_version_change(old_qutebrowser_version, qutebrowser.__version__)
+        self._changelog_filter = self._determine_changelog_filter()
         self._qutebrowser_version = qutebrowser.__version__

+    class VersionChange(enum.Enum):
+        unknown = 0
+        equal = 1
+        downgrade = 2
+        patch = 3
+        minor = 4
+        major = 5
+
     def _determine_version_change(self, old_version, new_version):
         if old_version is None:
             return VersionChange.unknown
@@ -83,6 +95,10 @@ class StateConfig(configparser.ConfigParser):
             self._version_change = VersionChange.equal
         elif old_version.split('.')[0] < new_version.split('.')[0]:
             self._version_change = VersionChange.major
+        elif old_version.split('.')[1] < new_version.split('.')[1]:
+            self._version_change = VersionChange.minor
         elif old_version.split('.')[2] < new_version.split('.')[2]:
             self._version_change = VersionChange.patch
         else:
             self._version_change = VersionChange.downgrade

+    def _determine_changelog_filter(self):
+        return self._version_change.matches_filter(self.config.val.changelog_after_upgrade)
+
     def _set_changed_attributes(self):
         self.qt_version_changed = self._qt_version != qt_version
         self.qutebrowser_version_changed = self._qutebrowser_version != qutebrowser.__version__
@@ -401,6 +417,10 @@ class StateConfig(configparser.ConfigParser):
         self._version_change = self._determine_version_change(old_qutebrowser_version, qutebrowser.__version__)
         self._changelog_filter = self._determine_changelog_filter()

+    def matches_filter(self, filterstr):
+        return self.value in filterstr.split(',')
+
     def _set_changed_attributes(self):
         self.qt_version_changed = self._qt_version != qt_version
         self.qutebrowser_version_changed = self._qutebrowser_version != qutebrowser.__version__
@@ -837,6 +857,10 @@ class StateConfig(configparser.ConfigParser):
         self._version_change = self._determine_version_change(old_qutebrowser_version, qutebrowser.__version__)
         self._changelog_filter = self._determine_changelog_filter()

+    def matches_filter(self, filterstr):
+        return self.value in filterstr.split(',')
+
     def _set_changed_attributes(self):
         self.qt_version_changed = self._qt_version != qt_version
         self.qutebrowser_version_changed = self._qutebrowser_version != qutebrowser.__version__