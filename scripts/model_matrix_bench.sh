#!/usr/bin/env bash
#
# model_matrix_bench.sh — run selfware's endpoint smoke probe across a fixed
# model set on OpenRouter and emit a Markdown results table.
#
# Usage:
#   OPENROUTER_API_KEY=... scripts/model_matrix_bench.sh
#   OPENROUTER_API_KEY=... scripts/model_matrix_bench.sh --models "id1,id2"
#
# Output: docs/quant_bench/2026-07-25-model-matrix.md

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SMOKE_BIN="$ROOT_DIR/target/release/examples/endpoint_smoke"
ENDPOINT="${ENDPOINT:-https://openrouter.ai/api/v1}"
OUT_MD="$ROOT_DIR/docs/quant_bench/2026-07-25-model-matrix.md"

DEFAULT_MODELS=(
  "poolside/laguna-s-2.1"
  "z-ai/glm-5.2"
  "moonshotai/kimi-k3"
  "xiaomi/mimo-v2.5"
  "deepseek/deepseek-v4-flash"
  "deepseek/deepseek-v4-pro"
  "tencent/hy3"
  "nvidia/nemotron-3-ultra-550b-a55b:free"
  "stepfun/step-3.7-flash"
  "minimax/minimax-m3"
  "qwen/qwen3.7-max"
)

# All 7 checks emitted by endpoint_smoke (multimodal only runs with --image).
CHECKS=(
  endpoint_reachable
  backend_classify
  plain_chat
  streaming
  tool_call
  tool_followup
  thinking_parse
)

MODELS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --models)
      [ $# -ge 2 ] || { echo "[error] --models requires a comma-separated value" >&2; exit 1; }
      IFS=',' read -r -a MODELS <<< "$2"
      shift 2
      ;;
    *)
      echo "[error] unknown argument: $1" >&2
      echo "usage: $0 [--models \"id1,id2\"]" >&2
      exit 1
      ;;
  esac
done
if [ "${#MODELS[@]}" -eq 0 ]; then
  MODELS=("${DEFAULT_MODELS[@]}")
fi

if [ -z "${OPENROUTER_API_KEY:-}" ]; then
  echo "[error] OPENROUTER_API_KEY is not set — export it before running this script" >&2
  exit 1
fi

if [ ! -x "$SMOKE_BIN" ]; then
  echo "[error] smoke binary not found: $SMOKE_BIN" >&2
  echo "[error] build it with: cargo build --release --example endpoint_smoke" >&2
  exit 1
fi

for tool in curl jq; do
  command -v "$tool" >/dev/null 2>&1 || { echo "[error] required tool missing: $tool" >&2; exit 1; }
done

MODELS_JSON="$(mktemp -t model-matrix-models.XXXXXX)"
trap 'rm -f "$MODELS_JSON"' EXIT

echo "[setup] fetching OpenRouter model list (for context windows) -> $MODELS_JSON" >&2
curl -fsS --max-time 60 "https://openrouter.ai/api/v1/models" \
  -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  -o "$MODELS_JSON" || {
    echo "[warn] could not fetch model list; context windows will show '?'" >&2
    echo '{"data":[]}' > "$MODELS_JSON"
  }

context_length() {
  local model="$1"
  local len
  len="$(jq -r --arg m "$model" \
    '.data[] | select(.id == $m) | .context_length // empty' "$MODELS_JSON" | head -n1)"
  if [ -n "$len" ]; then
    printf '%s' "$len"
  else
    printf '?'
  fi
}

# Extract the ms timing for a named check from one smoke run's output.
check_ms() {
  local output="$1" name="$2"
  # `|| true`: a missing check line makes grep exit 1, which pipefail would
  # otherwise turn into a fatal error under set -e.
  grep -E "^\[(PASS|FAIL)\] +${name} " <<< "$output" \
    | sed -E 's/^\[(PASS|FAIL)\] +[a-z_]+ +([0-9]+)ms .*/\2/' | head -n1 \
    || true
}

# Extract PASS/FAIL status for a named check.
check_status() {
  local output="$1" name="$2"
  grep -E "^\[(PASS|FAIL)\] +${name} " <<< "$output" \
    | sed -E 's/^\[(PASS|FAIL)\].*/\1/' | head -n1 \
    || true
}

mkdir -p "$(dirname "$OUT_MD")"

ROWS=()
total=${#MODELS[@]}
i=0
for model in "${MODELS[@]}"; do
  i=$((i + 1))
  echo "[run $i/$total] $model" >&2

  # endpoint_smoke exits 1 when any check fails — capture output, keep going.
  output="$("$SMOKE_BIN" --endpoint "$ENDPOINT" --model "$model" \
    --api-key "$OPENROUTER_API_KEY" 2>&1)" || true

  pass_count=0
  for check in "${CHECKS[@]}"; do
    if [ "$(check_status "$output" "$check")" = "PASS" ]; then
      pass_count=$((pass_count + 1))
    fi
  done

  window="$(context_length "$model")"
  plain_ms="$(check_ms "$output" plain_chat)"
  stream_ms="$(check_ms "$output" streaming)"
  tool_st="$(check_status "$output" tool_call)"
  think_st="$(check_status "$output" thinking_parse)"
  usage="$(grep -oE '\(usage: [0-9]+\+[0-9]+=[0-9]+ tokens\)' <<< "$output" | head -n1 \
    | sed -E 's/\(usage: (.*) tokens\)/\1/' || true)"

  ROWS+=("| ${model} | ${window} | ${pass_count}/7 | ${plain_ms:---} | ${stream_ms:---} | ${tool_st:---} | ${think_st:---} | ${usage:---} |")
  echo "[done $i/$total] $model -> ${pass_count}/7 passed" >&2
done

{
  echo "# Model Matrix Bench — 2026-07-25"
  echo
  echo "Endpoint: \`${ENDPOINT}\` via \`examples/endpoint_smoke.rs\` (7 checks per model)."
  echo
  echo "| model | window | pass | plain_chat ms | streaming ms | tool_call | thinking | usage tokens |"
  echo "|---|---|---|---|---|---|---|---|"
  printf '%s\n' "${ROWS[@]}"
  echo
  echo "Checks: ${CHECKS[*]}."
} > "$OUT_MD"

echo "[done] wrote $OUT_MD" >&2
