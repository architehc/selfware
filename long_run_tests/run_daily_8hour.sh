#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LONG_RUN_DIR="$ROOT_DIR/long_run_tests"
CRON_DIR="$LONG_RUN_DIR/cron"
LOCK_FILE="$CRON_DIR/daily_8hour.lock"
STAMP="$(date +%Y%m%d_%H%M%S)"
LAUNCH_LOG="$CRON_DIR/launcher_${STAMP}.log"

mkdir -p "$CRON_DIR"
exec > >(tee -a "$LAUNCH_LOG") 2>&1

export PATH="/home/ivo/.cargo/bin:/home/ivo/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

exec 9>"$LOCK_FILE"
if ! flock -n 9; then
  echo "[$(date '+%Y-%m-%d %H:%M:%S %Z')] Daily 8-hour run skipped: another run is active."
  exit 0
fi

echo "[$(date '+%Y-%m-%d %H:%M:%S %Z')] Starting daily Selfware 8-hour run"
echo "Root: $ROOT_DIR"
echo "Launcher log: $LAUNCH_LOG"

cd "$ROOT_DIR"

if [[ "${AUTO_BUILD:-1}" == "1" ]]; then
  echo "[$(date '+%Y-%m-%d %H:%M:%S %Z')] Building latest release binary"
  /home/ivo/.cargo/bin/cargo build --release --bin selfware
fi

echo "[$(date '+%Y-%m-%d %H:%M:%S %Z')] Launching harness"
DURATION_HOURS="${DURATION_HOURS:-8}" \
TIMEOUT="${TIMEOUT:-1200}" \
MIN_PROJECT_WINDOW_SECS="${MIN_PROJECT_WINDOW_SECS:-120}" \
MAX_ITERS="${MAX_ITERS:-100}" \
/bin/bash "$ROOT_DIR/long_run_tests/run_8hour_system_test_v4.sh"

echo "[$(date '+%Y-%m-%d %H:%M:%S %Z')] Daily Selfware 8-hour run finished"
