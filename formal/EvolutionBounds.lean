/-
  selfware autonomous evolution & shadow sandbox lifecycle — formal model.
  Core Lean 4 only (no Mathlib). Check with: `lean EvolutionBounds.lean`.

  What is proved here:
  * E1 loop termination: with fuel/retry bounds, the evolution mutation loop
       terminates in finite steps (no infinite compilation/repair loops).
  * E2 deadlock-freedom: every non-terminal state has an exit path to clean teardown;
       no circular trap states exist.
  * E3 clean reachability: every lifecycle state reaches `clean`, by
       promotion or by rollback + teardown. (That a rollback leaves the
       developer's HEAD untouched is a property of the shadow worktree, not
       of this state machine; it is not claimed here.)
  * E4 replicate consensus: `consensusMet results k` holds exactly when at
       least `k` arms improve fitness with 0 test regressions; with `k > 0`
       at least one such arm exists.

  Model only: the Rust runner (`src/evolve/multi_arm_runner.rs`) is not yet
  tied to this model by a conformance test.
-/

namespace Evolution

inductive EvolutionState where
  | idle
  | shadowSpawned
  | mutating
  | compiling
  | autoRepairing
  | evaluatingFitness
  | promoting
  | rollingBack
  | clean
  deriving DecidableEq, Repr

inductive EvolutionEvent where
  | spawnShadow
  | applyMutation
  | compileOk
  | compileFail
  | applyAutoRepair
  | repairExhausted
  | fitnessPassed
  | fitnessFailed
  | consensusMet
  | rollback
  | teardown
  | timeoutOrCancel
  deriving DecidableEq, Repr

/-- Whether the evolution state is terminal. -/
def EvolutionState.terminal : EvolutionState → Bool
  | .clean => true
  | _ => false

/-- State transition function for the evolution lifecycle. -/
def step : EvolutionState → EvolutionEvent → Option EvolutionState
  | .idle,              .spawnShadow       => some .shadowSpawned
  | .shadowSpawned,     .applyMutation     => some .mutating
  | .shadowSpawned,     .rollback          => some .rollingBack
  | .shadowSpawned,     .timeoutOrCancel   => some .rollingBack

  | .mutating,          .compileOk         => some .evaluatingFitness
  | .mutating,          .compileFail       => some .autoRepairing
  | .mutating,          .rollback          => some .rollingBack
  | .mutating,          .timeoutOrCancel   => some .rollingBack

  | .compiling,         .compileOk         => some .evaluatingFitness
  | .compiling,         .compileFail       => some .autoRepairing
  | .compiling,         .rollback          => some .rollingBack
  | .compiling,         .timeoutOrCancel   => some .rollingBack

  | .autoRepairing,     .applyAutoRepair   => some .compiling
  | .autoRepairing,     .repairExhausted   => some .rollingBack
  | .autoRepairing,     .rollback          => some .rollingBack
  | .autoRepairing,     .timeoutOrCancel   => some .rollingBack

  | .evaluatingFitness, .fitnessPassed     => some .promoting
  | .evaluatingFitness, .fitnessFailed     => some .rollingBack
  | .evaluatingFitness, .rollback          => some .rollingBack
  | .evaluatingFitness, .timeoutOrCancel   => some .rollingBack

  | .promoting,         .consensusMet      => some .clean
  | .promoting,         .rollback          => some .rollingBack
  | .promoting,         .timeoutOrCancel   => some .rollingBack

  | .rollingBack,       .teardown          => some .clean
  | .rollingBack,       .timeoutOrCancel   => some .clean

  | .clean,             _                  => none
  | _,                  _                  => none

/-! ## E1 — Bounded Self-Repair Loop Terminates -/

/-- Model of the self-repair loop with max retries `fuel`. Returns the number of repair attempts. -/
def repairLoop (compiles : Nat → Bool) : Nat → Nat → Nat
  | 0,        retries => retries
  | fuel + 1, retries =>
    if compiles (retries + 1) then retries + 1 else repairLoop compiles fuel (retries + 1)

/-- Theorem: Self-repair with maximum fuel `maxRetries` is bounded by `retries + maxRetries`. -/
theorem repair_loop_bounded (compiles : Nat → Bool) :
    ∀ maxRetries retries, repairLoop compiles maxRetries retries ≤ retries + maxRetries := by
  intro maxRetries
  induction maxRetries with
  | zero => intro retries; simp [repairLoop]
  | succ n ih =>
    intro retries
    simp only [repairLoop]
    split
    · omega
    · have := ih (retries + 1); omega

/-! ## E2 — Deadlock-Freedom -/

/-- Theorem: Every evolution state has at least one valid transition path toward `clean`. -/
theorem deadlock_free_exit_to_clean (s : EvolutionState) :
    s = .clean ∨ (∃ (e : EvolutionEvent) (s' : EvolutionState), step s e = some s') := by
  cases s with
  | idle =>
    right; exact ⟨.spawnShadow, .shadowSpawned, rfl⟩
  | shadowSpawned =>
    right; exact ⟨.rollback, .rollingBack, rfl⟩
  | mutating =>
    right; exact ⟨.rollback, .rollingBack, rfl⟩
  | compiling =>
    right; exact ⟨.rollback, .rollingBack, rfl⟩
  | autoRepairing =>
    right; exact ⟨.rollback, .rollingBack, rfl⟩
  | evaluatingFitness =>
    right; exact ⟨.rollback, .rollingBack, rfl⟩
  | promoting =>
    right; exact ⟨.rollback, .rollingBack, rfl⟩
  | rollingBack =>
    right; exact ⟨.teardown, .clean, rfl⟩
  | clean =>
    left; rfl

/-- Theorem: From `rollingBack`, a teardown event guarantees transition to `clean`. -/
theorem rollback_always_cleans :
    step .rollingBack .teardown = some .clean := by
  rfl

/-! ## E3 — Sandbox Isolation -/

/-- A sandbox mutation is isolated: either it reaches `clean` via promotion or via rollback. -/
inductive CanReachClean : EvolutionState → Prop where
  | base : CanReachClean .clean
  | step {s : EvolutionState} {e : EvolutionEvent} {s' : EvolutionState} :
      step s e = some s' → CanReachClean s' → CanReachClean s

theorem all_states_can_reach_clean (s : EvolutionState) : CanReachClean s := by
  cases s with
  | clean => exact CanReachClean.base
  | rollingBack =>
    apply CanReachClean.step (e := .teardown) rfl
    exact CanReachClean.base
  | promoting =>
    apply CanReachClean.step (e := .consensusMet) rfl
    exact CanReachClean.base
  | evaluatingFitness =>
    apply CanReachClean.step (e := .rollback) rfl
    apply CanReachClean.step (e := .teardown) rfl
    exact CanReachClean.base
  | autoRepairing =>
    apply CanReachClean.step (e := .rollback) rfl
    apply CanReachClean.step (e := .teardown) rfl
    exact CanReachClean.base
  | compiling =>
    apply CanReachClean.step (e := .rollback) rfl
    apply CanReachClean.step (e := .teardown) rfl
    exact CanReachClean.base
  | mutating =>
    apply CanReachClean.step (e := .rollback) rfl
    apply CanReachClean.step (e := .teardown) rfl
    exact CanReachClean.base
  | shadowSpawned =>
    apply CanReachClean.step (e := .rollback) rfl
    apply CanReachClean.step (e := .teardown) rfl
    exact CanReachClean.base
  | idle =>
    apply CanReachClean.step (e := .spawnShadow) rfl
    apply CanReachClean.step (e := .rollback) rfl
    apply CanReachClean.step (e := .teardown) rfl
    exact CanReachClean.base

/-! ## E4 — Multi-Arm Replicate Consensus -/

/-- Replicate arm evaluation structure. -/
structure ArmResult where
  fitnessDelta : Int
  testRegressions : Nat
  deriving DecidableEq, Repr

/-- An arm is considered passing if it strictly improves fitness and introduces zero regressions. -/
def ArmResult.isPassing (a : ArmResult) : Bool :=
  a.fitnessDelta > 0 && a.testRegressions == 0

/-- Replicate consensus: count of passing arms must be ≥ required consensus threshold K. -/
def consensusMet (results : List ArmResult) (k : Nat) : Bool :=
  (results.filter ArmResult.isPassing).length ≥ k

theorem consensus_iff_k_passing (results : List ArmResult) (k : Nat) :
    consensusMet results k = true ↔ k ≤ (results.filter ArmResult.isPassing).length := by
  simp [consensusMet]

theorem consensus_requires_passing_arms (results : List ArmResult) (k : Nat) (hk : k > 0) :
    consensusMet results k = true → (results.filter ArmResult.isPassing).length > 0 := by
  intro h
  simp [consensusMet] at h
  omega

end Evolution
