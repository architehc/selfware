#!/bin/bash
# Launch all 10 model evaluations in parallel with nohup.
set -uo pipefail

REPO="/home/habitat/selfware"
SWE_DIR="${REPO}/system_tests/swe_bench_pro"

MODELS=(
  "glm-5.2:1"
  "deepseek-v4-flash:2"
  "xiaomi-mimo-v2.5:3"
  "tencent-hy3-preview-free:4"
  "openrouter-owl-alpha:5"
  "stepfun-step-3.7-flash:6"
  "nvidia-nemotron-3-ultra-free:7"
  "poolside-laguna-xs.2-free:8"
  "qwen3-next-80b-a3b-free:9"
  "minimax-m3:10"
)

cd "${SWE_DIR}"
mkdir -p parallel_launches

for entry in "${MODELS[@]}"; do
  IFS=':' read -r MODEL IDX <<< "$entry"
  LOG="${SWE_DIR}/parallel_launches/${MODEL}.log"
  PID_FILE="${SWE_DIR}/parallel_launches/${MODEL}.pid"
  nohup "${SWE_DIR}/launch_model_agent.sh" "$MODEL" "$IDX" > "$LOG" 2>&1 &
  echo $! > "$PID_FILE"
  echo "Launched $MODEL (pid $!) -> $LOG"
done

echo "All 10 models launched. Monitor logs in ${SWE_DIR}/parallel_launches/"
