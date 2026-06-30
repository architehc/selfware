#!/bin/bash
# Launch one model on 10 SWE-bench Pro instances using standard agentless.
# Usage: launch_model_agent_10_baseline.sh <model-profile> <podman-root-index>
set -euo pipefail

MODEL_PROFILE="${1}"
ROOT_IDX="${2}"
REPO="/home/habitat/selfware"
SWE_DIR="${REPO}/system_tests/swe_bench_pro"
CONFIG_DIR="${REPO}/system_tests/projecte2e/config"
VENV="/tmp/SWE-bench_Pro-os/eval_venv/bin/python3"
PODMAN_ROOT="/tmp/podman-root-${ROOT_IDX}"
OUT_DIR="${SWE_DIR}/runs10_${MODEL_PROFILE}_baseline"
LOG_FILE="${OUT_DIR}/agent.log"
SAMPLE_FILE="${SWE_DIR}/sample_10.jsonl"
MAX_TASKS=10

mkdir -p "${OUT_DIR}"

export PATH="${PODMAN_ROOT}/bin:${PATH}"

if [ -f /tmp/selfware_api_key.env ]; then
  set -a
  source /tmp/selfware_api_key.env
  set +a
fi

cd "${SWE_DIR}"

echo "[$(date -Iseconds)] Starting model=${MODEL_PROFILE} baseline podman_root=${PODMAN_ROOT} out=${OUT_DIR}" | tee -a "${LOG_FILE}"

${VENV} run_selfware.py \
  --model-profile "${MODEL_PROFILE}" \
  --config-dir "${CONFIG_DIR}" \
  --sample-file "${SAMPLE_FILE}" \
  --max-tasks "${MAX_TASKS}" \
  --output-dir "${OUT_DIR}/out" \
  --workers 1 \
  --timeout 1800 \
  --adaptive \
  --auto-agentless \
  --retry-failures \
  --max-retries 1 \
  --diff-fallback \
  --early-diff-fallback \
  --fresh \
  2>&1 | tee -a "${LOG_FILE}" || true

echo "[$(date -Iseconds)] Predictions done; starting evaluation" | tee -a "${LOG_FILE}"

${VENV} evaluate_predictions.py \
  --predictions "${OUT_DIR}/out/predictions.jsonl" \
  --output-dir "${OUT_DIR}/eval" \
  --sample-file "${SAMPLE_FILE}" \
  --test-timeout 600 \
  2>&1 | tee -a "${LOG_FILE}" || true

echo "[$(date -Iseconds)] Evaluation done" | tee -a "${LOG_FILE}"

if [ -f "${OUT_DIR}/eval/evaluation_summary.md" ]; then
  cat "${OUT_DIR}/eval/evaluation_summary.md" | tee -a "${LOG_FILE}"
fi
