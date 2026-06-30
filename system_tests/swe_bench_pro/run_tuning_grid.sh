#!/bin/bash
# Per-model small-model tuning grid for SWE-bench Pro.
#
# Usage: run_tuning_grid.sh <model-profile> [--force] [--parallel] [--max-tasks N]
#
# Sweeps max_tokens (6000/8000/12000/16000) and edit-before-complete
# (edit_deadline_step on/off) on sample_3.jsonl, using --small-model-diff-fallback.
# Each grid cell gets an isolated podman root (20-27) and a distinct output dir.
# After the cells finish, evaluate_predictions.py is run for each cell and a
# Markdown summary table is written to runs_tune_<model>/tuning_summary.md.
set -uo pipefail

REPO="/home/habitat/selfware"
SWE_DIR="${REPO}/system_tests/swe_bench_pro"
CONFIG_DIR="${REPO}/system_tests/projecte2e/config"
VENV="/tmp/SWE-bench_Pro-os/eval_venv/bin/python3"
SAMPLE_FILE="${SWE_DIR}/sample_3.jsonl"
MAX_TASKS=3
TIMEOUT=1800
WORKERS=1
PARALLEL=false
FORCE=false

usage() {
  echo "Usage: $0 <model-profile> [--force] [--parallel] [--max-tasks N]"
  exit 1
}

if [[ $# -lt 1 ]]; then
  usage
fi

MODEL_PROFILE="$1"
shift

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      FORCE=true
      shift
      ;;
    --parallel)
      PARALLEL=true
      shift
      ;;
    --max-tasks)
      MAX_TASKS="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      usage
      ;;
  esac
done

if [[ ! -f "${SAMPLE_FILE}" ]]; then
  echo "Sample file not found: ${SAMPLE_FILE}"
  exit 1
fi

BASE_CONFIG="${CONFIG_DIR}/openrouter_${MODEL_PROFILE}.toml"
if [[ ! -f "${BASE_CONFIG}" ]]; then
  echo "Base config not found: ${BASE_CONFIG}"
  exit 1
fi

# Source API key if available.
if [[ -f /tmp/selfware_api_key.env ]]; then
  set -a
  # shellcheck source=/dev/null
  source /tmp/selfware_api_key.env
  set +a
fi

if [[ -z "${SELFWARE_API_KEY:-}" ]]; then
  echo "WARNING: SELFWARE_API_KEY is not set. API calls will fail."
fi

GRID_BASE="${SWE_DIR}/runs_tune_${MODEL_PROFILE}"
mkdir -p "${GRID_BASE}"

MAX_TOKENS_LIST=(6000 8000 12000 16000)
EDIT_LABELS=(on off)
EDIT_DEADLINE_ON=6
EDIT_DEADLINE_OFF=999
ROOT_START=20

run_cell() {
  local mt="$1"
  local edit_flag="$2"
  local root_idx="$3"

  local edit_label
  local edit_deadline
  if [[ "${edit_flag}" == "1" ]]; then
    edit_label="on"
    edit_deadline="${EDIT_DEADLINE_ON}"
  else
    edit_label="off"
    edit_deadline="${EDIT_DEADLINE_OFF}"
  fi

  local out_dir="${GRID_BASE}_mt${mt}_edit${edit_label}"
  local log_file="${out_dir}/agent.log"
  local config_dir="${out_dir}/config"
  local config_file="${config_dir}/openrouter_${MODEL_PROFILE}.toml"
  local podman_root="/tmp/podman-root-${root_idx}"

  # Do not clobber existing results unless --force was given.
  if [[ "${FORCE}" != true && -f "${out_dir}/eval/evaluation_report.json" ]]; then
    echo "[SKIP] ${out_dir} already has evaluation_report.json (use --force to overwrite)"
    return 0
  fi

  mkdir -p "${out_dir}"

  if [[ "${FORCE}" == true ]]; then
    rm -rf "${out_dir}/out" "${out_dir}/eval" "${config_dir}"
  fi

  mkdir -p "${config_dir}"

  echo "[$(date -Iseconds)] Building config model=${MODEL_PROFILE} max_tokens=${mt} edit=${edit_label} root=${podman_root}"

  # Generate a per-run TOML from the base config with max_tokens and
  # edit_deadline_step overrides.
  ${VENV} - <<PYEOF
import sys
import tomllib
import tomli_w
from pathlib import Path

base = Path("${BASE_CONFIG}")
out = Path("${config_file}")
with open(base, "rb") as f:
    cfg = tomllib.load(f)

cfg["max_tokens"] = ${mt}
agent = cfg.setdefault("agent", {})
agent["edit_deadline_step"] = ${edit_deadline}
# Ensure the agent table exists with sensible small-model defaults if absent.
agent.setdefault("max_iterations", 25)
agent.setdefault("max_no_edit_steps", 6)
agent.setdefault("context_window", 0)
agent.setdefault("native_function_calling", False)
agent.setdefault("streaming", False)
agent.setdefault("require_verification_before_completion", True)

with open(out, "wb") as f:
    tomli_w.dump(cfg, f)
PYEOF

  echo "[$(date -Iseconds)] Starting cell ${out_dir}"
  {
    echo "[$(date -Iseconds)] model=${MODEL_PROFILE} max_tokens=${mt} edit=${edit_label} podman_root=${podman_root}"
    export PATH="${podman_root}/bin:${PATH}"

    ${VENV} "${SWE_DIR}/run_selfware.py" \
      --model-profile "${MODEL_PROFILE}" \
      --config-dir "${config_dir}" \
      --sample-file "${SAMPLE_FILE}" \
      --max-tasks "${MAX_TASKS}" \
      --output-dir "${out_dir}/out" \
      --workers "${WORKERS}" \
      --timeout "${TIMEOUT}" \
      --auto-agentless \
      --small-model-diff-fallback \
      --diff-fallback \
      --early-diff-fallback \
      --retry-failures \
      --max-retries 1 \
      --fresh \
      2>&1

    echo "[$(date -Iseconds)] Predictions done; starting evaluation"

    ${VENV} "${SWE_DIR}/evaluate_predictions.py" \
      --predictions "${out_dir}/out/predictions.jsonl" \
      --output-dir "${out_dir}/eval" \
      --sample-file "${SAMPLE_FILE}" \
      --test-timeout 600 \
      2>&1

    echo "[$(date -Iseconds)] Evaluation done"
  } >> "${log_file}" 2>&1

  echo "[$(date -Iseconds)] Finished cell ${out_dir}"
}

# Launch the grid.
idx=0
for mt in "${MAX_TOKENS_LIST[@]}"; do
  for edit_flag in 1 0; do
    root_idx=$((ROOT_START + idx))
    if [[ "${PARALLEL}" == true ]]; then
      run_cell "${mt}" "${edit_flag}" "${root_idx}" &
    else
      run_cell "${mt}" "${edit_flag}" "${root_idx}"
    fi
    idx=$((idx + 1))
  done
done

if [[ "${PARALLEL}" == true ]]; then
  echo "[$(date -Iseconds)] Waiting for parallel grid cells to finish..."
  wait
  echo "[$(date -Iseconds)] All grid cells finished"
fi

# Aggregate results.
SUMMARY_FILE="${GRID_BASE}/tuning_summary.md"
{
  echo "# Small-model tuning grid summary: ${MODEL_PROFILE}"
  echo ""
  echo "| model | max_tokens | edit-before-complete | solved/total | empty-patch | recovery_fired |"
  echo "|-------|------------|----------------------|--------------|-------------|----------------|"
} > "${SUMMARY_FILE}"

for mt in "${MAX_TOKENS_LIST[@]}"; do
  for edit_flag in 1 0; do
    edit_label="${EDIT_LABELS[$((1 - edit_flag))]}"
    out_dir="${GRID_BASE}_mt${mt}_edit${edit_label}"
    report="${out_dir}/eval/evaluation_report.json"

    if [[ -f "${report}" ]]; then
      ${VENV} - <<PYEOF >> "${SUMMARY_FILE}"
import json
from pathlib import Path
report = Path("${report}")
with open(report, "r", encoding="utf-8") as f:
    r = json.load(f)
solved = r.get("overall_passed_instances", 0)
total = r.get("total_instances", 0)
empty = r.get("empty_patch_count", 0)
recovery = r.get("recovery_fired_count", 0)
print(f"| ${MODEL_PROFILE} | ${mt} | ${edit_label} | {solved}/{total} | {empty} | {recovery} |")
PYEOF
    else
      echo "| ${MODEL_PROFILE} | ${mt} | ${edit_label} | N/A | N/A | N/A |" >> "${SUMMARY_FILE}"
    fi
  done
done

echo ""
echo "Tuning summary written to: ${SUMMARY_FILE}"
cat "${SUMMARY_FILE}"
