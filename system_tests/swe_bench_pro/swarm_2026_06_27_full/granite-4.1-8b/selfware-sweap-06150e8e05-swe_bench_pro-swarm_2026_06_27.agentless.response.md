### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
if err := f.setupForwardingHeaders(sess, req); err != nil {
    return nil, trace.Wrap(err)
}
=======
if err := f.setupForwardingHeaders(sess, req); err != nil {
    return nil, trace.Wrap(err)
}
// Initialize the session uploader to ensure the required async upload directory exists.
if err := f.initializeSessionUploader(); err != nil {
    return nil, trace.Wrap(err)
}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
// portForward starts port forwarding to the remote cluster
func (f *Forwarder) portForward(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
=======
// portForward starts port forwarding to the remote cluster
func (f *Forwarder) portForward(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
// Initialize the session uploader to ensure the required async upload directory exists.
if err := f.initializeSessionUploader(); err != nil {
    return nil, trace.Wrap(err)
}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
// getExecutor returns an executor for the given session and request.
func (f *Forwarder) getExecutor(ctx context.Context, sess *clusterSession, req *http.Request) (*remoteCommandProxy, error) {
=======
// getExecutor returns an executor for the given session and request.
func (f *Forwarder) getExecutor(ctx context.Context, sess *clusterSession, req *http.Request) (*remoteCommandProxy, error) {
// Initialize the session uploader to ensure the required async upload directory exists.
if err := f.initializeSessionUploader(); err != nil {
    return nil, trace.Wrap(err)
}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
// initializeSessionUploader initializes the session uploader to ensure the required async upload directory exists.
func (f *Forwarder) initializeSessionUploader() error {
    log := f.log.WithField("func", "initializeSessionUploader")
    log.Debug("Initializing session uploader.")
    // Create the async upload directory if it does not exist.
    uploadDir := filepath.Join(f.DataDir, "log", "upload", "streaming", "default")
    if err := os.MkdirAll(uploadDir, 0755); err != nil {
        return trace.Wrap(err, "failed to create upload directory: %v", err)
    }
    log.Debug("Session uploader initialized.")
    return nil
}
=======
// initializeSessionUploader initializes the session uploader to ensure the required async upload directory exists.
func (f *Forwarder) initializeSessionUploader() error {
    log := f.log.WithField("func", "initializeSessionUploader")
    log.Debug("Initializing session uploader.")
    // Create the async upload directory if it does not exist.
    uploadDir := filepath.Join(f.DataDir, "log", "upload", "streaming", "default")
    if err := os.MkdirAll(uploadDir, 0755); err != nil {
        return trace.Wrap(err, "failed to create upload directory: %v", err)
    }
    log.Debug("Session uploader initialized.")
    return nil
}
>>>>>>> REPLACE