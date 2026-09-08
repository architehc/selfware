#!/bin/bash
# test-containment.sh — security test battery for the contained agent runner.
# Every check must PASS; any FAIL means the sandbox leaks.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IMAGE="${IMAGE:-selfware-contained:latest}"
NET_NAME="contained-egress"
ALLOWED_HOST="${ALLOWED_HOST:-e97b8112d15a.ngrok.app}"
PASS=0; FAIL=0

check() { # $1=name $2=0-pass/1-fail $3=detail
  if [ "$2" -eq 0 ]; then PASS=$((PASS+1)); echo "PASS  $1";
  else FAIL=$((FAIL+1)); echo "FAIL  $1  ($3)"; fi
}

echo "== build + lockdown =="
"$HERE/run-contained.sh" build > /dev/null
"$HERE/run-contained.sh" lockdown > /dev/null

NET_PREFIX=$(echo "172.29.0." | sed 's/\./\\./g')

echo "== probes =="
# 1. The model endpoint is reachable from inside.
out=$(docker run --rm --entrypoint sh --network "$NET_NAME" "$IMAGE" \
  -c "curl -s -o /dev/null -w '%{http_code}' -m 20 https://$ALLOWED_HOST/v1/models" 2>/dev/null)
[ "$out" = "200" ]; check "model endpoint reachable from container" $? "got HTTP $out"

# 2. Arbitrary egress is blocked (DNS may resolve; the packet must not flow).
out=$(docker run --rm --entrypoint sh --network "$NET_NAME" "$IMAGE" \
  -c "curl -s -o /dev/null -w '%{http_code}' -m 8 https://1.1.1.1/ || echo blocked" 2>/dev/null)
echo "$out" | grep -qE "blocked|000"; check "arbitrary HTTPS egress blocked" $? "got $out"

out=$(docker run --rm --entrypoint sh --network "$NET_NAME" "$IMAGE" \
  -c "curl -s -o /dev/null -w '%{http_code}' -m 8 http://example.com/ || echo blocked" 2>/dev/null)
echo "$out" | grep -qE "blocked|000"; check "arbitrary HTTP egress blocked" $? "got $out"

# 3. Non-root.
out=$(docker run --rm --entrypoint sh --network "$NET_NAME" "$IMAGE" -c 'whoami' 2>/dev/null)
[ "$out" = "agent" ]; check "runs as non-root user" $? "whoami=$out"

# 4. No capabilities.
out=$(docker run --rm --entrypoint sh --network "$NET_NAME" --cap-drop ALL --security-opt no-new-privileges "$IMAGE" \
  -c 'grep CapEff /proc/self/status' 2>/dev/null)
echo "$out" | grep -qE "CapEff:.*[0-9a-f]{16}" && echo "$out" | grep -q "0000000000000000"; check "effective capabilities empty" $? "$out"

# 5. Read-only root filesystem.
out=$(docker run --rm --entrypoint sh --network "$NET_NAME" --read-only "$IMAGE" \
  -c 'touch /etc/probe 2>/dev/null && echo writable || echo readonly' 2>/dev/null)
[ "$out" = "readonly" ]; check "root filesystem read-only" $? "got $out"

# 6. No host content visible: /work starts empty, no host mounts.
out=$(docker run --rm --entrypoint sh --network "$NET_NAME" "$IMAGE" -c 'ls -A /work | wc -l' 2>/dev/null)
[ "$out" = "0" ]; check "no host mounts (/work empty)" $? "got $out entries"

# 7. The shipped config scopes the agent's file access to /work only.
out=$(docker run --rm --entrypoint sh --network "$NET_NAME" "$IMAGE" \
  -c 'grep -A2 "allowed_paths" /etc/selfware/config.toml | head -1' 2>/dev/null)
echo "$out" | grep -q '/work/\*\*'; check "agent file access scoped to /work" $? "got: ${out:0:80}"

echo
echo "== result: $PASS passed, $FAIL failed =="
"$HERE/run-contained.sh" unlock > /dev/null
[ "$FAIL" -eq 0 ]
