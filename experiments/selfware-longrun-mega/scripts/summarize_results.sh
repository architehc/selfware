#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <run_dir>"
  exit 1
fi

RUN_DIR="$1"
RESULTS_CSV="${RUN_DIR}/results.csv"
SUMMARY_MD="${RUN_DIR}/summary.md"

if [[ ! -f "${RESULTS_CSV}" ]]; then
  echo "[summary] missing ${RESULTS_CSV}"
  exit 1
fi

total_tasks="$(tail -n +2 "${RESULTS_CSV}" | wc -l | tr -d ' ')"
ok_tasks="$(tail -n +2 "${RESULTS_CSV}" | awk -F, '$2=="ok"{c++} END{print c+0}')"
fail_tasks="$(tail -n +2 "${RESULTS_CSV}" | awk -F, '$2=="fail"{c++} END{print c+0}')"
total_secs="$(tail -n +2 "${RESULTS_CSV}" | awk -F, '{s+=$3} END{print s+0}')"

if [[ "${total_tasks}" -gt 0 ]]; then
  avg_secs="$((total_secs / total_tasks))"
else
  avg_secs=0
fi

recovery_signals="$(grep -R -h -E 'ErrorRecovery|recovery|Self-healing|retry' "${RUN_DIR}/logs" 2>/dev/null | wc -l | tr -d ' ')"
safety_blocks="$(grep -R -h -E 'Safety check failed|blocked' "${RUN_DIR}/logs" 2>/dev/null | wc -l | tr -d ' ')"

run_id="$(basename "${RUN_DIR}")"
generated_at="$(date -u +"%Y-%m-%d %H:%M:%S UTC")"

cat > "${SUMMARY_MD}" <<EOF
# Long-Run Summary (${run_id})

Generated: ${generated_at}

## Quantitative Metrics

- Total tasks: ${total_tasks}
- Successful tasks: ${ok_tasks}
- Failed tasks: ${fail_tasks}
- Total duration (secs): ${total_secs}
- Average task duration (secs): ${avg_secs}
- Recovery-related log signals: ${recovery_signals}
- Safety block signals: ${safety_blocks}

## Task Outcomes

| Task # | Status | Duration (s) | Task |
|---|---|---:|---|
EOF

tail -n +2 "${RESULTS_CSV}" | while IFS=, read -r idx status duration task; do
  clean_task="${task%\"}"
  clean_task="${clean_task#\"}"
  printf "| %s | %s | %s | %s |\n" "${idx}" "${status}" "${duration}" "${clean_task}" >> "${SUMMARY_MD}"
done

cat >> "${SUMMARY_MD}" <<'EOF'

## Improvement Notes

- What felt faster in this run?
- Where did the agent stall or loop?
- Which command/output patterns caused confusion?
- What should change in config, prompts, or workflow before next run?
EOF

echo "[summary] wrote ${SUMMARY_MD}"
