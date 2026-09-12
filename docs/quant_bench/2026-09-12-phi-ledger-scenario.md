# Evidence ledger — deterministic scenario evaluation

Shadow mode. Real dispatcher, scripted endpoint, independent filesystem oracle.
Purpose: establish whether recorded events explain the obligations, BEFORE any
threshold tuning or display.

## Method

A local HTTP server returns a fixed sequence of tool calls, so the dispatcher
exercises every observation path with no model variance. The fixture is a git
repo; `git diff --cached --numstat` is the oracle, computed independently of the
ledger.

    target/phi-session-eval/mock_llm.py     scripted endpoint
    target/phi-session-eval/fixture/        isolated git fixture
    target/phi-session-eval/config.toml     allowed_paths scoped to the fixture

Scripted turns: write, edit (real `new_str` schema), multi-file edit, delete,
failing test via shell, compile-only via shell, opaque shell mutation, passing
test via shell.

## Result

| path | oracle (net) | ledger (per edit) | verdict |
| --- | --- | --- | --- |
| `calc.py` | +2 / -0 | 3 | recorded (2 from write, 1 from edit) |
| `notes.md` | +2 / -0 | 3 | recorded (multi-edit replacement) |
| `opaque.txt` | +1 / -0 | — | **not recorded**, flagged |

    unattributed                  0
    unknown_size_obligations      0
    possible_unrecorded_mutations 1
    outstanding / untested        6 / 6

Journal:

    t4 shell_exec  run_finished  Failed   python3 -m unittest test_broken.py
    t5 shell_exec  opaque_run    Passed   printf 'x\n' >> opaque.txt   (mutated)
    t6 shell_exec  run_finished  Passed   python3 -m unittest test_calc.py

## What this establishes

**Schema assumptions hold against real traffic.** `unattributed` and
`unknown_size_obligations` are both zero across write, edit and multi-edit. The
`new_str` correction is confirmed by execution, not by reading the tool
definitions.

**Compile is not execution, through shell dispatch.** `python3 -m py_compile`
recorded `opaque_run`; `python3 -m unittest` recorded `run_finished`. Both pass
`shell_command_is_verification`, and only the second discharges.

**The prediction held.** Two passing test runs left `untested_lines` at 6,
because `cargo`/`unittest` report no per-file coverage and
`WorkspaceCoverageUnknown` discharges nothing.

## What it found

**A failing test run was recorded as `Passed`.** `shell_exec` reports ITS OWN
success — process spawned and reaped — not the command's exit status, so a red
suite arrived as `succeeded: true`. The observer trusted that flag and would
have discharged obligations on a failing run.

No unit test could catch this: they pass `succeeded` in directly and cannot see
the difference between a tool that worked and a command that passed. The
observer now reads `exit_code` from the tool result, and treats an unparsable or
absent exit code as failure rather than success.

The first version of this scenario did not catch it either — its "failing" test
was `unittest discover -p 'nope_*.py'`, which finds no tests and **exits 0**. The
fixture was wrong, not the code. A genuinely failing test was needed.

## Known limits of this comparison

**The two columns measure different things.** The oracle is the net diff against
HEAD; the ledger is the size of each edit. Two edits to one file give 2+1 = 3
against a net +2. Neither is wrong, and the numbers should not be expected to
match — the comparison validates *attribution* (which paths, which turns), not
magnitude.

**`opaque.txt` has no obligation.** It was created by a shell command the
observer cannot attribute. `possible_unrecorded_mutations: 1` is the honest
flag, and it means outstanding debt is a **floor, not a total**. Any consumer
that ignores that field will overstate how much is accounted for.

**`file_delete` never executed.** It was refused before dispatch — Step 4 has no
`tool_call_started`, unlike Step 5. The ledger correctly recorded nothing. The
deletion path therefore remains unexercised end to end.

**`cargo_fmt` is now observed** as an opaque mutation: it genuinely rewrites
files and genuinely names none, so recording uncertainty beats both silence and
an invented path. The registry sweep's uncovered list is empty.

## Live run — Flash Next against llm.selfware.design

Same fixture shape, real model, XML tool mode, `enable_thinking=false`.
The model fixed the bug (`return a - b` -> `return a + b`) and the suite went
green.

**Correction.** An earlier revision of this document said the task "reported
failure only on max iterations, because it kept going after succeeding". That
was wrong, and it buried a product failure under a benign explanation.

`turn_0010.json` shows the model declaring completion, reporting
`Ran 2 tests ... OK`, and correctly identifying the one Rust error as
pre-existing and out of scope. Selfware **refused** it:

    refused: FailingTestsAccepted: the latest verification after your edit
    failed: cargo_check failed: E0599 in experiments/context_select/main.rs

`experiments/context_select/main.rs` belongs to the **enclosing Selfware
workspace**, not the fixture. The fixture lived under `target/`, so `cargo`
walked up, found the parent workspace, and failed there. The completion gate
then bound that unrelated failure to this task and would not let it finish.

The model did not keep going after succeeding. It was not allowed to stop.

Two separable problems:

- **Verification is not project-aware.** The prompt instructed `cargo_check` /
  `cargo_test` for a Python task, and cargo's upward project discovery reached
  outside the task's workspace.
- **The completion gate binds failures from outside the task scope.** A green
  Python suite plus a red compile in an unrelated parent crate reads as
  "verification failed".

A Python project nested inside a Rust repository is an ordinary arrangement, so
this is not an artefact of the fixture's location. The fixture made it visible.

| | |
| --- | --- |
| oracle | `1 added / 1 removed  calculator.py` |
| citation | `unreviewed calculator.py (2 lines, turn 2)` |
| unattributed | **0** |
| unknown_size_obligations | **0** |
| possible_unrecorded_mutations | 2 |
| outstanding / untested | 2 / 2 |

**Zero unattributed for the cases exercised.** An actual Qwen tool call in XML
mode was attributed to the right file and turn, with a size.

The denominator matters: this run exercised **one** live edit, plus the scripted
write / edit / multi-edit / delete / shell cases. Zero unattributed across those
is evidence the schema keys are right for those shapes. It is not a general
statement about the classifier, and an observer that skipped an event entirely
would also report zero. Measuring observed events against an independently
enumerated list of mutations, with an explicit denominator, is still to do.

**Correction to the framing: "untested" was the wrong word.**

An earlier revision reported `untested_lines: 2` and described six green runs as
leaving the change "untested". That asserts more than the evidence supports. The
tests ran, after the edit, and passed. What was never established is whether they
*covered* the changed lines.

Execution, outcome, coverage and review are four separate facts. The ledger now
keeps them apart: `ObligationKind::UnconfirmedCoverage` is named for what is
missing, and each passing run that reported no coverage is counted on the
obligation rather than discarded. The citation reads:

    coverage unconfirmed calculator.py (2 lines, turn 2) — 6 passing runs reported no coverage

which is the whole finding in one line, and does not claim the change was never
tested.

**Six passing test runs did not discharge the obligation.**

    rec2 .. rec18   untested=2  unreviewed=2  outstanding=2

The model ran `python3 -m unittest test_calculator.py` six times, green every
time, and the obligation stood throughout — because unittest reports no per-file
coverage. This is the designed behaviour and the prediction held exactly. It is
also the finding most worth sitting with before anything surfaces this to a
user: a flat line through six green runs is correct, and it is not obviously
*useful*. Whether "2 lines untested after six passing suites" reads as honest or
as noise is a question for threshold evaluation, which is precisely why this is
still shadow mode.

`cargo_check` was recorded `opaque_run / Failed` — correctly not a test run, and
correctly failed, since this is not a Rust project.

### A known imprecision, found here — since corrected

`possible_unrecorded_mutations: 2` came from two read-only commands:

    ls -la; echo '---'; find . -maxdepth 2 -name '*.rs'

They were flagged because the check keyed on `;`, `&&` and `||` without asking
what the segments do — and worse, it flagged *every* non-verification shell
command, so a bare `ls` counted too.

Now each segment is classified. A command is treated as read-only only when every
segment is established as such, and a read-only *verb* is not enough:
`grep x f > out`, `find . -delete`, `sed -i`, and anything with `$(...)` or
backticks all preserve uncertainty. Anything the classifier cannot establish as
read-only stays flagged, because the cost of a false "nothing changed" is a
silently unreviewed edit.

## Not yet done

- ~~`file_delete` end to end~~ **Resolved.** It is `destructive: true`, and in
  headless mode the yolo gate returns `RequireConfirmation` with no operator to
  ask, so dispatch fails closed before `tool_call_started`. The setting is
  `[yolo].allow_destructive_shell`, not `[safety]` — Selfware prints
  `Unknown config key [safety].allow_destructive_shell — this key is ignored`,
  which is why two earlier attempts changed nothing. With it in the right
  section the deletion dispatches and the ledger records:

      unreviewed scratch.txt (0 lines, turn 4)

  The coverage obligation is retired — nothing left to test — while the removal
  itself still wants reading. Size is 0 because `file_delete` reports none, not
  because nothing happened.
- Incremental checkpoint/resume through a real Agent, rather than a constructed
  checkpoint.
- Threshold evaluation. Nothing here justifies a number yet, and "more debt" is
  not a correctness criterion: missing observations lower it and duplicate
  observations inflate it.
