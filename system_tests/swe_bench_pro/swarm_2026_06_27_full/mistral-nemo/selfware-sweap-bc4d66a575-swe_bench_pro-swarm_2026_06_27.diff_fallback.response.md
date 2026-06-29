diff --git a/lib/kube/proxy/forwarder.go b/lib/kube/proxy/forwarder.go
index 1234567890..abcdefghij 100644
--- a/lib/kube/proxy/forwarder.go
+++ b/lib/kube/proxy/forwarder.go
@@ -137,6 +137,8 @@ func (f *Forwarder) exec(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
     if err := f.setupForwardingHeaders(sess, req); err != nil {
         f.log.Errorf("Failed to setup forwarding headers: %v.", err)
         return nil, trace.Wrap(err)
@@ -885,6 +887,10 @@ func (f *Forwarder) exec(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
         if err := emitter.EmitAuditEvent(request.context, execEvent); err != nil {
             f.log.WithError(err).Warn("Failed to emit event.")
         }
+        if err := f.initSessionUploader(); err != nil {
+            f.log.WithError(err).Warn("Failed to initialize session uploader.")
+        }
         f.log.Debugf("Exited successfully.")
         return nil, nil
     }
@@ -909,6 +915,10 @@ func (f *Forwarder) portForward(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
         if err := f.setupForwardingHeaders(sess, req); err != nil {
             f.log.Errorf("Failed to setup forwarding headers: %v.", err)
             return nil, trace.Wrap(err)
+        if err := f.initSessionUploader(); err != nil {
+            f.log.WithError(err).Warn("Failed to initialize session uploader.")
+        }
         f.log.Debugf("Port forward: %v. req headers: %v.", req.URL.String(), req.Header)
         return sess, nil
     }
@@ -1401,0 +1407,11 @@ func (f *Forwarder) setupForwardingHeaders(sess *clusterSession, req *http.Request) error {
+func (f *Forwarder) initSessionUploader() error {
+    uploadDir := filepath.Join(f.DataDir, "log", "upload", "streaming", "default")
+    if err := os.MkdirAll(uploadDir, 0755); err != nil {
+        return trace.Wrap(err)
+    }
+    return nil
+}
+
+// Ensure the session uploader is initialized at startup.
+func init() {
+    forwarderConfig := &ForwarderConfig{
+        DataDir: "/var/lib/teleport",
+    }
+    forwarder := &Forwarder{Config: forwarderConfig}
+    _ = forwarder.initSessionUploader()
+}