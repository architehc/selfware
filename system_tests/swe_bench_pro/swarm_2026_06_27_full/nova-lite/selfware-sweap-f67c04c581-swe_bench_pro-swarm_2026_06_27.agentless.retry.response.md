### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
	f.log.Debugf("Created streams, getting executor.")

	executor, err := f.getExecutor(*ctx, sess, req)
	if err!= nil {
		f.log.WithError(err).Warning("Failed creating executor.")
		return nil, trace.Wrap(err)
	}
=======
	f.log.Debugf("Created streams, getting executor.")

	executor, err := f.getExecutor(*ctx, sess, req)
	if err!= nil {
		f.log.WithError(err).Warning("Failed creating executor.")
		return nil, trace.Wrap(err)
	}

	// Ensure the session uploader directory exists
	if err := os.MkdirAll(sessionUploaderDir, 0755); err!= nil {
		return nil, trace.Wrap(err, "failed to create session uploader directory")
	}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/auth.go
<<<<<<< SEARCH
func getKubeCreds(ctx context.Context, log logrus.FieldLogger, tpClusterName, kubeClusterName, kubeconfigPath string, newKubeService bool) (map[string]*kubeCreds, error) {
=======
func getKubeCreds(ctx context.Context, log logrus.FieldLogger, tpClusterName, kubeClusterName, kubeconfigPath string, newKubeService bool) (map[string]*kubeCreds, error) {
	log.WithField("kubeconfigPath", kubeconfigPath).WithField("kubeClusterName", kubeClusterName).WithField("newKubeService", newKubeService).Debug("Reading kubernetes creds.")

	// Load kubeconfig or local pod credentials.
	cfg, err := kubeutils.GetKubeConfig(kubeconfigPath, newKubeService, kubeClusterName)
	if err!= nil &&!trace.IsNotFound(err) {
		return nil, trace.Wrap(err)
	}
	if trace.IsNotFound(err) || len(cfg.Contexts) == 0 {
		if newKubeService {
			return nil, trace.BadParameter("no kubernetes credentials found; kubernetes_service requires either a valid kubeconfig_path or to run inside of a kubernetes pod")
		}
		log.Debugf("Could not load kubernetes credentials. This proxy will still handle kubernetes requests for trusted teleport clusters or kubernetes nodes in this teleport cluster")
		return map[string]*kubeCreds{}, nil
	}
	if!newKubeService {
		// Hack for proxy_service - register a k8s cluster named after the
		// teleport cluster name to route legacy requests.
		//
		// Also, remove all other contexts. Multiple kubeconfig entries are
		// only supported for kubernetes_service.
		cfg.Contexts = map[string]*rest.Config{
			tpClusterName: cfg.Contexts[cfg.CurrentContext],
		}
	}

	res := make(map[string]*kubeCreds, len(cfg.Contexts))
	// Convert kubeconfig contexts into kubeCreds.
	for cluster, clientCfg := range cfg.Contexts {
		log := log.WithField("cluster", cluster)
		log.Debug("Checking kubernetes impersonation permissions.")
		client, err := kubernetes.NewForConfig(clientCfg)
		if err!= nil {
			return nil, trace.Wrap(err, "failed to generate kubernetes client for cluster %q", cluster)
		}
		// For each loaded cluster, check impersonation permissions. This
		// failure is only critical for newKubeService.
		if err := checkImpersonationPermissions(ctx, client.AuthorizationV1().SelfSubjectAccessReviews()); err!= nil {
			// kubernetes_service must have valid RBAC permissions, otherwise
			// it's pointless.
			// proxy_service can run without them (e.g. a root proxy).
			if newKubeService {
				return nil, trace.Wrap(err)
			}
			log.WithError(err).Warning("Failed to test the necessary kubernetes permissions. This teleport instance will still handle kubernetes requests towards other kubernetes clusters")
			// We used to recommend users to set a dummy kubeconfig on root
			// proxies to get kubernetes support working for leaf clusters:
			// https://community.gravitational.com/t/enabling-teleport-to-act-as-a-kubernetes-proxy-for-trusted-leaf-clusters/418
			//
			// Since this is no longer necessary, recommend them to clean up
			// via logs.
			if kubeconfigPath!= "" {
				log.Info("If this is a proxy and you provided a dummy kubeconfig_path, you can remove it from teleport.yaml to get rid of this warning")
			}
		} else {
			log.Debug("Have all necessary kubernetes impersonation permissions.")
		}

		targetAddr, err := parseKubeHost(clientCfg.Host)
		if err!= nil {
			return nil, trace.Wrap(err)
		}
		tlsConfig, err := rest.TLSConfigFor(clientCfg)
		if err!= nil {
			return nil, trace.Wrap(err, "failed to generate TLS config from kubeconfig: %v", err)
		}
		transportConfig, err := clientCfg.TransportConfig()
		if err!= nil {
			return nil, trace.Wrap(err, "failed to generate transport config from kubeconfig: %v", err)
		}

		log.Debug("Initialized kubernetes credentials")
		res[cluster] = &kubeCreds{
			tlsConfig:       tlsConfig,
			transportConfig: transportConfig,
			targetAddr:      targetAddr,
			kubeClient:      client,
		}
	}
	return res, nil
}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/constants.go
<<<<<<< SEARCH
const (
	// The SPDY subprotocol "v4.channel.k8s.io" is used for remote command
	// attachment/execution. It is the 4th version of the subprotocol and
	// adds support for exit codes.
	StreamProtocolV4Name = "v4.channel.k8s.io"

	// DefaultStreamCreationTimeout
	DefaultStreamCreationTimeout = 30 * time.Second

	IdleTimeout = 15 * time.Minute
)
>>>>>>> REPLACE

### FILE: lib/kube/proxy/portforward.go
<<<<<<< SEARCH
// getStreamPair returns a httpStreamPair for requestID. This creates a
// new pair if one does not yet exist for the requestID. The returned bool is
// true if the pair was created.
func (h *portForwardProxy) getStreamPair(requestID string) (*httpStreamPair, bool) {
	h.streamPairsLock.Lock()
	defer h.streamPairsLock.Unlock()

	if p, ok := h.streamPairs[requestID]; ok {
		log.Infof("(conn=%p, request=%s) found existing stream pair", h.sourceConn, requestID)
		return p, false
	}

	log.Infof("(conn=%p, request=%s) creating new stream pair", h.sourceConn, requestID)

	p := newPortForwardPair(requestID)
	h.streamPairs[requestID] = p

	return p, true
}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/remotecommand.go
<<<<<<< SEARCH
func createRemoteCommandProxy(req remoteCommandRequest) (*remoteCommandProxy, error) {
	protocol, err := httpstream.Handshake(req.httpRequest, req.httpResponseWriter, []string{StreamProtocolV4Name})
	if err!= nil {
		return nil, trace.Wrap(err)
	}

	streamCh := make(chan streamAndReply)

	upgrader := spdystream.NewResponseUpgraderWithPings(req.pingPeriod)
	conn := upgrader.UpgradeResponse(req.httpResponseWriter, req.httpRequest, func(stream httpstream.Stream, replySent <-chan struct{}) error {
		select {
		case streamCh <- streamAndReply{Stream: stream, replySent: replySent}:
			return nil
		case <-req.context.Done():
			return trace.BadParameter("request has been cancelled")
		}
	})
	// from this point on, we can no longer call methods on response
	if conn == nil {
		// The upgrader is responsible for notifying the client of any errors that
		// occurred during upgrading. All we can do is return here at this point
		// if we weren't successful in upgrading.
		return nil, trace.ConnectionProblem(trace.BadParameter("missing connection"), "missing connection")
	}

	conn.SetIdleTimeout(IdleTimeout)

	var handler protocolHandler
	switch protocol {
	case "":
		log.Warningf("Client did not request protocol negotiation.")
		fallthrough
	case StreamProtocolV4Name:
		log.Infof("Negotiated protocol %v.", protocol)
		handler = &v4ProtocolHandler{}
	default:
		return nil, trace.BadParameter("protocol %v is not supported. upgrade the client", protocol)
	}

	// count the streams client asked for, starting with 1
	expectedStreams := 1
	if req.stdin {
		expectedStreams++
	}
	if req.stdout {
		expectedStreams++
	}
	if req.stderr {
		expectedStreams++
	}
	if req.tty && handler.supportsTerminalResizing() {
		expectedStreams++
	}

	expired := time.NewTimer(DefaultStreamCreationTimeout)
	defer expired.Stop()

	proxy, err := handler.waitForStreams(req.context, streamCh, expectedStreams, expired.C)
	if err!= nil {
		return nil, trace.Wrap(err)
	}

	proxy.conn = conn
	proxy.tty = req.tty

	if proxy.resizeStream!= nil {
		proxy.resizeQueue = newTermQueue(req.context, req.onResize)
		go proxy.resizeQueue.handleResizeEvents(proxy.resizeStream)
	}
	return proxy, nil
}
>>>>>>> REPLACE