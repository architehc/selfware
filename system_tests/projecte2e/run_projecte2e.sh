#!/usr/bin/env bash
set -euo pipefail

THIS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${THIS_DIR}/../.." && pwd)"
CONFIG_FILE="${CONFIG_FILE:-${THIS_DIR}/config/local_model.toml}"
# Resolve to absolute path so it works after cd into work dirs
CONFIG_FILE="$(cd "$(dirname "${CONFIG_FILE}")" && pwd)/$(basename "${CONFIG_FILE}")"
BIN="${REPO_ROOT}/target/release/selfware"
# Timeout multiplier for slower inference backends (e.g., local CPU/GPU inference)
# Usage: TIMEOUT_MULTIPLIER=4 CONFIG_FILE=... ./run_projecte2e.sh
TIMEOUT_MULTIPLIER="${TIMEOUT_MULTIPLIER:-1}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${THIS_DIR}/reports/${TIMESTAMP}"
WORK_ROOT="${THIS_DIR}/work"
LOG_ROOT="${OUT_DIR}/logs"
SCREENSHOT_DIR="${OUT_DIR}/screenshots"
RESULTS_TSV="${OUT_DIR}/results.tsv"
SUMMARY_MD="${OUT_DIR}/summary.md"

CODING_SCENARIOS=(
  "easy_calculator:easy:easy_calculator.txt:240"
  "easy_string_ops:easy:easy_string_ops.txt:240"
  "medium_json_merge:medium:medium_json_merge.txt:300"
  "medium_bitset:medium:medium_bitset.txt:300"
  "hard_scheduler:hard:hard_scheduler.txt:360"
  "hard_event_bus:hard:hard_event_bus.txt:420"
)

mkdir -p "${OUT_DIR}" "${WORK_ROOT}" "${LOG_ROOT}" "${SCREENSHOT_DIR}"

# Resolve timeout command (GNU coreutils on macOS installs as gtimeout)
if command -v timeout >/dev/null 2>&1; then
  TIMEOUT_CMD="timeout"
elif command -v gtimeout >/dev/null 2>&1; then
  TIMEOUT_CMD="gtimeout"
else
  echo "ERROR: 'timeout' (or 'gtimeout') command is required. Install coreutils." >&2
  exit 1
fi

# Extract endpoint and model from config to verify connectivity
ENDPOINT="$(grep '^endpoint' "${CONFIG_FILE}" | head -1 | sed 's/.*= *"//;s/".*//')"
MODEL_NAME="$(grep '^model' "${CONFIG_FILE}" | head -1 | sed 's/.*= *"//;s/".*//')"
if ! curl -fsS --connect-timeout 10 "${ENDPOINT}/models" >/dev/null; then
  echo "ERROR: Model endpoint is unreachable at ${ENDPOINT}/models" >&2
  exit 1
fi

echo "=============================================="
echo "  Selfware E2E Test Suite"
echo "  $(date)"
echo "  Model: ${MODEL_NAME}"
echo "  Endpoint: ${ENDPOINT}"
echo "  Timeout multiplier: ${TIMEOUT_MULTIPLIER}x"
echo "=============================================="
echo ""

echo "Building selfware with all features..."
(
  cd "${REPO_ROOT}"
  cargo build --all-features --release -q
)

if [[ ! -x "${BIN}" ]]; then
  echo "ERROR: selfware binary not found at ${BIN}" >&2
  exit 1
fi

echo "scenario|type|difficulty|baseline_status|post_status|agent_status|timed_out|duration_secs|score|changed_files|error_hits|notes" > "${RESULTS_TSV}"

# ── Coding scenario runner ───────────────────────────────────────────
run_coding_scenario() {
  local name="$1"
  local difficulty="$2"
  local prompt_file="$3"
  local validate_cmd="$4"
  local timeout_secs="$5"

  local template_dir="${THIS_DIR}/templates/${name}"
  local work_dir="${WORK_ROOT}/${name}"
  local log_dir="${LOG_ROOT}/${name}"

  echo ""
  echo "──────────────────────────────────────────"
  echo "  [$(echo "${difficulty}" | tr '[:lower:]' '[:upper:]')] ${name}"
  echo "──────────────────────────────────────────"

  rm -rf "${work_dir}" "${log_dir}"
  mkdir -p "${work_dir}" "${log_dir}"
  cp -R "${template_dir}/." "${work_dir}/"

  # Baseline: run validation before the agent
  set +e
  (
    cd "${work_dir}"
    bash -lc "${validate_cmd}"
  ) > "${log_dir}/baseline.log" 2>&1
  local baseline_status=$?
  set -e
  echo "  Baseline: exit=${baseline_status}"

  # Baseline sanity check (P0-4): every coding scenario seeds a deliberate
  # bug, so the baseline MUST be red. A green baseline means the seeded bug
  # is gone — e.g. the agent under test committed its fix back into the
  # template (the [AGENT CHECKPOINT] corruption) — and the scenario can
  # prove nothing about the agent. Fail loudly and force score 0 / FAIL
  # below; never award the 70/100 "pass" for a green baseline.
  local baseline_green=0
  if [[ ${baseline_status} -eq 0 ]]; then
    baseline_green=1
    echo "  *** BASELINE SANITY FAILURE: ${name} baseline is already GREEN — seeded bug missing (template corruption). Scenario INVALID: forced score 0 / FAIL regardless of agent outcome. ***" >&2
  fi

  # Run agent with terminal capture via `script`
  local start_ts
  start_ts="$(date +%s)"
  set +e
  local effective_timeout=$((timeout_secs * TIMEOUT_MULTIPLIER))
  "${TIMEOUT_CMD}" "${effective_timeout}" "${BIN}" \
    --config "${CONFIG_FILE}" \
    -C "${work_dir}" \
    -y \
    -p - < "${THIS_DIR}/prompts/${prompt_file}" > "${log_dir}/agent.log" 2>&1
  local agent_status=$?
  set -e
  local end_ts
  end_ts="$(date +%s)"
  local duration_secs=$((end_ts - start_ts))

  local timed_out=0
  if [[ ${agent_status} -eq 124 ]]; then
    timed_out=1
  fi

  echo "  Agent: exit=${agent_status} duration=${duration_secs}s timed_out=${timed_out}"

  # Post-validation: run same validation after agent
  set +e
  (
    cd "${work_dir}"
    bash -lc "${validate_cmd}"
  ) > "${log_dir}/post.log" 2>&1
  local post_status=$?
  set -e
  echo "  Post:  exit=${post_status}"

  local changed_files
  changed_files="$( (diff -qr "${template_dir}" "${work_dir}" 2>/dev/null || true) | wc -l | tr -d ' ')"

  local error_hits
  error_hits="$(grep -Eic "error|failed|panic|timed out|safety check failed|invalid" "${log_dir}/agent.log" || true)"

  grep -Ein "error|failed|panic|timed out|safety check failed|invalid|unknown tool" "${log_dir}/agent.log" | head -n 80 > "${log_dir}/error_highlights.log" || true

  # Scoring: 70 for passing tests, 20 bonus for fixing broken tests, 10 for
  # clean exit. A green-baseline scenario is INVALID (see sanity check
  # above) and scores 0 no matter what the agent did.
  local score=0
  if [[ ${post_status} -eq 0 && ${baseline_green} -eq 0 ]]; then
    score=$((score + 70))
  fi
  if [[ ${baseline_status} -ne 0 && ${post_status} -eq 0 ]]; then
    score=$((score + 20))
  fi
  if [[ ${agent_status} -eq 0 && ${timed_out} -eq 0 && ${baseline_green} -eq 0 ]]; then
    score=$((score + 10))
  fi

  local notes=""
  if [[ ${baseline_green} -eq 1 ]]; then
    notes="baseline_green_INVALID_template_corruption"
  fi
  if [[ ${timed_out} -eq 1 ]]; then
    notes="${notes:+${notes},}agent_timeout"
  fi

  local result_icon="FAIL"
  [[ ${post_status} -eq 0 && ${baseline_green} -eq 0 ]] && result_icon="PASS"
  echo "  Result: ${result_icon} (score=${score}/100)"

  echo "${name}|coding|${difficulty}|${baseline_status}|${post_status}|${agent_status}|${timed_out}|${duration_secs}|${score}|${changed_files}|${error_hits}|${notes}" >> "${RESULTS_TSV}"
}

# ── Swarm scenario runner ────────────────────────────────────────────
run_swarm_scenario() {
  local name="swarm_session"
  local log_dir="${LOG_ROOT}/${name}"
  mkdir -p "${log_dir}"

  echo ""
  echo "──────────────────────────────────────────"
  echo "  [SWARM] multi-chat session"
  echo "──────────────────────────────────────────"

  # Headless one-shot (P0-4): piping stdin into `multi-chat` fast-fails on
  # non-TTY — the interactive REPL requires a terminal, so this scenario
  # could never pass. Use the headless one-shot mode
  # (`selfware multi-chat "task"`) instead. The prompt file is written for
  # the interactive REPL (slash-commands and `exit` lines), so derive the
  # one-shot task from its natural-language lines only.
  local swarm_task
  swarm_task="$(grep -vE '^[[:space:]]*(/|exit[[:space:]]*$)' "${THIS_DIR}/prompts/swarm_session.txt" | tr '\n' ' ' | sed -E 's/[[:space:]]+/ /g;s/^ //;s/ $//')"
  if [[ -z "${swarm_task}" ]]; then
    echo "ERROR: could not derive a one-shot task from prompts/swarm_session.txt" >&2
    exit 1
  fi
  echo "  Task: ${swarm_task}"

  local start_ts
  start_ts="$(date +%s)"
  set +e
  local swarm_timeout=$((240 * TIMEOUT_MULTIPLIER))
  "${TIMEOUT_CMD}" "${swarm_timeout}" "${BIN}" \
    --config "${CONFIG_FILE}" \
    -C "${REPO_ROOT}" \
    -y \
    multi-chat -n 4 "${swarm_task}" > "${log_dir}/agent.log" 2>&1
  local agent_status=$?
  set -e
  local end_ts
  end_ts="$(date +%s)"
  local duration_secs=$((end_ts - start_ts))

  local timed_out=0
  [[ ${agent_status} -eq 124 ]] && timed_out=1

  # One-shot output markers: each completed agent prints a result line like
  #   "  ✓ Agent-0-Coder (Coder) — 1.23s"
  # under an "Agent Results:" header, and a successful fan-out ends with an
  # "Aggregated Result:" section (the one-shot analogue of the old
  # interactive-mode "Added Agent-"/"Status:" markers).
  local spawned_count
  spawned_count="$(grep -Ec "Agent-[0-9]+-" "${log_dir}/agent.log" || true)"
  local aggregated
  aggregated="$(grep -c "Aggregated Result:" "${log_dir}/agent.log" || true)"
  local error_hits
  error_hits="$(grep -Eic "error|failed|panic|timed out|invalid" "${log_dir}/agent.log" || true)"

  grep -Ein "error|failed|panic|timed out|invalid|unknown" "${log_dir}/agent.log" | head -n 80 > "${log_dir}/error_highlights.log" || true

  local score=0
  if [[ ${spawned_count} -ge 2 ]]; then
    score=$((score + 40))
  fi
  if [[ ${aggregated} -ge 1 ]]; then
    score=$((score + 30))
  fi
  if [[ ${agent_status} -eq 0 && ${timed_out} -eq 0 ]]; then
    score=$((score + 30))
  fi

  echo "  Agent: exit=${agent_status} duration=${duration_secs}s spawned=${spawned_count} aggregated=${aggregated}"
  echo "  Result: score=${score}/100"

  local notes="spawned=${spawned_count},aggregated=${aggregated}"
  if [[ ${timed_out} -eq 1 ]]; then
    notes="${notes},agent_timeout"
  fi

  echo "${name}|swarm|n/a|n/a|n/a|${agent_status}|${timed_out}|${duration_secs}|${score}|${spawned_count}|${error_hits}|${notes}" >> "${RESULTS_TSV}"
}

# ── Run all scenarios ────────────────────────────────────────────────
for spec in "${CODING_SCENARIOS[@]}"; do
  IFS=':' read -r name difficulty prompt timeout_secs <<< "${spec}"
  run_coding_scenario "${name}" "${difficulty}" "${prompt}" "cargo test -q" "${timeout_secs}"
done

run_swarm_scenario

# ── Generate report ──────────────────────────────────────────────────
coding_count="${#CODING_SCENARIOS[@]}"
avg_score="$(awk -F'|' 'NR>1{sum+=$9;n++} END{if(n==0){print 0}else{printf "%.1f", sum/n}}' "${RESULTS_TSV}")"
# A pass requires a green post-validation AND a red baseline ($4): a
# green-baseline scenario is invalid (template corruption) and never
# counts as a pass (P0-4).
pass_count="$(awk -F'|' 'NR>1 && $5==0 && $4+0!=0 {n++} END{print n+0}' "${RESULTS_TSV}")"
scenario_count="$(awk -F'|' 'NR>1{n++} END{print n+0}' "${RESULTS_TSV}")"
invalid_count="$(awk -F'|' 'NR>1 && $2=="coding" && $4+0==0 {n++} END{print n+0}' "${RESULTS_TSV}")"

rating="Poor"
awk -v s="${avg_score}" 'BEGIN{if(s>=85) print "Excellent"; else if(s>=70) print "Good"; else if(s>=50) print "Fair"; else print "Poor"}' > "${OUT_DIR}/rating.txt"
rating="$(cat "${OUT_DIR}/rating.txt")"

ALL_SCENARIOS=()
for spec in "${CODING_SCENARIOS[@]}"; do
  IFS=':' read -r name _ _ _ <<< "${spec}"
  ALL_SCENARIOS+=("${name}")
done
ALL_SCENARIOS+=("swarm_session")

{
  echo "# Selfware Project E2E Report"
  echo
  echo "- Timestamp: ${TIMESTAMP}"
  echo "- Model endpoint: ${ENDPOINT}"
  echo "- Model: ${MODEL_NAME}"
  echo "- Binary: \`target/release/selfware\` built with \`--all-features\`"
  echo "- Coding scenarios: ${pass_count}/${coding_count} passed"
  if [[ ${invalid_count} -gt 0 ]]; then
    echo "- ⚠ INVALID scenarios (baseline already GREEN — seeded bug missing, template corruption; forced to score 0): ${invalid_count}"
  fi
  echo "- Total scenarios: ${scenario_count}"
  echo "- Overall score: ${avg_score}/100"
  echo "- Performance rating: **${rating}**"
  echo
  echo "## Scenario Results"
  echo
  echo "| Scenario | Type | Difficulty | Baseline | Post | Agent Exit | Timeout | Duration (s) | Score | Delta/Spawn | Error Hits | Notes |"
  echo "|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|"
  awk -F'|' 'NR>1 {printf "| `%s` | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n", $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12}' "${RESULTS_TSV}"
  echo
  echo "## Error Highlights"
  echo
  for scenario in "${ALL_SCENARIOS[@]}"; do
    echo "### ${scenario}"
    local_file="${LOG_ROOT}/${scenario}/error_highlights.log"
    if [[ -s "${local_file}" ]]; then
      echo '```text'
      head -n 40 "${local_file}"
      echo '```'
    else
      echo "No high-signal error lines captured."
    fi
    echo
  done

  echo "## Terminal Screenshots"
  echo
  echo "ANSI typescript recordings saved in \`screenshots/\` directory."
  echo "View with: \`cat screenshots/<name>.typescript\` or replay with \`scriptreplay\`."
  echo
  for f in "${SCREENSHOT_DIR}"/*.typescript; do
    if [[ -f "$f" ]]; then
      echo "- \`$(basename "$f")\` ($(du -h "$f" | cut -f1))"
    fi
  done
  echo

  echo "## Artifacts"
  echo
  echo "- Results TSV: \`reports/${TIMESTAMP}/results.tsv\`"
  echo "- Full logs: \`reports/${TIMESTAMP}/logs/\`"
  echo "- Screenshots: \`reports/${TIMESTAMP}/screenshots/\`"
  echo "- Scenario workdirs after run: \`work/\`"
} > "${SUMMARY_MD}"

ln -sfn "${OUT_DIR}" "${THIS_DIR}/reports/latest"

echo ""
echo "=============================================="
echo "  E2E RUN COMPLETE"
echo "  Coding pass: ${pass_count}/${coding_count}"
if [[ ${invalid_count} -gt 0 ]]; then
  echo "  ⚠ WARNING: ${invalid_count} coding scenario(s) had GREEN baselines and were"
  echo "    INVALIDATED (template corruption — seeded bug missing). Their scores were"
  echo "    forced to 0; restore the broken baselines before trusting this report." >&2
fi
echo "  Overall score: ${avg_score}/100"
echo "  Rating: ${rating}"
echo "  Report: ${SUMMARY_MD}"
echo "=============================================="
