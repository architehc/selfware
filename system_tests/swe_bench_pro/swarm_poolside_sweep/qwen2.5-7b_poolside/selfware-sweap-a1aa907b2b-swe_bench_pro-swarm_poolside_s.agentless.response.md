### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
	if err := f.setupForwardingHeaders(sess, req); err != nil {
		f.log.Debugf("DENIED Port forward: %v.", req.URL.String())
		return nil, trace.Wrap(err)
	}
=======
	if err := f.setupForwardingHeaders(sess, req); err != nil {
		f.log.Debugf("DENIED Port forward: %v.", req.URL.String())
		return nil, trace.Wrap(err)
	}
	if err := f.initSessionUploader(sess); err != nil {
		f.log.WithError(err).Warning("Failed to initialize session uploader.")
		return nil, trace.Wrap(err)
	}
>>>>>>> REPLACE