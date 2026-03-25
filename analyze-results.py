#!/usr/bin/env python3
"""Analyze selfware GPU test results"""

import json
import glob
import sys
from datetime import datetime
from pathlib import Path

def analyze_test_results(test_dir):
    """Analyze test results from a directory"""
    test_dir = Path(test_dir)
    
    print(f"📊 Analyzing: {test_dir}")
    print("=" * 50)
    
    # Count tasks
    task_logs = list(test_dir.glob("task_*.log"))
    print(f"\n📁 Total tasks: {len(task_logs)}")
    
    # Analyze GPU stats
    gpu_stats_file = test_dir / "gpu_stats.csv"
    if gpu_stats_file.exists():
        print(f"\n🎮 GPU Statistics:")
        
        utils_gpu0 = []
        utils_gpu1 = []
        temps = []
        
        with open(gpu_stats_file) as f:
            next(f)  # Skip header
            for line in f:
                parts = line.strip().split(',')
                if len(parts) >= 7:
                    try:
                        util0 = int(parts[1])
                        util1 = int(parts[3])
                        temp = int(parts[5])
                        utils_gpu0.append(util0)
                        utils_gpu1.append(util1)
                        temps.append(temp)
                    except:
                        pass
        
        if utils_gpu0:
            print(f"  GPU0: avg={sum(utils_gpu0)//len(utils_gpu0)}%, "
                  f"min={min(utils_gpu0)}%, max={max(utils_gpu0)}%")
            print(f"  GPU1: avg={sum(utils_gpu1)//len(utils_gpu1)}%, "
                  f"min={min(utils_gpu1)}%, max={max(utils_gpu1)}%")
            print(f"  Temperature: avg={sum(temps)//len(temps)}°C, max={max(temps)}°C")
    
    # Check for errors
    errors = 0
    successes = 0
    for task_log in task_logs[:10]:  # Sample first 10
        with open(task_log) as f:
            content = f.read()
            if "error" in content.lower() or "failed" in content.lower():
                errors += 1
            elif "success" in content.lower() or "completed" in content.lower():
                successes += 1
    
    print(f"\n✅ Sample analysis (first 10 tasks):")
    print(f"  Success indicators: {successes}")
    print(f"  Error indicators: {errors}")
    print(f"  Success rate: {successes*10}%")

if __name__ == "__main__":
    # Find latest test directory
    test_dirs = sorted(Path("/home/ivo/selfware/gpu_max_test").glob("*/"), reverse=True)
    if test_dirs:
        analyze_test_results(test_dirs[0])
    else:
        print("No test directories found")
