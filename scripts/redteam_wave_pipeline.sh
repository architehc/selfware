#!/usr/bin/env bash
# redteam_wave_pipeline.sh — one consolidation pass over the probe wave files.
#
# Pipeline (corpus integrity rule: the gate corpus only grows through
# dual-source triage — a case promotes only when a model verdict equals the
# checker verdict, never on a model opinion alone):
#   1. checker verdict dump for every wave file lacking one (chkv_<ts>)
#   2. E2 + E3 neutral triage passes for every wave file lacking them
#   3. join: promote agreements into tests/redteam/corpus/tool_attacks.jsonl
#   4. gate: cargo test --test redteam_gate_test
#
# Does NOT commit — the caller (2h commit cron or a human) commits when the
# gate is green. Refuses to run while another instance is active, and skips
# the gate (failing the pass) if the tree's gate test would race a cargo lock.
#
# Env: E2_ENDPOINT / E2_MODEL / E3_ENDPOINT / E3_MODEL override the defaults.
set -u
cd "$(dirname "$0")/.."
SELFDEV="${SELFDEV:-$HOME/selfdev}"
E2_ENDPOINT="${E2_ENDPOINT:-http://localhost:31000/v1}"
E2_MODEL="${E2_MODEL:-/home/rig/models/qwen38-unc-kt}"
E3_ENDPOINT="${E3_ENDPOINT:-https://llm.selfware.design/v1}"
E3_MODEL="${E3_MODEL:-qwen38-flash-next}"
LOCK=/tmp/redteam_wave_pipeline.lock
exec 9>"$LOCK"
flock -n 9 || { echo "pipeline already running"; exit 0; }

log() { echo "[$(date +%H:%M:%S)] $*"; }

# 1. checker verdicts for waves missing them
for f in tests/redteam/corpus/probe_wave_1*.jsonl; do
    [ -e "$f" ] || continue
    ts=$(basename "$f" .jsonl | sed 's/probe_wave_//')
    v="$SELFDEV/chkv_$ts.jsonl"
    [ -f "$v" ] && continue
    log "checker dump $ts"
    PROBE_DUMP_INPUT="$f" PROBE_DUMP_OUTPUT="$v" \
        cargo test -q --test redteam_probe_dump -- --ignored >/dev/null 2>&1
done

# 2. model verdicts for waves missing them (skip endpoints that fail a probe)
endpoint_ok() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$1/models" | grep -q 200; }
# The first five waves predate the e2v_/e3v_ naming scheme; honor their files.
early_name() { case "$1" in
    1788908382) echo wave94;; 1788909297) echo wave59;; 1788910515) echo wave85;; \
    1788910691) echo wave90;; 1788910810) echo wave61;; *) echo "";; esac }
for f in tests/redteam/corpus/probe_wave_1*.jsonl; do
    [ -e "$f" ] || continue
    ts=$(basename "$f" .jsonl | sed 's/probe_wave_//')
    en=$(early_name "$ts")
    if [ ! -f "$SELFDEV/e2v_$ts.jsonl" ] && { [ -z "$en" ] || [ ! -f "$SELFDEV/${en}_e2_verdicts.jsonl" ]; } && endpoint_ok "$E2_ENDPOINT"; then
        log "E2 triage $ts"
        python3 scripts/redteam_triage.py --endpoint "$E2_ENDPOINT" --model "$E2_MODEL" \
            --lanes 12 --shard 0/1 --probe-file "$f" \
            --verdicts-file "$SELFDEV/e2v_$ts.jsonl" >/dev/null 2>&1
    fi
    if [ ! -f "$SELFDEV/e3v_$ts.jsonl" ] && { [ -z "$en" ] || [ ! -f "$SELFDEV/${en}_e3_verdicts.jsonl" ]; } && endpoint_ok "$E3_ENDPOINT"; then
        log "E3 triage $ts"
        python3 scripts/redteam_triage.py --endpoint "$E3_ENDPOINT" --model "$E3_MODEL" \
            --lanes 16 --shard 0/1 --probe-file "$f" \
            --verdicts-file "$SELFDEV/e3v_$ts.jsonl" >/dev/null 2>&1
    fi
done

# 3. join + promote dual-source agreements
log "join"
python3 scripts/redteam_promote.py
read PROMOTED DISAGREED NOVERDICT < "$SELFDEV/last_promote_counts.txt"
log "promoted=$PROMOTED disagreed=$DISAGREED no_checker_verdict=$NOVERDICT"

# 4. gate
if [ "$PROMOTED" -gt 0 ]; then
    log "gate"
    if cargo test --test redteam_gate_test > /tmp/gate_pipeline.log 2>&1; then
        log "GATE_GREEN PROMOTIONS_READY"
    else
        log "GATE_RED — see /tmp/gate_pipeline.log"
        exit 1
    fi
else
    log "nothing promoted; gate skipped"
fi
