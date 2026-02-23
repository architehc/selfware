#!/bin/bash
# Show final test summary

TEST_DIR=$(ls -td /home/thread/kimi-workspace/kimi-agent-claude/test_runs/test-* | head -1)

echo "╔══════════════════════════════════════════════════════════════════════════════╗"
echo "║                         🤖 Test Summary                                       ║"
echo "╚══════════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "Session: $(basename "$TEST_DIR")"
echo ""

# Config
cat "$TEST_DIR/config.json" 2>/dev/null | grep -E '"(project|scenario|duration)"' | sed 's/,$//' | sed 's/^/  /'

echo ""
echo "=== Timing ==="
if [ -f "$TEST_DIR/metrics/metrics.jsonl" ]; then
    FIRST=$(head -1 "$TEST_DIR/metrics/metrics.jsonl")
    LAST=$(tail -1 "$TEST_DIR/metrics/metrics.jsonl")
    START_E=$(echo "$FIRST" | grep -o '"e":[0-9]*' | cut -d':' -f2)
    END_E=$(echo "$LAST" | grep -o '"e":[0-9]*' | cut -d':' -f2)
    START_P=$(echo "$FIRST" | grep -o '"p":[0-9]*' | cut -d':' -f2)
    END_P=$(echo "$LAST" | grep -o '"p":[0-9]*' | cut -d':' -f2)
    
    echo "  Duration: $((END_E - START_E)) seconds ($(( (END_E - START_E) / 60 )) minutes)"
    echo "  Progress: $START_P% → $END_P%"
fi

echo ""
echo "=== Status ==="
echo "  $(cat "$TEST_DIR/status" 2>/dev/null || echo "unknown")"

echo ""
echo "=== Files Created ==="
if [ -d "$TEST_DIR/work" ]; then
    echo "  Total files: $(find "$TEST_DIR/work" -type f 2>/dev/null | wc -l)"
    echo "  Rust files: $(find "$TEST_DIR/work" -name "*.rs" 2>/dev/null | wc -l)"
    echo "  Config files: $(find "$TEST_DIR/work" -name "*.toml" 2>/dev/null | wc -l)"
    echo "  Documentation: $(find "$TEST_DIR/work" -name "*.md" 2>/dev/null | wc -l)"
    
    # Check for specific files
    echo ""
    echo "=== Key Deliverables ==="
    [ -f "$TEST_DIR/work/redqueue/Dockerfile" ] && echo "  ✅ Dockerfile"
    [ -d "$TEST_DIR/work/redqueue/.github" ] && echo "  ✅ CI/CD workflows"
    [ -f "$TEST_DIR/work/redqueue/Cargo.lock" ] && echo "  ✅ Dependencies resolved (Cargo.lock)"
    [ -d "$TEST_DIR/work/redqueue/target" ] && echo "  ✅ Compiled artifacts ($(du -sh "$TEST_DIR/work/redqueue/target" 2>/dev/null | cut -f1))"
    [ -d "$TEST_DIR/work/redqueue/tests" ] && echo "  ✅ Test suite"
fi

echo ""
echo "=== Log Summary ==="
echo "  Total lines: $(wc -l < "$TEST_DIR/logs/main.log" 2>/dev/null || echo "0")"
echo "  Tool calls: $(grep -c "<tool>" "$TEST_DIR/logs/main.log" 2>/dev/null || echo "0")"

echo ""
echo "Test directory: $TEST_DIR"
