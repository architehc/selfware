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
set -euo pipefail
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

mkdir -p "$SELFDEV"

# 1. Require complete current-input checker receipts, including after interrupted dumps.
for f in tests/redteam/corpus/probe_wave_1*.jsonl; do
    [ -e "$f" ] || continue
    ts=$(basename "$f" .jsonl | sed 's/probe_wave_//')
    v="$SELFDEV/chkv_$ts.jsonl"
    if python3 scripts/redteam_verdicts.py --kind checker --probe-file "$f" \
        --verdicts-file "$v" >/dev/null; then
        continue
    fi
    log "checker dump $ts"
    PROBE_DUMP_INPUT="$f" PROBE_DUMP_OUTPUT="$v" \
        cargo test -q --test redteam_probe_dump -- --ignored >/dev/null 2>&1
    python3 scripts/redteam_verdicts.py --kind checker --probe-file "$f" \
        --verdicts-file "$v" >/dev/null
done

# 2. Resume missing input-bound verdicts, including partially written files.
endpoint_ok() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$1/models" | grep -q 200; }
triage_incomplete=0
triage_wave() {
    local label="$1" endpoint="$2" model="$3" lanes="$4" probe="$5" verdicts="$6"
    if python3 scripts/redteam_triage.py --shard 0/1 --probe-file "$probe" \
        --verdicts-file "$verdicts" --check-complete >/dev/null; then
        return 0
    fi
    if ! endpoint_ok "$endpoint"; then
        log "$label unavailable; wave remains incomplete: $probe"
        return 1
    fi
    log "$label triage $probe"
    python3 scripts/redteam_triage.py --endpoint "$endpoint" --model "$model" \
        --lanes "$lanes" --shard 0/1 --probe-file "$probe" \
        --verdicts-file "$verdicts"
}
# The first five waves predate the e2v_/e3v_ naming scheme; honor their files.
early_name() { case "$1" in
    1788908382) echo wave94;; 1788909297) echo wave59;; 1788910515) echo wave85;; \
    1788910691) echo wave90;; 1788910810) echo wave61;; *) echo "";; esac }
for f in tests/redteam/corpus/probe_wave_1*.jsonl; do
    [ -e "$f" ] || continue
    ts=$(basename "$f" .jsonl | sed 's/probe_wave_//')
    en=$(early_name "$ts")
    e2v="$SELFDEV/e2v_$ts.jsonl"
    e3v="$SELFDEV/e3v_$ts.jsonl"
    if [ ! -f "$e2v" ] && [ -n "$en" ] && [ -f "$SELFDEV/${en}_e2_verdicts.jsonl" ]; then
        e2v="$SELFDEV/${en}_e2_verdicts.jsonl"
    fi
    if [ ! -f "$e3v" ] && [ -n "$en" ] && [ -f "$SELFDEV/${en}_e3_verdicts.jsonl" ]; then
        e3v="$SELFDEV/${en}_e3_verdicts.jsonl"
    fi
    triage_wave E2 "$E2_ENDPOINT" "$E2_MODEL" 12 "$f" "$e2v" || triage_incomplete=1
    triage_wave E3 "$E3_ENDPOINT" "$E3_MODEL" 16 "$f" "$e3v" || triage_incomplete=1
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

if [ "$triage_incomplete" -ne 0 ]; then
    log "TRIAGE_INCOMPLETE — rerun to resume missing verdicts"
    exit 1
fi
