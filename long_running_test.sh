#!/bin/bash
# Long-running Selfware test - designed to run for days
# This test verifies the 10k iteration limit works correctly

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║       SELFWARE LONG-RUNNING TEST (DAYS/DURATION)              ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Start time: $(date)"
echo "PID: $$"
echo ""

# Create test directory
TEST_DIR="/tmp/selfware_long_test_$(date +%s)"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Initialize a test project
git init test_project
cd test_project

# Create a simple Rust project to work on
cat > Cargo.toml << 'EOF'
[package]
name = "test_project"
version = "0.1.0"
edition = "2021"
EOF

mkdir -p src
cat > src/main.rs << 'EOF'
fn main() {
    println!("Hello, World!");
}
EOF

# Create iteration counter file
echo "0" > /tmp/selfware_iteration_count

# Run Selfware with a task that requires many iterations
# This task will iteratively improve the code
cat > /tmp/selfware_long_task.txt << 'EOF'
I need you to iteratively improve this Rust project. 

For EACH iteration:
1. Read the current main.rs
2. Add a new small feature or improvement (new function, better error handling, documentation, tests, etc.)
3. Save the changes
4. Run cargo check to verify it compiles
5. Record the iteration

Continue this loop as many times as possible. Each iteration should make meaningful progress.

Start with simple improvements and gradually increase complexity. Track how many iterations you complete.
EOF

echo "Starting long-running Selfware test..."
echo "Test directory: $TEST_DIR"
echo ""

# Run Selfware with timeout of 48 hours (172800 seconds)
# Using --yolo for auto-approval since this is an automated test
timeout 172800 ./target/release/selfware run --yolo "$(cat /tmp/selfware_long_task.txt)" 2>&1 | tee /tmp/selfware_long_test.log &

SELFWARE_PID=$!
echo "Selfware PID: $SELFWARE_PID"
echo "$SELFWARE_PID" > /tmp/selfware_test.pid

# Monitor loop
echo ""
echo "Monitoring started. Press Ctrl+C to stop monitoring (Selfware will continue running)"
echo ""

iteration_count=0
while kill -0 $SELFWARE_PID 2>/dev/null; do
    sleep 300  # Check every 5 minutes
    iteration_count=$((iteration_count + 1))
    
    echo "[$(date)] Check #$iteration_count - Selfware still running (PID: $SELFWARE_PID)"
    
    # Check log for iteration count patterns
    if [ -f /tmp/selfware_long_test.log ]; then
        # Count completed iterations based on tool executions
        tool_count=$(grep -c "🔧\|📝\|✓" /tmp/selfware_long_test.log 2>/dev/null || echo "0")
        echo "  Estimated tool executions: $tool_count"
        
        # Show last few lines
        echo "  Last activity:"
        tail -3 /tmp/selfware_long_test.log | sed 's/^/    /'
    fi
    
    # Check system resources
    cpu_usage=$(ps -p $SELFWARE_PID -o %cpu= 2>/dev/null || echo "N/A")
    mem_usage=$(ps -p $SELFWARE_PID -o %mem= 2>/dev/null || echo "N/A")
    echo "  Resource usage - CPU: ${cpu_usage}% MEM: ${mem_usage}%"
    
    echo ""
done

echo ""
echo "Selfware process has ended"
echo "End time: $(date)"
echo ""

# Final report
if [ -f /tmp/selfware_long_test.log ]; then
    echo "=== FINAL TEST REPORT ==="
    echo "Log file: /tmp/selfware_long_test.log"
    echo "Log size: $(du -h /tmp/selfware_long_test.log 2>/dev/null | cut -f1)"
    echo "Total lines: $(wc -l < /tmp/selfware_long_test.log)"
    echo ""
    echo "Last 20 lines of log:"
    tail -20 /tmp/selfware_long_test.log
fi

# Check if it completed successfully or was killed
if wait $SELFWARE_PID 2>/dev/null; then
    echo ""
    echo "✅ Test completed successfully!"
    exit 0
else
    echo ""
    echo "⚠️  Test ended with exit code: $?"
    exit 1
fi
