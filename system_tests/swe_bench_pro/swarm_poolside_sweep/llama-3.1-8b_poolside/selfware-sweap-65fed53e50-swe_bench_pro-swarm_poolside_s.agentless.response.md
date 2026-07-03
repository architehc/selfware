### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
func (f *Forwarder) getExecutor(*authContext, *clusterSession, *http.Request) (*executor.Executor, error)
=======
func (f *Forwarder) getExecutor(ctx *authContext, sess *clusterSession, req *http.Request) (*executor.Executor, error)
>>>>>>> REPLACE

### FILE: lib/kube/proxy/auth.go
<<<<<<< SEARCH
func getKubeCreds(ctx context.Context, log logrus.FieldLogger, tpClusterName, kubeClusterName, kubeconfigPath string, newKubeService bool) (map[string]*kubeCreds, error)
=======
func getKubeCreds(ctx context.Context, log logrus.FieldLogger, tpClusterName, kubeClusterName, kubeconfigPath string, newKubeService bool) (*kubeCreds, error)
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
func (f *Forwarder) getExecutor(*authContext, *clusterSession, *http.Request) (*executor.Executor, error)
=======
func (f *Forwarder) getExecutor(ctx *authContext, sess *clusterSession, req *http.Request) (*executor.Executor, error)
>>>>>>> REPLACE

### FILE: lib/kube/proxy/constants.go
<<<<<<< SEARCH
const (
    // The SPDY subprotocol "v4.channel.k8s.io" is used for remote command
    // attachment/execution. It is the 4th version of the subprotocol and
    // adds support for exit codes.
    StreamProtocolV4Name = "v4.channel.k8s.io"
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
func (f *Forwarder) getExecutor(*authContext, *clusterSession, *http.Request) (*executor.Executor, error)
=======
func (f *Forwarder) getExecutor(ctx *authContext, sess *clusterSession, req *http.Request) (*executor.Executor, error)
>>>>>>> REPLACE

### FILE: lib/kube/proxy/auth.go
<<<<<<< SEARCH
func getKubeCreds(ctx context.Context, log logrus.FieldLogger, tpClusterName, kubeClusterName, kubeconfigPath string, newKubeService bool) (map[string]*kubeCreds, error)
=======
func getKubeCreds(ctx context.Context, log logrus.FieldLogger, tpClusterName, kubeClusterName, kubeconfigPath string, newKubeService bool) (*kubeCreds, error)
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
func (f *Forwarder) getExecutor(*authContext, *clusterSession, *http.Request) (*executor.Executor, error)
=======
func (f *Forwarder) getExecutor(ctx *authContext, sess *clusterSession, req *http.Request) (*executor.Executor, error)
>>>>>>> REPLACE

### FILE: lib/kube/proxy/portforward.go
<<<<<<< SEARCH
func (h *portForwardProxy) run() {
    h.Debugf("Waiting for port forward streams.")
    for {
        select {
        case <-h.context.Done():
            h.Debugf("Context is closing, returning.")
            return
        case <-h.sourceConn.CloseChan():
            h.Debugf("Upgraded connection closed.")
            return
        case stream := <-h.streamChan:
            requestID, err := h.requestID(stream)
            if err!= nil {
                h.Warningf("Failed to parse request id: %v.", err)
                return
            }
            streamType := stream.Headers().Get(StreamType)
            h.Debugf("Received new stream %v of type %v.", requestID, streamType)
            //... rest of the function remains the same...
>>>>>>> REPLACE