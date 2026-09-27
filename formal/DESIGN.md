# Tasks, agents and resources as one state machine

Status: proposal (2026-09-26). Scope: the design only, no code yet.

## 1. Why

Every task selfware runs can start things that outlive the model call: Docker
containers, background processes, PTY shells, headless browsers, MCP servers,
HTTP servers on ports, git worktrees, temp dirs. Today:

- **Six unrelated status enums.**
  - checkpoint `TaskStatus` (InProgress/Completed/Failed/Paused)
  - `AgentState` (Planning/Executing/ErrorRecovery/Completed/Failed)
  - `RunStatus` and the run registry's string status
  - swarm `AgentStatus` + swarm `TaskStatus`
  - multi-chat `AgentStatus`
  - `record_state_transition(&str, &str)` strings

  Only `AgentState` validates transitions (`loop_control.rs:387`).
- **No ownership.**
  - A container id is returned to the model and forgotten (`container/tools.rs:321`).
  - Background processes live in a global `PROCESS_MANAGER` keyed by name, not by task.
  - Ports are reserved for 30 s and then untracked.
  - Browsers and the evolve HTTP server are untracked.
  - When a task ends, nothing it started is guaranteed to stop.
- **No per-agent view.** Tokens and latency exist per run (`RunSummary`, `session_usage.rs`). Time per state, agent type and last task type are not recorded together, and no UI lists agents and their tasks.
- **Journals can be listed, viewed, resumed and deleted, but not edited.** Nothing is navigable as a tree.

## 2. Goals

1. One typed lifecycle for **tasks**, **agents** and **resources**. Every transition is checked, recorded and replayable.
2. **Ownership.** Every resource has exactly one owner task, and every task has one owning agent. A task that reaches a terminal state tears down everything it owns, within a deadline. What survives is flagged as a leak.
3. **Zombie detection.** Periodically, and at startup, the recorded state is reconciled with reality (docker, pids, listening ports). Anything with no live owner is reported and can be reaped.
4. **Per-agent and per-task metrics** come from recorded transitions and usage, never estimated (Rule 4): time in each state, tokens, cost, tool calls, agent type, last task type, resources held.
5. **Navigation.** The user can go from session to agent to task to resource, open any task, **edit** it, and go back. This works in the TUI and the CLI.

## 3. Entities

```
Session ─┬─ Agent (role/type, model) ─┬─ Task (tree: parent → subtasks)
         │                            │     └─ Resource (container | process | pty |
         │                            │                  browser | port | mcp | server |
         │                            │                  worktree | tempdir)
         └─ …                         └─ …
```

- **TaskId** and **AgentId** are ULIDs, so they sort by time. A Task has a `parent: Option<TaskId>`, so swarm and multi-chat subtasks form a tree.
- **Task type** is the classification selfware already computes: read-only report, mutation, review, exact response, Q&A, workflow. It is stored on the task, so "last task type" is a lookup.
- **Agent type** is the swarm `AgentRole` (Architect, Coder, Tester, Reviewer, …), plus `Main` for the REPL/`run` agent.

## 4. State machines

### 4.1 Task

```
            ┌──────────────── edit ───────────────┐
            ▼                                     │
 Draft ─▶ Queued ─▶ Planning ─▶ Executing ◀──▶ Waiting{approval|user|resource}
                        │           │  ▲
                        │           ▼  │
                        │       Verifying
                        ▼           │
                   ┌────┴───────────┴──────────────┐
                   ▼            ▼          ▼        ▼
               Completed     Failed   Interrupted  Cancelled      (terminal)
                                          │
                                          └─ resume ─▶ Queued (same TaskId, next segment)
 any non-terminal ─ pause ─▶ Paused ─ resume ─▶ (previous state)
```

- Terminal states are sticky (the run registry already does this). `Interrupted` is the only terminal state that can resume, and resuming starts a new segment of the same task (this matches `--continue`).
- **Every transition into a terminal state runs the teardown effect (§5).** This is the "task done ⇒ its containers done" rule.
- Each outcome maps one-to-one onto what 0.9.2 already reports: `Completed` → outcome `completed`, `Failed{FailureKind}`, `Interrupted` → exit 130.

### 4.2 Agent

```
 Idle ─assign─▶ Working{task} ─▶ Blocked{task, reason} ─▶ Working{task}
   ▲                 │
   └──── done ───────┘          any ─ crash ─▶ Crashed ─ restart ─▶ Idle
                                any ─ stop  ─▶ Stopped (terminal)
```

An agent works on at most one task at a time. "Last task type" and "time working/blocked/idle" are derived from the event log.

### 4.3 Resource

```
 Requested ─▶ Starting ─▶ Live ─▶ Draining ─▶ Released          (terminal, clean)
                 │                   │
                 └──── fail ─────────┴─ deadline passed ─▶ Leaked   (terminal, alarm)
 Live ─ owner gone (reconcile) ─▶ Orphaned ─ reap ─▶ Draining
```

- `Draining` asks politely first (`docker stop`, SIGTERM, MCP shutdown), then forces (`docker rm -f`, SIGKILL on the process group, which `ProcessGroupGuard` already does).
- `Leaked` means teardown did not finish. It is shown loudly and re-tried by the reaper.

### 4.4 One implementation

A small in-repo `lifecycle` module, with no new crate:

```rust
trait Machine { type State; type Event;
    fn next(s: &Self::State, e: &Self::Event) -> Result<Self::State, InvalidTransition>;
    fn on_enter(s: &Self::State) -> Vec<Effect>;   // e.g. teardown on terminal
}
```

- The transition table is a `match` like `AgentState::is_valid_transition`, generalised.
- An invalid transition is a typed error, never a panic (`set_state` panics today). The existing enums become views of it.
- `record_state_transition` strings are replaced by typed events.

## 5. Ownership and teardown

- **Labels at creation.** Every spawn site registers the resource before or right after it starts, owned by the current TaskId:
  - containers are started with `--label selfware.task=<id> --label selfware.agent=<id> --label selfware.session=<id>`, so ownership survives a crash;
  - processes record pid and pgid;
  - ports record the bound address;
  - worktrees and temp dirs record their path.
- **Spawn sites** (all known from the survey):
  - `container/tools.rs` run/build/compose
  - `devops/process_manager.rs` (the background `process` tool)
  - `pty_shell.rs` sessions
  - `browser.rs` chrome/node
  - `mcp/transport.rs` children
  - `evolve/server.rs` HTTP bind
  - `git_worktree`
  - spill/temp dirs
  - `shell_exec` is bounded and killed by process group already. It only needs registering when it backgrounds something (`&`, `nohup`), and those should be refused or routed to the `process` tool.
- **Teardown effect.** On entering any terminal task state, every resource the task owns moves Live → Draining → Released within a deadline (default 10 s, configurable), in reverse order of creation. Anything left is marked Leaked, and the run summary gains a line such as `resources: 3 released, 1 leaked (container 8f2c… still running)`, true to Rule 3.
- **Opt-out.** A resource a user deliberately wants kept (e.g. a dev server started by "start the app") is marked `keep = true` at creation, which asks for confirmation. It is then re-owned by the Session instead of the task, and is still listed and reapable.

## 6. Zombie reconciliation

A reaper runs at startup, every N minutes in long sessions, and on `selfware resources reap`:

| Source of truth | Query | Zombie if |
|---|---|---|
| Docker | `docker ps -a --filter label=selfware.task` | the owner task is terminal or unknown |
| Processes | pid/pgid alive (`kill -0`) and the command matches | the owner task is terminal or its session process is dead |
| Ports | listening sockets (`lsof -iTCP -sTCP:LISTEN` / netlink) matched to recorded binds | no Live owner |
| Worktrees/temp | path exists | the owner task is terminal and was not kept |
| Runs | run-registry pid dead while "Running" (this "Stale" check exists today) | always |

Zombies are shown with owner, age and last task type. Reaping them uses the same Draining path, never a blind `rm -f`. Unlabelled containers are never touched.

## 7. Metrics (measured, Rule 4)

The source is one append-only **event log**: `~/.selfware/state/events.jsonl`, or SQLite once querying matters. Each record is `{ts, entity, id, from, to, cause}` plus the existing usage events. Everything is a projection of it:

- **Per task:** time in each state, turns, tokens (prompt/completion/total, main loop vs side calls, from `TaskUsage`), cost when the provider reports it, tool calls, verification/grounding outcome, resources (live, released, leaked).
- **Per agent:** type, current state, time working/blocked/idle, tasks completed/failed, the running token total, last task and its type, resources currently held.
- **Per session:** live agents, live resources, zombies.

## 8. Navigation and editing

**TUI:** a new **Tasks** pane, with a breadcrumb bar and a back stack.

```
Session › Agents › coder-2 › Task 01J9…  "add max_words to slugify()"
─────────────────────────────────────────────────────────────────────
 state      Executing (step 12, 3m41s)      type   mutation
 tokens     212k (main 177k · side 35k)     cost   not reported
 resources  ● container slugify-test (Live, :5000)  ● pty #2 (Live)
 timeline   Queued 0s → Planning 3s → Executing …
 [Enter] open  [e] edit  [p] pause  [x] cancel  [r] reap  [Esc] back
```

- **Enter** drills down (agent → task → subtask → resource or message). **Esc/Backspace** goes back. The stack is kept, so going back returns to the same scroll position.
- **Edit** (`e`) opens the task description and constraints (budget, max turns, allowed paths) in an inline editor, or in `$EDITOR`:
  - *Non-terminal task:* Pause → edit → Resume. The edit is recorded as an event, and the agent receives it as a user message ("task updated: …"), so the conversation stays truthful.
  - *Terminal task:* edit creates a **fork**, a new TaskId whose `parent` is the original, so history is never rewritten.

**CLI equivalents:**
- `selfware tasks [--tree]`
- `selfware task show <id>`
- `selfware task edit <id>`
- `selfware task pause|resume|cancel <id>`
- `selfware agents`
- `selfware resources [--zombies]`
- `selfware resources reap [--dry-run]`

`journal` and `runs` become views of the same data.

## 9. Migration, in phases

Each phase is shippable alone:

1. **Core.** Add the `lifecycle` module and event log. Put `AgentState` and the checkpoint `TaskStatus` behind it, replace the `record_state_transition` strings with typed events, and make invalid transitions a typed error instead of a panic.
2. **Resource registry and teardown.** Label containers, route the spawn sites in §5 through the registry, run teardown on terminal states, and add the leak line to the run summary.
3. **Reaper.** Add startup and periodic reconcile plus `resources --zombies/reap`, and fold the run registry's Stale check into it.
4. **Metrics projection.** Build per-task and per-agent stats from the log, with `agents`/`tasks` commands.
5. **Navigation and edit.** Add the TUI Tasks pane with breadcrumbs and a back stack, and the edit flow (pause/edit/resume, or fork).
6. **Swarm and multi-chat.** Move their agent and task statuses onto the same machines, so every agent type shows up in one place.

## 10. Open questions

- Should a kept resource (dev server) survive the session? Proposal: no, unless it is pinned with `selfware resources keep <id>`.
- Should the event log be JSONL (simple, greppable) or SQLite (queries, concurrency)? Proposal: start with JSONL and move to SQLite in phase 4 if the projections get slow.
- What is the teardown deadline, and should Daemon mode escalate leaks to the killswitch?
- How long should edits to running tasks be kept in the audit trail: indefinitely, or for the journal's retention?

## 11. Phase 1 as implemented (0.9.3)

- `src/lifecycle/`: `TaskMachine`, `AgentMachine`, `ResourceMachine` behind the
  `Machine` trait; `Tracked<M>` applies events, runs the proved invariants as
  runtime oracles (`debug_assert!` in debug/test builds, an error log in
  release) and appends to the event log. Refused events are
  `InvalidTransition`, never a panic.
- The task table is the Lean `step` function. `formal/task_table.json` is its
  export; the Rust test `rust_table_equals_the_lean_model_for_every_pair`
  compares all 140 (state, event) pairs, refusals included.
- `scripts/check_formal.sh` re-checks both Lean files and the exported table
  (`--write` regenerates it; skipped with a message when `lean` is absent).
  It is not a CI job: CI would need a Lean toolchain installed per run.
  Run it whenever `formal/` or `src/lifecycle/task.rs` changes.
- The event log is `~/.selfware/state/events.jsonl` (`SELFWARE_EVENT_LOG`
  overrides it or turns it `off`), one line per transition, best-effort.
- The main run (`run_task`, `continue_execution`) is mirrored onto the task
  machine; the terminal event comes from the same `RunEnd` the run summary
  reports. Not mapped yet: `need_input`/`input_arrived`, `verify`/`verified`/
  `reject`, `pause`, `cancel`, and the agent machine for the main agent.
- `selfware tasks [--limit N]` and `selfware task show <id>` read the log.
