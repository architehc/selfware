#!/usr/bin/env bash
# sealed_dev_sandbox.sh — run a dev container that can WRITE, reach the
# INTERNET through a domain-filtering proxy, and optionally use GPUs.
# The selected workspace (default: current directory) is a WRITABLE HOST MOUNT:
# dependencies can read its secrets and persist changes there. Use a disposable
# clean copy. This is not a verified security boundary or a production proxy.
#
# The generic proxy permits arbitrary data to allowed domains; internal Docker
# networking alone does not exclude host gateway services. GPU modes expose ALL
# vendor GPUs and the host driver; RAM/CPU limits do not cap GPU memory/runtime.
# Run as a nonroot operator. GPU/device and network isolation need separate tests.
#
#   ./sealed_dev_sandbox.sh -- sh -c 'npm ci && npm test'
#   ./sealed_dev_sandbox.sh --gpu nvidia --egress sealed -- python train.py
#   ./sealed_dev_sandbox.sh --self-test
#
set -euo pipefail

IMAGE="node:22-alpine"; WORKSPACE="$PWD"; GPU="none"; EGRESS="sealed"
MEMORY="2g"; CPUS="2"; PIDS="512"; INTERACTIVE=""
ALLOW_DEFAULT="npmjs.org npmjs.com nodejs.org pypi.org pythonhosted.org files.pythonhosted.org ghcr.io github.com githubusercontent.com pytorch.org download.pytorch.org huggingface.co"
ALLOW_EXTRA=""; SELFTEST=""
PROXY_IMG="rtlab/egress-proxy:latest"; NET_INT="sds_internal"; NET_EG="sds_egress"; PROXY="sds_proxy"
OWNER_KEY="io.selfware.sealed-dev.owner"; OWNER="sealed-dev-v1"
POLICY_KEY="io.selfware.sealed-dev.domains"; TEARDOWN=""

usage(){ sed -n '2,20p' "$0"; exit 0; }
fail(){ echo "sealed_dev_sandbox: $*" >&2; exit 2; }
need_value(){ [ $# -ge 2 ] && [ -n "$2" ] && [[ "$2" != --* ]] || fail "$1 requires a value"; }
while [ $# -gt 0 ]; do case "$1" in
  --image) need_value "$@"; IMAGE="$2"; shift 2;; --workspace) need_value "$@"; WORKSPACE="$2"; shift 2;;
  --gpu) need_value "$@"; GPU="$2"; shift 2;; --egress) need_value "$@"; EGRESS="$2"; shift 2;;
  --allow) need_value "$@"; ALLOW_EXTRA="$2"; shift 2;; --memory) need_value "$@"; MEMORY="$2"; shift 2;;
  --cpus) need_value "$@"; CPUS="$2"; shift 2;; --pids) need_value "$@"; PIDS="$2"; shift 2;;
  --it) INTERACTIVE="-it"; shift;; --self-test) SELFTEST=1; shift;;
  --teardown) TEARDOWN=1; shift;;
  -h|--help) usage;; --) shift; break;; *) echo "unknown: $1" >&2; exit 2;; esac; done

case "$GPU" in none|nvidia|amd) :;; *) fail "invalid --gpu";; esac
case "$EGRESS" in none|open|sealed) :;; *) fail "invalid --egress";; esac
[[ "$IMAGE" != -* && "$IMAGE" != *[[:space:]]* ]] || fail "invalid --image"
[[ "$MEMORY" =~ ^([0-9]+([.][0-9]+)?|[.][0-9]+)([bBkKmMgGtT]([iI]?[bB])?)?$ && "$MEMORY" =~ [1-9] ]] || fail "invalid --memory"
[[ "$CPUS" =~ ^([0-9]+([.][0-9]+)?|[.][0-9]+)$ && "$CPUS" =~ [1-9] ]] || fail "invalid --cpus"
[[ "$PIDS" =~ ^[0-9]+$ && "$PIDS" =~ [1-9] ]] || fail "invalid --pids"
[ -d "$WORKSPACE" ] || fail "workspace is not a directory"
WORKSPACE=$(cd -- "$WORKSPACE" && pwd -P) || fail "cannot resolve workspace"
[[ "$WORKSPACE" != *:* && "$WORKSPACE" != *$'\n'* ]] || fail "workspace contains unsupported mount delimiters"
for domain in $ALLOW_DEFAULT $ALLOW_EXTRA; do
  [[ "$domain" =~ ^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$ ]] || fail "allowlist entries must be domain names"
done
POLICY="$ALLOW_DEFAULT $ALLOW_EXTRA"

# List first so daemon/list failures cannot be mistaken for a missing resource.
# All functions propagate failures explicitly, including when called conditionally.
resource_exists(){
  local kind="$1" name="$2" names line format='{{.Name}}'
  case "$kind" in container) format='{{.Names}}';; image) format='{{.Repository}}:{{.Tag}}';; esac
  if [ "$kind" = container ]; then
    names=$(docker container ls -a --format "$format") || return 1
  else
    names=$(docker "$kind" ls --format "$format") || return 1
  fi
  FOUND=0
  while IFS= read -r line; do [ "$line" != "$name" ] || FOUND=1; done <<< "$names"
}
require_owned(){
  local kind="$1" name="$2" label
  label=$(docker "$kind" inspect --format "{{index .Labels \"$OWNER_KEY\"}}" "$name") || return 1
  [ "$label" = "$OWNER" ] || fail "refusing unowned $kind $name; left untouched"
}
require_config_owned(){
  local kind="$1" name="$2" label
  label=$(docker "$kind" inspect --format "{{index .Config.Labels \"$OWNER_KEY\"}}" "$name") || return 1
  [ "$label" = "$OWNER" ] || fail "refusing unowned $kind $name; left untouched"
}

if [ -n "$TEARDOWN" ]; then
  # Validate every target before deleting anything. Legacy unlabeled resources
  # require an operator to inspect/remove them separately; never adopt them.
  resource_exists container "$PROXY" || fail "cannot list containers"
  REMOVE_PROXY=$FOUND
  if [ "$REMOVE_PROXY" = 1 ]; then require_config_owned container "$PROXY" || fail "cannot inspect proxy"; fi
  resource_exists network "$NET_INT" || fail "cannot list networks"; REMOVE_INT=$FOUND
  if [ "$REMOVE_INT" = 1 ]; then require_owned network "$NET_INT" || fail "cannot inspect network"; fi
  resource_exists network "$NET_EG" || fail "cannot list networks"; REMOVE_EG=$FOUND
  if [ "$REMOVE_EG" = 1 ]; then require_owned network "$NET_EG" || fail "cannot inspect network"; fi
  if [ "$REMOVE_PROXY" = 1 ]; then docker rm -f "$PROXY" || fail "proxy removal failed"; fi
  if [ "$REMOVE_INT" = 1 ]; then docker network rm "$NET_INT" || fail "network removal failed"; fi
  if [ "$REMOVE_EG" = 1 ]; then docker network rm "$NET_EG" || fail "network removal failed"; fi
  echo "owned resources torn down"; exit 0
fi

# ── the dev-contained profile (applies in every egress mode) ─────────────────
seal_flags(){
  local run_uid run_gid
  run_uid=$(id -u) || fail "cannot resolve workload UID"
  run_gid=$(id -g) || fail "cannot resolve workload GID"
  [[ "$run_uid" =~ ^[0-9]+$ ]] || fail "invalid workload UID"
  [[ "$run_gid" =~ ^[0-9]+$ ]] || fail "invalid workload GID"
  SEAL=(--rm \
    --user "$run_uid:$run_gid" \
    --cap-drop ALL --security-opt no-new-privileges \
    --read-only \
    --tmpfs /tmp:rw,noexec,nosuid,size=256m \
    --tmpfs "/home:rw,nosuid,size=64m" \
    -e HOME=/workspace -e npm_config_cache=/workspace/.npm -e PIP_CACHE_DIR=/workspace/.pipcache \
    --pids-limit "$PIDS" --memory "$MEMORY" --memory-swap "$MEMORY" --cpus "$CPUS" \
    -v "$WORKSPACE":/workspace:rw -w /workspace)
  if [ -n "$INTERACTIVE" ]; then SEAL+=("$INTERACTIVE"); fi
  # GPU overlay — untestable on Docker Desktop/mac (no device passthrough);
  # run on a Linux host with the vendor toolkit. Flags are the real ones.
  case "$GPU" in
    nvidia) SEAL+=(--gpus all) ;;                        # needs nvidia-container-toolkit
    amd)    SEAL+=(--device /dev/kfd --device /dev/dri --group-add video --group-add render) ;;
    none)   : ;;
    *) echo "bad --gpu: $GPU" >&2; exit 2;;
  esac
}

ensure_proxy(){  # allowlisted egress proxy on a two-network split
  local d actual name h escaped have_image have_int have_eg have_proxy old_policy=''
  resource_exists image "$PROXY_IMG" || return 1; have_image=$FOUND
  if [ "$have_image" = 1 ]; then
    require_config_owned image "$PROXY_IMG" || return 1
    old_policy=$(docker image inspect --format "{{index .Config.Labels \"$POLICY_KEY\"}}" "$PROXY_IMG") || return 1
  fi
  resource_exists network "$NET_INT" || return 1; have_int=$FOUND
  resource_exists network "$NET_EG" || return 1; have_eg=$FOUND
  for name in "$NET_INT" "$NET_EG"; do
    if { [ "$name" = "$NET_INT" ] && [ "$have_int" = 1 ]; } || { [ "$name" = "$NET_EG" ] && [ "$have_eg" = 1 ]; }; then
      require_owned network "$name" || return 1
      actual=$(docker network inspect --format '{{.Driver}}|{{.Internal}}' "$name") || return 1
      if [ "$name" = "$NET_INT" ]; then
        [ "$actual" = 'bridge|true' ] || fail "internal network has incompatible configuration"
      else
        [ "$actual" = 'bridge|false' ] || fail "egress network has incompatible configuration"
      fi
    fi
  done
  resource_exists container "$PROXY" || return 1; have_proxy=$FOUND
  if [ "$have_proxy" = 1 ]; then
    require_config_owned container "$PROXY" || return 1
    actual=$(docker container inspect --format "{{index .Config.Labels \"$POLICY_KEY\"}}|{{.State.Running}}|{{range \$name, \$network := .NetworkSettings.Networks}}{{\$name}} {{end}}" "$PROXY") || return 1
    [ "$actual" = "$POLICY|true|$NET_EG $NET_INT " ] || fail "proxy configuration differs; use owned --teardown before changing policy"
    [ "$old_policy" = "$POLICY" ] || fail "proxy image policy differs from active proxy"
  fi
  if [ "$have_image" = 0 ] || [ "$old_policy" != "$POLICY" ]; then
    d="$(mktemp -d)" || return 1
    { echo 'Port 8888'; echo 'Listen 0.0.0.0'; echo 'Timeout 30'; echo 'Allow 0.0.0.0/0';
      echo 'FilterDefaultDeny Yes'; echo 'FilterExtended On'; echo 'FilterCaseSensitive Off';
      echo 'Filter "/etc/tinyproxy/filter"'; echo 'ConnectPort 443'; echo 'ConnectPort 563'; } > "$d/tinyproxy.conf" || return 1
    : > "$d/filter" || return 1
    for h in $ALLOW_DEFAULT $ALLOW_EXTRA; do
      escaped=$(printf '%s\n' "$h" | sed 's/\./\\./g') || return 1
      printf '(^|\\.)%s$\n' "$escaped" >> "$d/filter" || return 1
    done
    printf 'FROM alpine:latest\nRUN apk add --no-cache tinyproxy\nCOPY tinyproxy.conf /etc/tinyproxy/tinyproxy.conf\nCOPY filter /etc/tinyproxy/filter\nCMD ["tinyproxy","-d","-c","/etc/tinyproxy/tinyproxy.conf"]\n' > "$d/Dockerfile" || return 1
    if ! docker build -q --label "$OWNER_KEY=$OWNER" --label "$POLICY_KEY=$POLICY" -t "$PROXY_IMG" "$d" >/dev/null; then
      rm -rf "$d"; return 1
    fi
    rm -rf "$d" || return 1
  fi
  if [ "$have_int" = 0 ]; then
    docker network create --driver bridge --internal --label "$OWNER_KEY=$OWNER" "$NET_INT" >/dev/null || return 1
  fi
  if [ "$have_eg" = 0 ]; then
    docker network create --driver bridge --label "$OWNER_KEY=$OWNER" "$NET_EG" >/dev/null || return 1
  fi
  if [ "$have_proxy" = 0 ]; then
    docker run -d --name "$PROXY" --label "$OWNER_KEY=$OWNER" --label "$POLICY_KEY=$POLICY" --network "$NET_EG" "$PROXY_IMG" >/dev/null || return 1
    docker network connect "$NET_INT" "$PROXY" >/dev/null || return 1
    actual=$(docker container inspect --format '{{.State.Running}}' "$PROXY") || return 1
    [ "$actual" = true ] || fail "proxy did not remain running"
  fi
}

net_flags(){ case "$EGRESS" in
  none)   NET=(--network none) ;;
  open)   NET=(--network bridge) ;;   # explicit open bridge (trusted deps only)
  sealed) ensure_proxy >&2 || return 1
          NET=(--network "$NET_INT" \
            -e "http_proxy=http://$PROXY:8888" -e "https_proxy=http://$PROXY:8888" \
            -e "HTTP_PROXY=http://$PROXY:8888" -e "HTTPS_PROXY=http://$PROXY:8888") ;;
  *) echo "bad --egress: $EGRESS" >&2; exit 2;; esac; }

seal_flags
net_flags || fail "proxy/network setup failed; workload was not started"

if [ -n "$SELFTEST" ]; then
  echo "self-test · image=$IMAGE gpu=$GPU egress=$EGRESS"; IMAGE=alpine:latest
  seal_flags
  r(){ docker run "${SEAL[@]}" "${NET[@]}" alpine:latest sh -c "$1" 2>&1; }
  # Observations only, not an escape-resistance verdict. Transport failures must
  # still propagate instead of being hidden by printf's successful exit status.
  show_probe(){ local observed; observed=$(r "$2") || return 1; printf '  %-30s %s\n' "$1" "$observed"; }
  show_probe "write /workspace"     'echo x>o && echo ALLOW||echo deny'
  show_probe "write / (rootfs)"     'touch /e 2>/dev/null&&echo LEAK||echo blocked'
  show_probe "uid (non-root?)"      'id -u'
  show_probe "off-list exfil"       'wget -T6 -qO/dev/null https://example.com/&&echo REACHED||echo blocked'
  show_probe "LAN/host pivot"       'wget -T5 -qO/dev/null http://host.docker.internal/&&echo REACHED||echo blocked'
  show_probe "direct (proxy bypass)" 'wget -T5 -qY off -O/dev/null https://1.1.1.1/&&echo REACHED||echo blocked'
  exit 0
fi

[ $# -eq 0 ] && set -- sh
exec docker run "${SEAL[@]}" "${NET[@]}" "$IMAGE" "$@"
