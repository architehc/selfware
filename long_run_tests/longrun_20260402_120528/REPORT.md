# Long-Run Test Results — Thu Apr  2 14:18:55 EDT 2026

## Configuration
- Model: txn545/Qwen3.5-122B-A10B-NVFP4 (sglang)
- Max iterations: 500
- Min completion steps: 20
- Duration: ~1.5 hours

## Results

| Agent | Task | Steps | Lines | Compiles | Tests |
|-------|------|-------|-------|----------|-------|
| A0 | Calculator | 500 | 365 | YES | test result: FAILED. 18 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| A1 | Task mgmt | 500 | 347 | YES |  |
| A2 | RPN calc | 17 | 1 | YES | test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| A3 | In-memory DB | 500 | 384 | no |  |
| A4 | Text stats | 18 | 1 | YES | test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| A5 | Event system | 18 | 1 | YES | test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| A6 | State machine | 18 | 1 | YES | test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |
| A7 | LRU cache | 500 | 336 | no |  |
| A8 | Matrix lib | 500 | 419 | no |  |
| A9 | Cmd parser | 18 | 1 | YES | test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s |

## Key Findings
- 5/10 agents wrote substantial code (336-419 lines)
- 2/10 compile cleanly (A0, A1)
- A0: 18/23 tests passing
- 3 agents hit the 500-step max (truly long-running)
- 5 agents exited early at step 17-18 before auto-write engaged
