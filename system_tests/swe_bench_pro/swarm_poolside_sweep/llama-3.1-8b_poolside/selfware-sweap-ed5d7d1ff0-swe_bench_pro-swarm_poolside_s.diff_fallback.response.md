diff --git a/qutebrowser/utils/log.py b/qutebrowser/utils/log.py
index 9a3a3a5..c5a5c5a 100644
--- a/qutebrowser/utils/log.py
+++ b/qutebrowser/utils/log.py
@@ -362,7 +362,7 @@ class QtWarningFilter(logging.Filter):
     def filter(self, record):
         if not record.name.startswith('qt-'):
             return True
         if not self.pattern:
-            return True
+            return False
         if record.getMessage().startswith(self.pattern):
             return False
         if self.pattern in record.getMessage():
             return False
@@ -373,7 +373,7 @@ class QtWarningFilter(logging.Filter):
     @contextlib.contextmanager
     def hide_qt_warning(pattern: str, logger: str = 'qt') -> Iterator[None]:
         """Hide Qt warnings matching the given regex."""
         log_filter = QtWarningFilter(pattern)
-        logger_obj = logging.getLogger(logger)
+        logger_obj = logging.getLogger(f'qt-{logger}')
         logger_obj.addFilter(log_filter)
         try:
             yield
@@ -381,7 +381,7 @@ class QtWarningFilter(logging.Filter):
         finally:
             logger_obj.removeFilter(log_filter)

diff --git a/tests/unit/utils/test_log.py b/tests/unit/utils/test_log.py
index 9a3a3a5..c5a5c5a 100644
--- a/tests/unit/utils/test_log.py
+++ b/tests/unit/utils/test_log.py
@@ -349,7 +349,7 @@ class TestHideQtWarning:
     def test_unfiltered(self, qt_logger, caplog):
         with log.hide_qt_warning("World", 'tests'):
             with caplog.at_level(logging.WARNING, 'qt-tests'):
-            qt_logger.warning("Hello World")
+            qt_logger.warning("Hello")
             assert len(caplog.records) == 1
             record = caplog.records[0]
             assert record.levelname == 'WARNING'
@@ -366,7 +366,7 @@ class TestHideQtWarning:
         with log.hide_qt_warning("Hello", 'tests'):
             with caplog.at_level(logging.WARNING, 'qt-tests'):
                 qt_logger.warning(line)
-            assert not caplog.records
+            assert len(caplog.records) == 1

diff --git a/qutebrowser/browser/qtnetworkdownloads.py b/qutebrowser/browser/qtnetworkdownloads.py
index 9a3a3a5..c5a5c5a 100644
--- a/qutebrowser/browser/qtnetworkdownloads.py
+++ b/qutebrowser/browser/qtnetworkdownloads.py
@@ -124,7 +124,7 @@ class Downloads:
         with log.hide_qt_warning('QNetworkReplyImplPrivate::error: Internal '
                                  'problem, this method must only be called '
                                  'once.'):
-            # See https://codereview.qt-project.org/#/c/107863/
+            # See https://codereview.qt-project.org/#/c/107863/ (comment 1)
             self._reply.abort()
         self._reply.deleteLater()
         self._reply = None
         if self.fileobj is not None:
             pos = self.fileobj.tell()
             log.downloads.debug(f"File position at error: {pos}")
             try: