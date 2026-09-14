#!/usr/bin/env bash
# sealed_dev_sandbox.sh — run a dev container that can WRITE, reach the
# INTERNET (allowlisted), and use a GPU, while the blast radius of a malicious
# npm/pip package is contained.
set -euo pipefail

OWNER_KEY="io.selfware.sealed-dev.owner"
OWNER="sealed-dev-v1"
DOMAINS_KEY="io.selfware.sealed-dev.domains"

IMAGE="node:22-alpine"
WORKSPACE="$PWD"
GPU="none"
EGRESS="sealed"
MEMORY="2g"
CPUS="2"
PIDS="512"
INTERACTIVE=""
ALLOW_DEFAULT="npmjs.org npmjs.com nodejs.org pypi.org pythonhosted.org files.pythonhosted.org ghcr.io github.com githubusercontent.com pytorch.org download.pytorch.org huggingface.co"
ALLOW_EXTRA=""
SELFTEST=""
TEARDOWN=""

PROXY_IMG="rtlab/egress-proxy:latest"
NET_INT="sds_internal"
NET_EG="sds_egress"
PROXY="sds_proxy"

usage(){ sed -n '2,20p' "$0"; exit 0; }

while [ $# -gt 0 ]; do
  case "$1" in
    --image)
      [ $# -ge 2 ] || { echo "missing argument for --image" >&2; exit 2; }
      [[ "$2" =~ ^- ]] && { echo "invalid --image" >&2; exit 2; }
      IMAGE="$2"; shift 2 ;;
    --workspace)
      [ $# -ge 2 ] || { echo "missing argument for --workspace" >&2; exit 2; }
      WORKSPACE="$2"; shift 2 ;;
    --gpu)
      [ $# -ge 2 ] || { echo "missing argument for --gpu" >&2; exit 2; }
      [[ "$2" =~ ^- ]] && { echo "invalid --gpu" >&2; exit 2; }
      GPU="$2"; shift 2 ;;
    --egress)
      [ $# -ge 2 ] || { echo "missing argument for --egress" >&2; exit 2; }
      [[ "$2" =~ ^- ]] && { echo "invalid --egress" >&2; exit 2; }
      EGRESS="$2"; shift 2 ;;
    --allow)
      [ $# -ge 2 ] || { echo "missing argument for --allow" >&2; exit 2; }
      ALLOW_EXTRA="$2"; shift 2 ;;
    --memory)
      [ $# -ge 2 ] || { echo "missing argument for --memory" >&2; exit 2; }
      MEMORY="$2"; shift 2 ;;
    --cpus)
      [ $# -ge 2 ] || { echo "missing argument for --cpus" >&2; exit 2; }
      CPUS="$2"; shift 2 ;;
    --pids)
      [ $# -ge 2 ] || { echo "missing argument for --pids" >&2; exit 2; }
      PIDS="$2"; shift 2 ;;
    --it) INTERACTIVE="-it"; shift ;;
    --self-test) SELFTEST=1; shift ;;
    --teardown) TEARDOWN=1; shift ;;
    -h|--help) usage ;;
    --) shift; break ;;
    *) echo "unknown: $1" >&2; exit 2 ;;
  esac
done

# Validate basic options before calling any external command
case "$GPU" in
  none|nvidia|amd) ;;
  *) echo "invalid --gpu: $GPU" >&2; exit 2 ;;
esac

case "$EGRESS" in
  none|open|sealed) ;;
  *) echo "invalid --egress: $EGRESS" >&2; exit 2 ;;
esac

[ -d "$WORKSPACE" ] || { echo "workspace directory does not exist: $WORKSPACE" >&2; exit 2; }

# Validate CPUs (float or int > 0)
if ! [[ "$CPUS" =~ ^[0-9]*\.?[0-9]+$ ]] || [ "$(awk "BEGIN {print ($CPUS > 0)}")" != "1" ]; then
  echo "invalid --cpus: $CPUS" >&2; exit 2
fi

# Validate PIDs (int >= 1)
if ! [[ "$PIDS" =~ ^[0-9]+$ ]] || [ "$PIDS" -lt 1 ]; then
  echo "invalid --pids: $PIDS" >&2; exit 2
fi

# Validate Memory (e.g. 256m, 2g, but not 0g)
if ! [[ "$MEMORY" =~ ^([1-9][0-9]*)([bkmgBKMG])$ ]]; then
  echo "invalid --memory: $MEMORY" >&2; exit 2
fi

# Validate allow domain names
for dom in $ALLOW_EXTRA; do
  if [[ "$dom" =~ [^a-zA-Z0-9.-] ]]; then
    echo "invalid domain in --allow: $dom" >&2; exit 2
  fi
done

# Resolve UID and GID strictly
WORKLOAD_UID="$(id -u 2>&1)" || { echo "cannot resolve workload UID" >&2; exit 2; }
if ! [[ "$WORKLOAD_UID" =~ ^[0-9]+$ ]]; then
  echo "invalid workload UID: $WORKLOAD_UID" >&2; exit 2
fi

WORKLOAD_GID="$(id -g 2>&1)" || { echo "cannot resolve workload GID" >&2; exit 2; }
if ! [[ "$WORKLOAD_GID" =~ ^[0-9]+$ ]]; then
  echo "invalid workload GID: $WORKLOAD_GID" >&2; exit 2
fi

# Teardown handler
if [ -n "$TEARDOWN" ]; then
  EXISTING_IMAGES="$(docker image ls)"
  if echo "$EXISTING_IMAGES" | grep -qx "$PROXY_IMG"; then
    OWNER_VAL="$(docker image inspect --format '{{index .Config.Labels "'"$OWNER_KEY"'"}}' "$PROXY_IMG")"
    [ "$OWNER_VAL" = "$OWNER" ] || { echo "unowned image $PROXY_IMG" >&2; exit 2; }
  fi

  EXISTING_CONTAINERS="$(docker container ls)"
  if echo "$EXISTING_CONTAINERS" | grep -qx "$PROXY"; then
    OWNER_VAL="$(docker container inspect --format '{{index .Config.Labels "'"$OWNER_KEY"'"}}' "$PROXY")"
    [ "$OWNER_VAL" = "$OWNER" ] || { echo "unowned container $PROXY" >&2; exit 2; }
  fi

  EXISTING_NETWORKS="$(docker network ls)"
  for net in "$NET_INT" "$NET_EG"; do
    if echo "$EXISTING_NETWORKS" | grep -qx "$net"; then
      OWNER_VAL="$(docker network inspect --format '{{index .Labels "'"$OWNER_KEY"'"}}' "$net")"
      [ "$OWNER_VAL" = "$OWNER" ] || { echo "unowned network $net" >&2; exit 2; }
    fi
  done

  # Ownership check passed, remove resources
  if echo "$EXISTING_CONTAINERS" | grep -qx "$PROXY"; then
    docker rm -f "$PROXY"
  fi
  for net in "$NET_INT" "$NET_EG"; do
    if echo "$EXISTING_NETWORKS" | grep -qx "$net"; then
      docker network rm "$net"
    fi
  done
  echo "torn down"
  exit 0
fi

NORMALIZED_DOMAINS="$(printf '%s\n' $ALLOW_DEFAULT $ALLOW_EXTRA | sort -u | tr '\n' ' ' | sed 's/ $//')"

ensure_proxy() {
  local existing_images
  existing_images="$(docker image ls)"
  local existing_networks
  existing_networks="$(docker network ls)"
  local existing_containers
  existing_containers="$(docker container ls)"

  local need_build=0

  # Check image
  if echo "$existing_images" | grep -qx "$PROXY_IMG"; then
    local img_owner
    img_owner="$(docker image inspect --format '{{index .Config.Labels "'"$OWNER_KEY"'"}}' "$PROXY_IMG")"
    [ "$img_owner" = "$OWNER" ] || { echo "unowned image $PROXY_IMG" >&2; exit 2; }
    local img_domains
    img_domains="$(docker image inspect --format '{{index .Config.Labels "'"$DOMAINS_KEY"'"}}' "$PROXY_IMG")"
    if [ "$img_domains" != "$NORMALIZED_DOMAINS" ]; then
      need_build=1
    fi
  else
    need_build=1
  fi

  # Check networks
  for net in "$NET_INT" "$NET_EG"; do
    if echo "$existing_networks" | grep -qx "$net"; then
      local net_owner
      net_owner="$(docker network inspect --format '{{index .Labels "'"$OWNER_KEY"'"}}' "$net")"
      [ "$net_owner" = "$OWNER" ] || { echo "unowned network $net" >&2; exit 2; }
      if [ "$net" = "$NET_INT" ]; then
        local net_drv_int
        net_drv_int="$(docker network inspect --format '{{.Driver}}|{{.Internal}}' "$net")"
        [ "$net_drv_int" = "bridge|true" ] || { echo "invalid network configuration for $NET_INT" >&2; exit 2; }
      fi
    fi
  done

  # Check container
  if echo "$existing_containers" | grep -qx "$PROXY"; then
    local c_owner
    c_owner="$(docker container inspect --format '{{index .Config.Labels "'"$OWNER_KEY"'"}}' "$PROXY")"
    [ "$c_owner" = "$OWNER" ] || { echo "unowned container $PROXY" >&2; exit 2; }
    local c_domains
    c_domains="$(docker container inspect --format '{{index .Config.Labels "'"$DOMAINS_KEY"'"}}' "$PROXY")"
    if [ "$c_domains" != "$NORMALIZED_DOMAINS" ]; then
      echo "policy change requires explicit teardown" >&2
      exit 2
    fi
  fi

  # Build image if needed
  if [ "$need_build" -eq 1 ]; then
    local d; d="$(mktemp -d)"
    {
      echo 'Port 8888'; echo 'Listen 0.0.0.0'; echo 'Timeout 30'; echo 'Allow 0.0.0.0/0';
      echo 'FilterDefaultDeny Yes'; echo 'FilterExtended On'; echo 'FilterCaseSensitive Off';
      echo 'Filter "/etc/tinyproxy/filter"'; echo 'ConnectPort 443'; echo 'ConnectPort 563';
    } > "$d/tinyproxy.conf"
    : > "$d/filter"
    for h in $NORMALIZED_DOMAINS; do
      printf '(^|\\.)%s$\n' "$(echo "$h" | sed 's/\./\\./g')" >> "$d/filter"
    done
    printf 'FROM alpine:latest\nRUN apk add --no-cache tinyproxy\nCOPY tinyproxy.conf /etc/tinyproxy/tinyproxy.conf\nCOPY filter /etc/tinyproxy/filter\nCMD ["tinyproxy","-d","-c","/etc/tinyproxy/tinyproxy.conf"]\n' > "$d/Dockerfile"
    docker build -q --label "$OWNER_KEY=$OWNER" --label "$DOMAINS_KEY=$NORMALIZED_DOMAINS" -t "$PROXY_IMG" "$d" >/dev/null
    rm -rf "$d"
  fi

  # Create networks if needed
  if ! echo "$existing_networks" | grep -qx "$NET_INT"; then
    docker network create --driver bridge --internal --label "$OWNER_KEY=$OWNER" "$NET_INT" >/dev/null
  fi
  if ! echo "$existing_networks" | grep -qx "$NET_EG"; then
    docker network create --driver bridge --label "$OWNER_KEY=$OWNER" "$NET_EG" >/dev/null
  fi

  # Start proxy container if needed
  if ! echo "$existing_containers" | grep -qx "$PROXY"; then
    docker run -d --name "$PROXY" \
      --label "$OWNER_KEY=$OWNER" \
      --label "$DOMAINS_KEY=$NORMALIZED_DOMAINS" \
      --network "$NET_EG" "$PROXY_IMG" >/dev/null
    docker network connect "$NET_INT" "$PROXY" >/dev/null
    sleep 1
  fi

  local c_running
  c_running="$(docker container inspect --format '{{.State.Running}}' "$PROXY")"
  [ "$c_running" = "true" ] || { echo "container $PROXY is not running" >&2; exit 2; }
}

seal_flags() {
  printf '%s\n' --rm
  [ -n "$INTERACTIVE" ] && printf '%s\n' "$INTERACTIVE"
  printf '%s\n' \
    --user "$WORKLOAD_UID:$WORKLOAD_GID" \
    --cap-drop ALL --security-opt no-new-privileges \
    --read-only \
    --tmpfs /tmp:rw,noexec,nosuid,size=256m \
    --tmpfs /home:rw,nosuid,size=64m \
    -e HOME=/workspace -e npm_config_cache=/workspace/.npm -e PIP_CACHE_DIR=/workspace/.pipcache \
    --pids-limit "$PIDS" --memory "$MEMORY" --memory-swap "$MEMORY" --cpus "$CPUS" \
    -v "$(cd "$WORKSPACE" && pwd):/workspace:rw" -w /workspace

  case "$GPU" in
    nvidia) printf '%s\n' --gpus all ;;
    amd)    printf '%s\n' --device /dev/kfd --device /dev/dri --group-add video --group-add render ;;
    none)   ;;
  esac
}

net_flags() {
  case "$EGRESS" in
    none)   printf '%s\n' --network none ;;
    open)   printf '%s\n' --network bridge ;;
    sealed) printf '%s\n' --network "$NET_INT" \
              -e "http_proxy=http://$PROXY:8888" -e "https_proxy=http://$PROXY:8888" \
              -e "HTTP_PROXY=http://$PROXY:8888" -e "HTTPS_PROXY=http://$PROXY:8888" ;;
  esac
}

if [ "$EGRESS" = "sealed" ]; then
  ensure_proxy
fi

SEAL=(); while IFS= read -r _l; do SEAL+=("$_l"); done < <(seal_flags)
NET=(); while IFS= read -r _l; do NET+=("$_l"); done < <(net_flags)

if [ -n "$SELFTEST" ]; then
  IMAGE=alpine:latest
  SEAL=(); while IFS= read -r _l; do SEAL+=("$_l"); done < <(seal_flags)
  run_probe() {
    local label="$1" cmd="$2"
    local out
    out="$(docker run "${SEAL[@]}" "${NET[@]}" alpine:latest sh -c "$cmd" 2>&1)" || return $?
    printf '  %-30s %s\n' "$label" "$out"
  }
  run_probe "write /workspace" "echo x>.st_test && rm -f .st_test && echo ALLOW||echo deny" || exit $?
  run_probe "write / (rootfs)" "touch /e 2>/dev/null&&echo LEAK||echo blocked" || exit $?
  run_probe "uid (non-root?)" "id -u" || exit $?
  run_probe "off-list exfil" "wget -T6 -qO/dev/null https://example.com/&&echo REACHED||echo blocked" || exit $?
  run_probe "LAN/host pivot" "wget -T5 -qO/dev/null http://host.docker.internal/&&echo REACHED||echo blocked" || exit $?
  run_probe "direct (proxy bypass)" "wget -T5 -qY off -O/dev/null https://1.1.1.1/&&echo REACHED||echo blocked" || exit $?
  exit 0
fi

[ $# -eq 0 ] && set -- sh
exec docker run "${SEAL[@]}" "${NET[@]}" "$IMAGE" "$@"
