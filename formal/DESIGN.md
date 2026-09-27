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
  compares all 150 (state, event) pairs, refusals included.
- The resource table is the Lean `step` function of `formal/ResourceFsm.lean`
  (R1 `released` sticky, R2 `leaked` left only by `reap`/`stopped`, R3 every
  `draining` exit settles, R4 `released` only on confirmation, R5 no unsettled
  state is stuck, R6 the reaper path). `formal/resource_table.json` is its
  export; `rust_resource_table_equals_the_lean_model_for_every_pair` compares
  all 63 pairs. The registry needed one transition the first sketch lacked:
  `draining --abandon--> leaked`, a drain that gives up before its deadline
  (foreign or unknown handle, kind not stopped automatically, finalize
  failed) — added to the model first, never as a release.
- `scripts/check_formal.sh` re-checks the Lean files and both exported tables
  (`--write` regenerates them; skipped with a message when `lean` is absent).
  It is not a CI job: CI would need a Lean toolchain installed per run.
  Run it whenever `formal/` or `src/lifecycle/{task,resource}.rs` changes.
- `src/resources` (the task-owned resource registry) uses the lifecycle's
  `ResourceState`/`ResourceKind`/`ResourceEvent`; every registry state change
  goes through `ResourceMachine` and is appended to the event log
  (`entity: resource`, owner = the task, with a cause). A refused event is a
  typed `TransitionError`, logged. `resources.json` keeps its labels
  (`server_port` from the first registry still reads, as `port`).
- The event log is `~/.selfware/state/events.jsonl` (`SELFWARE_EVENT_LOG`
  overrides it or turns it `off`), one line per transition, best-effort.
- The main run (`run_task`, `continue_execution`) is mirrored onto the task
  machine; the terminal event comes from the same `RunEnd` the run summary
  reports. Not mapped yet: `need_input`/`input_arrived`, `verify`/`verified`/
  `reject`, (`pause`, `edit` and `cancel` are mapped since phase 5, §12; the
  agent machine since §13).
- `selfware tasks [--limit N]` and `selfware task show <id>` read the log;
  `task show` also lists the resource transitions the task owned.

## 12. Phase 5 as implemented (0.9.3)

- **Model.** The task machine gained `edit` (paused → paused only). The Lean
  model proves P9: an edit is accepted only while paused, keeps the task
  paused, and the edited task can still resume, be cancelled or time out.
  `task_table.json` has 40 transitions; the oracle checks P9 at runtime.
- **Control.** `lifecycle::control::TaskControl` is shared by the agent and
  an in-process UI. Requests are acted on at one safe point, the top of a
  loop iteration (between steps), so a pause never interrupts a model or tool
  call. Recorded as `pause`, `edit`, `resume`; a cancel request ends the run
  `cancelled` (not `interrupted`). Time paused is measured and taken out
  of every wall clock the run is held to (the agent's segment clock behind
  `max_wall_secs`, the API client's wall-budget anchor, hence also the
  deadline-based `timeout`); per-call caps need nothing, no call is in
  flight. The run summary, the Tasks pane and `task show` report it when
  nonzero.
- **Edit of a live task** changes the description, max turns and token
  budget (the API client is rebuilt so its own budget stop follows). It is
  recorded with the measured usage, and the model receives
  "Task updated: …" as a user message. An edit that would end the task on
  the spot is refused. An edited description is what every report shows
  from then on — run summary, structured result (`task_edited`), journal
  (checkpoint saved at the edit, `original_task_description` kept),
  `task show`, the Tasks pane, the outcome telemetry — each noting that it
  was edited and what the task was started as.
- **`allowed_paths` stays read-only mid-task.** The `[safety]` config is
  copied at agent build into independent holders: the `SafetyChecker`,
  each file/git/worktree/LSP tool's own config inside the `ToolRegistry`,
  the FIM tool, the process-global file-tool fallback, the YOLO manager's
  deny list and the citation resolver. They cannot be swapped as one unit
  at the pause point; a partial swap would leave some holders on the old
  list, so a narrowing would silently not apply everywhere (a widening
  relative to what the user asked for). Changing it means a new task.
- **Edit of a finished task** forks it: a new task id whose records carry
  `parent`; the original's history is untouched.
- **Usage in the log.** Terminal records and edits carry `usage`: total
  tokens, the main-loop/side-call split when the task start was observed
  (not for resumed segments), and the provider cost only when reported.
- **TUI.** Ctrl+T opens the Tasks pane: breadcrumb, back stack, Enter/Esc,
  and `[e] [p] [x] [r]`. Its view is a pure function of the log, the
  resource registry (read-only listing), the live task and the journal.
  Its agent list is the `selfware agents` projection (§13).
- **CLI.** `tasks --tree`, `task show` (parent, usage, forks),
  `task edit <id>` (fork; prints `selfware run --fork-of <id> …`),
  `run --fork-of`. `task pause|resume|cancel`, and `edit` of a live task in
  another process, say that cross-process control is not supported yet and
  exit non-zero. No control channel between processes exists yet.

## 13. Agents on the lifecycle (0.9.3)

- Resource teardown at run end is driven by the task's `TeardownOwned`
  effect; without it (no tracker, already terminal, refused transition) the
  drain still runs and a warning names why. A resource entering `leaked`
  raises `LeakAlarm` (warning + the run-summary line + the headless
  result's `resources` object).
- The main agent is on `AgentMachine` (type `main`, id `main-<8 hex>`):
  idle → working on `assign` at task/segment start, blocked while its task
  is paused, `done` at its end, `stop` when dropped. Its tasks carry it as
  `owner` (tasks recorded before this have no owner and list under `main`).
- `selfware agents [--limit N]` and the Tasks pane's agent list project per
  agent: type, state, time in state (from the recorded timestamp; a live
  state recorded by a process that is gone is shown as such), tasks
  completed / failed / stopped, token total (the sum of each task's
  measured `usage.total_tokens` on its last terminal record — the one
  source of per-task tokens — with the count of ended tasks carrying none),
  last task and its type, and unreleased resources the registry attributes
  to it. Sub-agents, swarm roles and multi-agent chat are not on the agent
  machine yet.
