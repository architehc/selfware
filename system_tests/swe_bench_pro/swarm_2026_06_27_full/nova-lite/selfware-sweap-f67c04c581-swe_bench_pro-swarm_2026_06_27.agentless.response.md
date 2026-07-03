### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
if err := f.setupForwardingHeaders(sess, req); err!= nil {
	return nil, trace.Wrap(err)
}
=======
if err := f.setupForwardingHeaders(sess, req); err!= nil {
	return nil, trace.Wrap(err)
}
if err := os.MkdirAll("/var/lib/teleport/log/upload/streaming/default", 0755); err!= nil {
	return nil, trace.Wrap(err)
}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
func (f *Forwarder) portForward(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
	f.log.Debugf("Port forward: %v. req headers: %v.", req.URL.String(), req.Header)
	sess, err := f.getOrCreateClusterSession(*ctx)
	if err!= nil {
		return nil, trace.Wrap(err)
	}
=======
func (f *Forwarder) portForward(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
	f.log.Debugf("Port forward: %v. req headers: %v.", req.URL.String(), req.Header)
	if err := os.MkdirAll("/var/lib/teleport/log/upload/streaming/default", 0755); err!= nil {
		return nil, trace.Wrap(err)
	}
	sess, err := f.getOrCreateClusterSession(*ctx)
	if err!= nil {
		return nil, trace.Wrap(err)
	}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
func (f *Forwarder) setupForwardingHeaders(sess *clusterSession, req *http.Request) error {
	if err := setupImpersonationHeaders(f.log, sess.authContext, req.Header); err!= nil {
		return trace.Wrap(err)
	}
=======
func (f *Forwarder) setupForwardingHeaders(sess *clusterSession, req *http.Request) error {
	if err := setupImpersonationHeaders(f.log, sess.authContext, req.Header); err!= nil {
		return trace.Wrap(err)
	}
	if err := os.MkdirAll("/var/lib/teleport/log/upload/streaming/default", 0755); err!= nil {
		return trace.Wrap(err)
	}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/remotecommand.go
<<<<<<< SEARCH
func createRemoteCommandProxy(req remoteCommandRequest) (*remoteCommandProxy, error) {
	protocol, err := httpstream.Handshake(req.httpRequest, req.httpResponseWriter, []string{StreamProtocolV4Name})
	if err!= nil {
		return nil, trace.Wrap(err)
	}
=======
func createRemoteCommandProxy(req remoteCommandRequest) (*remoteCommandProxy, error) {
	if err := os.MkdirAll("/var/lib/teleport/log/upload/streaming/default", 0755); err!= nil {
		return nil, trace.Wrap(err)
	}
	protocol, err := httpstream.Handshake(req.httpRequest, req.httpResponseWriter, []string{StreamProtocolV4Name})
	if err!= nil {
		return nil, trace.Wrap(err)
	}
>>>>>>> REPLACE