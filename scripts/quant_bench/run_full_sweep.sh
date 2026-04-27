#!/usr/bin/env bash
# Run the quant_benchmark example across every .gguf in $MODELS_DIR.
#
# For each model:
#   1. Stop any running llama-server.
#   2. Boot llama-server with that model (and mmproj if available + the model
#      isn't the existing 35B-A3B which uses its own mmproj).
#   3. Wait for /v1/models to respond.
#   4. Run examples/quant_benchmark with the configured scenarios.
#   5. Stop llama-server.
#
# After all models finish, write reports/quant_bench/comparison.md.
#
# Override defaults via env vars: MODELS_DIR, REPORTS_DIR, SELFWARE_BIN,
# QUANT_BENCH_BIN, LLAMA_SERVER, PORT, NGL, CTX, SCENARIO_TIMEOUT_SECS.

set -uo pipefail

MODELS_DIR="${MODELS_DIR:-${HOME}/models/qwen36-quants}"
EXTRA_MODEL="${EXTRA_MODEL:-${HOME}/models/Qwen3.6-35B-A3B-UD-Q3_K_XL.gguf}"
EXTRA_MODEL_MMPROJ="${EXTRA_MODEL_MMPROJ:-${HOME}/models/mmproj-F16.gguf}"
EXTRA_MODEL_ALIAS="${EXTRA_MODEL_ALIAS:-qwen3.6-35b-a3b}"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
REPORTS_DIR="${REPORTS_DIR:-${REPO_ROOT}/reports/quant_bench}"
SELFWARE_BIN="${SELFWARE_BIN:-${REPO_ROOT}/target/release/selfware}"
QUANT_BENCH_BIN="${QUANT_BENCH_BIN:-${REPO_ROOT}/target/release/examples/quant_benchmark}"
LLAMA_SERVER="${LLAMA_SERVER:-/home/ivo/llama.cpp/build/bin/llama-server}"
LLAMA_LOG_DIR="${LLAMA_LOG_DIR:-${REPORTS_DIR}/llama-logs}"
PORT="${PORT:-8000}"
NGL="${NGL:-99}"
CTX="${CTX:-65536}"
TENSOR_SPLIT="${TENSOR_SPLIT:-24,24}"
SCENARIO_TIMEOUT_SECS="${SCENARIO_TIMEOUT_SECS:-300}"
BOOT_TIMEOUT_SECS="${BOOT_TIMEOUT_SECS:-180}"

mkdir -p "${REPORTS_DIR}" "${LLAMA_LOG_DIR}"

log() { printf '[%s] %s\n' "$(date '+%H:%M:%S')" "$*"; }

stop_server() {
    pkill -f 'llama-server' 2>/dev/null || true
    sleep 2
    pkill -9 -f 'llama-server' 2>/dev/null || true
    sleep 1
}

wait_for_ready() {
    local deadline=$(( $(date +%s) + BOOT_TIMEOUT_SECS ))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        if curl -sf -m 1 "http://127.0.0.1:${PORT}/v1/models" >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
    done
    return 1
}

boot_server() {
    local gguf="$1"
    local mmproj="$2"
    local alias="$3"
    local logfile="$4"

    local args=(
        -m "$gguf"
        --jinja
        -c "$CTX"
        -ngl "$NGL"
        --tensor-split "$TENSOR_SPLIT"
        --chat-template-kwargs '{"enable_thinking": false}'
        --host 0.0.0.0
        --port "$PORT"
        --alias "$alias"
    )
    if [ -n "$mmproj" ] && [ -f "$mmproj" ]; then
        args+=( --mmproj "$mmproj" )
    fi

    nohup "$LLAMA_SERVER" "${args[@]}" >"$logfile" 2>&1 &
    echo $!
}

# ----------------------------------------------------------------------------
# Build the list of models to sweep. Format: alias|gguf|mmproj
# ----------------------------------------------------------------------------
HAUHAUCS_MMPROJ="${MODELS_DIR}/mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf"

declare -a MODELS

if [ -f "$EXTRA_MODEL" ]; then
    MODELS+=("${EXTRA_MODEL_ALIAS}|${EXTRA_MODEL}|${EXTRA_MODEL_MMPROJ}")
fi

while IFS= read -r gguf; do
    case "$gguf" in
        *mmproj*) continue ;;
    esac
    fname="$(basename "$gguf")"
    quant_short=""
    case "$fname" in
        *-IQ2_M.gguf)  quant_short="IQ2_M"  ;;
        *-IQ3_XS.gguf) quant_short="IQ3_XS" ;;
        *-IQ3_M.gguf)  quant_short="IQ3_M"  ;;
        *-IQ4_XS.gguf) quant_short="IQ4_XS" ;;
        *-Q2_K_P.gguf) quant_short="Q2_K_P" ;;
        *-Q3_K_P.gguf) quant_short="Q3_K_P" ;;
        *-Q4_K_P.gguf) quant_short="Q4_K_P" ;;
        *-Q5_K_P.gguf) quant_short="Q5_K_P" ;;
        *-Q6_K_P.gguf) quant_short="Q6_K_P" ;;
        *-Q8_K_P.gguf) quant_short="Q8_K_P" ;;
        *)             quant_short="$(basename "$fname" .gguf)" ;;
    esac
    alias="qwen3.6-27b-$(echo "$quant_short" | tr 'A-Z' 'a-z' | tr -d '_')"
    MODELS+=("${alias}|${gguf}|${HAUHAUCS_MMPROJ}|${quant_short}")
done < <(ls "${MODELS_DIR}"/*.gguf 2>/dev/null | sort)

log "Sweep plan (${#MODELS[@]} models):"
for entry in "${MODELS[@]}"; do
    IFS='|' read -r alias gguf _ quant_short <<< "$entry"
    sz=$(du -h "$gguf" 2>/dev/null | cut -f1)
    log "  • ${alias}  (${quant_short:-?}, ${sz})"
done

# ----------------------------------------------------------------------------
# Sweep
# ----------------------------------------------------------------------------
mkdir -p "${REPORTS_DIR}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="${REPORTS_DIR}/${TIMESTAMP}"
mkdir -p "$RUN_DIR"
log "Reports → ${RUN_DIR}"

for entry in "${MODELS[@]}"; do
    IFS='|' read -r alias gguf mmproj quant_short <<< "$entry"
    quant_label="${quant_short:-$(basename "$gguf" .gguf)}"
    case "$gguf" in
        *35B-A3B*) quant_label="Qwen3.6-35B-A3B-Q3_K_XL" ;;
        *) quant_label="Qwen3.6-27B-HauhauCS-${quant_label}" ;;
    esac

    log "──────────────────────────────────────────────────────────────"
    log "MODEL: ${alias} (${quant_label})"
    log "──────────────────────────────────────────────────────────────"

    stop_server
    log "  booting llama-server..."
    boot_server "$gguf" "$mmproj" "$alias" "${LLAMA_LOG_DIR}/${alias}.log" >/dev/null
    if ! wait_for_ready; then
        log "  ❌ boot timed out — skipping ${alias}"
        stop_server
        continue
    fi
    log "  ✓ server ready"

    out="${RUN_DIR}/${quant_label}.json"
    log "  running quant_benchmark → ${out}"
    "$QUANT_BENCH_BIN" \
        --endpoint "http://127.0.0.1:${PORT}/v1" \
        --quant "$quant_label" \
        --output "$out" \
        --model "$alias" \
        --selfware-bin "$SELFWARE_BIN" \
        --scenario-timeout-secs "$SCENARIO_TIMEOUT_SECS" \
        2>&1 | tee "${RUN_DIR}/${quant_label}.txt" \
        | grep -E '^(==|  agent exit|  post-validator|## |- |\| )' || true

    stop_server
    log "  done with ${alias}"
done

# ----------------------------------------------------------------------------
# Collate
# ----------------------------------------------------------------------------
log "Collating ${RUN_DIR}/*.json → ${RUN_DIR}/comparison.md"
python3 "${REPO_ROOT}/scripts/quant_bench/collate.py" "$RUN_DIR" > "${RUN_DIR}/comparison.md" || \
    log "  collate failed (collate.py may not exist yet)"

log "DONE. See ${RUN_DIR}/comparison.md"
