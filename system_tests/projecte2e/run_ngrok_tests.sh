#!/usr/bin/env bash
set -euo pipefail

THIS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${THIS_DIR}/../.." && pwd)"
CONFIG_FILE="${THIS_DIR}/config/crazyshit_model.toml"

echo "=============================================="
echo " Starting Selfware E2E Test Suite via Ngrok"
echo " Configuration: ${CONFIG_FILE}"
echo "=============================================="

# Ensure the binary is built
echo "Building selfware..."
cargo build --release

# Setup API Key if needed
if [ -z "${SELFWARE_API_KEY:-}" ]; then
    export SELFWARE_API_KEY="dummy-key-for-sglang"
    echo "Using dummy API key for standard local endpoints"
fi

# Run the project E2E harness using the new configuration
export CONFIG_FILE
bash "${THIS_DIR}/run_projecte2e.sh"

echo "=============================================="
echo " Tests Complete."
echo " Check reports directory for results."
echo "=============================================="
