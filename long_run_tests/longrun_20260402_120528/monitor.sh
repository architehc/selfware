#!/bin/bash
DIR="$(dirname "$0")"
while true; do
  clear
  echo "=== $(date) ==="
  for i in $(seq 0 9); do
    f="$DIR/agent_${i}.log"
    ST=$(grep -c "Step.*Executing" "$f" 2>/dev/null || echo 0)
    AW=$(grep -c "Auto-writing\|auto-writing" "$f" 2>/dev/null || echo 0)
    DN=$(grep -c "Task complete" "$f" 2>/dev/null || echo 0)
    W="$DIR/agent_$i"
    SZ=$(wc -l "$W/src/lib.rs" 2>/dev/null | awk '{print $1}' || echo 1)
    CK=$(cd "$W" 2>/dev/null && cargo check 2>&1 | grep -c "Finished" || echo 0)
    echo "  A$i: steps=$ST writes=$AW lib=$SZ compiles=$CK done=$DN"
  done
  sleep 30
done
