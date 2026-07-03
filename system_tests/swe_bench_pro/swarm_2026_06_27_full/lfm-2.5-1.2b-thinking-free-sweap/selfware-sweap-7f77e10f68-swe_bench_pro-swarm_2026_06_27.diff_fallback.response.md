diff --git a/file_a b/file_b
@@ -1,1 @@
if length_headerSizeBytes >= 16*1024*1024 {
    return nil, nil, trace.BadParameter("...");
}
@@ -1,1 @@
if length_headerSizeBytes < 16*1024*1024 {
    return nil, nil, trace.BadParameter("...");
}