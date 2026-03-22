#!/bin/bash
TEST_DIR="$(dirname "$0")"
SUMMARY_FILE="$TEST_DIR/RESULTS.md"

echo "# Development Test Results" > "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"
echo "**Date:** $(date)" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"

for task in flappy_bird portfolio_website tqec_sim rust_game; do
    echo "## $task" >> "$SUMMARY_FILE"
    echo "" >> "$SUMMARY_FILE"
    
    for i in 1 2 3 4; do
        workdir="$TEST_DIR/$task/instance_$i"
        echo "### Instance $i" >> "$SUMMARY_FILE"
        echo "" >> "$SUMMARY_FILE"
        
        # List all created files
        echo "**Files created:**" >> "$SUMMARY_FILE"
        echo '```' >> "$SUMMARY_FILE"
        find "$workdir" -type f -name "*.html" -o -name "*.css" -o -name "*.js" -o -name "*.rs" -o -name "*.toml" 2>/dev/null | head -20 >> "$SUMMARY_FILE" || echo "(none)" >> "$SUMMARY_FILE"
        echo '```' >> "$SUMMARY_FILE"
        echo "" >> "$SUMMARY_FILE"
        
        # Check for specific deliverables
        if [[ -f "$workdir/index.html" ]]; then
            echo "- ✅ index.html exists" >> "$SUMMARY_FILE"
            lines=$(wc -l < "$workdir/index.html" 2>/dev/null)
            echo "  - Lines: $lines" >> "$SUMMARY_FILE"
        fi
        
        if [[ -f "$workdir/Cargo.toml" ]]; then
            echo "- ✅ Cargo.toml exists (Rust project)" >> "$SUMMARY_FILE"
        fi
        
        if [[ -d "$workdir/src" ]]; then
            rs_files=$(find "$workdir/src" -name "*.rs" 2>/dev/null | wc -l)
            echo "- ✅ src/ directory with $rs_files Rust files" >> "$SUMMARY_FILE"
        fi
        
        # Last 20 lines of log
        if [[ -f "$workdir/selfware.log" ]]; then
            echo "" >> "$SUMMARY_FILE"
            echo "**Last log entries:**" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
            tail -20 "$workdir/selfware.log" 2>/dev/null >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
        fi
        
        echo "" >> "$SUMMARY_FILE"
    done
done

echo "Results saved to: $SUMMARY_FILE"
cat "$SUMMARY_FILE"
