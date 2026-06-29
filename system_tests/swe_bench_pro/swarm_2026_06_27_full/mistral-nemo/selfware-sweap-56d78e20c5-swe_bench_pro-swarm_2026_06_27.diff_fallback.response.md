diff --git a/qutebrowser/utils/log.py b/qutebrowser/utils/log.py
index 123456..789abc 100644
--- a/qutebrowser/utils/log.py
+++ b/qutebrowser/utils/log.py
@@ -362,3 +362,3 @@ def hide_qt_warning(pattern: str, logger: str = 'qt') -> Iterator[None]:
     log_filter = QtWarningFilter(pattern)
     logger_obj = logging.getLogger(logger)
     logger_obj.addFilter(log_filter)
-    yield
+    try:
+        yield
+    finally:
         logger_obj.removeFilter(log_filter)