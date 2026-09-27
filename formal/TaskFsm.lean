/-
  selfware task / resource lifecycle — formal model (proof of concept).
  Core Lean 4 only (no Mathlib). Check with: `lean TaskFsm.lean`.

  What is proved here:
  * P1 terminal task states are sticky; only `interrupted` can be resumed.
  * P2 no task state can hang: every non-terminal state has a timeout exit.
  * P3 finishing a task leaves none of its resources `live` (teardown starts).
  * P4 bounded teardown: a `draining` resource is `released` or `leaked`
    once the clock reaches its deadline — whatever the ticks in between.
  * P5 the reaper turns every orphan (live resource of a finished task)
    into `draining`, so P4 then bounds it.
  * P6 self-healing retries terminate: each retry consumes budget.
  * P7 circuit breaker: an open breaker half-opens once its cooldown has
    passed (bounded unavailability); a closed breaker trips instead of
    absorbing failures past its threshold.
  * P8 retry with backoff never runs past its wall-clock deadline, for any
    delay policy.
  * P9 a task is only edited while paused, and an edit changes nothing but
    the recorded description/constraints: the state stays `paused`, and
    the task can still resume or be abandoned afterwards. A finished task
    is never edited (it is forked into a new task instead).
-/

namespace Selfware

/-! ## Task states and events -/

inductive TaskState where
  | queued | planning | executing | waiting | verifying | paused
  | completed | failed | interrupted | cancelled
  deriving DecidableEq, Repr

inductive TaskEvent where
  | start | planned | needInput | inputArrived | verify | verified | reject
  | pause | resume | succeed | fail | interrupt | cancel | timeout
  | edit
  deriving DecidableEq, Repr

def TaskState.terminal : TaskState → Bool
  | .completed | .failed | .interrupted | .cancelled => true
  | _ => false

/-- The transition table. `none` = the event is refused in that state
    (in Rust: a typed `InvalidTransition`, never a panic). -/
def step : TaskState → TaskEvent → Option TaskState
  | .queued,    .start        => some .planning
  | .planning,  .planned      => some .executing
  | .executing, .needInput    => some .waiting
  | .waiting,   .inputArrived => some .executing
  | .executing, .verify       => some .verifying
  | .verifying, .verified     => some .completed
  | .verifying, .reject       => some .executing
  | .executing, .succeed      => some .completed
  -- pause / resume (resume from pause re-enters execution; the real
  -- implementation remembers the pre-pause state)
  | s,          .pause        => if s.terminal || s == .paused then none else some .paused
  | .paused,    .resume       => some .executing
  -- the user edits a paused task (description / constraints); the task
  -- stays paused until it is resumed
  | .paused,    .edit         => some .paused
  -- an interrupted task resumes as a new segment of the same task
  | .interrupted, .resume     => some .queued
  -- failure / interrupt / cancel from any live state
  | s,          .fail         => if s.terminal then none else some .failed
  | s,          .interrupt    => if s.terminal then none else some .interrupted
  | s,          .cancel       => if s.terminal then none else some .cancelled
  -- every waiting-ish state has a deadline: its timeout fails the task
  | s,          .timeout      => if s.terminal then none else some .failed
  | _,          _             => none

def allStates : List TaskState :=
  [.queued, .planning, .executing, .waiting, .verifying, .paused,
   .completed, .failed, .interrupted, .cancelled]

def allEvents : List TaskEvent :=
  [.start, .planned, .needInput, .inputArrived, .verify, .verified, .reject,
   .pause, .resume, .succeed, .fail, .interrupt, .cancel, .timeout, .edit]

theorem allStates_complete (s : TaskState) : s ∈ allStates := by
  cases s <;> simp [allStates]

theorem allEvents_complete (e : TaskEvent) : e ∈ allEvents := by
  cases e <;> simp [allEvents]

/-! ### P1 — terminal states are sticky; only `interrupted` resumes -/

theorem terminal_sticky_table :
    ∀ s ∈ allStates, ∀ e ∈ allEvents, s.terminal = true →
      step s e = none ∨ (s = .interrupted ∧ e = .resume) := by
  decide

theorem terminal_sticky (s : TaskState) (e : TaskEvent) (h : s.terminal = true) :
    step s e = none ∨ (s = .interrupted ∧ e = .resume) :=
  terminal_sticky_table s (allStates_complete s) e (allEvents_complete e) h

/-! ### P2 — nothing hangs: every non-terminal state has a timeout exit -/

theorem timeout_exits_table :
    ∀ s ∈ allStates, s.terminal = false → step s .timeout = some .failed := by
  decide

theorem no_state_hangs (s : TaskState) (h : s.terminal = false) :
    step s .timeout = some .failed :=
  timeout_exits_table s (allStates_complete s) h

/-! ### P9 — edits happen only while paused and keep the task paused -/

theorem edit_only_while_paused_table :
    ∀ s ∈ allStates, ∀ t ∈ allStates, step s .edit = some t →
      s = .paused ∧ t = .paused := by
  decide

theorem edit_only_while_paused (s t : TaskState) (h : step s .edit = some t) :
    s = .paused ∧ t = .paused :=
  edit_only_while_paused_table s (allStates_complete s) t (allStates_complete t) h

/-- An edited task is not stuck: it can resume, and it can still be
    cancelled or time out. -/
theorem edited_task_can_leave :
    step .paused .resume = some .executing ∧
    step .paused .cancel = some .cancelled ∧
    step .paused .timeout = some .failed := by
  decide

/-! ## Resources owned by tasks, with time -/

inductive ResState where
  | live | draining | released | leaked
  deriving DecidableEq, Repr

structure Resource where
  owner    : Nat          -- TaskId
  state    : ResState
  deadline : Nat          -- absolute tick by which draining must finish
  deriving Repr

/-- Start teardown for everything `task` owns: live → draining with a
    deadline `grace` ticks from `now`. -/
def teardownStart (task now grace : Nat) (r : Resource) : Resource :=
  if r.owner = task ∧ r.state = .live then
    { r with state := .draining, deadline := now + grace }
  else r

/-- One clock tick for a draining resource: the environment may report the
    stop as done (`stopped = true`); once the deadline is reached the
    resource is forced out of `draining` either way (released if it
    stopped, otherwise leaked → alarm). -/
def tick (now : Nat) (stopped : Bool) (r : Resource) : Resource :=
  match r.state with
  | .draining =>
      if stopped then { r with state := .released }
      else if r.deadline ≤ now then { r with state := .leaked }
      else r
  | _ => r

def settled (r : Resource) : Bool :=
  r.state == .released || r.state == .leaked

/-! ### P3 — finishing a task leaves none of its resources live -/

theorem teardown_leaves_nothing_live (task now grace : Nat) (rs : List Resource) :
    ∀ r ∈ rs.map (teardownStart task now grace),
      r.owner = task → r.state ≠ .live := by
  intro r hr howner
  simp only [List.mem_map] at hr
  obtain ⟨r0, _, rfl⟩ := hr
  unfold teardownStart at howner ⊢
  by_cases h : r0.owner = task ∧ r0.state = .live
  · simp [h]
  · simp [h] at howner ⊢
    intro hl
    exact h ⟨howner, hl⟩

/-! ### P4 — bounded teardown under an arbitrary environment -/

/-- Run the clock from `now` for `n` ticks; `env t` says whether the stop
    completed at tick `t` (any schedule the outside world chooses). -/
def run (env : Nat → Bool) : Nat → Nat → Resource → Resource
  | _,   0,     r => r
  | now, n + 1, r => run env (now + 1) n (tick now (env now) r)

theorem tick_keeps_settled (now : Nat) (b : Bool) (r : Resource)
    (h : settled r = true) : settled (tick now b r) = true := by
  unfold tick
  cases hs : r.state <;> simp_all [settled]

theorem run_keeps_settled (env : Nat → Bool) (n now : Nat) (r : Resource)
    (h : settled r = true) : settled (run env now n r) = true := by
  induction n generalizing now r with
  | zero => simpa [run] using h
  | succ n ih => exact ih _ _ (tick_keeps_settled _ _ _ h)

theorem tick_keeps_deadline (now : Nat) (b : Bool) (r : Resource) :
    (tick now b r).deadline = r.deadline := by
  unfold tick; cases r.state <;> simp <;> split <;> (try split) <;> rfl

/-- Draining at `now` with `deadline ≤ now + n`: after `n+1` ticks the
    resource is released or leaked — no matter what the environment does. -/
theorem teardown_bounded (env : Nat → Bool) :
    ∀ (n now : Nat) (r : Resource),
      r.state = .draining → r.deadline ≤ now + n →
      settled (run env now (n + 1) r) = true := by
  intro n
  induction n with
  | zero =>
    intro now r hd hdl
    simp [run, tick, hd]
    split
    · simp [settled]
    · have : r.deadline ≤ now := by omega
      simp [this, settled]
  | succ n ih =>
    intro now r hd hdl
    show settled (run env (now + 1) (n + 1) (tick now (env now) r)) = true
    cases hs : (tick now (env now) r).state with
    | draining =>
      apply ih
      · exact hs
      · rw [tick_keeps_deadline]; omega
    | live =>
      exfalso; unfold tick at hs; rw [hd] at hs; simp at hs
      split at hs <;> (try split at hs) <;> simp_all
    | released => exact run_keeps_settled _ _ _ _ (by simp [settled, hs])
    | leaked   => exact run_keeps_settled _ _ _ _ (by simp [settled, hs])

/-! ### P5 — the reaper drains every orphan -/

/-- An orphan: a live resource whose owner is in a terminal state. -/
def reap (isTerminal : Nat → Bool) (now grace : Nat) (r : Resource) : Resource :=
  if r.state = .live ∧ isTerminal r.owner = true then
    { r with state := .draining, deadline := now + grace }
  else r

theorem reaper_drains_orphans (isTerminal : Nat → Bool) (now grace : Nat) (r : Resource)
    (hlive : r.state = .live) (hdead : isTerminal r.owner = true) :
    (reap isTerminal now grace r).state = .draining ∧
    (reap isTerminal now grace r).deadline = now + grace := by
  simp [reap, hlive, hdead]

/-! ### P6 — self-healing retries terminate -/

/-- A retry loop: each attempt either succeeds or consumes one unit of
    budget; with budget 0 the loop gives up (fails honestly). -/
def retry (attempt : Nat → Bool) : Nat → Bool
  | 0 => false
  | b + 1 => attempt b || retry attempt b

/-- Termination is structural (Lean accepts `retry` only because budget
    strictly decreases); and a success within budget is found. -/
theorem retry_finds_success (attempt : Nat → Bool) (b k : Nat)
    (hk : k < b) (hs : attempt k = true) : retry attempt b = true := by
  induction b with
  | zero => omega
  | succ b ih =>
    simp only [retry, Bool.or_eq_true]
    by_cases h : k = b
    · subst h; exact Or.inl hs
    · exact Or.inr (ih (by omega))

/-! ## Self-healing with time: circuit breaker and deadline-bounded retry -/

/-! ### P7 — circuit breaker: an open breaker is retried after its cooldown

States: `closed failures`, `open since`, `halfOpen`. `threshold`
consecutive failures open it; after `cooldown` ticks it half-opens and lets
one probe through. -/

inductive Breaker where
  | closed (failures : Nat)
  | opened (since : Nat)
  | halfOpen
  deriving DecidableEq, Repr

/-- One observation at time `now`: `some ok` = a call ran and succeeded or
    failed; `none` = no call (time passes). -/
def breakerStep (threshold cooldown now : Nat) : Breaker → Option Bool → Breaker
  | .closed _,  some true  => .closed 0
  | .closed n,  some false => if n + 1 ≥ threshold then .opened now else .closed (n + 1)
  | .closed n,  none       => .closed n
  | .opened s,  _          => if s + cooldown ≤ now then .halfOpen else .opened s
  | .halfOpen,  some true  => .closed 0
  | .halfOpen,  some false => .opened now
  | .halfOpen,  none       => .halfOpen

/-- Bounded unavailability: an open breaker leaves `opened` at the first
    observation at or after `since + cooldown`, whatever is observed. -/
theorem breaker_reopens_after_cooldown (threshold cooldown since now : Nat)
    (obs : Option Bool) (h : since + cooldown ≤ now) :
    breakerStep threshold cooldown now (.opened since) obs = .halfOpen := by
  simp [breakerStep, h]

/-- While closed, the failure counter never reaches the threshold (it trips
    to `opened` instead) — the breaker cannot silently absorb failures. -/
theorem breaker_closed_counter_below_threshold (threshold cooldown now n m : Nat)
    (hn : n < threshold) (obs : Option Bool)
    (h : breakerStep threshold cooldown now (.closed n) obs = .closed m) :
    m < threshold := by
  cases obs with
  | none =>
    simp [breakerStep] at h; omega
  | some ok =>
    cases ok with
    | true => simp [breakerStep] at h; omega
    | false =>
      simp only [breakerStep] at h
      by_cases ht : n + 1 ≥ threshold
      · simp [ht] at h
      · simp [ht] at h; omega

/-! ### P8 — retry with backoff never overruns the wall-clock deadline

`delay k` is the wait before attempt `k` (any policy: exponential, jittered…).
An attempt starts only if its wait still fits before `deadline`; so the
loop's elapsed time is bounded by the deadline, for every delay policy. -/

/-- Returns the time at which the retry loop stops. -/
def retryUntil (delay : Nat → Nat) (attempt : Nat → Bool) (deadline : Nat) :
    Nat → Nat → Nat
  | 0,      now => now
  | fuel+1, now =>
    if now + delay fuel ≤ deadline then
      if attempt fuel then now + delay fuel
      else retryUntil delay attempt deadline fuel (now + delay fuel)
    else now

theorem retry_respects_deadline (delay : Nat → Nat) (attempt : Nat → Bool) (deadline : Nat) :
    ∀ (fuel now : Nat), now ≤ deadline →
      retryUntil delay attempt deadline fuel now ≤ deadline := by
  intro fuel
  induction fuel with
  | zero => intro now h; simpa [retryUntil] using h
  | succ fuel ih =>
    intro now h
    simp only [retryUntil]
    split
    · split
      · assumption
      · exact ih _ (by assumption)
    · exact h

/-! ## Export: the transition table the Rust implementation must match -/

def TaskState.name : TaskState → String
  | .queued => "queued" | .planning => "planning" | .executing => "executing"
  | .waiting => "waiting" | .verifying => "verifying" | .paused => "paused"
  | .completed => "completed" | .failed => "failed"
  | .interrupted => "interrupted" | .cancelled => "cancelled"

def TaskEvent.name : TaskEvent → String
  | .start => "start" | .planned => "planned" | .needInput => "need_input"
  | .inputArrived => "input_arrived" | .verify => "verify" | .verified => "verified"
  | .reject => "reject" | .pause => "pause" | .resume => "resume"
  | .succeed => "succeed" | .fail => "fail" | .interrupt => "interrupt"
  | .cancel => "cancel" | .timeout => "timeout" | .edit => "edit"

def exportTable : String :=
  let rows := allStates.foldr (fun s acc =>
    allEvents.foldr (fun e acc2 =>
      match step s e with
      | some t => s!"[\"{s.name}\",\"{e.name}\",\"{t.name}\"]" :: acc2
      | none => acc2) acc) []
  "[" ++ ",".intercalate rows ++ "]"

end Selfware

#eval Selfware.exportTable
