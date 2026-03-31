# Kubernetes Deployment

These manifests run Selfware as a CLI toolbox pod. They do not expose an HTTP
service or a health endpoint, so there is no `Service` manifest.

## Quick Start

1. Create the namespace:
   ```bash
   kubectl apply -f k8s/namespace.yaml
   ```

2. Deploy the toolbox pod:
   ```bash
   kubectl apply -f k8s/deployment.yaml
   ```

3. Verify and use it:
   ```bash
   kubectl -n selfware get pods
   kubectl -n selfware exec -it deployment/selfware -- selfware doctor
   kubectl -n selfware exec -it deployment/selfware -- env \
     SELFWARE_ENDPOINT=http://<your-endpoint>/v1 \
     SELFWARE_MODEL=<your-model> \
     selfware chat
   ```
