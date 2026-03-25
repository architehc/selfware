#!/bin/bash
# Real-time dashboard for selfware GPU test

clear
while true; do
    # Move cursor to top
    tput cup 0 0
    
    echo "╔══════════════════════════════════════════════════════════════╗"
    echo "║         SELFWARE GPU TEST - LIVE DASHBOARD                  ║"
    echo "╚══════════════════════════════════════════════════════════════╝"
    echo ""
    
    # GPU Status
    echo "🎮 GPU Status:"
    nvidia-smi --query-gpu=index,utilization.gpu,memory.used,temperature.gpu,power.draw \
        --format=csv,noheader 2>/dev/null | while IFS=',' read -r idx util mem temp power; do
        printf "   GPU%s: %s%% util | %s mem | %s°C | %sW\n" \
            "$(echo $idx | tr -d ' ')" \
            "$(echo $util | tr -d ' ' | head -c 3)" \
            "$(echo $mem | tr -d ' ')" \
            "$(echo $temp | tr -d ' ')" \
            "$(echo $power | tr -d ' ' | head -c 5)"
    done
    
    echo ""
    
    # Container Status
    ACTIVE=$(docker ps -q --filter "name=selfware-gpu" 2>/dev/null | wc -l)
    echo "📦 Active Containers: $ACTIVE"
    
    # Test Progress
    LATEST_DIR=$(ls -td /home/ivo/selfware/gpu_max_test/*/ 2>/dev/null | head -1)
    if [ -n "$LATEST_DIR" ]; then
        TASK_COUNT=$(ls "$LATEST_DIR"/task_*.log 2>/dev/null | wc -l)
        GPU_SAMPLES=$(wc -l < "$LATEST_DIR"/gpu_stats.jsonl 2>/dev/null || echo "0")
        
        echo "📊 Test Progress:"
        echo "   Tasks launched: $TASK_COUNT"
        echo "   GPU samples: $GPU_SAMPLES"
        
        # Calculate runtime
        if [ -f "$LATEST_DIR"/gpu_stats.jsonl ]; then
            FIRST_TS=$(head -1 "$LATEST_DIR"/gpu_stats.jsonl | python3 -c "import sys,json;print(json.load(sys.stdin)['timestamp'])" 2>/dev/null)
            NOW_TS=$(date +%s)
            if [ -n "$FIRST_TS" ]; then
                ELAPSED=$((NOW_TS - FIRST_TS))
                HOURS=$((ELAPSED / 3600))
                MINS=$(((ELAPSED % 3600) / 60))
                echo "   Runtime: ${HOURS}h ${MINS}m / 6h target"
            fi
        fi
    fi
    
    echo ""
    echo "Last update: $(date '+%H:%M:%S') | Press Ctrl+C to exit"
    
    sleep 5
done
