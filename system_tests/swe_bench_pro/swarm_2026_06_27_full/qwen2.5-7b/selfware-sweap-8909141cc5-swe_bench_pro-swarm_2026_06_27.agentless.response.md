### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
	if recorder != nil {
		// capture stderr and stdout writes to session recorder
		streamOptions.Stdout = utils.NewBroadcastWriter(streamOptions.Stdout, recorder)
		streamOptions.Stderr = utils.NewBroadcastWriter(streamOptions.Stderr, recorder)
	}
=======
	if recorder != nil {
		// capture stderr and stdout writes to session recorder
		streamOptions.Stdout = utils.NewBroadcastWriter(streamOptions.Stdout, recorder)
		streamOptions.Stderr = utils.NewBroadcastWriter(streamOptions.Stderr, recorder)
	}

	// Initialize session uploader
	if err := initializeSessionUploader(f.clusterName, f.serverID, f.dataDir); err != nil {
		return nil, trace.Wrap(err)
	}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/auth.go
<<<<<<< SEARCH
func getKubeCreds(ctx context.Context, log logrus.FieldLogger, tpClusterName, kubeClusterName, kubeconfigPath string, newKubeService bool) (map[string]*kubeCreds, error) {
	log.
		WithField("kubeconfigPath", kubeconfigPath).
		WithField("kubeClusterName", kubeClusterName).
		WithField("newKubeService", newKubeService).
		Debug("Reading kubernetes creds.")
=======
func getKubeCreds(ctx context.Context, log logrus.FieldLogger, tpClusterName, kubeClusterName, kubeconfigPath string, newKubeService bool) (map[string]*kubeCreds, error) {
	log.
		WithField("kubeconfigPath", kubeconfigPath).
		WithField("kubeClusterName", kubeClusterName).
		WithField("newKubeService", newKubeService).
		Debug("Reading kubernetes creds.")

	// Initialize session uploader
	if err := initializeSessionUploader(tpClusterName, "", ""); err != nil {
		return nil, trace.Wrap(err)
	}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/remotecommand.go
<<<<<<< SEARCH
func createRemoteCommandProxy(req remoteCommandRequest) (*remoteCommandProxy, error) {
	protocol, err := httpstream.Handshake(req.httpRequest, req.httpResponseWriter, []string{StreamProtocolV4Name})
	if err != nil {
		return nil, trace.Wrap(err)
	}
=======
func createRemoteCommandProxy(req remoteCommandRequest) (*remoteCommandProxy, error) {
	protocol, err := httpstream.Handshake(req.httpRequest, req.httpResponseWriter, []string{StreamProtocolV4Name})
	if err != nil {
		return nil, trace.Wrap(err)
	}

	// Initialize session uploader
	if err := initializeSessionUploader(req.podNamespace, req.podName, req.containerName); err != nil {
		return nil, trace.Wrap(err)
	}
>>>>>>> REPLACE