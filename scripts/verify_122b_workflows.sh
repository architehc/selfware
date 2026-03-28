#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ROOT}/target/release/selfware"
CONFIG="${ROOT}/selfware-auto-txn545-Qwen3-5-122B-A10B-NVFP4.toml"
ENDPOINT="https://crazyshit.ngrok.io/v1"
MODEL="txn545/Qwen3.5-122B-A10B-NVFP4"

if [[ ! -x "${BIN}" ]]; then
  echo "release binary not found at ${BIN}" >&2
  echo "run: cargo build --release" >&2
  exit 1
fi

echo "==> Probing endpoint compatibility"
"${BIN}" auto-config --endpoint "${ENDPOINT}" --model "${MODEL}" --toml

echo
echo "==> Validating product workflows"
"${BIN}" workflow validate "${ROOT}/workflows/product_build.yml"

echo
echo "==> Dry-run end-to-end product build"
"${BIN}" -c "${CONFIG}" workflow run "${ROOT}/workflows/product_build.yml" \
  --input idea="Daily briefing app for founders" \
  --input target_user="Busy founders managing multiple calendars" \
  --input constraints="Use the existing Selfware repository and ship a narrow wedge" \
  --input success_metric="A founder gets a usable morning brief in under two minutes" \
  --input repo_context="Rust CLI with workflow runtime, API client, and tests" \
  --dry-run

echo
echo "==> Live-run discovery workflow against the 122B endpoint"
timeout 240 "${BIN}" -c "${CONFIG}" workflow run "${ROOT}/workflows/product_discovery.yml" \
  --input idea="Daily briefing app for founders" \
  --input target_user="Busy founders managing multiple calendars" \
  --input constraints="Use the existing Selfware repository and ship a narrow wedge" \
  --input success_metric="A founder gets a usable morning brief in under two minutes"
