# Task: Coverage gap analysis

List every .rs file in src/ that has ZERO #[test] functions. Then categorize them:

1. **No tests needed** — simple modules, type definitions, pure data structures
2. **Should have tests** — logic modules, parsers, algorithms, safety checks
3. **Critical untested** — anything in src/safety/, src/agent/, src/tools/ with zero tests

Write the full report to /home/ivo/selfware/swarm_outputs/COVERAGE_GAPS.md with:
- Total files analyzed
- Number with zero tests
- Top 10 highest-priority modules that need tests
- For each of the top 3, write a starter test skeleton
