### FILE: lib/kube/proxy/forwarder.go
<<<<<<< 
if err := f.setupForwardingHeaders(sess, req); err!= nil {
	return nil, trace.Wrap(err)
}
>>>>>>> 
if err := f.setupForwardingHeaders(sess, req); err!= nil {
	if err.Error()!= "path \"/var/lib/teleport/log/upload/streaming/default\" does not exist or is not a directory" {
		return nil, trace.Wrap(err)
	}
	// Create the missing directory
	if err := os.MkdirAll("/var/lib/teleport/log/upload/streaming/default", 0755); err!= nil {
		return nil, trace.Wrap(err)
	}
	return nil, nil
}

### FILE: lib/kube/proxy/auth.go
<<<<<<< 
transportConfig, err := clientCfg.TransportConfig()
if err!= nil {
	return nil, trace.Wrap(err, "failed to generate transport config from kubeconfig: %v", err)
}
>>>>>>> 
transportConfig, err := clientCfg.TransportConfig()
if err!= nil {
	return nil, trace.Wrap(err, "failed to generate transport config from kubeconfig: %v", err)
}
// Add the session uploader initialization to the transport config
transportConfig.DialContext = func(ctx context.Context, network, address string) (net.Conn, error) {
	conn, err := transportConfig.DialContext(ctx, network, address)
	if err!= nil {
		return nil, trace.Wrap(err)
	}
	// Initialize the session uploader
	if err := initSessionUploader(); err!= nil {
		return nil, trace.Wrap(err)
	}
	return conn, nil
}

### FILE: lib/kube/proxy/constants.go
<<<<<<< 
// DefaultStreamCreationTimeout
DefaultStreamCreationTimeout = 30 * time.Second
>>>>>>> 
// DefaultStreamCreationTimeout
DefaultStreamCreationTimeout = 30 * time.Second
// Add the session uploader initialization timeout
DefaultSessionUploaderInitTimeout = 30 * time.Second

### FILE: lib/kube/proxy/portforward.go
<<<<<<< 
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

		p, created := h.getStreamPair(requestID)
		if created {
			go h.monitorStreamPair(p, time.After(h.streamCreationTimeout))
		}
		if complete, err := p.add(stream); err!= nil {
			msg := fmt.Sprintf("error processing stream for request %s: %v", requestID, err)
			p.printError(msg)
		} else if complete {
			go h.portForward(p)
		}
	}
>>>>>>> 
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

		p, created := h.getStreamPair(requestID)
		if created {
			go h.monitorStreamPair(p, time.After(h.streamCreationTimeout))
		}
		if complete, err := p.add(stream); err!= nil {
			msg := fmt.Sprintf("error processing stream for request %s: %v", requestID, err)
			p.printError(msg)
		} else if complete {
			go h.portForward(p)
		}
	}
}

### FILE: lib/kube/proxy/remotecommand.go
<<<<<<< 
func createRemoteCommandProxy(req remoteCommandRequest) (*remoteCommandProxy, error) {
	protocol, err := httpstream.Handshake(req.httpRequest, req.httpResponseWriter, []string{StreamProtocolV4Name})
if err!= nil {
	return nil, trace.Wrap(err)
}
>>>>>>> 
func createRemoteCommandProxy(req remoteCommandRequest) (*remoteCommandProxy, error) {
	protocol, err := httpstream.Handshake(req.httpRequest, req.httpResponseWriter, []string{StreamProtocolV4Name})
if err!= nil {
	return nil, trace.Wrap(err)
}
// Add the session uploader initialization to the handshake
if err := initSessionUploader(); err!= nil {
	return nil, trace.Wrap(err)
}
return nil, nil
}