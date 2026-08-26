#!/bin/bash
# run-contained.sh — run selfware against the uncensored endpoint inside a
# locked-down container: no host mounts, no privileges, and egress restricted
# to the model endpoint only (host iptables DOCKER-USER allowlist).
#
# Usage:
#   deploy/contained/run-contained.sh build        # build the image
#   deploy/contained/run-contained.sh lockdown     # create net + iptables rules
#   deploy/contained/run-contained.sh run "task"   # contained agent run
#   deploy/contained/run-contained.sh unlock       # remove iptables rules
set -euo pipefail

IMAGE="${IMAGE:-selfware-contained:latest}"
NET_NAME="contained-egress"
NET_SUBNET="172.29.0.0/24"
ALLOWED_HOST="${ALLOWED_HOST:-e97b8112d15a.ngrok.app}"
SELFWARE_BIN="${SELFWARE_BINARY:-/home/rig/harbor-agents/dist/selfware-bullseye}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

case "${1:-}" in
  build)
    cp "$SELFWARE_BIN" "$HERE/selfware-bullseye"
    docker build -t "$IMAGE" "$HERE"
    ;;
  lockdown)
    docker network create --subnet "$NET_SUBNET" "$NET_NAME" 2>/dev/null || true
    # Resolve the endpoint's current IPv4 addresses (the contained bridge is
    # v4-only; ngrok rotates IPs — re-run lockdown if the endpoint stops
    # answering). SUDO_PW may carry the sudo password for non-interactive runs.
    SUDO="sudo"; [ -n "${SUDO_PW:-}" ] && SUDO="sudo -S"
    [ -n "${SUDO_PW:-}" ] && export SUDO_PASS="$SUDO_PW"
    mapfile -t IPS < <(dig +short A "$ALLOWED_HOST" | grep -E '^[0-9.]+$' | sort -u)
    if [ ${#IPS[@]} -eq 0 ]; then echo "cannot resolve $ALLOWED_HOST (A)" >&2; exit 1; fi
    echo "allowing egress to: ${IPS[*]}"
    for ip in "${IPS[@]}"; do
      if [ -n "${SUDO_PW:-}" ]; then
        echo "$SUDO_PW" | sudo -S iptables -C DOCKER-USER -s "$NET_SUBNET" -d "$ip" -j ACCEPT 2>/dev/null || \
          echo "$SUDO_PW" | sudo -S iptables -I DOCKER-USER -s "$NET_SUBNET" -d "$ip" -j ACCEPT
      else
        sudo iptables -C DOCKER-USER -s "$NET_SUBNET" -d "$ip" -j ACCEPT 2>/dev/null || \
          sudo iptables -I DOCKER-USER -s "$NET_SUBNET" -d "$ip" -j ACCEPT
      fi
    done
    if [ -n "${SUDO_PW:-}" ]; then
      echo "$SUDO_PW" | sudo -S iptables -C DOCKER-USER -s "$NET_SUBNET" -j DROP 2>/dev/null || \
        echo "$SUDO_PW" | sudo -S iptables -A DOCKER-USER -s "$NET_SUBNET" -j DROP
    else
      sudo iptables -C DOCKER-USER -s "$NET_SUBNET" -j DROP 2>/dev/null || \
        sudo iptables -A DOCKER-USER -s "$NET_SUBNET" -j DROP
    fi
    echo "egress locked to $ALLOWED_HOST for $NET_SUBNET"
    ;;
  unlock)
    SUDO="sudo"; [ -n "${SUDO_PW:-}" ] && SUDO="sudo -S"
    if [ -n "${SUDO_PW:-}" ]; then
      echo "$SUDO_PW" | sudo -S iptables-save | grep "DOCKER-USER.*$NET_SUBNET" | while read -r line; do
        echo "$SUDO_PW" | sudo -S iptables -D DOCKER-USER ${line#-A } || true
      done
    else
      sudo iptables-save | grep "DOCKER-USER.*$NET_SUBNET" | while read -r line; do
        sudo iptables -D DOCKER-USER ${line#-A } || true
      done
    fi
    docker network rm "$NET_NAME" 2>/dev/null || true
    echo "egress lockdown removed"
    ;;
  run)
    shift
    TASK="${*?'usage: run \"task\"'}"
    docker run --rm \
      --network "$NET_NAME" \
      --read-only --tmpfs /tmp:rw,noexec,nosuid,size=256m \
      --cap-drop ALL --security-opt no-new-privileges \
      --memory 4g --cpus 2 --pids-limit 256 \
      -e SELFWARE_API_KEY="${SELFWARE_API_KEY:-unused}" \
      "$IMAGE" run -m yolo -c /etc/selfware/config.toml "$TASK"
    ;;
  *) echo "usage: $0 build|lockdown|run \"task\"|unlock" >&2; exit 1 ;;
esac
