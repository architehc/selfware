#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SANDBOX_DIR="$ROOT_DIR/long_run_tests/guided_scheduler_lab"
CONFIG_PATH="$ROOT_DIR/selfware.toml"
BINARY_PATH="$ROOT_DIR/target/debug/selfware"

exec "$BINARY_PATH" \
  --config "$CONFIG_PATH" \
  -C "$SANDBOX_DIR" \
  --daemon \
  chat
