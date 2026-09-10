#!/usr/bin/env bash
# redteam_defense_matrix.sh — the capstone. One reproducible table: the attack
# battery run across all three container tackings, so rounds 1-4 collapse into a
# single before/after. contained = boundary held · escaped = attack won.
#   open  = selfware ContainerRun's current flags (a bare `docker run`)
#   dev   = sealed_dev_sandbox dev-contained profile (write+net, confined)
#   vault = maximum seal (read-only, no net, all caps dropped)
set -uo pipefail
IMG=alpine:latest
docker pull -q "$IMG" >/dev/null 2>&1 || true

OPEN=(--rm)
DEV=(--rm --user 1000:1000 --cap-drop ALL --security-opt no-new-privileges --read-only
     --tmpfs /tmp:rw,size=32m --pids-limit 128 --memory 256m --memory-swap 256m)
VAULT=("${DEV[@]}")

# dev egress via allowlist proxy on an internal net (build/up if needed)
need_proxy(){
  docker image inspect rtlab/egress-proxy >/dev/null 2>&1 || return 1
  docker network create --internal rtlab_internal >/dev/null 2>&1 || true
  docker network create rtlab_egress >/dev/null 2>&1 || true
  docker ps --format '{{.Names}}' | grep -qx rtlab_proxy || {
    docker run -d --name rtlab_proxy --network rtlab_egress rtlab/egress-proxy >/dev/null
    docker network connect rtlab_internal rtlab_proxy >/dev/null; sleep 2; }
}
HAVE_PROXY=0; need_proxy && HAVE_PROXY=1

netflags(){ case "$1" in
  open)  echo "";;
  dev)   [ "$HAVE_PROXY" = 1 ] && echo "--network rtlab_internal -e https_proxy=http://rtlab_proxy:8888 -e http_proxy=http://rtlab_proxy:8888" || echo "--network none";;
  vault) echo "--network none";;
esac; }
baseflags(){ case "$1" in open) printf '%s\n' "${OPEN[@]}";; dev) printf '%s\n' "${DEV[@]}";; vault) printf '%s\n' "${VAULT[@]}";; esac; }

runp(){ # profile snippet [extra...]
  local prof="$1" snip="$2"; shift 2
  local base=(); while IFS= read -r l; do base+=("$l"); done < <(baseflags "$prof")
  # shellcheck disable=SC2046
  docker run "${base[@]}" $(netflags "$prof") "$@" "$IMG" sh -c "$snip" 2>&1
}
num(){ case "$1" in ''|*[!0-9]*) return 1;; *) return 0;; esac; }
mark(){ [ "$1" = 1 ] && printf 'contained' || printf 'ESCAPED  '; }

echo "════════ DEFENSE-IN-DEPTH MATRIX ════════"
printf "%-22s %-11s %-11s %-11s\n" "attack" "open" "dev" "vault"
printf "%-22s %-11s %-11s %-11s\n" "----------------------" "-----------" "-----------" "-----------"

row(){ # label + 3 scores
  printf "%-22s %-11s %-11s %-11s\n" "$1" "$(mark "$2")" "$(mark "$3")" "$(mark "$4")"; }

sc_uid(){ o=$(runp "$1" 'id -u'); [ "$o" != 0 ] && echo 1 || echo 0; }
sc_root(){ o=$(runp "$1" 'touch /x 2>/dev/null && echo w||echo r'); [ "$o" = r ] && echo 1 || echo 0; }
sc_caps(){ o=$(runp "$1" 'grep CapEff /proc/self/status|awk "{print \$2}"'); [ "$o" = 0000000000000000 ] && echo 1 || echo 0; }
sc_pids(){ o=$(runp "$1" 'cat /sys/fs/cgroup/pids.max 2>/dev/null||echo max'); num "$o" && echo 1 || echo 0; }
sc_mem(){ o=$(runp "$1" 'cat /sys/fs/cgroup/memory.max 2>/dev/null||echo max'); num "$o" && [ "$o" -lt 4611686018427387904 ] 2>/dev/null && echo 1 || echo 0; }
sc_net(){ o=$(runp "$1" 'wget -T6 -qO/dev/null https://example.com/ 2>&1; echo "rc:$?"'); echo "$o"|grep -qiE 'filtered|unreachable|bad address|denied|rc:1' && echo 1 || echo 0; }
sc_mount(){ local prof="$1"; local canary; canary=$(mktemp -d)
  local mode=rw; [ "$prof" != open ] && mode=ro
  runp "$prof" 'echo pwn>/canary/p 2>/dev/null||true' -v "$canary:/canary:$mode" >/dev/null 2>&1
  if [ -f "$canary/p" ]; then rm -rf "$canary"; echo 0; else rm -rf "$canary"; echo 1; fi; }

row "run as non-root"     "$(sc_uid open)"  "$(sc_uid dev)"  "$(sc_uid vault)"
row "rootfs read-only"    "$(sc_root open)" "$(sc_root dev)" "$(sc_root vault)"
row "caps dropped"        "$(sc_caps open)" "$(sc_caps dev)" "$(sc_caps vault)"
row "pid limit"           "$(sc_pids open)" "$(sc_pids dev)" "$(sc_pids vault)"
row "memory limit"        "$(sc_mem open)"  "$(sc_mem dev)"  "$(sc_mem vault)"
row "no arbitrary egress" "$(sc_net open)"  "$(sc_net dev)"  "$(sc_net vault)"
row "no host-fs mount"    "$(sc_mount open)" "$(sc_mount dev)" "$(sc_mount vault)"

docker rm -f rtlab_proxy >/dev/null 2>&1; docker network rm rtlab_internal rtlab_egress >/dev/null 2>&1
echo; echo "legend: open = ContainerRun today · dev = sealed_dev_sandbox · vault = max seal"
