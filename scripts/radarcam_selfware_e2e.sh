#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${SELFWARE_BIN:-$ROOT_DIR/target/debug/selfware}"
CONFIG="${SELFWARE_CONFIG:-$ROOT_DIR/selfware-radarcam.toml}"
WORKDIR="${SELFWARE_WORKDIR:-/home/ivo/radarcam}"
VISION_IMAGE="${SELFWARE_VISION_IMAGE:-$WORKDIR/samples/sample_3_00000693.jpg}"

cargo build --bin selfware --manifest-path "$ROOT_DIR/Cargo.toml"

echo "[1/4] validating config"
"$BIN" --config "$CONFIG" --validate-config

echo "[2/4] diagnosing endpoint"
"$BIN" --config "$CONFIG" llm-doctor

echo "[3/4] validating workspace text task"
"$BIN" -C "$WORKDIR" --config "$CONFIG" -p \
  "What is the default full validation CLI command for this workspace? Answer in one line."

echo "[4/4] validating live vision task"
"$BIN" -C "$WORKDIR" --config "$CONFIG" -p \
  "Use vision_analyze on $VISION_IMAGE and answer in one short sentence describing the main subject."
