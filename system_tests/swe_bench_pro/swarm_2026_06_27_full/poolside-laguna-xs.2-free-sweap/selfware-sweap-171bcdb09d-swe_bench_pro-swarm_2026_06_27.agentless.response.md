
### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
	if err = executor.Stream(streamOptions); err != nil {
		f.log.WithError(err).Warning("Executor failed while streaming.")
		return nil, trace.Wrap(err)
	}
	if err := proxy.sendStatus(err); err != nil {
		f.log.WithError(err).Warning("Failed to send status. Exec command was aborted by client.")
		return nil, trace.Wrap(err)
	}

	if request.tty {
		sessionDataEvent := &events.SessionData{
			Metadata: events.Metadata{
				Type:        events.SessionDataEvent,
				Code:        events.SessionDataCode,
				ClusterName: f.ClusterName,
			},
			ServerMetadata: events.ServerMetadata{
				ServerID:        f.ServerID,
				ServerNamespace: f.Namespace,
			},
			SessionMetadata: events.SessionMetadata{
				SessionID: string(sessionID),
			},
			UserMetadata: events.UserMetadata{
				User:  ctx.User.GetName(),
				Login: ctx.User.GetName(),
			},
			ConnectionMetadata: events.ConnectionMetadata{
				RemoteAddr: req.RemoteAddr,
				LocalAddr:  sess.teleportCluster.targetAddr,
				Protocol:   events.EventProtocolKube,
			},
			// Bytes transmitted from user to pod.
			BytesTransmitted: trackIn.Count(),
			// Bytes received from pod by user.
			BytesReceived: trackOut.Count() + trackErr.Count(),
		}
		if err := emitter.EmitAuditEvent(request.context, sessionDataEvent); err != nil {
			f.log.WithError(err).Warn("Failed to emit session data event.")
		}
		sessionEndEvent := &events.SessionEnd{
			Metadata: events.Metadata{
				Type:        events.SessionEndEvent,
				Code:        events.SessionEndCode,
				ClusterName: f.ClusterName,
			},
			ServerMetadata: events.ServerMetadata{
				ServerID:        f.ServerID,
				ServerNamespace: f.Namespace,
			},
			SessionMetadata: events.SessionMetadata{
				SessionID: string(sessionID),
			},
			UserMetadata: events.UserMetadata{
				User:  ctx.User.GetName(),
				Login: ctx.User.GetName(),
			},
			ConnectionMetadata: events.ConnectionMetadata{
				RemoteAddr: req.RemoteAddr,
				LocalAddr:  sess.teleportCluster.targetAddr,
				Protocol:   events.EventProtocolKube,
			},
			Interactive: true,
			// There can only be 1 participant, k8s sessions are not join-able.
			Participants:              []string{ctx.User.GetName()},
			StartTime:                 sessionStart,
			EndTime:                   f.Clock.Now().UTC(),
			KubernetesClusterMetadata: ctx.eventClusterMeta(),
			KubernetesPodMetadata:     eventPodMeta,
			InitialCommand:            request.cmd,
		}
		if err := emitter.EmitAuditEvent(request.context, sessionEndEvent); err != nil {
			f.log.WithError(err).Warn("Failed to emit session end event.")
		}
	} else {
		// send an exec event
		execEvent := &events.Exec{
			Metadata: events.Metadata{
				Type:        events.ExecEvent,
				ClusterName: f.ClusterName,
			},
			ServerMetadata: events.ServerMetadata{
				ServerID:        f.ServerID,
				ServerNamespace: f.Namespace,
			},
			SessionMetadata: events.SessionMetadata{
				SessionID: string(sessionID),
			},
			UserMetadata: events.UserMetadata{
				User:  ctx.User.GetName(),
				Login: ctx.User.GetName(),
			},
			ConnectionMetadata: events.ConnectionMetadata{
				RemoteAddr: req.RemoteAddr,
				LocalAddr:  sess.teleportCluster.targetAddr,
				Protocol:   events.EventProtocolKube,
			},
			CommandMetadata: events.CommandMetadata{
				Command: strings.Join(request.cmd, " "),
			},
			KubernetesClusterMetadata: ctx.eventClusterMeta(),
			KubernetesPodMetadata:     eventPodMeta,
		}
		if err != nil {
			execEvent.Code = events.ExecFailureCode
			execEvent.Error = err.Error()
			if exitErr, ok := err.(utilexec.ExitError); ok && exitErr.Exited() {
				execEvent.ExitCode = fmt.Sprintf("%d", exitErr.ExitStatus())
			}
		} else {
			execEvent.Code = events.ExecCode
		}
		if err := emitter.EmitAuditEvent(request.context, execEvent); err != nil {
			f.log.WithError(err).Warn("Failed to emit event.")
		}
	}
=======
	if err = executor.Stream(streamOptions); err != nil {
		f.log.WithError(err).Warning("Executor failed while streaming.")
		return nil, trace.Wrap(err)
	}
	if err := proxy.sendStatus(err); err != nil {
		f.log.WithError(err).Warning("Failed to send status. Exec command was aborted by client.")
		return nil, trace.Wrap(err)
	}

	// Use a background context for audit events to ensure they are recorded
	// even if the client disconnects.
	auditCtx := context.Background()

	if request.tty {
		sessionDataEvent := &events.SessionData{
			Metadata: events.Metadata{
				Type:        events.SessionDataEvent,
				Code:        events.SessionDataCode,
				ClusterName: f.ClusterName,
			},
			ServerMetadata: events.ServerMetadata{
				ServerID:        f.ServerID,
				ServerNamespace: f.Namespace,
			},
			SessionMetadata: events.SessionMetadata{
				SessionID: string(sessionID),
			},
			UserMetadata: events.UserMetadata{
				User:  ctx.User.GetName(),
				Login: ctx.User.GetName(),
			},
			ConnectionMetadata: events.ConnectionMetadata{
				RemoteAddr: req.RemoteAddr,
				LocalAddr:  sess.teleportCluster.targetAddr,
				Protocol:   events.EventProtocolKube,
			},
			// Bytes transmitted from user to pod.
			BytesTransmitted: trackIn.Count(),
			// Bytes received from pod by user.
			BytesReceived: trackOut.Count() + trackErr.Count(),
		}
		if err := emitter.EmitAuditEvent(auditCtx, sessionDataEvent); err != nil {
			f.log.WithError(err).Warn("Failed to emit session data event.")
		}
		sessionEndEvent := &events.SessionEnd{
			Metadata: events.Metadata{
				Type:        events.SessionEndEvent,
				Code:        events.SessionEndCode,
				ClusterName: f.ClusterName,
			},
			ServerMetadata: events.ServerMetadata{
				ServerID:        f.ServerID,
				ServerNamespace: f.Namespace,
			},
			SessionMetadata: events.SessionMetadata{
				SessionID: string(sessionID),
			},
			UserMetadata: events.UserMetadata{
				User:  ctx.User.GetName(),
				Login: ctx.User.GetName(),
			},
			ConnectionMetadata: events.ConnectionMetadata{
				RemoteAddr: req.RemoteAddr,
				LocalAddr:  sess.teleportCluster.targetAddr,
				Protocol:   events.EventProtocolKube,
			},
			Interactive: true,
			// There can only be 1 participant, k8s sessions are not join-able.
			Participants:              []string{ctx.User.GetName()},
			StartTime:                 sessionStart,
			EndTime:                   f.Clock.Now().UTC(),
			KubernetesClusterMetadata: ctx.eventClusterMeta(),
			KubernetesPodMetadata:     eventPodMeta,
			InitialCommand:            request.cmd,
		}
		if err := emitter.EmitAuditEvent(auditCtx, sessionEndEvent); err != nil {
			f.log.WithError(err).Warn("Failed to emit session end event.")
		}
	} else {
		// send an exec event
		execEvent := &events.Exec{
			Metadata: events.Metadata{
				Type:        events.ExecEvent,
				ClusterName: f.ClusterName,
			},
			ServerMetadata: events.ServerMetadata{
				ServerID:        f.ServerID,
				ServerNamespace: f.Namespace,
			},
			SessionMetadata: events.SessionMetadata{
				SessionID: string(sessionID),
			},
			UserMetadata: events.UserMetadata{
				User:  ctx.User.GetName(),
				Login: ctx.User.GetName(),
			},
			ConnectionMetadata: events.ConnectionMetadata{
				RemoteAddr: req.RemoteAddr,
				LocalAddr:  sess.teleportCluster.targetAddr,
				Protocol:   events.EventProtocolKube,
			},
			CommandMetadata: events.CommandMetadata{
				Command: strings.Join(request.cmd, " "),
			},
			KubernetesClusterMetadata: ctx.eventClusterMeta(),
			KubernetesPodMetadata:     eventPodMeta,
		}
		if err != nil {
			execEvent.Code = events.ExecFailureCode
			execEvent.Error = err.Error()
			if exitErr, ok := err.(utilexec.ExitError); ok && exitErr.Exited() {
				execEvent.ExitCode = fmt.Sprintf("%d", exitErr.ExitStatus())
			}
		} else {
			execEvent.Code = events.ExecCode
		}
		if err := emitter.EmitAuditEvent(auditCtx, execEvent); err != nil {
			f.log.WithError(err).Warn("Failed to emit event.")
		}
	}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
func (f *Forwarder) portForward(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
	f.log.Debugf("Port forward: %v. req headers: %v.", req.URL.String(), req.Header)
	sess, err := f.getOrCreateClusterSession(*ctx)
	if err != nil {
		// This error goes to kubernetes client and is not visible in the logs
		// of the teleport server if not logged here.
		f.log.Errorf("Failed to create cluster session: %v.", err)
		return nil, trace.Wrap(err)
	}

	if err := f.setupForwardingHeaders(sess, req); err != nil {
		f.log.Debugf("DENIED Port forward: %v.", req.URL.String())
		return nil, trace.Wrap(err)
	}

	dialer, err := f.getDialer(*ctx, sess, req)
	if err != nil {
		return nil, trace.Wrap(err)
	}

	onPortForward := func(addr string, success bool) {
		if sess.noAuditEvents {
			return
		}
		portForward := &events.PortForward{
			Metadata: events.Metadata{
				Type: events.PortForwardEvent,
				Code: events.PortForwardCode,
			},
			UserMetadata: events.UserMetadata{
				Login: ctx.User.GetName(),
				User:  ctx.User.GetName(),
			},
			ConnectionMetadata: events.ConnectionMetadata{
				LocalAddr:  sess.teleportCluster.targetAddr,
				RemoteAddr: req.RemoteAddr,
				Protocol:   events.EventProtocolKube,
			},
			Addr: addr,
			Status: events.Status{
				Success: success,
			},
		}
		if !success {
			portForward.Code = events.PortForwardFailureCode
		}
		if err := f.StreamEmitter.EmitAuditEvent(req.Context(), portForward); err != nil {
			f.log.WithError(err).Warn("Failed to emit event.")
		}
	}
=======
func (f *Forwarder) portForward(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (interface{}, error) {
	f.log.Debugf("Port forward: %v. req headers: %v.", req.URL.String(), req.Header)
	sess, err := f.getOrCreateClusterSession(*ctx)
	if err != nil {
		// This error goes to kubernetes client and is not visible in the logs
		// of the teleport server if not logged here.
		f.log.Errorf("Failed to create cluster session: %v.", err)
		return nil, trace.Wrap(err)
	}

	if err := f.setupForwardingHeaders(sess, req); err != nil {
		f.log.Debugf("DENIED Port forward: %v.", req.URL.String())
		return nil, trace.Wrap(err)
	}

	dialer, err := f.getDialer(*ctx, sess, req)
	if err != nil {
		return nil, trace.Wrap(err)
	}

	onPortForward := func(addr string, success bool) {
		if sess.noAuditEvents {
			return
		}
		// Use a background context for audit events to ensure they are recorded
		// even if the client disconnects.
		auditCtx := context.Background()
		portForward := &events.PortForward{
			Metadata: events.Metadata{
				Type: events.PortForwardEvent,
				Code: events.PortForwardCode,
			},
			UserMetadata: events.UserMetadata{
				Login: ctx.User.GetName(),
				User:  ctx.User.GetName(),
			},
			ConnectionMetadata: events.ConnectionMetadata{
				LocalAddr:  sess.teleportCluster.targetAddr,
				RemoteAddr: req.RemoteAddr,
				Protocol:   events.EventProtocolKube,
			},
			Addr: addr,
			Status: events.Status{
				Success: success,
			},
		}
		if !success {
			portForward.Code = events.PortForwardFailureCode
		}
		if err := f.StreamEmitter.EmitAuditEvent(auditCtx, portForward); err != nil {
			f.log.WithError(err).Warn("Failed to emit event.")
		}
	}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
const (
	// ImpersonateHeaderPrefix is K8s impersonation prefix for impersonation feature:
	// https://kubernetes.io/docs/reference/access-authn-authz/authentication/#user-impersonation
	ImpersonateHeaderPrefix = "Impersonate-"
	// ImpersonateUserHeader is impersonation header for users
	ImpersonateUserHeader = "Impersonate-User"
	// ImpersonateGroupHeader is K8s impersonation header for user
	ImpersonateGroupHeader = "Impersonate-Group"
	// impersonationRequestDeniedMessage is access denied message for impersonation
	ImpersonationRequestDeniedMessage = "impersonation request has been denied"
)
=======
const (
	// ImpersonateHeaderPrefix is K8s impersonation prefix for impersonation feature:
	// https://kubernetes.io/docs/reference/access-authn-authz/authentication/#user-impersonation
	ImpersonateHeaderPrefix = "Impersonate-"
	// ImpersonateUserHeader is impersonation header for users
	ImpersonateUserHeader = "Impersonate-User"
	// ImpersonateGroupHeader is K8s impersonation header for user
	ImpersonateGroupHeader = "Impersonate-Group"
	// ImpersonationRequestDeniedMessage is access denied message for impersonation
	ImpersonationRequestDeniedMessage = "impersonation request has been denied"
)
>>>>>>> REPLACE
