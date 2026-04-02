# Guided Scheduler Lab

This directory exists to stress-test Selfware over a longer autonomous session.

## Primary Goal

Turn this crate into a solid, well-tested task scheduler library. The current code is intentionally incomplete and a few tests are expected to fail.

## Milestones

1. Make the existing integration tests pass without deleting coverage.
2. Improve the implementation quality after the initial green run:
   - preserve parsed priority and recurrence metadata when tasks are added
   - make dependency unlocking work correctly after upstream tasks complete
   - keep parser errors explicit and human-readable
3. Add at least 3 additional tests that cover edge cases you discover while fixing the code.
4. Leave behind concise engineering notes in `RUN_NOTES.md`:
   - what was broken
   - what you changed
   - what still feels weak

## Operating Rules

- Work in small batches and re-run tests often.
- Do not stop just because the first test run turns green. Continue into milestone 3 and 4.
- Prefer clean, boring fixes over speculative refactors.
- If you need to choose, optimize for correctness and recoverability.
