#!/bin/zsh
set -eu

log_file="/tmp/selfware-zed-mcp.log"
{
  printf '=== %s ===\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'cwd=%s\n' "$PWD"
  printf 'args=%s\n' "$*"
  env | grep -E '^(SELFWARE_|RUST_LOG=|NO_COLOR=)' || true
} >> "$log_file"

cd /Users/ivo/selfware
/Users/ivo/selfware/target/debug/selfware "$@"
exit_code=$?

{
  printf 'exit=%s\n' "$exit_code"
} >> "$log_file"

exit "$exit_code"
