### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
	if err := f.setupForwardingHeaders(sess, req); err != nil {
		return nil, trace.Wrap(err)
	}
=======
	if err := f.setupForwardingHeaders(sess, req); err != nil {
		f.log.Debugf("DENIED Port forward: %v.", req.URL.String())
		return nil, trace.Wrap(err)
	}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/auth.go
<<<<<<< SEARCH
	targetAddr, err := parseKubeHost(clientCfg.Host)
	if err != nil {
		return nil, trace.Wrap(err, "failed to parse kubernetes host: %v", err)
	}
=======
	targetAddr, err := parseKubeHost(clientCfg.Host)
	if err != nil {
		return nil, trace.Wrap(err, "failed to parse kubernetes host: %v", err)
	}
	tlsConfig, err := rest.TLSConfigFor(clientCfg)
	if err != nil {
		return nil, trace.Wrap(err, "failed to generate TLS config from kubeconfig: %v", err)
	}
	transportConfig, err := clientCfg.TransportConfig()
	if err != nil {
		return nil, trace.Wrap(err, "failed to generate transport config from kubeconfig: %v", err)
	}
>>>>>>> REPLACE
