#!/bin/bash
RUN_DIR="$(dirname "$0")"
SUMMARY_FILE="$RUN_DIR/summary_report.md"

echo "# Parallel Selfware Run Summary" > "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"
echo "**Date:** $(date)" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"

for task in task_a task_b task_c task_d; do
    echo "## $task" >> "$SUMMARY_FILE"
    echo "" >> "$SUMMARY_FILE"
    
    for i in 1 2 3 4; do
        instance_dir="$RUN_DIR/$task/instance_$i"
        output_file="$instance_dir/output.log"
        error_file="$instance_dir/error.log"
        
        echo "### Instance $i" >> "$SUMMARY_FILE"
        echo "" >> "$SUMMARY_FILE"
        
        # Check if output exists and has content
        if [[ -f "$output_file" ]]; then
            lines=$(wc -l < "$output_file" 2>/dev/null || echo "0")
            size=$(du -h "$output_file" 2>/dev/null | cut -f1 || echo "0")
            echo "- **Status:** Completed" >> "$SUMMARY_FILE"
            echo "- **Output lines:** $lines" >> "$SUMMARY_FILE"
            echo "- **Output size:** $size" >> "$SUMMARY_FILE"
            
            # Extract key findings (last 50 lines)
            echo "" >> "$SUMMARY_FILE"
            echo "**Key output (last 30 lines):**" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
            tail -30 "$output_file" 2>/dev/null >> "$SUMMARY_FILE" || echo "(no output)" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
        else
            echo "- **Status:** No output file" >> "$SUMMARY_FILE"
        fi
        
        # Check for errors
        if [[ -f "$error_file" && -s "$error_file" ]]; then
            echo "" >> "$SUMMARY_FILE"
            echo "**Errors:**" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
            cat "$error_file" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
        fi
        
        echo "" >> "$SUMMARY_FILE"
    done
done

echo "Summary report saved to: $SUMMARY_FILE"
cat "$SUMMARY_FILE"
