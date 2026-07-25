#!/usr/bin/env bash
#
# context_quality_bench.sh — measure whether the auto-picked context tiers
# (map / lite) preserve answer quality, using a fixed question set and a
# fixed judge model (moonshotai/kimi-k3 on OpenRouter).
#
# Tier artifacts come from `examples/tier_bench.rs --dump DIR`. We use only
# map.txt and lite.txt: compact.txt is also dumped but is ~1.1M tokens, far
# beyond the judge model's context window, so it is deliberately excluded.
#
# Usage:
#   OPENROUTER_API_KEY=... scripts/context_quality_bench.sh
#   OPENROUTER_API_KEY=... CONTEXT_QUALITY_WORKDIR=/tmp/x scripts/context_quality_bench.sh
#
# Output: docs/quant_bench/2026-07-25-context-quality.md

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIER_BENCH_BIN="$ROOT_DIR/target/release/examples/tier_bench"
WORKDIR="${CONTEXT_QUALITY_WORKDIR:-/tmp/context-quality-bench}"
TIER_DIR="$WORKDIR/tiers"
OUT_MD="$ROOT_DIR/docs/quant_bench/2026-07-25-context-quality.md"

JUDGE_MODEL="moonshotai/kimi-k3"
API_URL="https://openrouter.ai/api/v1/chat/completions"
SYSTEM_PROMPT="You answer questions about a codebase using only the provided context artifact. If the answer is not in the artifact, say NOT_IN_CONTEXT."

# question|expected_substring (case-insensitive match on the response)
QUESTIONS=(
  "Which function resolves the auto context mode to a concrete tier?|fit_tier"
  "What type does the fit_tier function return?|FitOutcome"
  "Which struct measures the real token cost of each context tier?|TierMeasurer"
  "Which module strips comments and cfg(test) blocks to reduce source?|context_reduce"
  "What does the Map tier emit for each component?|card"
)

# compact.txt intentionally absent from this list — see header comment.
TIERS=(map lite)

if [ -z "${OPENROUTER_API_KEY:-}" ]; then
  echo "[error] OPENROUTER_API_KEY is not set — export it before running this script" >&2
  exit 1
fi

for tool in curl jq; do
  command -v "$tool" >/dev/null 2>&1 || { echo "[error] required tool missing: $tool" >&2; exit 1; }
done

# Ensure tier artifacts exist; dump them with the Rust example if not.
if [ ! -f "$TIER_DIR/map.txt" ] || [ ! -f "$TIER_DIR/lite.txt" ]; then
  if [ ! -x "$TIER_BENCH_BIN" ]; then
    echo "[error] tier_bench binary not found: $TIER_BENCH_BIN" >&2
    echo "[error] build it with: cargo build --release --example tier_bench" >&2
    exit 1
  fi
  echo "[setup] dumping tier artifacts -> $TIER_DIR" >&2
  # tier_bench builds the graph from ./src — must run from the repo root.
  (cd "$ROOT_DIR" && ./target/release/examples/tier_bench --dump "$TIER_DIR") >&2
fi

for tier in "${TIERS[@]}"; do
  [ -f "$TIER_DIR/$tier.txt" ] || {
    echo "[error] missing tier artifact: $TIER_DIR/$tier.txt" >&2
    exit 1
  }
done

# Run one question against one tier artifact. Prints "<result>|<latency_s>"
# where result is CORRECT, NOT_IN_CONTEXT, MISS or ERROR.
ask() {
  local tier="$1" question="$2" expected="$3"
  local artifact="$TIER_DIR/$tier.txt"

  local body
  body="$(jq -n \
    --rawfile artifact "$artifact" \
    --arg model "$JUDGE_MODEL" \
    --arg sys "$SYSTEM_PROMPT" \
    --arg q "$question" \
    '{
       model: $model,
       max_tokens: 2048,
       temperature: 0,
       messages: [
         {role: "system", content: $sys},
         {role: "user", content: ($artifact + "\n\nQuestion: " + $q)}
       ]
     }')"

  local resp_file http_out latency
  resp_file="$(mktemp -t context-quality-resp.XXXXXX)"
  http_out="$(curl -sS --max-time 300 -o "$resp_file" -w '%{http_code} %{time_total}' \
    -X POST "$API_URL" \
    -H "Authorization: Bearer $OPENROUTER_API_KEY" \
    -H "Content-Type: application/json" \
    -d "$body")" || {
      rm -f "$resp_file"
      printf 'ERROR|-'
      return
    }
  latency="${http_out##* }"

  local content
  content="$(jq -r '.choices[0].message.content // empty' "$resp_file" 2>/dev/null || true)"
  rm -f "$resp_file"

  if [ -z "$content" ]; then
    printf 'ERROR|%s' "$latency"
    return
  fi

  local lower_content lower_expected
  lower_content="$(printf '%s' "$content" | tr '[:upper:]' '[:lower:]')"
  lower_expected="$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')"

  if [[ "$lower_content" == *"$lower_expected"* ]]; then
    printf 'CORRECT|%s' "$latency"
  elif [[ "$content" == *NOT_IN_CONTEXT* ]]; then
    printf 'NOT_IN_CONTEXT|%s' "$latency"
  else
    printf 'MISS|%s' "$latency"
  fi
}

mkdir -p "$(dirname "$OUT_MD")"

# bash 3.2 (macOS stock) has no associative arrays — keep per-tier state in
# plain variables keyed off the fixed map/lite order in $TIERS.
ROWS=()
CORRECT_MAP=0
CORRECT_LITE=0

total=$(( ${#TIERS[@]} * ${#QUESTIONS[@]} ))
i=0
for qpair in "${QUESTIONS[@]}"; do
  question="${qpair%%|*}"
  expected="${qpair##*|}"
  map_result="-" map_latency="-" lite_result="-" lite_latency="-"
  for tier in "${TIERS[@]}"; do
    i=$((i + 1))
    echo "[run $i/$total] tier=$tier q=${question:0:60}..." >&2
    out="$(ask "$tier" "$question" "$expected")"
    result="${out%%|*}"
    latency="${out##*|}"
    if [ "$tier" = "map" ]; then
      map_result="$result" map_latency="$latency"
      if [ "$result" = "CORRECT" ]; then CORRECT_MAP=$((CORRECT_MAP + 1)); fi
    else
      lite_result="$result" lite_latency="$latency"
      if [ "$result" = "CORRECT" ]; then CORRECT_LITE=$((CORRECT_LITE + 1)); fi
    fi
    echo "[done $i/$total] tier=$tier -> ${result} (${latency}s)" >&2
  done

  ROWS+=("| ${question} | ${map_result} | ${lite_result} | map ${map_latency}s / lite ${lite_latency}s |")
done

n_questions=${#QUESTIONS[@]}

if [ "$CORRECT_MAP" -eq "$CORRECT_LITE" ]; then
  takeaway="Map and Lite tiers answer equally well (${CORRECT_MAP}/${n_questions} each) — the cheaper tier preserves answer quality on this question set."
elif [ "$CORRECT_LITE" -gt "$CORRECT_MAP" ]; then
  takeaway="Lite outperforms Map (${CORRECT_LITE}/${n_questions} vs ${CORRECT_MAP}/${n_questions}) — full source in Lite carries answers the Map cards lack."
else
  takeaway="Map outperforms Lite (${CORRECT_MAP}/${n_questions} vs ${CORRECT_LITE}/${n_questions}) — the component index alone preserves answer quality better than raw source."
fi
error_count="$(printf '%s\n' "${ROWS[@]}" | grep -c 'ERROR' || true)"
if [ "$error_count" -gt 0 ]; then
  takeaway="WARNING: ${error_count} call(s) errored — results incomplete, re-run before trusting them. ${takeaway}"
fi

{
  echo "# Context Quality Bench — 2026-07-25"
  echo
  echo "Judge model: \`${JUDGE_MODEL}\` (max_tokens=2048, temperature=0)."
  echo "Artifacts: \`$TIER_DIR/{map,lite}.txt\` from \`examples/tier_bench.rs --dump\`."
  echo
  echo "## Per-tier score"
  echo
  echo "| tier | correct |"
  echo "|---|---|"
  echo "| map | ${CORRECT_MAP}/${n_questions} |"
  echo "| lite | ${CORRECT_LITE}/${n_questions} |"
  echo
  echo "## Per-question results"
  echo
  echo "| question | map | lite | latency |"
  echo "|---|---|---|---|"
  printf '%s\n' "${ROWS[@]}"
  echo
  echo "**Takeaway:** ${takeaway}"
} > "$OUT_MD"

echo "[done] wrote $OUT_MD" >&2
