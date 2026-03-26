# SWE-bench Evaluation Report

**Status**: FAILED - No tasks completed

Tasks were started but did not complete successfully. Check individual task logs in:
`/home/ivo/selfware/swebench_eval/20260325_121127/tasks/`

## Debug Commands

```bash
# Check running containers
docker ps --filter name=swebench

# Check task logs
ls -la /home/ivo/selfware/swebench_eval/20260325_121127/tasks/*/

# Check if vLLM is still running
curl http://localhost:8000/health
```
