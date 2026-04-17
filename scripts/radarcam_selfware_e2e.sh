#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${SELFWARE_BIN:-$ROOT_DIR/target/debug/selfware}"
CONFIG="${SELFWARE_CONFIG:-$ROOT_DIR/selfware-radarcam.toml}"
WORKDIR="${SELFWARE_WORKDIR:-/home/ivo/radarcam}"
VISION_IMAGE="${SELFWARE_VISION_IMAGE:-$WORKDIR/samples/sample_3_00000693.jpg}"
HEADLESS_FLAGS="${SELFWARE_HEADLESS_FLAGS:---yolo --ascii --compact}"
SESSION_LOG_DIR="${SELFWARE_SESSION_LOG_DIR:-$HOME/.local/share/selfware/session_logs}"

read -r -a HEADLESS_ARGV <<<"$HEADLESS_FLAGS"

latest_workdir_session_log() {
  while read -r _ path; do
    if rg -q "\"cwd\":\"$WORKDIR\"" "$path"; then
      printf '%s\n' "$path"
      return 0
    fi
  done < <(find "$SESSION_LOG_DIR" -type f -printf '%T@ %p\n' | sort -nr)
  return 1
}

cargo build --bin selfware --manifest-path "$ROOT_DIR/Cargo.toml"

echo "[1/4] validating config"
"$BIN" --config "$CONFIG" --validate-config

echo "[2/4] running Rust regression checks"
cargo test --manifest-path "$ROOT_DIR/Cargo.toml" --lib explicit_tool
cargo test --manifest-path "$ROOT_DIR/Cargo.toml" --lib capability_disclaimer
cargo test --manifest-path "$ROOT_DIR/Cargo.toml" --lib extract_mentioned_path_finds_absolute_markdown_file

echo "[3/4] diagnosing endpoint"
"$BIN" --config "$CONFIG" llm-doctor

echo "[4/4] validating live vision task"
"$BIN" "${HEADLESS_ARGV[@]}" -C "$WORKDIR" --config "$CONFIG" -p \
  "Use vision_analyze on $VISION_IMAGE and answer in one short sentence describing the main subject."

VISION_LOG="$(latest_workdir_session_log)"
if [[ -z "${VISION_LOG:-}" ]]; then
  echo "Could not locate a RadarCam selfware session log under $SESSION_LOG_DIR" >&2
  exit 1
fi

if ! rg -q '"event_type":"tool_call".*"tool_name":"vision_analyze".*"success":true' "$VISION_LOG"; then
  echo "Expected a successful vision_analyze tool call in $VISION_LOG" >&2
  exit 1
fi

echo "Verified successful vision_analyze tool execution in $VISION_LOG"
