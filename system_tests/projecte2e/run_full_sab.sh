#!/usr/bin/env bash
# =============================================================================
# Selfware Agentic Benchmark Suite (SAB) — Parallel Runner
#
# Runs all 12 coding scenarios concurrently (up to MAX_PARALLEL jobs) against
# the Qwen3-Coder-Next-FP8 endpoint.  Monitors progress, generates a
# comprehensive markdown report.
#
# Compatible with bash 3.x (macOS default).
#
# Usage:
#   ./run_full_sab.sh                      # Run all scenarios
#   ./run_full_sab.sh expert_async_race    # Run a single scenario
# =============================================================================
set -uo pipefail

THIS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${THIS_DIR}/../.." && pwd)"
CONFIG_FILE="${CONFIG_FILE:-${THIS_DIR}/config/crazyshit_model.toml}"
CONFIG_FILE="$(cd "$(dirname "${CONFIG_FILE}")" && pwd)/$(basename "${CONFIG_FILE}")"
BIN="${REPO_ROOT}/target/release/selfware"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${THIS_DIR}/reports/${TIMESTAMP}"
WORK_ROOT="${THIS_DIR}/work"
LOG_ROOT="${OUT_DIR}/logs"
RESULTS_DIR="${OUT_DIR}/results"
SUMMARY_MD="${OUT_DIR}/REPORT.md"
PROGRESS_LOG="${OUT_DIR}/progress.log"
PID_DIR="${OUT_DIR}/pids"

MAX_PARALLEL="${MAX_PARALLEL:-6}"
POLL_INTERVAL="${POLL_INTERVAL:-300}"  # 5 minutes

# All scenarios: name:difficulty:prompt:timeout_secs:validate_cmd
ALL_SCENARIOS=(
  "easy_calculator:easy:easy_calculator.txt:240:cargo test -q"
  "easy_string_ops:easy:easy_string_ops.txt:240:cargo test -q"
  "medium_json_merge:medium:medium_json_merge.txt:300:cargo test -q"
  "medium_bitset:medium:medium_bitset.txt:300:cargo test -q"
  "hard_scheduler:hard:hard_scheduler.txt:600:cargo test -q"
  "hard_event_bus:hard:hard_event_bus.txt:900:cargo test -q"
  "expert_async_race:expert:expert_async_race.txt:900:cargo test -q"
  "security_audit:hard:security_audit.txt:600:cargo test -q"
  "perf_optimization:hard:perf_optimization.txt:600:cargo test -q"
  "codegen_task_runner:hard:codegen_task_runner.txt:600:cargo test -q"
  "testgen_ringbuf:medium:testgen_ringbuf.txt:480:cargo test -q"
  "refactor_monolith:medium:refactor_monolith.txt:600:cargo test -q"
)

# Resolve timeout command
if command -v gtimeout >/dev/null 2>&1; then
  TIMEOUT_CMD="gtimeout"
elif command -v timeout >/dev/null 2>&1; then
  TIMEOUT_CMD="timeout"
else
  echo "ERROR: 'timeout' (or 'gtimeout') required. Install coreutils." >&2
  exit 1
fi

# Filter scenarios if argument given
if [[ $# -gt 0 ]]; then
  FILTER="$1"
  FILTERED=()
  for spec in "${ALL_SCENARIOS[@]}"; do
    name="${spec%%:*}"
    if [[ "${name}" == *"${FILTER}"* ]]; then
      FILTERED+=("${spec}")
    fi
  done
  if [[ ${#FILTERED[@]} -eq 0 ]]; then
    echo "No scenarios matched filter: ${FILTER}" >&2
    exit 1
  fi
  ALL_SCENARIOS=("${FILTERED[@]}")
fi

mkdir -p "${OUT_DIR}" "${WORK_ROOT}" "${LOG_ROOT}" "${RESULTS_DIR}" "${PID_DIR}"

# ── Connectivity check ──────────────────────────────────────────────
ENDPOINT="$(grep '^endpoint' "${CONFIG_FILE}" | head -1 | sed 's/.*= *"//;s/".*//')"
MODEL_NAME="$(grep '^model' "${CONFIG_FILE}" | head -1 | sed 's/.*= *"//;s/".*//')"
echo "Checking endpoint: ${ENDPOINT}/models"
if ! curl -fsS --connect-timeout 15 "${ENDPOINT}/models" >/dev/null 2>&1; then
  echo "ERROR: Endpoint unreachable at ${ENDPOINT}/models" >&2
  exit 1
fi
echo "Endpoint OK"

# ── Build ────────────────────────────────────────────────────────────
echo ""
echo "============================================================"
echo "  SELFWARE AGENTIC BENCHMARK SUITE (SAB)"
echo "  $(date)"
echo "  Endpoint: ${ENDPOINT}"
echo "  Model: ${MODEL_NAME}"
echo "  Scenarios: ${#ALL_SCENARIOS[@]}"
echo "  Max parallel: ${MAX_PARALLEL}"
echo "  Poll interval: ${POLL_INTERVAL}s"
echo "  Output: ${OUT_DIR}"
echo "============================================================"
echo ""

echo "Building selfware (release, all features)..."
(cd "${REPO_ROOT}" && cargo build --all-features --release -q 2>&1) || {
  echo "ERROR: Build failed" >&2
  exit 1
}

if [[ ! -x "${BIN}" ]]; then
  echo "ERROR: Binary not found at ${BIN}" >&2
  exit 1
fi
echo "Build OK"
echo ""

# ── Scenario runner (called per-scenario in background) ─────────────
run_scenario() {
  local name="$1"
  local difficulty="$2"
  local prompt_file="$3"
  local timeout_secs="$4"
  local validate_cmd="$5"
  local result_file="${RESULTS_DIR}/${name}.json"
  local log_dir="${LOG_ROOT}/${name}"
  local template_dir="${THIS_DIR}/templates/${name}"
  local work_dir="${WORK_ROOT}/${name}"

  mkdir -p "${log_dir}"
  rm -rf "${work_dir}"
  mkdir -p "${work_dir}"
  cp -R "${template_dir}/." "${work_dir}/"

  # Remove any leftover target dir from template
  rm -rf "${work_dir}/target" "${work_dir}/Cargo.lock"

  # Baseline validation (with timeout to prevent hanging on slow algorithms)
  local baseline_status=0
  (cd "${work_dir}" && "${TIMEOUT_CMD}" 120 ${validate_cmd}) > "${log_dir}/baseline.log" 2>&1 || baseline_status=$?

  # Run agent
  local start_ts
  start_ts="$(date +%s)"

  local agent_status=0
  "${TIMEOUT_CMD}" "${timeout_secs}" "${BIN}" \
    --config "${CONFIG_FILE}" \
    -C "${work_dir}" \
    -y \
    -p - < "${THIS_DIR}/prompts/${prompt_file}" > "${log_dir}/agent.log" 2>&1 || agent_status=$?

  local end_ts
  end_ts="$(date +%s)"
  local duration=$((end_ts - start_ts))
  local timed_out=0
  [[ ${agent_status} -eq 124 ]] && timed_out=1

  # Post validation (with timeout to prevent hanging on slow algorithms)
  local post_status=0
  (cd "${work_dir}" && "${TIMEOUT_CMD}" 120 ${validate_cmd}) > "${log_dir}/post.log" 2>&1 || post_status=$?

  # Count changes
  local changed_files
  changed_files="$(diff -qr "${template_dir}" "${work_dir}" 2>/dev/null | grep -v target | grep -v Cargo.lock | wc -l | tr -d ' ')" || changed_files=0

  # Error analysis
  local error_hits
  error_hits="$(grep -Eic 'error|failed|panic|timed.out|safety.check.failed' "${log_dir}/agent.log" 2>/dev/null)" || error_hits=0

  grep -Ein 'error|failed|panic|timed.out|safety.check|unknown.tool' \
    "${log_dir}/agent.log" 2>/dev/null | head -50 > "${log_dir}/error_highlights.log" || true

  # Scoring
  local score=0
  [[ ${post_status} -eq 0 ]] && score=$((score + 70))
  [[ ${baseline_status} -ne 0 && ${post_status} -eq 0 ]] && score=$((score + 20))
  [[ ${agent_status} -eq 0 && ${timed_out} -eq 0 ]] && score=$((score + 10))

  # Rating
  local rating="FROST"
  [[ ${score} -ge 30 ]] && rating="WILT"
  [[ ${score} -ge 60 ]] && rating="GROW"
  [[ ${score} -ge 85 ]] && rating="BLOOM"

  # Write result JSON
  cat > "${result_file}" <<EOJSON
{
  "name": "${name}",
  "difficulty": "${difficulty}",
  "baseline_status": ${baseline_status},
  "post_status": ${post_status},
  "agent_status": ${agent_status},
  "timed_out": ${timed_out},
  "duration_secs": ${duration},
  "score": ${score},
  "rating": "${rating}",
  "changed_files": ${changed_files},
  "error_hits": ${error_hits},
  "completed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOJSON

  echo "[$(date +%H:%M:%S)] ${name}: score=${score}/100 rating=${rating} duration=${duration}s" >> "${PROGRESS_LOG}"
}

# ── Launch all scenarios in parallel ─────────────────────────────────
echo "Launching ${#ALL_SCENARIOS[@]} scenarios (max ${MAX_PARALLEL} parallel)..."
echo ""

PIDS=""
QUEUE_IDX=0

launch_next() {
  if [[ ${QUEUE_IDX} -ge ${#ALL_SCENARIOS[@]} ]]; then
    return 1
  fi
  local spec="${ALL_SCENARIOS[${QUEUE_IDX}]}"
  IFS=':' read -r name difficulty prompt timeout_secs validate_cmd <<< "${spec}"
  QUEUE_IDX=$((QUEUE_IDX + 1))

  echo "  [$(date +%H:%M:%S)] Starting: ${name} (${difficulty}, timeout=${timeout_secs}s)"
  run_scenario "${name}" "${difficulty}" "${prompt}" "${timeout_secs}" "${validate_cmd}" &
  local pid=$!
  echo "${name}" > "${PID_DIR}/${pid}"
  PIDS="${PIDS} ${pid}"
  return 0
}

# Fill initial batch
count=0
while [[ ${count} -lt ${MAX_PARALLEL} ]]; do
  launch_next || break
  count=$((count + 1))
done

echo ""
echo "All ${count} scenarios launched. Monitoring every ${POLL_INTERVAL}s..."
echo ""

# ── Monitor loop ─────────────────────────────────────────────────────
SAB_START="$(date +%s)"
LAST_POLL="${SAB_START}"

while true; do
  # Check if any PIDs are still running
  still_running=""
  for pid in ${PIDS}; do
    if kill -0 "${pid}" 2>/dev/null; then
      still_running="${still_running} ${pid}"
    else
      wait "${pid}" 2>/dev/null || true
      # Show completion
      if [[ -f "${PID_DIR}/${pid}" ]]; then
        finished_name="$(cat "${PID_DIR}/${pid}")"
        rm -f "${PID_DIR}/${pid}"
        rf="${RESULTS_DIR}/${finished_name}.json"
        if [[ -f "${rf}" ]]; then
          result_line="$(python3 -c "import json; d=json.load(open('${rf}')); print(f'{d[\"rating\"]:5s} {d[\"score\"]:3d}/100 in {d[\"duration_secs\"]}s')" 2>/dev/null || echo "?")"
          echo "  [$(date +%H:%M:%S)] DONE: ${finished_name} -> ${result_line}"
        else
          echo "  [$(date +%H:%M:%S)] DONE: ${finished_name} -> (no result file)"
        fi
        # Launch next if queued
        if launch_next 2>/dev/null; then
          new_pid="$(echo ${PIDS} | tr ' ' '\n' | tail -1)"
          still_running="${still_running} ${new_pid}"
        fi
      fi
    fi
  done
  PIDS="${still_running}"

  # Exit if nothing running
  if [[ -z "${PIDS}" ]]; then
    break
  fi

  NOW="$(date +%s)"
  # Progress report every POLL_INTERVAL
  if [[ $((NOW - LAST_POLL)) -ge ${POLL_INTERVAL} ]]; then
    LAST_POLL="${NOW}"
    ELAPSED=$(( NOW - SAB_START ))
    COMPLETED_COUNT=$(ls "${RESULTS_DIR}"/*.json 2>/dev/null | wc -l | tr -d ' ')
    TOTAL="${#ALL_SCENARIOS[@]}"
    RUNNING_COUNT=$(echo ${PIDS} | wc -w | tr -d ' ')

    echo ""
    echo "──── Progress Report $(date +%H:%M:%S) ────"
    echo "  Elapsed: $((ELAPSED / 60))m $((ELAPSED % 60))s"
    echo "  Completed: ${COMPLETED_COUNT}/${TOTAL}"
    echo "  Running: ${RUNNING_COUNT} jobs"

    if [[ ${COMPLETED_COUNT} -gt 0 ]]; then
      echo "  Results so far:"
      for rf in "${RESULTS_DIR}"/*.json; do
        [[ -f "${rf}" ]] || continue
        python3 -c "import json; d=json.load(open('${rf}')); print(f'    {d[\"name\"]:30s} {d[\"rating\"]:5s} {d[\"score\"]:3d}/100  {d[\"duration_secs\"]}s')" 2>/dev/null || true
      done
    fi

    # Check endpoint health
    if curl -fsS --connect-timeout 5 "${ENDPOINT}/models" >/dev/null 2>&1; then
      echo "  Endpoint: healthy"
    else
      echo "  Endpoint: UNREACHABLE"
    fi
    echo ""
  fi

  sleep 10
done

# ── Generate comprehensive report ───────────────────────────────────
echo ""
echo "All scenarios complete. Generating report..."

SAB_END="$(date +%s)"
TOTAL_ELAPSED=$((SAB_END - SAB_START))

# Aggregate results via python
python3 -c "
import json, os, glob, sys

results_dir = '${RESULTS_DIR}'
log_root = '${LOG_ROOT}'
timestamp = '${TIMESTAMP}'
endpoint = '${ENDPOINT}'
model_name = '${MODEL_NAME}'
token_budget = int('$(grep "^token_budget" "${CONFIG_FILE}" | head -1 | sed "s/[^0-9]//g")' or '0')
total_elapsed = ${TOTAL_ELAPSED}
all_scenarios = '''$(printf '%s\n' "${ALL_SCENARIOS[@]}")'''.strip().split('\n')

results = []
for rf in sorted(glob.glob(os.path.join(results_dir, '*.json'))):
    with open(rf) as f:
        results.append(json.load(f))

total = len(all_scenarios)
completed = len(results)
passed = sum(1 for r in results if r['post_status'] == 0)
total_score = sum(r['score'] for r in results)
avg_score = total_score // completed if completed > 0 else 0

bloom = sum(1 for r in results if r['rating'] == 'BLOOM')
grow = sum(1 for r in results if r['rating'] == 'GROW')
wilt = sum(1 for r in results if r['rating'] == 'WILT')
frost = sum(1 for r in results if r['rating'] == 'FROST')

if avg_score >= 85: overall = 'BLOOM'
elif avg_score >= 60: overall = 'GROW'
elif avg_score >= 30: overall = 'WILT'
else: overall = 'FROST'

icon_map = {'BLOOM': '🌸', 'GROW': '🌿', 'WILT': '🥀', 'FROST': '❄️'}

lines = []
lines.append('# Selfware Agentic Benchmark Suite (SAB) Report')
lines.append('')
lines.append('## Summary')
lines.append('')
lines.append('| Metric | Value |')
lines.append('|--------|-------|')
lines.append(f'| Date | {timestamp} |')
lines.append(f'| Model | {model_name} |')
lines.append(f'| Endpoint | {endpoint} |')
lines.append(f'| Max Context | {token_budget:,} tokens |')
lines.append(f'| Total Scenarios | {total} |')
lines.append(f'| Completed | {completed} |')
lines.append(f'| Passed (tests green) | {passed}/{completed} |')
lines.append(f'| Average Score | {avg_score}/100 |')
lines.append(f'| Overall Rating | **{icon_map.get(overall, \"?\")} {overall}** |')
lines.append(f'| Total Duration | {total_elapsed // 60}m {total_elapsed % 60}s |')
lines.append('')
lines.append('### Rating Distribution')
lines.append('')
lines.append('| Rating | Count | Description |')
lines.append('|--------|-------|-------------|')
lines.append(f'| 🌸 BLOOM | {bloom} | Ship it. Model handles this reliably. |')
lines.append(f'| 🌿 GROW | {grow} | Usable with occasional human review. |')
lines.append(f'| 🥀 WILT | {wilt} | Model struggles. Needs prompt tuning. |')
lines.append(f'| ❄️ FROST | {frost} | Not ready for this task class. |')
lines.append('')
lines.append('## Detailed Results')
lines.append('')
lines.append('| Scenario | Difficulty | Score | Rating | Duration | Baseline | Post | Agent Exit | Timeout | Changed | Errors |')
lines.append('|----------|-----------|-------|--------|----------|----------|------|------------|---------|---------|--------|')
for r in sorted(results, key=lambda x: -x['score']):
    icon = icon_map.get(r['rating'], '?')
    lines.append(f'| \`{r[\"name\"]}\` | {r[\"difficulty\"]} | {r[\"score\"]}/100 | {icon} {r[\"rating\"]} | {r[\"duration_secs\"]}s | {r[\"baseline_status\"]} | {r[\"post_status\"]} | {r[\"agent_status\"]} | {r[\"timed_out\"]} | {r[\"changed_files\"]} | {r[\"error_hits\"]} |')

lines.append('')
lines.append('## Category Breakdown')
lines.append('')
for diff in ['easy', 'medium', 'hard', 'expert']:
    group = [r for r in results if r['difficulty'] == diff]
    if group:
        g_pass = sum(1 for r in group if r['post_status'] == 0)
        g_avg = sum(r['score'] for r in group) // len(group)
        lines.append(f'### {diff.title()} ({g_pass}/{len(group)} passed, avg {g_avg}/100)')
        lines.append('')
        for r in group:
            icon = icon_map.get(r['rating'], '?')
            lines.append(f'- \`{r[\"name\"]}\`: {icon} {r[\"score\"]}/100 in {r[\"duration_secs\"]}s')
        lines.append('')

lines.append('## Error Highlights')
lines.append('')
for spec in all_scenarios:
    name = spec.split(':')[0]
    ef = os.path.join(log_root, name, 'error_highlights.log')
    lines.append(f'### {name}')
    if os.path.exists(ef) and os.path.getsize(ef) > 0:
        lines.append('\`\`\`')
        with open(ef) as f:
            for i, line in enumerate(f):
                if i >= 30: break
                lines.append(line.rstrip())
        lines.append('\`\`\`')
    else:
        lines.append('No significant errors captured.')
    lines.append('')

lines.append('## Progress Timeline')
lines.append('')
lines.append('\`\`\`')
plog = os.path.join(os.path.dirname(results_dir), 'progress.log')
if os.path.exists(plog):
    with open(plog) as f:
        lines.append(f.read().rstrip())
else:
    lines.append('(no progress log)')
lines.append('\`\`\`')
lines.append('')
lines.append('## Artifacts')
lines.append('')
lines.append(f'- Report: \`system_tests/projecte2e/reports/{timestamp}/REPORT.md\`')
lines.append(f'- Results: \`system_tests/projecte2e/reports/{timestamp}/results/\`')
lines.append(f'- Logs: \`system_tests/projecte2e/reports/{timestamp}/logs/<scenario>/\`')

with open('${SUMMARY_MD}', 'w') as f:
    f.write('\n'.join(lines) + '\n')

print(f'Report written to ${SUMMARY_MD}')
print(f'')
print(f'============================================================')
print(f'  SAB RUN COMPLETE')
print(f'')
print(f'  Scenarios: {completed}/{total}')
print(f'  Passed:    {passed}/{completed}')
print(f'  Score:     {avg_score}/100')
print(f'  Rating:    {icon_map.get(overall, \"?\")} {overall}')
print(f'')
print(f'  🌸 BLOOM: {bloom}  🌿 GROW: {grow}  🥀 WILT: {wilt}  ❄️ FROST: {frost}')
print(f'')
print(f'  Duration:  {total_elapsed // 60}m {total_elapsed % 60}s')
print(f'============================================================')
" || echo "ERROR: Report generation failed"

# Symlink latest
ln -sfn "${OUT_DIR}" "${THIS_DIR}/reports/latest"
