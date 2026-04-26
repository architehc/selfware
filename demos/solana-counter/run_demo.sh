#!/bin/bash
set -e

echo "=== SELFWARE SOLANA DEMO ==="
echo "Building a Solana counter program with AI assistance"
echo ""

# Ensure we're in the demo directory
cd "$(dirname "$0")"

# Check prerequisites
command -v solana >/dev/null 2>&1 || { echo "Solana CLI not found. Run setup.sh first."; exit 1; }
command -v anchor >/dev/null 2>&1 || { echo "Anchor CLI not found. Run setup.sh first."; exit 1; }
command -v selfware >/dev/null 2>&1 || { echo "Selfware not found. Run: cargo install selfware"; exit 1; }

# Verify local validator is running
if ! solana cluster-version 2>/dev/null; then
    echo "Local validator not running. Starting one..."
    solana-test-validator &
    sleep 5
    solana config set --url localhost
fi

# Create selfware config
cat > selfware.toml << 'TOML'
endpoint = "http://127.0.0.1:8000/v1"
model = "txn545/Qwen3.5-122B-A10B-NVFP4"
max_tokens = 65536
temperature = 0.7

[agent]
max_iterations = 50
step_timeout_secs = 600

[safety]
allowed_paths = ["./**"]
TOML

# Run selfware with the prompt
echo "Starting selfware..."
echo ""
selfware run "$(cat prompt.md)"

echo ""
echo "=== DEMO COMPLETE ==="
echo ""

# Show results if the project was created
if [ -d "counter" ]; then
    echo "Generated project structure:"
    find counter -type f -name "*.rs" -o -name "*.ts" -o -name "*.toml" | sort
    echo ""
    echo "Program ID:"
    grep -r "declare_id" counter/programs/counter/src/lib.rs 2>/dev/null || echo "(not found)"
fi
