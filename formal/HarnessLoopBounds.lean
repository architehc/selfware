/-
  selfware agent loop state & iteration budget — formal model of
  `src/agent/loop_control.rs` (`AgentLoop`).
  Core Lean 4 only (no Mathlib). Check with: `lean HarnessLoopBounds.lean`.

  Every write of `AgentLoop::state` is one event of this model:

  * the checked transitions of `AgentLoop::transition_to` / `set_state`
    (`is_valid_transition`): `startExecution`, `step`, `error`, `recover`,
    `succeed`, `fail`;
  * the direct writes: `capTrip` (`next_state` refusing a slot past the cap),
    `extensionResume` (`resume_after_extension`), `chainResume`
    (`reset_budget_for_resume`, the auto-continue chain — from the budget
    stop in the runner, from any active state as a plain budget reset),
    `restoreProgress`
    (`restore_progress`: the resume path on a freshly built loop, or a
    progress reset of an active loop) and `taskReset`
    (`reset_for_task`).

  `capped` is `AgentState::Failed` whose reason is
  `MAX_ITERATIONS_STOP_REASON`; every other `Failed` is `failed`. The Rust
  side projects its state the same way (`ModelState::of`), runs this table
  as a runtime oracle on every write, and a conformance test compares the
  exported table (`formal/agent_state_table.json`) with it pair by pair.

  What is proved here:
  * L1 terminal sinks: `completed` and `failed` are left only by
    `taskReset` (a new task); no checked transition leaves them.
  * L1b only a budget stop is resumable: `capped` is left only by
    `extensionResume` / `chainResume` (→ `executing`) or `taskReset`; the
    extension resumes nothing else, and neither resumes a real failure
    (`failed`) or a completed run.
  * L2 no active state is stuck: `planning`, `executing` and
    `errorRecovery` each have a checked exit.
  * L3 every state reaches a terminal state through checked transitions
    (`capped` is already stopped: it is terminal for the run unless resumed).
  * L4 adaptive budget: a grant adds `max(original / 4, 1)` and at most 4
    are granted, so a segment's cap is ≤ `2 · original` once
    `original ≥ 4`, and ≤ `original + 4` below that (the grant is at least
    one iteration).
  * L5 chain bound: with at most `1 + MAX_AUTO_CONTINUES` = 4 segments, each
    at most its segment cap, the chain's iterations are ≤ 4 · segment cap.
  * L6 loop termination: `next_state` from `executing` at iteration `i ≤ cap`
    reaches `capped` after exactly `cap - i + 1` calls, never passing `cap`;
    a stopped state (`completed`, `failed`, `capped`) is left unchanged.
-/

namespace HarnessLoop

inductive AgentState where
  | planning
  | executing
  | errorRecovery
  | completed
  | failed
  | capped
  deriving DecidableEq, Repr

inductive AgentEvent where
  -- checked (`transition_to`)
  | startExecution
  | step
  | error
  | recover
  | succeed
  | fail
  -- direct writes
  | capTrip
  | extensionResume
  | chainResume
  | restoreProgress
  | taskReset
  deriving DecidableEq, Repr

def AgentState.isTerminal : AgentState → Bool
  | .completed => true
  | .failed => true
  | _ => false

/-- Stopped: `next_state` leaves the state unchanged. -/
def AgentState.isStopped : AgentState → Bool
  | .completed => true
  | .failed => true
  | .capped => true
  | _ => false

def AgentEvent.isChecked : AgentEvent → Bool
  | .startExecution | .step | .error | .recover | .succeed | .fail => true
  | _ => false

def step : AgentState → AgentEvent → Option AgentState
  -- a new task starts from `planning`, whatever the state
  | _,              .taskReset       => some .planning

  | .planning,      .startExecution  => some .executing
  | .planning,      .error           => some .errorRecovery
  | .planning,      .fail            => some .failed
  | .planning,      .capTrip         => some .capped
  | .planning,      .restoreProgress => some .executing
  | .planning,      .chainResume     => some .executing

  | .executing,     .step            => some .executing
  | .executing,     .error           => some .errorRecovery
  | .executing,     .succeed         => some .completed
  | .executing,     .fail            => some .failed
  | .executing,     .capTrip         => some .capped
  | .executing,     .chainResume     => some .executing
  | .executing,     .restoreProgress => some .executing

  | .errorRecovery, .recover         => some .executing
  | .errorRecovery, .fail            => some .failed
  | .errorRecovery, .capTrip         => some .capped
  | .errorRecovery, .chainResume     => some .executing
  | .errorRecovery, .restoreProgress => some .executing

  | .capped,        .extensionResume => some .executing
  | .capped,        .chainResume     => some .executing

  | _,              _                => none

def allStates : List AgentState :=
  [.planning, .executing, .errorRecovery, .completed, .failed, .capped]

def allEvents : List AgentEvent :=
  [.startExecution, .step, .error, .recover, .succeed, .fail,
   .capTrip, .extensionResume, .chainResume, .restoreProgress, .taskReset]

/-! ## L1 — Terminal sinks -/

theorem completed_left_only_by_task_reset (e : AgentEvent) (t : AgentState)
    (h : step .completed e = some t) : e = .taskReset := by
  cases e <;> simp [step] at h ⊢

theorem failed_left_only_by_task_reset (e : AgentEvent) (t : AgentState)
    (h : step .failed e = some t) : e = .taskReset := by
  cases e <;> simp [step] at h ⊢

theorem terminal_states_have_no_checked_exit (s : AgentState) (e : AgentEvent)
    (hs : s.isTerminal = true) (he : e.isChecked = true) : step s e = none := by
  cases s <;> cases e <;> simp_all [AgentState.isTerminal, AgentEvent.isChecked, step]

/-! ## L1b — Only a budget stop is resumable -/

theorem capped_exits (e : AgentEvent) (t : AgentState) (h : step .capped e = some t) :
    (e = .extensionResume ∧ t = .executing) ∨ (e = .chainResume ∧ t = .executing)
      ∨ (e = .taskReset ∧ t = .planning) := by
  cases e <;> simp [step] at h ⊢ <;> exact h.symm

theorem real_failure_never_resumed :
    step .failed .extensionResume = none ∧ step .failed .chainResume = none := by
  decide

theorem extension_resume_only_from_capped (s t : AgentState)
    (h : step s .extensionResume = some t) : s = .capped := by
  cases s <;> simp [step] at h ⊢

/-- The chain reset re-enters `executing` from an active state or the budget
stop, and a progress restore from an active state — never from a finished
run. -/
theorem chain_resume_never_from_terminal (s : AgentState) (h : s.isTerminal = true) :
    step s .chainResume = none ∧ step s .restoreProgress = none := by
  cases s <;> simp_all [AgentState.isTerminal, step]

/-- A progress restore never resumes a stopped run, not even a budget stop. -/
theorem restore_progress_only_from_active (s t : AgentState)
    (h : step s .restoreProgress = some t) : s.isStopped = false := by
  cases s <;> simp_all [AgentState.isStopped, step]

/-! ## L2 — No active state is stuck -/

theorem active_states_have_checked_exit (s : AgentState)
    (h : s.isStopped = false) : ∃ e t, e.isChecked = true ∧ step s e = some t := by
  cases s with
  | planning => exact ⟨.startExecution, .executing, rfl, rfl⟩
  | executing => exact ⟨.succeed, .completed, rfl, rfl⟩
  | errorRecovery => exact ⟨.recover, .executing, rfl, rfl⟩
  | completed => simp [AgentState.isStopped] at h
  | failed => simp [AgentState.isStopped] at h
  | capped => simp [AgentState.isStopped] at h

/-! ## L3 — Every state reaches a stopped state through checked transitions -/

inductive CanStop : AgentState → Prop where
  | stopped (s : AgentState) (h : s.isStopped = true) : CanStop s
  | step_to (s : AgentState) (e : AgentEvent) (next : AgentState)
      (hc : e.isChecked = true) (hstep : step s e = some next) (hreach : CanStop next) :
      CanStop s

theorem all_states_can_stop (s : AgentState) : CanStop s := by
  have hc : CanStop .completed := .stopped _ rfl
  have hf : CanStop .failed := .stopped _ rfl
  cases s with
  | completed => exact hc
  | failed => exact hf
  | capped => exact .stopped _ rfl
  | executing => exact .step_to _ .succeed _ rfl rfl hc
  | errorRecovery => exact .step_to _ .fail _ rfl rfl hf
  | planning => exact .step_to _ .fail _ rfl rfl hf

/-! ## L4 — Adaptive budget ceiling (`extend_budget_once`) -/

def MAX_GRANTS : Nat := 4

/-- `(original_max / 4).max(1)` -/
def grantSize (originalMax : Nat) : Nat := max (originalMax / 4) 1

def effectiveCap (originalMax grants : Nat) : Nat :=
  originalMax + (min grants MAX_GRANTS) * grantSize originalMax

theorem effective_cap_le_two_original (originalMax grants : Nat) (h4 : 4 ≤ originalMax) :
    effectiveCap originalMax grants ≤ 2 * originalMax := by
  simp only [effectiveCap, MAX_GRANTS, grantSize]
  have hq : 1 ≤ originalMax / 4 := (Nat.le_div_iff_mul_le (by decide)).mpr (by omega)
  have hmax : max (originalMax / 4) 1 = originalMax / 4 := Nat.max_eq_left hq
  rw [hmax]
  have hmin : min grants 4 ≤ 4 := Nat.min_le_right grants 4
  have hdiv : 4 * (originalMax / 4) ≤ originalMax := Nat.mul_div_le originalMax 4
  have hgrant : (min grants 4) * (originalMax / 4) ≤ 4 * (originalMax / 4) :=
    Nat.mul_le_mul_right (originalMax / 4) hmin
  omega

theorem effective_cap_small_original (originalMax grants : Nat) (h4 : originalMax < 4) :
    effectiveCap originalMax grants ≤ originalMax + 4 := by
  simp only [effectiveCap, MAX_GRANTS, grantSize]
  have hq : originalMax / 4 = 0 := Nat.div_eq_of_lt h4
  rw [hq]
  have hmin : min grants 4 ≤ 4 := Nat.min_le_right grants 4
  simp
  omega

/-- The bound the Rust test checks for every original cap. -/
def segmentCeiling (originalMax : Nat) : Nat := max (2 * originalMax) (originalMax + 4)

theorem effective_cap_le_ceiling (originalMax grants : Nat) :
    effectiveCap originalMax grants ≤ segmentCeiling originalMax := by
  simp only [segmentCeiling]
  by_cases h : 4 ≤ originalMax
  · have := effective_cap_le_two_original originalMax grants h
    omega
  · have := effective_cap_small_original originalMax grants (by omega)
    omega

/-! ## L5 — Chain iteration bound -/

def MAX_AUTO_CONTINUES : Nat := 3

def maxSegments : Nat := 1 + MAX_AUTO_CONTINUES

def chainTotal : List Nat → Nat
  | [] => 0
  | x :: xs => x + chainTotal xs

theorem chain_total_le (segs : List Nat) (cap : Nat) (h : ∀ x ∈ segs, x ≤ cap) :
    chainTotal segs ≤ segs.length * cap := by
  induction segs with
  | nil => simp [chainTotal]
  | cons x xs ih =>
    have hx : x ≤ cap := h x (List.mem_cons_self ..)
    have hxs : ∀ y ∈ xs, y ≤ cap := fun y hy => h y (List.mem_cons_of_mem _ hy)
    have := ih hxs
    simp only [chainTotal, List.length_cons, Nat.succ_mul]
    omega

theorem chain_iterations_bounded (segs : List Nat) (originalMax : Nat)
    (hlen : segs.length ≤ maxSegments)
    (hseg : ∀ x ∈ segs, x ≤ segmentCeiling originalMax) :
    chainTotal segs ≤ 4 * segmentCeiling originalMax := by
  have h := chain_total_le segs (segmentCeiling originalMax) hseg
  have hm : maxSegments = 4 := rfl
  have : segs.length * segmentCeiling originalMax ≤ 4 * segmentCeiling originalMax :=
    Nat.mul_le_mul_right _ (by omega)
  omega

/-! ## L6 — `next_state` terminates -/

/-- `AgentLoop::next_state`: `(state, iteration)` after one call. Planning does
not consume a slot; the cap is checked before the slot is taken; a stopped
state is returned unchanged. -/
def slot (s : AgentState) (iter : Nat) : Nat :=
  if s = .planning then iter else iter + 1

def nextState (s : AgentState) (iter cap : Nat) : AgentState × Nat :=
  if s.isStopped then (s, iter)
  else if slot s iter > cap then (.capped, iter) else (s, slot s iter)

theorem next_state_stays_within_cap (s : AgentState) (iter cap : Nat) (h : iter ≤ cap) :
    (nextState s iter cap).2 ≤ cap := by
  unfold nextState
  split
  · exact h
  · split
    · exact h
    · simp only
      omega

theorem next_state_keeps_stopped (s : AgentState) (iter cap : Nat) (h : s.isStopped = true) :
    nextState s iter cap = (s, iter) := by
  simp [nextState, h]

def runNext : Nat → AgentState → Nat → Nat → AgentState × Nat
  | 0, s, iter, _ => (s, iter)
  | k + 1, s, iter, cap =>
    let r := nextState s iter cap
    runNext k r.1 r.2 cap

theorem executing_reaches_capped (d iter cap : Nat) (h : iter + d = cap) :
    (runNext (d + 1) .executing iter cap).1 = .capped := by
  induction d generalizing iter with
  | zero =>
    have hc : iter = cap := by omega
    subst hc
    simp [runNext, nextState, slot, AgentState.isStopped]
  | succ d ih =>
    have hlt : ¬ (iter + 1 > cap) := by omega
    have hstep : nextState .executing iter cap = (.executing, iter + 1) := by
      simp [nextState, slot, AgentState.isStopped, hlt]
    show (runNext (d + 1) (nextState .executing iter cap).1 (nextState .executing iter cap).2 cap).1 = .capped
    rw [hstep]
    exact ih (iter + 1) (by omega)

/-! ## Export transition table for conformance testing -/

def AgentState.name : AgentState → String
  | .planning => "planning"
  | .executing => "executing"
  | .errorRecovery => "error_recovery"
  | .completed => "completed"
  | .failed => "failed"
  | .capped => "capped"

def AgentEvent.name : AgentEvent → String
  | .startExecution => "start_execution"
  | .step => "step"
  | .error => "error"
  | .recover => "recover"
  | .succeed => "succeed"
  | .fail => "fail"
  | .capTrip => "cap_trip"
  | .extensionResume => "extension_resume"
  | .chainResume => "chain_resume"
  | .restoreProgress => "restore_progress"
  | .taskReset => "task_reset"

def exportTable : String :=
  let rows := allStates.foldr (fun s acc =>
    allEvents.foldr (fun e acc2 =>
      match step s e with
      | some t => s!"[\"{s.name}\",\"{e.name}\",\"{t.name}\"]" :: acc2
      | none => acc2) acc) []
  "[" ++ ",".intercalate rows ++ "]"

end HarnessLoop

#eval HarnessLoop.exportTable
