diff --git a/qutebrowser/utils/log.py b/qutebrowser/utils/log.py
index 8319664..698818f 100644
--- a/qutebrowser/utils/log.py
+++ b/qutebrowser/utils/log.py
@@ -363,7 +363,7 @@
     """Hide Qt warnings matching the given regex."""
     log_filter = QtWarningFilter(pattern)
     logger_obj = logging.getLogger(logger)
-    logger_obj.addFilter(log_filter)
+    logger_obj.addFilter(log_filter)
     try:
         yield
     finally:
diff --git a/qutebrowser/browser/qtnetworkdownloads.py b/qutebrowser/browser/qtnetworkdownloads.py
index 8599712..7599997 100644
--- a/qutebrowser/browser/qtnetworkdownloads.py
+++ b/qutebrowser/browser/qtnetworkdownloads.py
@@ -124,7 +124,7 @@
                                   'problem, this method must only be called '
                                   'once.'):
             # See https://codereview.qt-project.org/#/c/107863/
-            self._reply.abort()
+            self._reply.abort()
         self._reply.deleteLater()
         self._reply = None
         if self.fileobj is not None:
diff --git a/tests/unit/utils/test_log.py b/tests/unit/utils/test_log.py
index 8399847..8999997 100644
--- a/tests/unit/utils/test_log.py
+++ b/tests/unit/utils/test_log.py
@@ -365,7 +365,7 @@
         with log.hide_qt_warning("Hello", 'qt-tests'):
             with caplog.at_level(logging.WARNING, 'qt-tests'):
                 qt_logger.warning(line)
-        assert not caplog.records
+        assert not caplog.records
 
 
 @pytest.mark.parametrize('suffix, expected', [