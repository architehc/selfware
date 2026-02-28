# Kubernetes Deployment

## Quick Start

1. Create the namespace:
   ```bash
   kubectl apply -f k8s/namespace.yaml
   ```

2. Create the secret (replace with your actual API key):
   ```bash
   kubectl create secret generic selfware-secrets \
     --namespace selfware \
     --from-literal=anthropic-api-key=YOUR_API_KEY
   ```

3. Deploy:
   ```bash
   kubectl apply -f k8s/configmap.yaml
   kubectl apply -f k8s/deployment.yaml
   kubectl apply -f k8s/service.yaml
   ```

4. Verify:
   ```bash
   kubectl -n selfware get pods
   kubectl -n selfware logs -f deployment/selfware
   ```
