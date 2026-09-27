/-
  selfware workflow (SWL/YAML executor) bounds — formal model, core Lean 4.
  Check with: `lean WorkflowBounds.lean`.

  W1 `until` with a required cap stops within `maxIter` passes.
  W2 clamped exponential backoff: each wait ≤ cap, the total retry wait of a
     step ≤ attempts × cap (no overflow, no unbounded sleep).
  W3 a workflow budget checked between steps overruns by at most one step:
     if every step is bounded by its timeout `T`, the run ends by `budget + T`.
  W4 a resumed run never re-executes a completed step.
-/

namespace Workflow

/-! ## W1 — `until` terminates within its cap -/

/-- Run `body` until `done` holds or `fuel` passes are used; returns the
    number of passes made. -/
def untilLoop (done : Nat → Bool) : Nat → Nat → Nat
  | 0,        passes => passes
  | fuel + 1, passes =>
    if done (passes + 1) then passes + 1 else untilLoop done fuel (passes + 1)

theorem until_bounded (done : Nat → Bool) :
    ∀ maxIter passes, untilLoop done maxIter passes ≤ passes + maxIter := by
  intro maxIter
  induction maxIter with
  | zero => intro passes; simp [untilLoop]
  | succ n ih =>
    intro passes
    simp only [untilLoop]
    split
    · omega
    · have := ih (passes + 1); omega

/-- If the condition first holds on pass `k ≤ maxIter`, the loop stops there. -/
theorem until_stops_at_first_success (done : Nat → Bool) (k : Nat)
    (hk : done k = true) (hfirst : ∀ j, j < k → done j = false) :
    ∀ maxIter passes, passes < k → k ≤ passes + maxIter →
      untilLoop done maxIter passes = k := by
  intro maxIter
  induction maxIter with
  | zero => intro passes h1 h2; omega
  | succ n ih =>
    intro passes h1 h2
    simp only [untilLoop]
    by_cases hk' : passes + 1 = k
    · subst hk'; simp [hk]
    · have : done (passes + 1) = false := hfirst _ (by omega)
      simp [this]
      exact ih (passes + 1) (by omega) (by omega)

/-! ## W2 — clamped backoff -/

/-- `delay · 2^n`, saturating, then clamped to `cap`. `Nat` does not
    overflow, so this models the saturating Rust arithmetic; the clamp is
    what bounds it. -/
def backoff (delay cap n : Nat) : Nat := min (delay * 2 ^ n) cap

theorem backoff_le_cap (delay cap n : Nat) : backoff delay cap n ≤ cap :=
  Nat.min_le_right _ _

def totalWait (delay cap : Nat) : Nat → Nat
  | 0 => 0
  | n + 1 => totalWait delay cap n + backoff delay cap n

theorem total_wait_bounded (delay cap : Nat) :
    ∀ attempts, totalWait delay cap attempts ≤ attempts * cap := by
  intro attempts
  induction attempts with
  | zero => simp [totalWait]
  | succ n ih =>
    simp only [totalWait]
    have := backoff_le_cap delay cap n
    rw [Nat.succ_mul]; omega

/-! ## W3 — budget checked between steps -/

/-- Run steps with durations `dur i` (each ≤ `T`), starting at time `t`,
    checking the budget before each step: a step starts only while
    `t < budget`. Returns the end time. -/
def runSteps (dur : Nat → Nat) (budget : Nat) : Nat → Nat → Nat → Nat
  | 0,     _, t => t
  | n + 1, i, t => if t < budget then runSteps dur budget n (i + 1) (t + dur i) else t

theorem budget_overrun_at_most_one_step (dur : Nat → Nat) (budget T : Nat)
    (hT : ∀ i, dur i ≤ T) :
    ∀ n i t, t ≤ budget + T → runSteps dur budget n i t ≤ budget + T := by
  intro n
  induction n with
  | zero => intro i t h; simpa [runSteps] using h
  | succ n ih =>
    intro i t h
    simp only [runSteps]
    split
    · apply ih
      have := hT i; omega
    · exact h

/-! ## W4 — resume never re-runs a completed step -/

/-- Steps to execute on resume: those not recorded as completed. -/
def toRun (steps : List Nat) (completed : Nat → Bool) : List Nat :=
  steps.filter (fun s => !completed s)

theorem resume_skips_completed (steps : List Nat) (completed : Nat → Bool) :
    ∀ s ∈ toRun steps completed, completed s = false := by
  intro s hs
  simp [toRun, List.mem_filter] at hs
  simpa using hs.2

theorem resume_keeps_pending (steps : List Nat) (completed : Nat → Bool) (s : Nat)
    (hs : s ∈ steps) (hp : completed s = false) : s ∈ toRun steps completed := by
  simp [toRun, List.mem_filter, hs, hp]

end Workflow
