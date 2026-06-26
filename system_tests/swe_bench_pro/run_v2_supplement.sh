#!/bin/bash
# Supplementary v2 runs replacing rate-limited free models with cheap paid ones.
set -uo pipefail

REPO="/home/habitat/selfware"
SWE_DIR="${REPO}/system_tests/swe_bench_pro"
CONFIG_DIR="${REPO}/system_tests/projecte2e/config"
VENV="/tmp/SWE-bench_Pro-os/eval_venv/bin/python3"
SAMPLE_FILE="${SWE_DIR}/sample_50.jsonl"

# profile:root_idx:sample_size
MODELS=(
  "amazon-nova-lite-sweap:13:50"
  "qwen3-coder-30b-a3b-instruct-sweap:14:50"
  "gemma-4-31b-sweap:15:50"
  "qwen2.5-7b:16:10"
)

cd "${SWE_DIR}"

for entry in "${MODELS[@]}"; do
  IFS=':' read -r profile root_idx n <<< "${entry}"
  out_dir="${SWE_DIR}/runs${n}_${profile}-v2"
  log_file="${out_dir}/agent.log"
  podman_root="/tmp/podman-root-${root_idx}"

  mkdir -p "${out_dir}"
  rm -f "${out_dir}/out/predictions.jsonl" "${out_dir}/out/predictions.json"

  export PATH="${podman_root}/bin:${PATH}"

  if [ -f /tmp/selfware_api_key.env ]; then
    set -a
    source /tmp/selfware_api_key.env
    set +a
  fi

  echo "[$(date -Iseconds)] Starting supplement model=${profile} n=${n} podman_root=${podman_root} out=${out_dir}" | tee -a "${log_file}"

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

  sleep 5
done

echo "[$(date -Iseconds)] Launched ${#MODELS[@]} supplementary model runs."
