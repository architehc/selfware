#!/usr/bin/env bash
set -euo pipefail

# ════════════════════════════════════════════════════════════════════
#  Selfware Full System Test Suite with 30-second Monitoring
#  Expected duration: ~2-3 hours
# ════════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
SESSION_DIR="${REPO_ROOT}/test_runs/system-${TIMESTAMP}"
LOG_FILE="${SESSION_DIR}/master.log"
PROGRESS_FILE="${SESSION_DIR}/progress.json"
MONITOR_INTERVAL=30

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

mkdir -p "${SESSION_DIR}/phases"

# ── Prerequisite checks ─────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
    echo "ERROR: cargo is not installed or not in PATH" >&2
    exit 1
fi

if ! command -v python3 &>/dev/null; then
    echo "WARNING: python3 not found; monitor output may be degraded" >&2
fi

if ! command -v bc &>/dev/null; then
    echo "WARNING: bc not found; pass-rate calculation will show 0" >&2
fi

# ── Logging ──────────────────────────────────────────────────────────
log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1" | tee -a "${LOG_FILE}"; }
log_ok()   { echo -e "${GREEN}[$(date +%H:%M:%S)] OK${NC}   $1" | tee -a "${LOG_FILE}"; }
log_fail() { echo -e "${RED}[$(date +%H:%M:%S)] FAIL${NC} $1" | tee -a "${LOG_FILE}"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] WARN${NC} $1" | tee -a "${LOG_FILE}"; }

# ── Progress tracking ───────────────────────────────────────────────
GLOBAL_START=$(date +%s)
CURRENT_PHASE=""
PHASE_START=0
PHASES_DONE=0
PHASES_TOTAL=6
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_TOTAL=0

update_progress() {
    local now=$(date +%s)
    local elapsed=$((now - GLOBAL_START))
    local h=$((elapsed / 3600))
    local m=$(((elapsed % 3600) / 60))
    local s=$((elapsed % 60))
    local phase_elapsed=$((now - PHASE_START))

    cat > "${PROGRESS_FILE}" << EOF
{
  "timestamp": "$(date -Iseconds)",
  "elapsed": "${h}h ${m}m ${s}s",
  "elapsed_seconds": ${elapsed},
  "current_phase": "${CURRENT_PHASE}",
  "phase_elapsed_seconds": ${phase_elapsed},
  "phases_completed": ${PHASES_DONE},
  "phases_total": ${PHASES_TOTAL},
  "tests_passed": ${TESTS_PASSED},
  "tests_failed": ${TESTS_FAILED},
  "tests_total": ${TESTS_TOTAL}
}
EOF
}

start_phase() {
    CURRENT_PHASE="$1"
    PHASE_START=$(date +%s)
    log "${BOLD}━━━ Phase $((PHASES_DONE + 1))/${PHASES_TOTAL}: ${CURRENT_PHASE} ━━━${NC}"
    update_progress
}

end_phase() {
    local now=$(date +%s)
    local dur=$((now - PHASE_START))
    log "Phase '${CURRENT_PHASE}' completed in $((dur / 60))m $((dur % 60))s"
    PHASES_DONE=$((PHASES_DONE + 1))
    update_progress
}

# ── Monitor (background) ────────────────────────────────────────────
# Monitor reads state from the progress file (written by main process)
# since forked processes don't share shell variables.
monitor_loop() {
    local pfile="$1"
    local lfile="$2"
    local start="$3"
    while true; do
        sleep ${MONITOR_INTERVAL}
        local now=$(date +%s)
        local elapsed=$((now - start))
        local h=$((elapsed / 3600))
        local m=$(((elapsed % 3600) / 60))
        local s=$((elapsed % 60))

        if [ -f "${pfile}" ]; then
            local phase=$(python3 -c "import json; print(json.load(open('${pfile}')).get('current_phase','?'))" 2>/dev/null || echo "?")
            local pdone=$(python3 -c "import json; print(json.load(open('${pfile}')).get('phases_completed',0))" 2>/dev/null || echo "?")
            local ptotal=$(python3 -c "import json; print(json.load(open('${pfile}')).get('phases_total',6))" 2>/dev/null || echo "6")
            local tp=$(python3 -c "import json; print(json.load(open('${pfile}')).get('tests_passed',0))" 2>/dev/null || echo "0")
            local tf=$(python3 -c "import json; print(json.load(open('${pfile}')).get('tests_failed',0))" 2>/dev/null || echo "0")
            local tt=$(python3 -c "import json; print(json.load(open('${pfile}')).get('tests_total',0))" 2>/dev/null || echo "0")
            echo -e "${CYAN}[MONITOR ${h}h${m}m${s}s]${NC} Phase: ${phase} | Done: ${pdone}/${ptotal} | Pass: ${tp} Fail: ${tf} Total: ${tt}" | tee -a "${lfile}"
        else
            echo -e "${CYAN}[MONITOR ${h}h${m}m${s}s]${NC} Waiting for progress data..." | tee -a "${lfile}"
        fi
    done
}

# ── Cleanup ──────────────────────────────────────────────────────────
cleanup() {
    log_warn "Received interrupt, cleaning up..."
    [[ -n "${MONITOR_PID:-}" ]] && kill "${MONITOR_PID}" 2>/dev/null || true
    update_progress
    generate_report
    exit 130
}
trap cleanup SIGINT SIGTERM

# ── Report generator ────────────────────────────────────────────────
generate_report() {
    local now=$(date +%s)
    local elapsed=$((now - GLOBAL_START))
    local h=$((elapsed / 3600))
    local m=$(((elapsed % 3600) / 60))

    cat > "${SESSION_DIR}/final_report.md" << EOF
# Selfware System Test Report

- **Date:** $(date)
- **Duration:** ${h}h ${m}m
- **Phases completed:** ${PHASES_DONE}/${PHASES_TOTAL}

## Results

| Metric | Value |
|--------|-------|
| Tests Passed | ${TESTS_PASSED} |
| Tests Failed | ${TESTS_FAILED} |
| Tests Total | ${TESTS_TOTAL} |
| Pass Rate | $(if [ ${TESTS_TOTAL} -gt 0 ]; then echo "scale=1; ${TESTS_PASSED} * 100 / ${TESTS_TOTAL}" | bc; else echo "0"; fi)% |

## Phase Results

$(cat "${SESSION_DIR}/phases/"*.txt 2>/dev/null || echo "No phase results recorded")

## Errors

$(if [ -f "${SESSION_DIR}/errors.log" ]; then cat "${SESSION_DIR}/errors.log"; else echo "No errors recorded"; fi)
EOF

    log "Report saved to ${SESSION_DIR}/final_report.md"
}

# ════════════════════════════════════════════════════════════════════
#  MAIN
# ════════════════════════════════════════════════════════════════════

cat << 'BANNER'
╔═══════════════════════════════════════════════════════════════╗
║   Selfware Full System Test Suite                             ║
║   Monitoring every 30 seconds                                 ║
║   Expected duration: ~2-3 hours                               ║
╚═══════════════════════════════════════════════════════════════╝
BANNER

log "Session: ${SESSION_DIR}"
log "Monitor interval: ${MONITOR_INTERVAL}s"

# Start background monitor (pass file paths and start time since fork won't share vars)
monitor_loop "${PROGRESS_FILE}" "${LOG_FILE}" "${GLOBAL_START}" &
MONITOR_PID=$!

# ── Phase 1: Unit Tests ─────────────────────────────────────────────
start_phase "Unit Tests (cargo test --lib)"

phase1_log="${SESSION_DIR}/phases/01_unit_tests.log"
if cargo test --lib --manifest-path="${REPO_ROOT}/Cargo.toml" 2>&1 | tee "${phase1_log}" | tail -1; then
    passed=$(grep -oP '\d+ passed' "${phase1_log}" | head -1 | grep -oP '\d+')
    failed=$(grep -oP '\d+ failed' "${phase1_log}" | head -1 | grep -oP '\d+' || echo 0)
    TESTS_PASSED=$((TESTS_PASSED + ${passed:-0}))
    TESTS_FAILED=$((TESTS_FAILED + ${failed:-0}))
    TESTS_TOTAL=$((TESTS_TOTAL + ${passed:-0} + ${failed:-0}))
    echo "Phase 1 - Unit Tests: ${passed:-0} passed, ${failed:-0} failed" > "${SESSION_DIR}/phases/01_summary.txt"
    log_ok "Unit tests: ${passed:-0} passed, ${failed:-0} failed"
else
    log_fail "Unit tests had failures"
    echo "Phase 1 - Unit Tests: FAILED" > "${SESSION_DIR}/phases/01_summary.txt"
fi
end_phase

# ── Phase 2: External Unit Tests ────────────────────────────────────
start_phase "External Unit Tests (cargo test --test unit)"

phase2_log="${SESSION_DIR}/phases/02_ext_unit_tests.log"
if cargo test --test unit --manifest-path="${REPO_ROOT}/Cargo.toml" 2>&1 | tee "${phase2_log}" | tail -1; then
    passed=$(grep -oP '\d+ passed' "${phase2_log}" | head -1 | grep -oP '\d+')
    failed=$(grep -oP '\d+ failed' "${phase2_log}" | head -1 | grep -oP '\d+' || echo 0)
    TESTS_PASSED=$((TESTS_PASSED + ${passed:-0}))
    TESTS_FAILED=$((TESTS_FAILED + ${failed:-0}))
    TESTS_TOTAL=$((TESTS_TOTAL + ${passed:-0} + ${failed:-0}))
    echo "Phase 2 - External Unit Tests: ${passed:-0} passed, ${failed:-0} failed" > "${SESSION_DIR}/phases/02_summary.txt"
    log_ok "External unit tests: ${passed:-0} passed"
else
    log_fail "External unit tests had failures"
    echo "Phase 2 - External Unit Tests: FAILED" > "${SESSION_DIR}/phases/02_summary.txt"
fi
end_phase

# ── Phase 3: Integration Tests (interactive/CLI) ────────────────────
start_phase "Integration Tests (interactive CLI)"

phase3_log="${SESSION_DIR}/phases/03_integration_tests.log"
set +e
cargo test --test integration --features integration --manifest-path="${REPO_ROOT}/Cargo.toml" \
    -- interactive 2>&1 | tee "${phase3_log}" | tail -3
phase3_exit=$?
set -e

passed=$(grep -oP '\d+ passed' "${phase3_log}" | head -1 | grep -oP '\d+' || echo 0)
failed=$(grep -oP '\d+ failed' "${phase3_log}" | head -1 | grep -oP '\d+' || echo 0)
ignored=$(grep -oP '\d+ ignored' "${phase3_log}" | head -1 | grep -oP '\d+' || echo 0)
TESTS_PASSED=$((TESTS_PASSED + ${passed:-0}))
TESTS_FAILED=$((TESTS_FAILED + ${failed:-0}))
TESTS_TOTAL=$((TESTS_TOTAL + ${passed:-0} + ${failed:-0}))
echo "Phase 3 - Integration (Interactive): ${passed:-0} passed, ${failed:-0} failed, ${ignored:-0} ignored" > "${SESSION_DIR}/phases/03_summary.txt"

if [ ${phase3_exit} -eq 0 ]; then
    log_ok "Integration tests: ${passed:-0} passed"
else
    log_fail "Integration tests: ${failed:-0} failed"
    grep -E "^(test .* FAILED|failures:)" "${phase3_log}" >> "${SESSION_DIR}/errors.log" 2>/dev/null || true
fi
end_phase

# ── Phase 4: Project E2E Tests ──────────────────────────────────────
start_phase "Project E2E Tests (6 coding scenarios + swarm)"

phase4_log="${SESSION_DIR}/phases/04_projecte2e.log"
set +e
bash "${SCRIPT_DIR}/projecte2e/run_projecte2e.sh" 2>&1 | tee "${phase4_log}"
phase4_exit=$?
set -e

# Parse projecte2e results
e2e_report_dir="${SCRIPT_DIR}/projecte2e/reports/latest"
if [ -L "${e2e_report_dir}" ] && [ -f "${e2e_report_dir}/results.tsv" ]; then
    # Count coding scenarios (have numeric post_status) and swarm separately
    e2e_passed=$(awk -F'|' 'NR>1 && $5=="0" {n++} END{print n+0}' "${e2e_report_dir}/results.tsv")
    e2e_coding_total=$(awk -F'|' 'NR>1 && $5!="n/a" {n++} END{print n+0}' "${e2e_report_dir}/results.tsv")
    e2e_total=$(awk -F'|' 'NR>1 {n++} END{print n+0}' "${e2e_report_dir}/results.tsv")
    e2e_score=$(awk -F'|' 'NR>1{sum+=$9;n++} END{if(n==0){print 0}else{printf "%.1f", sum/n}}' "${e2e_report_dir}/results.tsv")
    # For swarm, count as passed if score >= 70
    swarm_passed=$(awk -F'|' 'NR>1 && $2=="swarm" && $9>=70 {n++} END{print n+0}' "${e2e_report_dir}/results.tsv")
    total_passed=$((e2e_passed + swarm_passed))
    TESTS_PASSED=$((TESTS_PASSED + total_passed))
    TESTS_FAILED=$((TESTS_FAILED + (e2e_total - total_passed)))
    TESTS_TOTAL=$((TESTS_TOTAL + e2e_total))
    echo "Phase 4 - Project E2E: coding ${e2e_passed}/${e2e_coding_total} passed, total ${total_passed}/${e2e_total} passed, avg score ${e2e_score}/100" > "${SESSION_DIR}/phases/04_summary.txt"
    # Copy the detailed report
    cp "${e2e_report_dir}/summary.md" "${SESSION_DIR}/phases/04_e2e_report.md" 2>/dev/null || true
    cp "${e2e_report_dir}/results.tsv" "${SESSION_DIR}/phases/04_e2e_results.tsv" 2>/dev/null || true
    log_ok "E2E scenarios: coding ${e2e_passed}/${e2e_coding_total}, total ${total_passed}/${e2e_total}, avg score: ${e2e_score}/100"
else
    echo "Phase 4 - Project E2E: Could not parse results" > "${SESSION_DIR}/phases/04_summary.txt"
    log_warn "Could not find E2E results TSV"
fi
end_phase

# ── Phase 5: Mega Project Test (2-hour task_queue) ──────────────────
start_phase "Mega Project Test (task_queue, 2h duration)"

phase5_log="${SESSION_DIR}/phases/05_mega_test.log"
set +e
CHECKPOINT_INTERVAL=5 bash "${SCRIPT_DIR}/long_running/run_mega_test.sh" task_queue 2 2 2>&1 | tee "${phase5_log}"
phase5_exit=$?
set -e

# Find the mega test session directory
mega_session=$(ls -td "${REPO_ROOT}/test_runs/mega-"* 2>/dev/null | head -1)
if [ -n "${mega_session}" ] && [ -f "${mega_session}/final_report.json" ]; then
    mega_status=$(python3 -c "import json; d=json.load(open('${mega_session}/final_report.json')); print(d.get('status','unknown'))" 2>/dev/null || echo "unknown")
    mega_duration=$(python3 -c "import json; d=json.load(open('${mega_session}/final_report.json')); print(d.get('duration_formatted','?'))" 2>/dev/null || echo "?")
    mega_checkpoints=$(python3 -c "import json; d=json.load(open('${mega_session}/final_report.json')); print(d.get('checkpoints_created',0))" 2>/dev/null || echo "0")
    echo "Phase 5 - Mega Test: status=${mega_status}, duration=${mega_duration}, checkpoints=${mega_checkpoints}" > "${SESSION_DIR}/phases/05_summary.txt"
    cp "${mega_session}/final_report.json" "${SESSION_DIR}/phases/05_mega_report.json" 2>/dev/null || true

    if [ "${mega_status}" = "completed" ] || [ "${mega_status}" = "timeout" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        if [ "${mega_status}" = "timeout" ]; then
            log_warn "Mega test reached duration limit: ${mega_duration}, ${mega_checkpoints} checkpoints"
        else
            log_ok "Mega test completed: ${mega_duration}, ${mega_checkpoints} checkpoints"
        fi
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_fail "Mega test status: ${mega_status}"
    fi
    TESTS_TOTAL=$((TESTS_TOTAL + 1))
else
    echo "Phase 5 - Mega Test: exit=${phase5_exit}" > "${SESSION_DIR}/phases/05_summary.txt"
    if [ ${phase5_exit} -eq 0 ] || [ ${phase5_exit} -eq 124 ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        if [ ${phase5_exit} -eq 124 ]; then
            log_warn "Mega test reached duration limit"
        else
            log_ok "Mega test completed"
        fi
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_fail "Mega test failed (exit ${phase5_exit})"
    fi
    TESTS_TOTAL=$((TESTS_TOTAL + 1))
fi
end_phase

# ── Phase 6: Clippy + Fmt + Property Tests ──────────────────────────
start_phase "Code Quality (clippy, fmt, property tests)"

phase6_log="${SESSION_DIR}/phases/06_quality.log"

# Clippy
log "Running clippy..."
set +e
cargo clippy --all-targets --all-features --manifest-path="${REPO_ROOT}/Cargo.toml" -- -D warnings 2>&1 | tee -a "${phase6_log}"
clippy_exit=$?
set -e
TESTS_TOTAL=$((TESTS_TOTAL + 1))
if [ ${clippy_exit} -eq 0 ]; then
    TESTS_PASSED=$((TESTS_PASSED + 1))
    log_ok "Clippy: clean"
else
    TESTS_FAILED=$((TESTS_FAILED + 1))
    log_fail "Clippy: warnings/errors"
fi

# Fmt check
log "Running fmt check..."
set +e
cargo fmt --all --manifest-path="${REPO_ROOT}/Cargo.toml" -- --check 2>&1 | tee -a "${phase6_log}"
fmt_exit=$?
set -e
TESTS_TOTAL=$((TESTS_TOTAL + 1))
if [ ${fmt_exit} -eq 0 ]; then
    TESTS_PASSED=$((TESTS_PASSED + 1))
    log_ok "Fmt: clean"
else
    TESTS_FAILED=$((TESTS_FAILED + 1))
    log_fail "Fmt: unformatted code"
fi

# Property tests
log "Running property tests..."
set +e
cargo test --manifest-path="${REPO_ROOT}/Cargo.toml" prop_ 2>&1 | tee -a "${phase6_log}"
prop_exit=$?
set -e
TESTS_TOTAL=$((TESTS_TOTAL + 1))
if [ ${prop_exit} -eq 0 ]; then
    TESTS_PASSED=$((TESTS_PASSED + 1))
    log_ok "Property tests: passed"
else
    TESTS_FAILED=$((TESTS_FAILED + 1))
    log_fail "Property tests: failed"
fi

echo "Phase 6 - Quality: clippy=${clippy_exit}, fmt=${fmt_exit}, proptest=${prop_exit}" > "${SESSION_DIR}/phases/06_summary.txt"
end_phase

# ── Done ─────────────────────────────────────────────────────────────
kill "${MONITOR_PID}" 2>/dev/null || true
update_progress
generate_report

NOW=$(date +%s)
TOTAL=$((NOW - GLOBAL_START))
TH=$((TOTAL / 3600))
TM=$(((TOTAL % 3600) / 60))

cat << EOF

╔═══════════════════════════════════════════════════════════════╗
║                   SYSTEM TEST COMPLETE                        ║
╠═══════════════════════════════════════════════════════════════╣
║  Duration:     ${TH}h ${TM}m
║  Passed:       ${TESTS_PASSED}
║  Failed:       ${TESTS_FAILED}
║  Total:        ${TESTS_TOTAL}
║  Pass Rate:    $(if [ ${TESTS_TOTAL} -gt 0 ]; then echo "scale=1; ${TESTS_PASSED} * 100 / ${TESTS_TOTAL}" | bc; else echo "0"; fi)%
║  Report:       ${SESSION_DIR}/final_report.md
╚═══════════════════════════════════════════════════════════════╝
EOF

if [ ${TESTS_FAILED} -gt 0 ]; then
    exit 1
else
    exit 0
fi
