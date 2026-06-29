### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
		if err := f.setupForwardingHeaders(sess, req); err != nil {
			return nil, trace.Wrap(err)
		}
=======
		if err := f.setupForwardingHeaders(sess, req); err != nil {
			f.log.WithError(err).Warning("Failed to setup forwarding headers.")
			return nil, trace.Wrap(err)
		}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/auth.go
<<<<<<< SEARCH
		if err := checkImpersonationPermissions(ctx, client.AuthorizationV1().SelfSubjectAccessReviews()); err != nil {
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
			if kubeconfigPath != "" {
				log.Info("If this is a proxy and you provided a dummy kubeconfig_path, you can remove it from teleport.yaml to get rid of this warning")
			}
		} else {
			log.Debug("Have all necessary kubernetes impersonation permissions.")
		}
<<<<<<< REPLACE
		if err := checkImpersonationPermissions(ctx, client.AuthorizationV1().SelfSubjectAccessReviews()); err != nil {
			if newKubeService {
				return nil, trace.Wrap(err)
			}
			f.log.WithError(err).Warning("Failed to test the necessary kubernetes permissions. This teleport instance will still handle kubernetes requests towards other kubernetes clusters")
		} else {
			f.log.Debug("Have all necessary kubernetes impersonation permissions.")
		}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/portforward.go
<<<<<<< SEARCH
		// Copy from the local port to the target side.
		if _, err := io.Copy(dataStream, p.dataStream); err != nil && !strings.Contains(err.Error(), "use of closed network connection") {
			h.Warningf("Error copying from local connection to remote stream: %v.", err)
			// break out of the select below without waiting for the other copy to finish
			close(localError)
		}
	}()

		h.Debugf("Streams have been created, Waiting for copy to complete.")

		// wait for either a local->remote error or for copying from remote->local to finish
		select {
		case <-remoteDone:
		case <-localError:
		case <-h.context.Done():
			h.Debugf("Context is closing, cleaning up.")
		}

		// always expect something on errorChan (it may be nil)
		select {
		case <-errClose:
		case <-h.context.Done():
			h.Debugf("Context is closing, cleaning up.")
		}
		h.Infof("Port forwarding pair completed.")
		return nil
<<<<<<< REPLACE
		// Copy from the local port to the target side.
		if _, err := io.Copy(dataStream, p.dataStream); err != nil && !strings.Contains(err.Error(), "use of closed network connection") {
			h.Warningf("Error copying from local connection to remote stream: %v.", err)
			// break out of the select below without waiting for the other copy to finish
			close(localError)
		}
	}()

		h.Debugf("Streams have been created, Waiting for copy to complete.")

		// wait for either a local->remote error or for copying from remote->local to finish
		select {
		case <-remoteDone:
		case <-localError:
		case <-h.context.Done():
			h.Debugf("Context is closing, cleaning up.")
		}

		// always expect something on errorChan (it may be nil)
		select {
		case <-errClose:
		case <-h.context.Done():
			h.Debugf("Context is closing, cleaning up.")
		}
		h.Infof("Port forwarding pair completed.")
		return nil
>>>>>>> REPLACE

### FILE: lib/kube/proxy/remotecommand.go
<<<<<<< SEARCH
		// Wrap stdin/out/err with data trackers, but keep them as nil if they were
		// nil. Otherwise, executor will try to use these tracking read/writers
		// when the underlying stream is nil.
		trackIn := utils.NewTrackingReader(streamOptions.Stdin)
		if streamOptions.Stdin != nil {
			streamOptions.Stdin = trackIn
		}
		trackOut := utils.NewTrackingWriter(streamOptions.Stdout)
		if streamOptions.Stdout != nil {
			streamOptions.Stdout = trackOut
		}
		trackErr := utils.NewTrackingWriter(streamOptions.Stderr)
		if streamOptions.Stderr != nil {
			streamOptions.Stderr = trackErr
		}
		if recorder != nil {
			// capture stderr and stdout writes to session recorder
			streamOptions.Stdout = utils.NewBroadcastWriter(streamOptions.Stdout, recorder)
			streamOptions.Stderr = utils.NewBroadcastWriter(streamOptions.Stderr, recorder)
		}

		if err = executor.Stream(streamOptions); err != nil {
			f.log.WithError(err).Warning("Executor failed while streaming.")
			return nil, trace.Wrap(err)
		}
		if err := proxy.sendStatus(err); err != nil {
			f.log.WithError(err).Warning("Failed to send status. Exec command was aborted by client.")
			return nil, trace.Wrap(err)
		}
<<<<<<< REPLACE
		// Wrap stdin/out/err with data trackers, but keep them as nil if they were
		// nil. Otherwise, executor will try to use these tracking read/writers
		// when the underlying stream is nil.
		trackIn := utils.NewTrackingReader(streamOptions.Stdin)
		if streamOptions.Stdin != nil {
			streamOptions.Stdin = trackIn
		}
		trackOut := utils.NewTrackingWriter(streamOptions.Stdout)
		if streamOptions.Stdout != nil {
			streamOptions.Stdout = trackOut
		}
		trackErr := utils.NewTrackingWriter(streamOptions.Stderr)
		if streamOptions.Stderr != nil {
			streamOptions.Stderr = trackErr
		}
		if recorder != nil {
			// capture stderr and stdout writes to session recorder
			streamOptions.Stdout = utils.NewBroadcastWriter(streamOptions.Stdout, recorder)
			streamOptions.Stderr = utils.NewBroadcastWriter(streamOptions.Stderr, recorder)
		}

		if err = executor.Stream(streamOptions); err != nil {
			f.log.WithError(err).Warning("Executor failed while streaming.")
			return nil, trace.Wrap(err)
		}
		if err := proxy.sendStatus(err); err != nil {
			f.log.WithError(err).Warning("Failed to send status. Exec command was aborted by client.")
			return nil, trace.Wrap(err)
		}
>>>>>>> REPLACE