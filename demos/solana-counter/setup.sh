#!/bin/bash
set -e

echo "=== Solana Counter Demo: Environment Setup ==="
echo ""

# Install Solana CLI
echo "[1/5] Installing Solana CLI..."
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

echo ""
echo "[2/5] Installing Anchor CLI..."
cargo install --git https://github.com/coral-xyz/anchor anchor-cli

echo ""
echo "[3/5] Starting local validator..."
solana-test-validator &
sleep 5

echo ""
echo "[4/5] Configuring Solana for localhost..."
solana config set --url localhost
solana-keygen new --no-bip39-passphrase --force

echo ""
echo "[5/5] Airdropping SOL..."
solana airdrop 100

echo ""
echo "=== Setup Complete ==="
echo "Solana CLI: $(solana --version)"
echo "Anchor CLI: $(anchor --version)"
echo "Validator PID: $(pgrep -f solana-test-validator || echo 'not running')"
echo ""
echo "Run ./run_demo.sh to start the demo."
