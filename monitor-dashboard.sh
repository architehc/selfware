#!/bin/bash
# Live dashboard for 6-hour GPU test

RESULTS_DIR=$(ls -td /home/ivo/selfware/gpu_max_test/*/ | head -1)

clear
while true; do
    tput cup 0 0
    
    echo "╔══════════════════════════════════════════════════════════════╗"
    echo "║           GPU MAX-OUT TEST - LIVE DASHBOARD                 ║"
    echo "╚══════════════════════════════════════════════════════════════╝"
    echo ""
    
    # GPU Status
    echo "🎮 GPU Status:"
    nvidia-smi --query-gpu=index,utilization.gpu,memory.used,temperature.gpu,power.draw \
        --format=csv,noheader | while read line; do
        echo "   GPU $line"
    done
    
    echo ""
    
    # Active Tasks
    ACTIVE=$(docker ps -q --filter "name=selfware-gpu" | wc -l)
    TOTAL=$(ls $RESULTS_DIR/task_*.log 2>/dev/null | wc -l)
    echo "📊 Tasks: $ACTIVE active / $TOTAL total launched"
    
    # Recent completions
    echo ""
    echo "📝 Recent Task Logs:"
    ls -lt $RESULTS_DIR/task_*.log 2>/dev/null | head -3 | awk '{print "   " $9}' | xargs -I{} sh -c 'tail -1 {} 2>/dev/null | head -c 60'
    
    # GPU History
    echo ""
    echo "📈 GPU Util History (last 5):"
    tail -5 $RESULTS_DIR/gpu_stats.csv 2>/dev/null | cut -d',' -f2 || echo "   Collecting data..."
    
    echo ""
    echo "Last updated: $(date '+%H:%M:%S') | Press Ctrl+C to exit"
    
    sleep 10
done
