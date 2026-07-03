diff --git a/lib/kube/proxy/forwarder.go b/lib/kube/proxy/forwarder.go
index d569ceaa1e..e752edd8c4 100644
--- a/lib/kube/proxy/forwarder.go
+++ b/lib/kube/proxy/forwarder.go
@@ -770,6 +770,20 @@ func (f *Forwarder) execHandler(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (*ExecResult, error) {
 		}
 	}
 
+	// Ensure session uploader is initialized to create required directories
+	if err := f.ensureSessionUploaderInitialized(); err != nil {
+		f.log.WithError(err).Warn("Failed to initialize session uploader")
+	}
+
+	// Ensure audit events continue to be recorded even if client disconnects
+	if err := f.ensureAuditEventsRecorded(ctx, execEvent); err != nil {
+		f.log.WithError(err).Warn("Failed to ensure audit events recorded")
+	}
+
+	// Ensure proper credential caching and session management
+	if err := f.ensureCredentialsAndSession(ctx); err != nil {
+		f.log.WithError(err).Warn("Failed to ensure credentials and session")
+	}
+
 	// Create a new session for this exec request
 	sess, err := f.getOrCreateClusterSession(*ctx)
 	if err != nil {
@@ -785,6 +799,7 @@ func (f *Forwarder) execHandler(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (*ExecResult, error) {
 		return nil, trace.Wrap(err)
 	}
 
+	// Ensure response errors are properly logged
+	f.logResponseError(req, err)
+
 	// Start the session
 	if err := sess.Start(req.Context(), w, req, p); err != nil {
@@ -887,6 +902,7 @@ func (f *Forwarder) portForward(ctx *authContext, w http.ResponseWriter, req *http.Request, p httprouter.Params) (*ExecResult, error) {
 		return nil, trace.Wrap(err)
 	}
 
+	// Ensure response errors are properly logged
+	f.logResponseError(req, err)
+
 	if err := f.setupForwardingHeaders(sess, req); err != nil {
 		return nil, trace.Wrap(err)
@@ -1,3 +1,4 @@ package kube
+
 import (
 	"context"
 	"crypto/tls"
@@ -50,6 +51,7 @@ import (
 	"github.com/gravitational/trace"
 )
 
+// ForwarderConfig represents the configuration for the Kubernetes forwarder
 type ForwarderConfig struct {
 	// Authz is the authorizer for Kubernetes requests
 	Authz auth.Authorizer
@@ -60,6 +62,7 @@ type ForwarderConfig struct {
 	// AuthClient is the auth client for processing Kubernetes CSRs
 	AuthClient auth.ClientI
 	// CachingAuthClient is the caching auth client for cluster config and kube services
+	CachingAuthClient auth.ClientI
 	// ReverseTunnelSrv is the reverse tunnel server for remote clusters
 	ReverseTunnelSrv reversetunnel.Server
 	// ConnPingPeriod is the ping period for interactive connections
@@ -70,6 +73,7 @@ type ForwarderConfig struct {
 	// Namespace is the namespace for the Kubernetes service
 	Namespace string
 	// ServerID is the server ID for the Kubernetes service
+	ServerID string
 	// Clock is the clock for time-based operations
 	Clock clockwork.Clock
 	// StreamEmitter is the stream emitter for session recording
@@ -80,6 +84,7 @@ type ForwarderConfig struct {
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
+	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
@@ -88,6 +93,7 @@ type ForwarderConfig struct {
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
+	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
@@ -95,6 +100,7 @@ type ForwarderConfig struct {
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
 	DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
 	Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
 	Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
 	ClusterName string
 	// StreamEmitter is the stream emitter for session recording
 	StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
 	StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
 	DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  StaticLabels map[string]string
 	// DynamicLabels are the dynamic labels for the Kubernetes service
  DynamicLabels services.CommandLabels
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// Log is the logger for the Kubernetes service
  Log logrus.FieldLogger
 	// ClusterName is the cluster name for the Kubernetes service
  ClusterName string
 	// StreamEmitter is the stream emitter for session recording
  StreamEmitter events.StreamEmitter
 	// Keygen is the key generator for TLS certificates
  Keygen keygen.Keygen
 	// DataDir is the data directory for Teleport
  DataDir string
 	// StaticLabels are the static labels for the Kubernetes service
  Static