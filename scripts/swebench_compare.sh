#!/bin/bash
# Compare SWE-bench Evaluation Results
#
# Compare two evaluation runs and generate a detailed comparison report

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

# Print usage
usage() {
    cat << EOF
SWE-bench Comparison Tool

Usage: $0 [OPTIONS] BASELINE CURRENT

Arguments:
    BASELINE    Path to baseline results (JSON file or directory)
    CURRENT     Path to current results (JSON file or directory)

Options:
    -o, --output FILE      Output file for comparison report
    -f, --format FORMAT    Output format: markdown, json, csv (default: markdown)
    -h, --help             Show this help message

Examples:
    # Compare two runs
    $0 swebench_eval/run_20260324_120000/report.json swebench_eval/run_20260324_130000/report.json

    # Compare and save report
    $0 -o comparison.md run1/ run2/

EOF
}

# Parse arguments
parse_args() {
    OUTPUT=""
    FORMAT="markdown"
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            -o|--output)
                OUTPUT="$2"
                shift 2
                ;;
            -f|--format)
                FORMAT="$2"
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            -*)
                echo "Unknown option: $1"
                usage
                exit 1
                ;;
            *)
                break
                ;;
        esac
    done

    if [[ $# -lt 2 ]]; then
        echo "Error: Missing required arguments"
        usage
        exit 1
    fi

    BASELINE="$1"
    CURRENT="$2"
}

# Find report.json in directory if needed
find_report() {
    local path="$1"
    
    if [[ -f "$path" ]]; then
        echo "$path"
    elif [[ -d "$path" ]]; then
        if [[ -f "$path/report.json" ]]; then
            echo "$path/report.json"
        else
            echo "Error: report.json not found in $path"
            exit 1
        fi
    else
        echo "Error: Path not found: $path"
        exit 1
    fi
}

# Extract metric from JSON
get_metric() {
    local file="$1"
    local metric="$2"
    jq -r "$metric" "$file" 2>/dev/null || echo "N/A"
}

# Generate comparison report
generate_comparison() {
    local baseline="$1"
    local current="$2"
    local output="${3:-}"
    
    local baseline_resolved
    local current_resolved
    local baseline_rate
    local current_rate
    
    baseline_resolved=$(get_metric "$baseline" '.summary.resolved')
    current_resolved=$(get_metric "$current" '.summary.resolved')
    baseline_rate=$(get_metric "$baseline" '.summary.resolution_rate')
    current_rate=$(get_metric "$current" '.summary.resolution_rate')
    
    local delta
    delta=$(echo "$current_rate - $baseline_rate" | bc 2>/dev/null || echo "0")
    
    # Generate report
    local report=""
    
    report+="# SWE-bench Comparison Report\n\n"
    report+="Generated: $(date)\n\n"
    
    report+="## Summary\n\n"
    report+="| Metric | Baseline | Current | Delta |\n"
    report+="|--------|----------|---------|-------|\n"
    
    local metrics=(
        ".summary.total_tasks:Total Tasks"
        ".summary.resolved:Resolved"
        ".summary.resolution_rate:Resolution Rate"
        ".summary.failed:Failed"
        ".summary.avg_patch_quality:Avg Patch Quality"
        ".summary.avg_duration_secs:Avg Duration"
        ".summary.avg_tokens_used:Avg Tokens"
    )
    
    for metric_def in "${metrics[@]}"; do
        local metric_path="${metric_def%%:*}"
        local metric_name="${metric_def##*:}"
        
        local baseline_val
        local current_val
        baseline_val=$(get_metric "$baseline" "$metric_path")
        current_val=$(get_metric "$current" "$metric_path")
        
        # Format numbers
        if [[ "$metric_path" == *"rate"* ]] || [[ "$metric_path" == *"avg"* ]]; then
            baseline_val=$(printf "%.2f" "$baseline_val" 2>/dev/null || echo "$baseline_val")
            current_val=$(printf "%.2f" "$current_val" 2>/dev/null || echo "$current_val")
            local delta_val=$(echo "$current_val - $baseline_val" | bc 2>/dev/null || echo "0")
            delta_val=$(printf "%+.2f" "$delta_val" 2>/dev/null || echo "N/A")
            report+="| $metric_name | $baseline_val | $current_val | $delta_val |\n"
        else
            report+="| $metric_name | $baseline_val | $current_val | - |\n"
        fi
    done
    
    report+="\n"
    
    # Find improved and regressed tasks
    report+="## Task Changes\n\n"
    
    local improved
    improved=$(jq -r '
        .results[] | select(.resolved == true) | .task_id' "$current" 2>/dev/null | sort)
    local baseline_resolved_tasks
    baseline_resolved_tasks=$(jq -r '
        .results[] | select(.resolved == true) | .task_id' "$baseline" 2>/dev/null | sort)
    
    local new_successes
    new_successes=$(comm -23 <(echo "$improved") <(echo "$baseline_resolved_tasks") 2>/dev/null || true)
    
    if [[ -n "$new_successes" ]]; then
        report+="### New Successes\n\n"
        while IFS= read -r task; do
            [[ -n "$task" ]] && report+="- ✅ $task\n"
        done <<< "$new_successes"
        report+="\n"
    fi
    
    local new_failures
    new_failures=$(comm -13 <(echo "$improved") <(echo "$baseline_resolved_tasks") 2>/dev/null || true)
    
    if [[ -n "$new_failures" ]]; then
        report+="### New Failures\n\n"
        while IFS= read -r task; do
            [[ -n "$task" ]] && report+="- ❌ $task\n"
        done <<< "$new_failures"
        report+="\n"
    fi
    
    # Output report
    if [[ -n "$output" ]]; then
        echo -e "$report" > "$output"
        echo "Comparison report saved to: $output"
    else
        echo -e "$report"
    fi
}

# Print colorful summary
print_summary() {
    local baseline="$1"
    local current="$2"
    
    local baseline_rate
    local current_rate
    baseline_rate=$(get_metric "$baseline" '.summary.resolution_rate')
    current_rate=$(get_metric "$current" '.summary.resolution_rate')
    
    local delta
    delta=$(echo "$current_rate - $baseline_rate" | bc 2>/dev/null || echo "0")
    
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║                   Comparison Summary                         ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    local baseline_pct
    local current_pct
    baseline_pct=$(printf "%.1f%%" $(echo "$baseline_rate * 100" | bc -l 2>/dev/null || echo "0"))
    current_pct=$(printf "%.1f%%" $(echo "$current_rate * 100" | bc -l 2>/dev/null || echo "0"))
    
    echo -e "  Baseline: $baseline_pct"
    echo -e "  Current:  $current_pct"
    
    if (( $(echo "$delta > 0" | bc -l 2>/dev/null || echo "0") )); then
        echo -e "  Delta:    ${GREEN}+$(printf "%.2f%%" $(echo "$delta * 100" | bc -l 2>/dev/null || echo "0"))${NC}"
        echo -e "  ${GREEN}✓ Improvement detected!${NC}"
    elif (( $(echo "$delta < 0" | bc -l 2>/dev/null || echo "0") )); then
        echo -e "  Delta:    ${RED}$(printf "%.2f%%" $(echo "$delta * 100" | bc -l 2>/dev/null || echo "0"))${NC}"
        echo -e "  ${RED}⚠ Regression detected${NC}"
    else
        echo -e "  Delta:    ${YELLOW}No change${NC}"
    fi
    
    echo ""
}

# Main execution
main() {
    parse_args "$@"
    
    # Find report files
    BASELINE=$(find_report "$BASELINE")
    CURRENT=$(find_report "$CURRENT")
    
    echo "Baseline: $BASELINE"
    echo "Current:  $CURRENT"
    echo ""
    
    # Generate comparison
    generate_comparison "$BASELINE" "$CURRENT" "$OUTPUT"
    
    # Print summary
    print_summary "$BASELINE" "$CURRENT"
}

main "$@"
