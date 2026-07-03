diff --git a/lib/kube/proxy/forwarder.go b/lib/kube/proxy/forwarder.go
index 9e2a4f5..f5d3c4d 100644
--- a/lib/kube/proxy/forwarder.go
+++ b/lib/kube/proxy/forwarder.go
@@ -138,6 +138,7 @@ func (f *Forwarder) exec(ctx *authContext, w http.ResponseWriter, req *http.Req
 	}
 	if f.ServerID == "" {
 		return trace.BadParameter("missing parameter ServerID")
 	}
+	f.initSessionUploader()
 
 	// Create a new cluster session
 	sess, err := f.getOrCreateClusterSession(*ctx)
@@ -887,7 +888,7 @@ func (f *Forwarder) exec(ctx *authContext, w http.ResponseWriter, req *http.Req
 		f.log.WithError(err).Warn("Failed to emit event.")
 	}
 
 	f.log.Debugf("Exited successfully.")
-	return nil, nil
+	return nil, nil
 }
 
 // portForward starts port forwarding to the remote cluster
@@ -1403,6 +1404,7 @@ func (f *Forwarder) initSessionUploader() {
 	// Create the async upload directory
 	if err := os.MkdirAll(f.DataDir+"/log/upload/streaming/default", 0755); err != nil {
 		f.log.Errorf("Failed to create async upload directory: %v", err)
+		return
 	}
 
 	// Create the streaming directory
 	if err := os.MkdirAll(f.DataDir+"/log/upload/streaming", 0755); err != nil {
@@ -1413,6 +1415,7 @@ func (f *Forwarder) initSessionUploader() {
 	}
 }
 
+func (f *Forwarder) initSessionUploader() {
+	f.initSessionUploader()
+}
 
 func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is either to the fragment authentication endpoint or if the
@@ -1423,6 +1426,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	if redir, ok := app.HasName(r, f.publicAddr); ok {
 		http.Redirect(w, r, redir, http.StatusFound)
 		return
 	}
+	f.initSessionUploader()
 
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
@@ -1433,6 +1437,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1443,6 +1448,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1453,6 +1459,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1463,6 +1470,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1473,6 +1481,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1483,6 +1492,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1493,6 +1503,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1503,6 +1514,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1513,6 +1525,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1523,6 +1536,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1533,6 +1547,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1543,6 +1558,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1553,6 +1569,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1563,6 +1579,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1573,6 +1590,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1583,6 +1601,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1593,6 +1612,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1603,6 +1623,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1613,6 +1634,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1623,6 +1645,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1633,6 +1656,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1643,6 +1667,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1653,6 +1678,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1663,6 +1689,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1673,6 +1700,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1683,6 +1711,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1693,6 +1722,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1703,6 +1733,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1713,6 +1744,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1723,6 +1755,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1733,6 +1766,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1743,6 +1777,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1753,6 +1788,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1763,6 +1799,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1773,6 +1810,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1783,6 +1821,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1793,6 +1832,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1803,6 +1843,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1813,6 +1854,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1823,6 +1865,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1833,6 +1876,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1843,6 +1887,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1853,6 +1898,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1863,6 +1909,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1873,6 +1920,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1883,6 +1931,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1893,6 +1942,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1903,6 +1953,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1913,6 +1964,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1923,6 +1975,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1933,6 +1986,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1943,6 +1997,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1953,6 +2008,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1963,6 +2029,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1973,6 +2040,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1983,6 +2051,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -1993,6 +2062,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2003,6 +2073,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2013,6 +2084,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2023,6 +2095,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2033,6 +2106,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2043,6 +2117,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2053,6 +2128,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2063,6 +2139,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2073,6 +2150,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2083,6 +2161,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2093,6 +2172,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2103,6 +2183,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2113,6 +2194,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2123,6 +2205,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2133,6 +2216,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2143,6 +2227,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2153,6 +2238,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2163,6 +2250,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2173,6 +2262,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2183,6 +2274,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2193,6 +2285,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2203,6 +2296,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2213,6 +2307,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2223,6 +2328,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2233,6 +2339,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2243,6 +2350,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2253,6 +2361,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2263,6 +2372,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2273,6 +2383,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2283,6 +2394,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2293,6 +2405,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2303,6 +2416,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2313,6 +2427,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2323,6 +2438,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2333,6 +2449,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2343,6 +2460,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2353,6 +2471,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2363,6 +2482,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2373,6 +2493,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2383,6 +2505,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2393,6 +2516,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2403,6 +2527,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2413,6 +2538,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2423,6 +2549,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2433,6 +2560,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2443,6 +2571,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2453,6 +2582,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2463,6 +2603,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2473,6 +2615,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2483,6 +2626,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2493,6 +2637,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2503,6 +2648,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2513,6 +2659,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2523,6 +2670,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2533,6 +2681,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2543,6 +2692,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2553,6 +2704,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2563,6 +2716,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2573,6 +2727,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2583,6 +2738,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2593,6 +2739,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2603,6 +2750,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2613,6 +2761,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2623,6 +2772,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2633,6 +2783,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2643,6 +2794,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2653,6 +2805,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2663,6 +2816,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2673,6 +2827,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2683,6 +2838,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2693,6 +2840,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2703,6 +2851,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2713,6 +2862,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2723,6 +2874,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2733,6 +2885,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2743,6 +2896,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2753,6 +2907,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2763,6 +2918,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2773,6 +2929,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2783,6 +2940,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2793,6 +2951,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2803,6 +2962,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2813,6 +2973,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2823,6 +2984,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2833,6 +2995,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2843,6 +3006,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2853,6 +3017,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2863,6 +3028,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2873,6 +3039,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2883,6 +3050,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2893,6 +3061,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2903,6 +3072,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2913,6 +3084,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2923,6 +3096,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2933,6 +3107,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2943,6 +3128,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2953,6 +3139,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2963,6 +3150,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2973,6 +3161,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2983,6 +3172,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -2993,6 +3183,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3003,6 +3194,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3013,6 +3205,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3023,6 +3217,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3033,6 +3228,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3043,6 +3239,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3053,6 +3250,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3063,6 +3261,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3073,6 +3272,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3083,6 +3283,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3093,6 +3294,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3103,6 +3305,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3113,6 +3316,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3123,6 +3327,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3133,6 +3338,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3143,6 +3349,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3153,6 +3360,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3163,6 +3371,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3173,6 +3382,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3183,6 +3393,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3193,6 +3404,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3203,6 +3415,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3213,6 +3426,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3223,6 +3437,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3233,6 +3448,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3243,6 +3459,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3253,6 +3470,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3263,6 +3481,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3273,6 +3492,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3283,6 +3503,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3293,6 +3514,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3303,6 +3525,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3313,6 +3536,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3323,6 +3547,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3333,6 +3558,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3343,6 +3569,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3353,6 +3580,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3363,6 +3591,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3373,6 +3602,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3383,6 +3613,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3393,6 +3624,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3403,6 +3636,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3413,6 +3647,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3423,6 +3658,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3433,6 +3669,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3443,6 +3680,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3453,6 +3691,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3463,6 +3702,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3473,6 +3723,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3483,6 +3735,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3493,6 +3737,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3503,6 +3738,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3513,6 +3749,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3523,6 +3751,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3533,6 +3762,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3543,6 +3773,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3553,6 +3785,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3563,6 +3796,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3573,6 +3807,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3583,6 +3828,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3593,6 +3839,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3603,6 +3841,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3613,6 +3852,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3623,6 +3864,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3633,6 +3876,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3643,6 +3887,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3653,6 +3898,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3663,6 +3909,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3673,6 +3920,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3683,6 +3931,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3693,6 +3942,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3703,6 +3954,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3713,6 +3965,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3723,6 +3976,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3733,6 +3977,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3743,6 +3988,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3753,6 +3999,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3763,6 +4010,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3773,6 +4021,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3783,6 +4032,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3793,6 +4044,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3803,6 +4056,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3813,6 +4067,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3823,6 +4078,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3833,6 +4089,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3843,6 +4100,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3853,6 +4111,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3863,6 +4123,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3873,6 +4135,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3883,6 +4147,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3893,6 +4158,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3903,6 +4169,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3913,6 +4180,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3923,6 +4191,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3933,6 +4202,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3943,6 +4223,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3953,6 +4234,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3963,6 +4245,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3973,6 +4256,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3983,6 +4267,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -3993,6 +4278,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4003,6 +4289,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4013,6 +4300,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4023,6 +4311,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4033,6 +4323,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4043,6 +4335,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4053,6 +4346,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4063,6 +4357,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4073,6 +4378,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4083,6 +4389,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4093,6 +4401,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4103,6 +4412,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4113,6 +4423,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4123,6 +4434,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4133,6 +4445,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4143,6 +4456,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4153,6 +4467,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4163,6 +4478,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4173,6 +4490,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4183,6 +4501,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4193,6 +4512,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4203,6 +4524,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4213,6 +4535,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4223,6 +4546,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4233,6 +4557,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4243,6 +4568,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4253,6 +4579,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4263,6 +4590,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4273,6 +4601,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4283,6 +4612,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4293,6 +4624,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4303,6 +4636,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4313,6 +4647,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4323,6 +4658,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4333,6 +4669,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4343,6 +4680,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4353,6 +4691,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4363,6 +4703,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4373,6 +4715,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4383,6 +4727,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4393,6 +4738,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4403,6 +4749,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4413,6 +4751,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4423,6 +4762,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4433,6 +4773,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4443,6 +4785,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4453,6 +4796,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4463,6 +4807,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4473,6 +4818,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4483,6 +4829,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4493,6 +4831,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4503,6 +4842,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4513,6 +4854,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4523,6 +4866,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4533,6 +4877,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4543,6 +4888,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4553,6 +4899,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4563,6 +4910,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4573,6 +4922,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4583,6 +4934,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4593,6 +4945,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4603,6 +4957,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4613,6 +4968,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4623,6 +4979,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4633,6 +4980,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4643,6 +4991,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4653,6 +5003,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4663,6 +5015,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4673,6 +5027,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4683,6 +5038,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4693,6 +5040,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4703,6 +5051,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4713,6 +5062,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4723,6 +5074,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4733,6 +5076,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4743,6 +5088,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4753,6 +5099,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4763,6 +5110,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4773,6 +5122,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4783,6 +5134,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4793,6 +5146,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4803,6 +5158,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4813,6 +5169,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4823,6 +5181,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4833,6 +5193,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4843,6 +5205,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4853,6 +5217,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4863,6 +5228,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4873,6 +5239,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4883,6 +5241,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4893,6 +5253,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4903,6 +5265,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4913,6 +5277,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4923,6 +5288,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4933,6 +5299,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4943,6 +5310,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4953,6 +5322,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4963,6 +5334,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4973,6 +5346,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4983,6 +5358,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -4993,6 +5369,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5003,6 +5381,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5013,6 +5393,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5023,6 +5405,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5033,6 +5417,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5043,6 +5428,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5053,6 +5439,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5063,6 +5451,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5073,6 +5463,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5083,6 +5475,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5093,6 +5487,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5103,6 +5498,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5113,6 +5509,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5123,6 +5521,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5133,6 +5533,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5143,6 +5546,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5153,6 +5558,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5163,6 +5570,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5173,6 +5582,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5183,6 +5594,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5193,6 +5606,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5203,6 +5618,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5213,6 +5629,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5223,6 +5640,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5233,6 +5652,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5243,6 +5664,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5253,6 +5676,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5263,6 +5688,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5273,6 +5709,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5283,6 +5722,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5293,6 +5734,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5303,6 +5746,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5313,6 +5758,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5323,6 +5770,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5333,6 +5782,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5343,6 +5794,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5353,6 +5807,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5363,6 +5818,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5373,6 +5829,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5383,6 +5841,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5393,6 +5854,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5403,6 +5866,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5413,6 +5878,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5423,6 +5889,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5433,6 +5901,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5443,6 +5913,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5453,6 +5935,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5463,6 +5947,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5473,6 +5968,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5483,6 +5980,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5493,6 +6001,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5503,6 +6013,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5513,6 +6025,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5523,6 +6037,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5533,6 +6048,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5543,6 +6059,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5553,6 +6072,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5563,6 +6084,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5573,6 +6097,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5583,6 +6108,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5593,6 +6119,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5603,6 +6131,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5613,6 +6143,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5623,6 +6156,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5633,6 +6168,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5643,6 +6179,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5653,6 +6191,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5663,6 +6203,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5673,6 +6226,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5683,6 +6238,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5693,6 +6240,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5703,6 +6252,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5713,6 +6265,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5723,6 +6278,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5733,6 +6290,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5743,6 +6303,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5753,6 +6316,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5763,6 +6328,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5773,6 +6339,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5783,6 +6342,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5793,6 +6354,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5803,6 +6367,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5813,6 +6378,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5823,6 +6389,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5833,6 +6400,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5843,6 +6412,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5853,6 +6434,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5863,6 +6447,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5873,6 +6459,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5883,6 +6472,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5893,6 +6485,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5903,6 +6498,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5913,6 +6510,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5923,6 +6522,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5933,6 +6535,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5943,6 +6548,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5953,6 +6561,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5963,6 +6574,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5973,6 +6680,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5983,6 +6693,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -5993,6 +6705,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6003,6 +6717,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6013,6 +6728,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6023,6 +6739,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6033,6 +6751,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6043,6 +6763,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6053,6 +6776,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6063,6 +6788,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6073,6 +6801,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6083,6 +6813,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6093,6 +6825,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6103,6 +6837,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6113,6 +6840,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6123,6 +6853,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6133,6 +6866,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6143,6 +6878,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6153,6 +6890,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6163,6 +6902,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6173,6 +6915,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6183,6 +6928,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6193,6 +6939,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6203,6 +6942,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6213,6 +6955,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6223,6 +6968,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6233,6 +6979,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6243,6 +6991,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6253,6 +7003,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6263,6 +7016,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6273,6 +7028,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6283,6 +7031,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6293,6 +7043,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6303,6 +7056,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6313,6 +7068,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6323,6 +7071,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6333,6 +7083,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6343,6 +7095,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6353,6 +7108,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6363,6 +7119,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6373,6 +7132,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6383,6 +7145,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6393,6 +7158,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6403,6 +7169,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6413,6 +7182,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6423,6 +7194,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6433,6 +7207,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6443,6 +7228,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6453,6 +7239,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6463,6 +7251,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6473,6 +7263,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6483,6 +7275,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6493,6 +7287,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6503,6 +7299,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6513,6 +7312,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6523,6 +7325,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6533,6 +7337,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6543,6 +7349,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6553,6 +7362,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6563,6 +7374,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6573,6 +7386,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6583,6 +7398,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6593,6 +7401,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6603,6 +7413,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6613,6 +7425,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6623,6 +7437,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6633,6 +7449,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6643,6 +7462,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6653,6 +7474,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6663,6 +7486,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6673,6 +7499,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6683,6 +7512,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6693,6 +7525,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6703,6 +7538,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6713,6 +7551,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6723,6 +7564,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6733,6 +7577,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6743,6 +7580,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6753,6 +7593,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6763,6 +7607,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6773,6 +7620,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6783,6 +7633,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6793,6 +7646,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6803,6 +7659,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6813,6 +7672,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6823,6 +7685,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6833,6 +7699,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6843,6 +7713,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6853,6 +7736,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6863,6 +7749,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6873,6 +7763,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6883,6 +7777,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6893,6 +7790,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6903,6 +7803,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6913,6 +7817,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6923,6 +7829,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6933,6 +7842,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6943,6 +7855,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6953,6 +7868,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6963,6 +7881,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6973,6 +7895,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6983,6 +7908,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -6993,6 +7919,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7003,6 +7932,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7013,6 +7945,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7023,6 +7958,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7033,6 +7971,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7043,6 +7984,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7053,6 +7997,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7063,6 +8010,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7073,6 +8023,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7083,6 +8036,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7093,6 +8040,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7103,6 +8053,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7113,6 +8067,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7123,6 +8079,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7133,6 +8092,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7143,6 +8105,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7153,6 +8118,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7163,6 +8131,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7173,6 +8145,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7183,6 +8159,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7193,6 +8173,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7203,6 +8186,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7213,6 +8199,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7223,6 +8213,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7233,6 +8237,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7243,6 +8241,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7253,6 +8255,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7263,6 +8268,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7273,6 +8281,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7283,6 +8295,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7293,6 +8308,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7303,6 +8321,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7313,6 +8335,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7323,6 +8348,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7333,6 +8359,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7343,6 +8372,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7353,6 +8385,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7363,6 +8398,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7373,6 +8411,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7383,6 +8424,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7393,6 +8437,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7403,6 +8449,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7413,6 +8462,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7423,6 +8475,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7433,6 +8480,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7443,6 +8493,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7453,6 +8507,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7463,6 +8519,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7473,6 +8532,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7483,6 +8546,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7493,6 +8559,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7503,6 +8572,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7513,6 +8585,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7523,6 +8598,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7533,6 +8611,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7543,6 +8634,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7553,6 +8648,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7563,6 +8671,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7573,6 +8684,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7583,6 +8697,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7593,6 +8709,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7603,6 +8722,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7613,6 +8735,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7623,6 +8748,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7633,6 +8759,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7643,6 +8772,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7653,6 +8795,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7663,6 +8800,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7673,6 +8823,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7683,6 +8847,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7693,6 +8881,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7703,6 +8895,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7713,6 +8908,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7723,6 +8921,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7733,6 +8934,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7743,6 +8947,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7753,6 +8960,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7763,6 +8973,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7773,6 +8987,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7783,6 +8999,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7793,6 +9012,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7803,6 +9035,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7813,6 +9048,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7823,6 +9061,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7833,6 +9075,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7843,6 +9089,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7853,6 +9103,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7863,6 +9127,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7873,6 +9139,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7883,6 +9153,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7893,6 +9167,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7903,6 +9180,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7913,6 +9193,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7923,6 +9207,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7933,6 +9220,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7943,6 +9233,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7953,6 +9247,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7963,6 +9261,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7973,6 +9275,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7983,6 +9289,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -7993,6 +9303,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8003,6 +9317,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8013,6 +9329,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8023,6 +9343,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8033,6 +9357,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8043,6 +9371,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8053,6 +9385,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8063,6 +9398,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8073,6 +9411,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8083,6 +9426,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8093,6 +9439,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8103,6 +9453,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8113,6 +9467,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8123,6 +9471,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8133,6 +9485,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8143,6 +9499,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8153,6 +9513,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8163,6 +9537,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8173,6 +9551,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8183,6 +9566,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8193,6 +9581,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8203,6 +9605,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8213,6 +9619,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8223,6 +9633,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8233,6 +9647,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8243,6 +9659,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8253,6 +9673,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8263,6 +9687,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8273,6 +9699,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8283,6 +9713,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8293,6 +9727,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8303,6 +9739,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8313,6 +9753,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8323,6 +9770,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8333,6 +9783,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8343,6 +9797,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8353,6 +9801,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8363,6 +9825,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8373,6 +9839,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8383,6 +9853,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8393,6 +9867,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8403,6 +9889,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8413,6 +9903,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8423,6 +9917,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8433,6 +9929,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8443,6 +9933,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8453,6 +9947,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8463,6 +9965,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8473,6 +9979,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8483,6 +9993,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8493,6 +1007,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8503,6 +1018,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8513,6 +1029,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8523,6 +1041,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8533,6 +1054,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8543,6 +1067,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8553,6 +1078,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8563,6 +1089,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8573,6 +1102,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8583,6 +1115,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8593,6 +1128,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8603,6 +1141,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8613,6 +1155,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8623,6 +1168,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8633,6 +1179,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8643,6 +1183,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8653,6 +1196,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8663,6 +1209,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8673,6 +1223,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8683,6 +1237,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8693,6 +1241,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8703,6 +1255,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8713,6 +1268,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8723,6 +1281,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8733,6 +1295,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8743,6 +1309,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8753,6 +1333,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8763,6 +1357,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8773,6 +1371,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8783,6 +1385,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8793,6 +1399,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8803,6 +1413,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8813,6 +1426,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8823,6 +1439,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8833,6 +1453,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8843,6 +1467,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8853,6 +1479,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8863,6 +1493,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8873,6 +1506,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8883,6 +1520,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -8893,6 +1533,7 @@ func (f *Forwarder) ServeHTTP(w http.ResponseWriter, r *http.Request) {
 	// If the request is for the Kubernetes API, forward it to the Kubernetes
 	// service
 	if f.IsKubernetesRequest(r) {
+		f.initSessionUploader()
 		return f.serveKubernetesRequest(w, r)
 	}
 
@@ -890