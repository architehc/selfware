#!/usr/bin/env bash
# Orchestrate the Qwen3.6 quant benchmark suite.
#
# For each quant we:
#   1. Boot `llama-server` with the recommended flags from the model card.
#   2. Wait for `/v1/models` to return.
#   3. Run `cargo run --release --example quant_benchmark`.
#   4. Tear the server down.
#
# After all quants are done we collate the per-quant JSON outputs into
# `${REPORTS}/comparison.md`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MODELS_DIR="${HOME}/models/qwen36-quants"
REPORTS_DIR="${REPO_ROOT}/reports/quant_bench"
LLAMA_SERVER="${LLAMA_SERVER:-llama-server}"
PORT_BASE="${PORT_BASE:-8080}"
NGL="${NGL:-99}"
CTX="${CTX:-131072}"
WAIT_SECS="${WAIT_SECS:-300}"
TIMEOUT_SECS="${TIMEOUT_SECS:-180}"

MMPROJ="${MODELS_DIR}/mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf"

# (logical_name, gguf_filename) pairs.
QUANTS=(
    "IQ2_M:Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ2_M.gguf"
    "Q8_K_P:Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf"
)

usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

  --models-dir DIR     Directory containing the GGUFs (default: ${MODELS_DIR})
  --reports-dir DIR    Where to write per-quant + comparison reports
                       (default: ${REPORTS_DIR})
  --llama-server BIN   Path to llama-server binary (default: ${LLAMA_SERVER})
  --port-base N        First TCP port; subsequent quants use port-base + i
                       (default: ${PORT_BASE})
  --ngl N              -ngl flag for llama-server (default: ${NGL})
  --ctx N              -c flag for llama-server (default: ${CTX})
  --wait-secs N        Max seconds to wait for /v1/models (default: ${WAIT_SECS})
  --timeout-secs N     Per-request timeout for the example
                       (default: ${TIMEOUT_SECS})
  --skip-multimodal    Pass --skip-multimodal to the example
  -h, --help           Show this message
EOF
}

SKIP_MULTIMODAL=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --models-dir)    MODELS_DIR="$2"; shift 2;;
        --reports-dir)   REPORTS_DIR="$2"; shift 2;;
        --llama-server)  LLAMA_SERVER="$2"; shift 2;;
        --port-base)     PORT_BASE="$2"; shift 2;;
        --ngl)           NGL="$2"; shift 2;;
        --ctx)           CTX="$2"; shift 2;;
        --wait-secs)     WAIT_SECS="$2"; shift 2;;
        --timeout-secs)  TIMEOUT_SECS="$2"; shift 2;;
        --skip-multimodal) SKIP_MULTIMODAL=1; shift;;
        -h|--help)       usage; exit 0;;
        *) echo "unknown argument: $1" >&2; usage; exit 2;;
    esac
done

MMPROJ="${MODELS_DIR}/mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf"

mkdir -p "${REPORTS_DIR}"

log() {
    printf '[run_bench] %s\n' "$*" >&2
}

# Sanity checks
if [[ ! -d "${MODELS_DIR}" ]]; then
    log "ERROR: models dir not found: ${MODELS_DIR} (run download_quants.sh first)"
    exit 1
fi
if ! command -v "${LLAMA_SERVER}" >/dev/null 2>&1; then
    log "ERROR: ${LLAMA_SERVER} not on PATH; build llama.cpp with -DLLAMA_CURL=ON or pass --llama-server"
    exit 1
fi
if [[ ! -f "${MMPROJ}" ]]; then
    log "WARN: mmproj not found at ${MMPROJ}; multimodal test will be unreliable"
fi

# Tear down the server cleanly even on Ctrl-C / errors.
SERVER_PID=""
cleanup() {
    if [[ -n "${SERVER_PID}" ]] && kill -0 "${SERVER_PID}" 2>/dev/null; then
        log "stopping llama-server (pid ${SERVER_PID})"
        kill "${SERVER_PID}" 2>/dev/null || true
        for _ in $(seq 1 20); do
            if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
                break
            fi
            sleep 0.5
        done
        if kill -0 "${SERVER_PID}" 2>/dev/null; then
            log "force-killing llama-server"
            kill -9 "${SERVER_PID}" 2>/dev/null || true
        fi
    fi
    SERVER_PID=""
}
trap cleanup EXIT INT TERM

wait_for_endpoint() {
    local port="$1"
    local deadline=$((SECONDS + WAIT_SECS))
    while (( SECONDS < deadline )); do
        if curl -sSf "http://127.0.0.1:${port}/v1/models" >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "${SERVER_PID}" ]] && ! kill -0 "${SERVER_PID}" 2>/dev/null; then
            log "llama-server died while waiting (pid ${SERVER_PID})"
            return 1
        fi
        sleep 2
    done
    return 1
}

run_one_quant() {
    local quant_label="$1"
    local gguf_file="$2"
    local port="$3"
    local gguf_path="${MODELS_DIR}/${gguf_file}"

    if [[ ! -f "${gguf_path}" ]]; then
        log "skipping ${quant_label}: ${gguf_path} not found"
        return 0
    fi

    log "=== ${quant_label} on port ${port} ==="
    log "model: ${gguf_path}"

    # Build llama-server arguments.  We follow the model card's recommended
    # flags: --jinja, -c 131072, -ngl 99, chat-template-kwargs disabling
    # thinking, plus --mmproj when present.
    local log_file="${REPORTS_DIR}/${quant_label}.server.log"
    local args=(
        --model "${gguf_path}"
        --port "${port}"
        --host 127.0.0.1
        --jinja
        -c "${CTX}"
        -ngl "${NGL}"
        --chat-template-kwargs '{"enable_thinking": false}'
    )
    if [[ -f "${MMPROJ}" ]]; then
        args+=(--mmproj "${MMPROJ}")
    fi

    log "launching: ${LLAMA_SERVER} ${args[*]}"
    "${LLAMA_SERVER}" "${args[@]}" >"${log_file}" 2>&1 &
    SERVER_PID=$!
    log "llama-server pid=${SERVER_PID}, log=${log_file}"

    if ! wait_for_endpoint "${port}"; then
        log "ERROR: ${quant_label} endpoint never came up; see ${log_file}"
        cleanup
        return 1
    fi
    log "endpoint ready on port ${port}"

    local extra_args=()
    if [[ "${SKIP_MULTIMODAL}" -eq 1 ]]; then
        extra_args+=("--skip-multimodal")
    fi

    set +e
    (
        cd "${REPO_ROOT}"
        cargo run --release --example quant_benchmark -- \
            --endpoint "http://127.0.0.1:${port}/v1" \
            --quant "${quant_label}" \
            --output "${REPORTS_DIR}" \
            --timeout-secs "${TIMEOUT_SECS}" \
            "${extra_args[@]}" \
            >"${REPORTS_DIR}/${quant_label}.stdout.json" \
            2>"${REPORTS_DIR}/${quant_label}.stderr.log"
    )
    local rc=$?
    set -e

    if [[ ${rc} -ne 0 ]]; then
        log "WARN: example exited rc=${rc} for ${quant_label}; partial report may still exist"
    fi

    cleanup
    log "${quant_label} complete"
}

# ── Main loop ─────────────────────────────────────────────────────────────────

i=0
for entry in "${QUANTS[@]}"; do
    label="${entry%%:*}"
    file="${entry#*:}"
    port=$((PORT_BASE + i))
    run_one_quant "${label}" "${file}" "${port}" || log "continuing to next quant"
    i=$((i + 1))
done

# ── Collate ───────────────────────────────────────────────────────────────────

log "collating ${REPORTS_DIR}/*.json into comparison.md"
python3 - "${REPORTS_DIR}" <<'PYEOF'
import json
import os
import sys
from datetime import datetime, timezone

reports_dir = sys.argv[1]
rows = []
for name in sorted(os.listdir(reports_dir)):
    if not name.endswith(".json"):
        continue
    path = os.path.join(reports_dir, name)
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
    except Exception as e:
        print(f"skip {name}: {e}", file=sys.stderr)
        continue
    if "tests" not in data or "summary" not in data:
        continue
    rows.append(data)

if not rows:
    print("no per-quant reports found; nothing to collate", file=sys.stderr)
    sys.exit(0)

test_names = []
for row in rows:
    for t in row["tests"]:
        if t["name"] not in test_names:
            test_names.append(t["name"])

out_path = os.path.join(reports_dir, "comparison.md")
with open(out_path, "w", encoding="utf-8") as out:
    out.write("# Qwen3.6 quant comparison\n\n")
    out.write(f"_Generated: {datetime.now(timezone.utc).isoformat()}_\n\n")
    out.write("## Summary\n\n")
    headers = ["Quant", "Pass/Total", "tok/s (median)", "Total (s)"]
    out.write("| " + " | ".join(headers) + " |\n")
    out.write("|" + "|".join(["---"] * len(headers)) + "|\n")
    for row in rows:
        s = row["summary"]
        tps = s.get("tokens_per_sec_median")
        tps_s = f"{tps:.1f}" if isinstance(tps, (int, float)) else "n/a"
        total_s = row.get("total_duration_ms", 0) / 1000.0
        out.write(
            f"| `{row['quant']}` | {s['passed']}/{s['total']} | {tps_s} | {total_s:.2f} |\n"
        )

    out.write("\n## Per-test breakdown\n\n")
    headers = ["Quant"] + test_names
    out.write("| " + " | ".join(headers) + " |\n")
    out.write("|" + "|".join(["---"] * len(headers)) + "|\n")
    for row in rows:
        results = {t["name"]: t for t in row["tests"]}
        cells = [f"`{row['quant']}`"]
        for n in test_names:
            t = results.get(n)
            if t is None:
                cells.append("—")
            else:
                mark = "OK" if t["passed"] else "FAIL"
                cells.append(f"{mark} ({t['duration_ms']} ms)")
        out.write("| " + " | ".join(cells) + " |\n")

    out.write("\n## Per-quant detail\n\n")
    for row in rows:
        out.write(f"### `{row['quant']}`\n\n")
        out.write(f"- Endpoint: `{row.get('endpoint', '?')}`\n")
        out.write(f"- Model: `{row.get('model', '?')}`\n")
        out.write(f"- Started: {row.get('started_at', '?')}\n\n")
        out.write("| Test | Pass | ms | Detail |\n|---|---|---|---|\n")
        for t in row["tests"]:
            detail = t["detail"].replace("|", "\\|").replace("\n", " ")
            mark = "OK" if t["passed"] else "FAIL"
            out.write(f"| `{t['name']}` | {mark} | {t['duration_ms']} | {detail} |\n")
        out.write("\n")

print(f"wrote {out_path}")
PYEOF

log "all done. see ${REPORTS_DIR}/comparison.md"
