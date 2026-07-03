diff --git a/qutebrowser/config/configfiles.py b/qutebrowser/config/configfiles.py
index 7349989..8879989 100644
--- a/qutebrowser/config/configfiles.py
+++ b/qutebrowser/config/configfiles.py
@@ -59,6 +59,22 @@
         self._filename = os.path.join(standarddir.data(), 'state')
         self.read(self._filename, encoding='utf-8')
         qt_version = qVersion()
+
+        class VersionChange:
+            unknown = "unknown"
+            equal = "equal"
+            downgrade = "downgrade"
+            patch = "patch"
+            minor = "minor"
+            major = "major"
+
+            def matches_filter(self, filterstr: str) -> bool:
+                return filterstr.lower() in self.name.lower()
+
+        StateConfig.VersionChange = VersionChange
+
+
         # We handle this here, so we can avoid setting qt_version_changed if
         # the config is brand new, but can still set it when qt_version wasn't
         # there before...
@@ -67,6 +83,7 @@
             old_qt_version = self['general'].get('qt_version', None)
             old_qutebrowser_version = self['general'].get('version', None)
             self.qt_version_changed = old_qt_version != qt_version
+
             self.qutebrowser_version_changed = (
                 old_qutebrowser_version != qutebrowser.__version__)
         else:
@@ -77,6 +104,37 @@
             try:
                 self.add_section(sect)
             except configparser.DuplicateSectionError:
+
+    def _set_changed_attributes(self):
+        """Set qt_version_changed and qutebrowser_version_changed attributes."""
+        qt_version = qVersion()
+        old_qt_version = self['general'].get('qt_version', None)
+        old_qutebrowser_version = self['general'].get('version', None)
+
+        self.qt_version_changed = old_qt_version != qt_version
+
+        current_version = qutebrowser.__version__
+        if old_qutebrowser_version is None:
+            self.qutebrowser_version_changed = VersionChange.unknown
+        else:
+            try:
+                old_version = tuple(map(int, old_qutebrowser_version.split('.')))
+                current_version_tuple = tuple(map(int, current_version.split('.')))
+
+                if old_version == current_version_tuple:
+                    self.qutebrowser_version_changed = VersionChange.equal
+                elif old_version < current_version_tuple:
+                    self.qutebrowser_version_changed = VersionChange.downgrade
+                elif current_version_tuple[0] != old_version[0]:
+                    self.qutebrowser_version_changed = VersionChange.major
+                elif current_version_tuple[1] != old_version[1]:
+                    self.qutebrowser_version_changed = VersionChange.minor
+                else:
+                    self.qutebrowser_version_changed = VersionChange.patch
+            except ValueError:
+                log.warning(f"Could not parse version: {old_qutebrowser_version}")
+                self.qutebrowser_version_changed = VersionChange.unknown
+
+
                 pass
 
 
diff --git a/qutebrowser/app.py b/qutebrowser/app.py
index 899333a..899333a 100644
--- a/qutebrowser/app.py
+++ b/qutebrowser/app.py
@@ -387,8 +387,8 @@
     # Show changelog on new releases
     if not configfiles.state.qutebrowser_version_changed:
         return
-    if not config.val.changelog_after_upgrade:
-        log.init.debug("Showing changelog is disabled")
+    if config.val.changelog_after_upgrade and configfiles.state.qutebrowser_version_changed.matches_filter(config.val.changelog_after_upgrade):
+        log.init.debug("Showing changelog")
         return
 
     try:
diff --git a/doc/help/settings.asciidoc b/doc/help/settings.asciidoc
index 9903941..9903941 100644
--- a/doc/help/settings.asciidoc
+++ b/doc/help/settings.asciidoc
@@ -20,7 +20,7 @@
   |<<colors.completion.item.selected.fg,colors.completion.item.selected.fg>>|Foreground color of the selected completion item.
   |<<changelog_after_upgrade,changelog_after_upgrade>>|Whether to show a changelog after qutebrowser was upgraded.
   |<<colors.completion.category.bg,colors.completion.category.bg>>|Background color of the completion widget category headers.
-  |<<colors.completion.category.border.bottom,colors.completion.category.border.bottom>>|Bottom border color of the completion widget category headers.
+  |<<colors.completion.category.border.bottom,colors.completion.category.border.bottom>>|Bottom border color of the completion widget category headers.
   |<<colors.completion.category.border.top,colors.completion.category.border.top>>|Top border color of the completion widget category headers.
   |<<colors.completion.category.fg,colors.completion.category.fg>>|Foreground color of completion widget category headers.
   |<<colors.completion.even.bg,colors.completion.even.bg>>|Background color of the completion widget for even rows.
diff --git a/doc/changelog.asciidoc b/doc/changelog.asciidoc
index 9903941..9903941 100644
--- a/doc/changelog.asciidoc
+++ b/doc/changelog.asciidoc
@@ -120,7 +120,7 @@
   
   Changed
   ~~~~~~
-
+
   - `config.py` files now are required to have either
