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
green; the task reported failure only on max iterations, because it kept going
after succeeding.

| | |
| --- | --- |
| oracle | `1 added / 1 removed  calculator.py` |
| citation | `unreviewed calculator.py (2 lines, turn 2)` |
| unattributed | **0** |
| unknown_size_obligations | **0** |
| possible_unrecorded_mutations | 2 |
| outstanding / untested | 2 / 2 |

**Zero unattributed against real model traffic.** The schema keys hold outside
the scripted fixture: an actual Qwen tool call in XML mode was attributed to the
right file and turn, with a size.

**Six passing test runs did not move the debt.**

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

### A known imprecision, found here

`possible_unrecorded_mutations: 2` came from two read-only commands:

    ls -la; echo '---'; find . -maxdepth 2 -name '*.rs'

They are flagged because the compound-command check keys on `;`, `&&` and `||`
without asking what the segments do. The error is in the conservative direction
— overstating uncertainty rather than understating it — but it does inflate the
figure, and a consumer treating it as a count of real unrecorded mutations would
be misled. Narrowing it needs per-segment classification, not a different
separator list.

## Not yet done

- **`file_delete` end to end.** It is dropped before dispatch with no
  `tool_call_started` and no logged refusal, in both the scripted and
  read-before-delete variants. `mutating_tools_so_far` does not advance. The
  ledger correctly records nothing for a tool that never ran, so this is a
  product finding outside the ledger's scope — but it means the deletion path is
  covered only by unit tests.
- Incremental checkpoint/resume through a real Agent, rather than a constructed
  checkpoint.
- Threshold evaluation. Nothing here justifies a number yet, and "more debt" is
  not a correctness criterion: missing observations lower it and duplicate
  observations inflate it.
