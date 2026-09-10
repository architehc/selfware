#!/usr/bin/env bash
# sealed_dev_sandbox.sh — run a dev container that can WRITE, reach the
# INTERNET (allowlisted), and use a GPU, while the blast radius of a malicious
# npm/pip package is contained: non-root, caps dropped, read-only rootfs except
# the project, no host secrets, resource-limited, and NO path to your LAN/host.
#
# Threat model: not kernel escape — supply-chain. A postinstall script must not
# be able to read ~/.aws, persist to the host, mine on idle cycles unbounded,
# exfil to an arbitrary domain, or pivot to your LAN. This profile stops all of
# those while staying usable for real dev.
#
#   ./sealed_dev_sandbox.sh -- npm ci && npm test
#   ./sealed_dev_sandbox.sh --gpu nvidia --egress sealed -- python train.py
#   ./sealed_dev_sandbox.sh --self-test
#
set -euo pipefail

IMAGE="node:22-alpine"; WORKSPACE="$PWD"; GPU="none"; EGRESS="sealed"
MEMORY="2g"; CPUS="2"; PIDS="512"; INTERACTIVE=""
ALLOW_DEFAULT="npmjs.org npmjs.com nodejs.org pypi.org pythonhosted.org files.pythonhosted.org ghcr.io github.com githubusercontent.com pytorch.org download.pytorch.org huggingface.co"
ALLOW_EXTRA=""; SELFTEST=""
PROXY_IMG="rtlab/egress-proxy"; NET_INT="sds_internal"; NET_EG="sds_egress"; PROXY="sds_proxy"

usage(){ sed -n '2,20p' "$0"; exit 0; }
while [ $# -gt 0 ]; do case "$1" in
  --image) IMAGE="$2"; shift 2;; --workspace) WORKSPACE="$2"; shift 2;;
  --gpu) GPU="$2"; shift 2;; --egress) EGRESS="$2"; shift 2;;
  --allow) ALLOW_EXTRA="$2"; shift 2;; --memory) MEMORY="$2"; shift 2;;
  --cpus) CPUS="$2"; shift 2;; --pids) PIDS="$2"; shift 2;;
  --it) INTERACTIVE="-it"; shift;; --self-test) SELFTEST=1; shift;;
  --teardown) docker rm -f "$PROXY" 2>/dev/null||true; docker network rm "$NET_INT" "$NET_EG" 2>/dev/null||true; echo "torn down"; exit 0;;
  -h|--help) usage;; --) shift; break;; *) echo "unknown: $1" >&2; exit 2;; esac; done

# ── the dev-contained profile (applies in every egress mode) ─────────────────
seal_flags(){
  printf '%s\n' --rm $INTERACTIVE \
    --user "$(id -u):$(id -g)" \
    --cap-drop ALL --security-opt no-new-privileges \
    --read-only \
    --tmpfs /tmp:rw,noexec,nosuid,size=256m \
    --tmpfs "/home:rw,nosuid,size=64m" \
    -e HOME=/workspace -e npm_config_cache=/workspace/.npm -e PIP_CACHE_DIR=/workspace/.pipcache \
    --pids-limit "$PIDS" --memory "$MEMORY" --memory-swap "$MEMORY" --cpus "$CPUS" \
    -v "$WORKSPACE":/workspace:rw -w /workspace
  # GPU overlay — untestable on Docker Desktop/mac (no device passthrough);
  # run on a Linux host with the vendor toolkit. Flags are the real ones.
  case "$GPU" in
    nvidia) printf '%s\n' --gpus all ;;                        # needs nvidia-container-toolkit
    amd)    printf '%s\n' --device /dev/kfd --device /dev/dri --group-add video --group-add render ;;
    none)   : ;;
    *) echo "bad --gpu: $GPU" >&2; exit 2;;
  esac
}

ensure_proxy(){  # allowlisted egress proxy on a two-network split
  if ! docker image inspect "$PROXY_IMG" >/dev/null 2>&1; then
    local d; d="$(mktemp -d)"
    { echo 'Port 8888'; echo 'Listen 0.0.0.0'; echo 'Timeout 30'; echo 'Allow 0.0.0.0/0';
      echo 'FilterDefaultDeny Yes'; echo 'FilterExtended On'; echo 'FilterCaseSensitive Off';
      echo 'Filter "/etc/tinyproxy/filter"'; echo 'ConnectPort 443'; echo 'ConnectPort 563'; } > "$d/tinyproxy.conf"
    : > "$d/filter"; for h in $ALLOW_DEFAULT $ALLOW_EXTRA; do
      printf '(^|\\.)%s$\n' "$(echo "$h" | sed 's/\./\\./g')" >> "$d/filter"; done
    printf 'FROM alpine:latest\nRUN apk add --no-cache tinyproxy\nCOPY tinyproxy.conf /etc/tinyproxy/tinyproxy.conf\nCOPY filter /etc/tinyproxy/filter\nCMD ["tinyproxy","-d","-c","/etc/tinyproxy/tinyproxy.conf"]\n' > "$d/Dockerfile"
    docker build -q -t "$PROXY_IMG" "$d" >/dev/null; rm -rf "$d"
  fi
  docker network create --internal "$NET_INT" >/dev/null 2>&1 || true
  docker network create "$NET_EG" >/dev/null 2>&1 || true
  if ! docker ps --format '{{.Names}}' | grep -qx "$PROXY"; then
    docker rm -f "$PROXY" >/dev/null 2>&1 || true
    docker run -d --name "$PROXY" --network "$NET_EG" "$PROXY_IMG" >/dev/null
    docker network connect "$NET_INT" "$PROXY" >/dev/null; sleep 1
  fi
}

net_flags(){ case "$EGRESS" in
  none)   printf '%s\n' --network none ;;
  open)   printf '%s\n' --cap-drop ALL ;;   # default bridge (LAN reachable — trusted deps only)
  sealed) ensure_proxy >&2
          printf '%s\n' --network "$NET_INT" \
            -e "http_proxy=http://$PROXY:8888" -e "https_proxy=http://$PROXY:8888" \
            -e "HTTP_PROXY=http://$PROXY:8888" -e "HTTPS_PROXY=http://$PROXY:8888" ;;
  *) echo "bad --egress: $EGRESS" >&2; exit 2;; esac; }

SEAL=(); while IFS= read -r _l; do SEAL+=("$_l"); done < <(seal_flags)
NET=(); while IFS= read -r _l; do NET+=("$_l"); done < <(net_flags)

if [ -n "$SELFTEST" ]; then
  echo "self-test · image=$IMAGE gpu=$GPU egress=$EGRESS"; IMAGE=alpine:latest
  SEAL=(); while IFS= read -r _l; do SEAL+=("$_l"); done < <(seal_flags)
  r(){ docker run "${SEAL[@]}" "${NET[@]}" alpine:latest sh -c "$1" 2>&1; }
  printf '  %-30s %s\n' "write /workspace"     "$(r 'echo x>o && echo ALLOW||echo deny')"
  printf '  %-30s %s\n' "write / (rootfs)"     "$(r 'touch /e 2>/dev/null&&echo LEAK||echo blocked')"
  printf '  %-30s %s\n' "uid (non-root?)"      "$(r 'id -u')"
  printf '  %-30s %s\n' "off-list exfil"       "$(r 'wget -T6 -qO/dev/null https://example.com/&&echo REACHED||echo blocked')"
  printf '  %-30s %s\n' "LAN/host pivot"       "$(r 'wget -T5 -qO/dev/null http://host.docker.internal/&&echo REACHED||echo blocked')"
  printf '  %-30s %s\n' "direct (proxy bypass)" "$(r 'wget -T5 -qY off -O/dev/null https://1.1.1.1/&&echo REACHED||echo blocked')"
  exit 0
fi

[ $# -eq 0 ] && set -- sh
exec docker run "${SEAL[@]}" "${NET[@]}" "$IMAGE" "$@"
