#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "${ROOT_DIR}/../.." && pwd)"
CONFIG_FILE="${ROOT_DIR}/selfware.longrun.toml"
TASK_FILE="${ROOT_DIR}/tasks/mega_tasks.txt"
PROJECT_DIR="${ROOT_DIR}/project/mega-workspace"
RUN_ID="$(date +%Y%m%d_%H%M%S)"
RUN_DIR="${ROOT_DIR}/runs/${RUN_ID}"
RESULTS_CSV="${RUN_DIR}/results.csv"

SELFWARE_BIN=""

mkdir -p "${RUN_DIR}/logs"

if command -v selfware >/dev/null 2>&1; then
  SELFWARE_BIN="$(command -v selfware)"
else
  LOCAL_BIN="${REPO_ROOT}/target/debug/selfware"
  if [[ ! -x "${LOCAL_BIN}" ]]; then
    echo "[run] selfware not found in PATH; building local binary"
    cargo build --manifest-path "${REPO_ROOT}/Cargo.toml"
  fi
  if [[ ! -x "${LOCAL_BIN}" ]]; then
    echo "[run] failed to find/build local selfware binary"
    exit 1
  fi
  SELFWARE_BIN="${LOCAL_BIN}"
fi

if [[ ! -d "${PROJECT_DIR}" ]]; then
  echo "[run] project not found at ${PROJECT_DIR}"
  echo "[run] run scripts/bootstrap_megaproject.sh first"
  exit 1
fi

echo "[run] run_id=${RUN_ID}"
echo "[run] project=${PROJECT_DIR}"
echo "[run] config=${CONFIG_FILE}"
echo "[run] tasks=${TASK_FILE}"
echo "[run] selfware_bin=${SELFWARE_BIN}"

{
  echo "task_index,status,duration_secs,task"
} > "${RESULTS_CSV}"

echo "[run] collecting baseline status"
"${SELFWARE_BIN}" --config "${CONFIG_FILE}" -C "${PROJECT_DIR}" status --output-format json \
  > "${RUN_DIR}/baseline_status.json" 2> "${RUN_DIR}/baseline_status.stderr" || true

task_index=0
while IFS= read -r task || [[ -n "${task}" ]]; do
  # Skip comments and empty lines
  if [[ -z "${task}" || "${task}" =~ ^[[:space:]]*# ]]; then
    continue
  fi

  task_index=$((task_index + 1))
  task_log="${RUN_DIR}/logs/task_${task_index}.log"

  echo "[run] task ${task_index}: ${task}"
  start_ts="$(date +%s)"

  set +e
  "${SELFWARE_BIN}" --config "${CONFIG_FILE}" --yolo -C "${PROJECT_DIR}" run "${task}" \
    > "${task_log}" 2>&1
  exit_code=$?
  set -e

  end_ts="$(date +%s)"
  duration=$((end_ts - start_ts))

  if [[ ${exit_code} -eq 0 ]]; then
    status="ok"
  else
    status="fail"
  fi

  escaped_task="${task//\"/\"\"}"
  echo "${task_index},${status},${duration},\"${escaped_task}\"" >> "${RESULTS_CSV}"
done < "${TASK_FILE}"

echo "[run] building run summary"
bash "${ROOT_DIR}/scripts/summarize_results.sh" "${RUN_DIR}"

echo "[run] done"
echo "[run] artifacts: ${RUN_DIR}"
