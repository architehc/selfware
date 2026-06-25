#!/bin/bash
# Launch a matrix of v2 harness runs on cheap/free OpenRouter models.
# Each run uses an isolated podman root to avoid storage races.
# Usage: run_v2_matrix.sh
set -uo pipefail

REPO="/home/habitat/selfware"
SWE_DIR="${REPO}/system_tests/swe_bench_pro"
CONFIG_DIR="${REPO}/system_tests/projecte2e/config"
VENV="/tmp/SWE-bench_Pro-os/eval_venv/bin/python3"
SAMPLE_FILE="${SWE_DIR}/sample_50.jsonl"

# Array entries: "profile:root_idx:sample_size"
# root_idx 1 is reserved for the already-running glm-5.2 baseline.
MODELS=(
  "qwen3.5-flash-sweap:2:50"
  "deepseek-v4-flash-sweap:3:50"
  "gemma-4-26b-a4b-free-sweap:4:50"
  "qwen3-coder-free-sweap:5:50"
  "cohere-north-mini-code-free-sweap:6:10"
  "lfm-2.5-1.2b-thinking-free-sweap:7:10"
  "poolside-laguna-xs.2-free-sweap:8:10"
  "seed-2.0-mini-sweap:9:50"
  "qwen3.5-9b-sweap:10:50"
  "qwen3.5-27b-sweap:11:50"
  "gpt-5-mini:12:50"
)

cd "${SWE_DIR}"

for entry in "${MODELS[@]}"; do
  IFS=':' read -r profile root_idx n <<< "${entry}"
  out_dir="${SWE_DIR}/runs${n}_${profile}-v2"
  log_file="${out_dir}/agent.log"
  podman_root="/tmp/podman-root-${root_idx}"

  mkdir -p "${out_dir}"

  # Start fresh so that earlier failed/empty predictions are not reused.
  rm -f "${out_dir}/out/predictions.jsonl" "${out_dir}/out/predictions.json"

  export PATH="${podman_root}/bin:${PATH}"

  if [ -f /tmp/selfware_api_key.env ]; then
    set -a
    # shellcheck source=/dev/null
    source /tmp/selfware_api_key.env
    set +a
  fi

  echo "[$(date -Iseconds)] Starting model=${profile} n=${n} podman_root=${podman_root} out=${out_dir}" | tee -a "${log_file}"

  nohup bash -c "
    ${VENV} run_selfware.py \\
      --model-profile '${profile}' \\
      --config-dir '${CONFIG_DIR}' \\
      --sample-file '${SAMPLE_FILE}' \\
      --max-tasks '${n}' \\
      --output-dir '${out_dir}/out' \\
      --workers 1 \\
      --timeout 2400 \\
      --adaptive \\
      --auto-agentless \\
      --retry-failures \\
      --max-retries 1 \\
      --diff-fallback \\
      --early-diff-fallback \\
      2>&1 | tee -a '${log_file}'

    echo '[$(date -Iseconds)] Predictions done; starting evaluation' | tee -a '${log_file}'

    ${VENV} evaluate_predictions.py \\
      --predictions '${out_dir}/out/predictions.jsonl' \\
      --output-dir '${out_dir}/eval' \\
      --sample-file '${SAMPLE_FILE}' \\
      --test-timeout 600 \\
      2>&1 | tee -a '${log_file}' || true

    echo '[$(date -Iseconds)] Evaluation done' | tee -a '${log_file}'
  " >> "${log_file}" 2>&1 &
  pid=$!
  echo "[$(date -Iseconds)] Launched ${profile} as PID ${pid}" | tee -a "${log_file}"
  disown %1 || true

  # Slight stagger to avoid a thundering herd on shared registries/storage locks.
  sleep 5
done

echo "[$(date -Iseconds)] Launched ${#MODELS[@]} model runs."
