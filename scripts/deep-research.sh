#!/bin/bash
# Deep-research v1: GLM-5.2 strategizes, Fara1.5-27B browses and collects.
#
#   scripts/deep-research.sh "your research question"
#
# Flow:
#   1. GLM (:30000, strategist) turns the question into 3-4 focused sub-queries.
#   2. For each sub-query, a selfware agent running ON Fara (:8090) uses
#      browser_fetch / browser_links / page_control to search and collect,
#      writing notes to ./research/<run>/NN-notes.md.
#   3. GLM synthesizes the collected notes into ./research/<run>/REPORT.md.
#
# GLM's server caps prompt+completion at 4096 tokens — every GLM call here
# keeps its prompt compact (notes are truncated before synthesis).
#
# Strategist selection: local GLM :30000 is the default, but WITHOUT the V100
# expert cache it decodes at ~1 t/s (the V100 is Fara's). For interactive
# research runs use the hosted strategist instead:
#   STRATEGIST=openrouter scripts/deep-research.sh "question"
# FAST=1 uses fara-research-fast.toml (no-think collection: ~3x quicker,
# noticeably shallower notes).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

RESEARCH_CONFIG="fara-research.toml"
[ "${FAST:-0}" = "1" ] && RESEARCH_CONFIG="fara-research-fast.toml"

QUESTION="${1:?usage: deep-research.sh \"question\"}"
# Fara's llama-server (systemd fara15-server.service) requires an API key;
# pass it via env so no secret lands in this repo. GLM ignores it.
export SELFWARE_API_KEY="$(cat "$HOME/fara/api-key.local")"

if [ "${STRATEGIST:-local}" = "openrouter" ]; then
  export STRATEGIST_ENDPOINT="https://openrouter.ai/api/v1"
  export STRATEGIST_MODEL="z-ai/glm-5.2"
  export STRATEGIST_KEY="$(cat "$HOME/.openrouter_key")"
else
  export STRATEGIST_ENDPOINT="${STRATEGIST_ENDPOINT:-http://127.0.0.1:30000/v1}"
  export STRATEGIST_MODEL="${STRATEGIST_MODEL:-/home/rig/models/glm-5.2-awq-int4}"
  export STRATEGIST_KEY="${STRATEGIST_KEY:-}"
fi
RUN_DIR="research/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$RUN_DIR"
echo "$QUESTION" > "$RUN_DIR/QUESTION.txt"

glm_ask() { # glm_ask <prompt> [max_tokens] — one compact GLM completion, prints content
  python3 - "$1" "${2:-1024}" <<'EOF'
import json, os, sys, urllib.request
prompt = sys.argv[1][:8000]
headers = {"Content-Type": "application/json"}
if os.environ.get("STRATEGIST_KEY"):
    headers["Authorization"] = "Bearer " + os.environ["STRATEGIST_KEY"]
req = urllib.request.Request(
    os.environ["STRATEGIST_ENDPOINT"] + "/chat/completions",
    data=json.dumps({
        "model": os.environ["STRATEGIST_MODEL"],
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": int(sys.argv[2]), "temperature": 0.3,
    }).encode(), headers=headers)
with urllib.request.urlopen(req, timeout=3600) as r:
    msg = json.load(r)["choices"][0]["message"]
# Reasoning models may burn the whole budget thinking, leaving content null.
content = msg.get("content") or ""
if not content.strip():
    sys.stderr.write("glm_ask: empty content (reasoning consumed max_tokens?)\n")
    sys.exit(1)
print(content)
EOF
}

echo "== [1/3] GLM strategy =="
PLAN=$(glm_ask "You are the strategist for a web research agent. Break this question into exactly 3 focused web-search sub-queries, one per line, no numbering, no commentary: $QUESTION")
echo "$PLAN" | tee "$RUN_DIR/PLAN.txt"
if [ -z "$(echo "$PLAN" | tr -d '[:space:]')" ]; then
  echo "ERROR: strategist returned an empty plan (GLM down or timed out) — aborting." >&2
  exit 1
fi

echo "== [2/3] Fara collection =="
i=0
while IFS= read -r sub; do
  [ -z "$sub" ] && continue
  i=$((i+1))
  notes="$RUN_DIR/$(printf '%02d' "$i")-notes.md"
  echo "-- sub-query $i: $sub"
  timeout 1500 ./target/release/selfware run -m yolo -c "$RESEARCH_CONFIG" \
    "Web research task: '$sub'. Recipe, follow it exactly: (1) browser_fetch https://html.duckduckgo.com/html/?q=<url-encoded query>. (2) From those results pick the 2 most promising URLs and browser_fetch each — hard limit of 3 browser_fetch calls total for the whole task, no page_control. (3) file_write concise findings with source URLs to $notes. Finish immediately after the file is written." \
    || echo "(sub-query $i agent failed)" >> "$notes"
done <<< "$PLAN"

echo "== [3/3] GLM synthesis =="
NOTES=$(head -c 2500 "$RUN_DIR"/*-notes.md 2>/dev/null)
glm_ask "Synthesize a concise research answer with source URLs. Question: $QUESTION
Collected notes:
$NOTES" "$([ "${STRATEGIST:-local}" = "openrouter" ] && echo 4096 || echo 2048)" | tee "$RUN_DIR/REPORT.md"
echo "== done: $RUN_DIR/REPORT.md =="
# Best-effort: surface the report in Open WebUI's Notes (if the UI is up).
scripts/research-to-webui.sh "$RUN_DIR" || echo "(Open WebUI import skipped)"
