# Post-Test Action Checklist

## When 6-Hour Test Completes (~5 hours from now)

### Immediate (First 30 minutes)
- [ ] Stop the test: `pkill -f run-6hour-test`
- [ ] Collect all logs: `tar czf test_results_$(date +%Y%m%d).tar.gz gpu_max_test/*/`
- [ ] Generate final report: `python3 analyze-results.py`
- [ ] Check GPU health: `nvidia-smi -q | grep -A5 "Temperature"`

### Analysis (Next 1 hour)
- [ ] Calculate average throughput (tasks/hour)
- [ ] Identify peak performance periods
- [ ] Find any error patterns
- [ ] Compare against SWE-bench baseline

### Production Deployment (Next 2 hours)
- [ ] Run: `./production-deploy.sh`
- [ ] Configure systemd service
- [ ] Set up log rotation
- [ ] Configure monitoring/alerting

### Optimization (Next week)
- [ ] Implement result caching (30-50% cost savings)
- [ ] Add auto-scaler (4→32 instances)
- [ ] Set up Prometheus metrics
- [ ] Create Grafana dashboard

## Key Metrics to Track

| Metric | Target | Current |
|--------|--------|---------|
| GPU Utilization | >80% | 90-100% ✅ |
| Success Rate | >95% | TBD |
| Throughput | >100 tasks/hr | ~300+ ✅ |
| Temperature | <75°C | 60-65°C ✅ |
| Error Rate | <5% | TBD |

## Files to Review After Test

```
gpu_max_test/YYYYMMDD_HHMMSS/
├── gpu_stats.csv          # GPU metrics every 5 min
├── task_*.log            # Individual task logs
└── [Future: results.json] # Structured results
```

## Production Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Load Balancer │────▶│  Selfware Pods  │────▶│   vLLM (2x GPU) │
│   (Auto-scale)  │     │   (4-32 inst)   │     │   (Qwen3.5-27B) │
└─────────────────┘     └─────────────────┘     └─────────────────┘
         │                       │
         ▼                       ▼
┌─────────────────┐     ┌─────────────────┐
│  Result Cache   │     │  Prometheus     │
│  (Redis/SQLite) │     │  + Grafana      │
└─────────────────┘     └─────────────────┘
```

## Questions to Answer After Test

1. What's the sustained throughput over 6 hours?
2. Does GPU temperature stabilize or increase over time?
3. Are there any memory leaks (increasing usage over time)?
4. What's the failure rate under sustained load?
5. How consistent is the latency?

## Next Feature Priority

Based on test results, implement in this order:

1. **Result Caching** - If similar tasks detected
2. **Auto-scaling** - If queue backlog detected
3. **Checkpoint/Resume** - If long tasks fail
4. **Multi-modal** - If vision tasks needed
5. **Batch API** - If bulk processing needed

---

*Test started: $(ls -td gpu_max_test/*/ | head -1 | xargs basename)*
*Expected completion: 6 hours after start*
