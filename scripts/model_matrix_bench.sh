#!/usr/bin/env bash
#
# model_matrix_bench.sh — run selfware's endpoint smoke probe across a fixed
# model set on OpenRouter and emit a Markdown results table merging declared
# capabilities (OpenRouter metadata) with measured ones (live probes).
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
  "qwen/qwen3.6-27b"
  "google/gemma-4-31b-it"
  "google/gemma-4-26b-a4b-it:free"
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
PROBE_PNG="$(mktemp -t model-matrix-probe.XXXXXX)"
trap 'rm -f "$MODELS_JSON" "$PROBE_PNG"' EXIT

# 1x1 red PNG used to probe multimodal support. FreeBSD base64 on stock macOS
# accepts --decode (no -d); verify the magic bytes before use.
PROBE_PNG_B64='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
printf '%s' "$PROBE_PNG_B64" | base64 --decode > "$PROBE_PNG"
if [ "$(head -c 8 "$PROBE_PNG" | od -An -tx1 | tr -d ' \n')" != "89504e470d0a1a0a" ]; then
  echo "[error] probe image at $PROBE_PNG is not a valid PNG" >&2
  exit 1
fi

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

# Declared input modalities joined with "+" (e.g. "text+image"), or "?" if unknown.
input_modalities() {
  local model="$1"
  local mods
  mods="$(jq -r --arg m "$model" \
    '.data[] | select(.id == $m) | (.architecture.input_modalities // []) | join("+")' \
    "$MODELS_JSON" | head -n1)"
  if [ -n "$mods" ]; then
    printf '%s' "$mods"
  else
    printf '?'
  fi
}

# yes/no whether a supported_parameters member is declared (e.g. tools, reasoning).
declared_flag() {
  local model="$1" param="$2"
  local found
  found="$(jq -r --arg m "$model" --arg p "$param" \
    '.data[] | select(.id == $m) | (.supported_parameters // []) | index($p) != null' \
    "$MODELS_JSON" | head -n1)"
  if [ "$found" = "true" ]; then
    printf 'yes'
  else
    printf 'no'
  fi
}

# "$1.48/$4.43" from per-token pricing strings; "?" when unknown.
price_in_out() {
  local model="$1"
  local pin pout
  pin="$(jq -r --arg m "$model" \
    '.data[] | select(.id == $m) | .pricing.prompt // empty' "$MODELS_JSON" | head -n1)"
  pout="$(jq -r --arg m "$model" \
    '.data[] | select(.id == $m) | .pricing.completion // empty' "$MODELS_JSON" | head -n1)"
  if [ -z "$pin" ] || [ -z "$pout" ]; then
    printf '?'
    return
  fi
  awk -v i="$pin" -v o="$pout" \
    'BEGIN { printf "$%.2f/$%.2f", i * 1000000, o * 1000000 }'
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
DECLARED_ROWS=()
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

  total_checks=7
  multimodal_st="—"
  mods="$(input_modalities "$model")"
  if grep -qw image <<< "$mods"; then
    echo "[run $i/$total] $model vision: re-probing with --image" >&2
    mm_output="$("$SMOKE_BIN" --endpoint "$ENDPOINT" --model "$model" \
      --api-key "$OPENROUTER_API_KEY" --image "$PROBE_PNG" 2>&1)" || true
    multimodal_st="$(check_status "$mm_output" multimodal)"
    if [ -z "$multimodal_st" ]; then
      multimodal_st="—"
    fi
    # With --image the pass count is out of 8 checks; recount from that run.
    if [ "$multimodal_st" != "—" ]; then
      total_checks=8
      pass_count=0
      for check in "${CHECKS[@]}" multimodal; do
        if [ "$(check_status "$mm_output" "$check")" = "PASS" ]; then
          pass_count=$((pass_count + 1))
        fi
      done
    fi
  fi

  window="$(context_length "$model")"
  plain_ms="$(check_ms "$output" plain_chat)"
  stream_ms="$(check_ms "$output" streaming)"
  tool_st="$(check_status "$output" tool_call)"
  think_st="$(check_status "$output" thinking_parse)"
  usage="$(grep -oE '\(usage: [0-9]+\+[0-9]+=[0-9]+ tokens\)' <<< "$output" | head -n1 \
    | sed -E 's/\(usage: (.*) tokens\)/\1/' || true)"

  ROWS+=("| ${model} | ${window} | ${pass_count}/${total_checks} | ${multimodal_st} | ${plain_ms:---} | ${stream_ms:---} | ${tool_st:---} | ${think_st:---} | ${usage:---} |")
  DECLARED_ROWS+=("| ${model} | ${mods} | $(declared_flag "$model" tools) | $(declared_flag "$model" structured_outputs) | $(declared_flag "$model" reasoning) | $(price_in_out "$model") |")
  echo "[done $i/$total] $model -> ${pass_count}/${total_checks} passed" >&2
done

{
  echo "# Model Matrix Bench — 2026-07-25"
  echo
  echo "Endpoint: \`${ENDPOINT}\` via \`examples/endpoint_smoke.rs\`. Declared = OpenRouter"
  echo "supported_parameters/modalities metadata; measured = live probes (7 checks per model,"
  echo "8 when the multimodal re-probe runs)."
  echo
  echo "## Measured capabilities"
  echo
  echo "| model | window | pass | multimodal | plain_chat ms | streaming ms | tool_call | thinking | usage tokens |"
  echo "|---|---|---|---|---|---|---|---|---|"
  printf '%s\n' "${ROWS[@]}"
  echo
  echo "Checks: ${CHECKS[*]} (+ multimodal for vision-capable models)."
  echo
  echo "## Declared capabilities"
  echo
  echo "| model | modalities | tools | structured outputs | reasoning | price in/out per M tokens |"
  echo "|---|---|---|---|---|---|"
  printf '%s\n' "${DECLARED_ROWS[@]}"
} > "$OUT_MD"

echo "[done] wrote $OUT_MD" >&2
