/-
  selfware supervision, reasoning step-down, and multi-agent budget guard —
  formal model. Core Lean 4 only (no Mathlib).
  Check with: `lean SupervisionBounds.lean` (or `./scripts/check_formal.sh`).

  Code modelled:
  * SV1 `CircuitBreaker` (`src/supervision/circuit_breaker.rs`):
    - Three-state machine (`Closed`, `Open`, `HalfOpen`) with atomic counters
      (`failure_count`, `success_count`, `half_open_requests`) and RAII
      `HalfOpenSlotGuard`.
    - Proves that state transitions always zero all counters (`transition_resets_counters`),
      late successes in `Open` are ignored (`open_late_success_noop`),
      non-counted failures / cancelled futures release their `HalfOpen` probe
      slot (`uncounted_failure_preserves_state`, `half_open_no_deadlock_after_uncounted`),
      counted failures in `HalfOpen` immediately reopen the breaker
      (`half_open_counted_failure_reopens`), and `success_threshold` counted
      successes in `HalfOpen` close the breaker (`half_open_closes_at_threshold`).
  * SV2 `RunRegistry` (`src/supervision/run_registry.rs`):
    - Persistent run status transitions (`update_status`), liveness derivation
      (`owner_pid_alive`, `effective_status`), and cross-process abort (`abort`).
    - Proves terminal-state stickiness (`terminal_status_sticky`,
      `terminal_sticky_foldl`, `aborted_never_becomes_completed`), PID reuse
      immunity even when the reused PID belongs to a new `selfware` process
      (`pid_reuse_even_selfware_never_signalled`,
      `signalled_requires_live_matching_selfware`), and honest status preservation
      when signalling fails (`failed_signal_preserves_running`).
  * SV3 `ReasoningStepDown` (`src/api/client.rs`):
    - `lower_reasoning_effort` and `apply_reasoning_step_down` across standard
      models, Qwen models (`xhigh -> medium`), and profiles where
      `reasoning_effort_honored = false`.
    - Proves strict rank decrease on every step-down
      (`lowerEffort_rank_decreases`, `step_down_strictly_decreases_potential`),
      termination within at most 4 steps (`step_down_terminates_in_4`),
      preservation of explicit `thinking` budgets
      (`thinking_budget_never_injects_effort`), and direct thinking disablement
      when `reasoning_effort_honored = false`
      (`unhonored_effort_disables_thinking_immediately`).
  * SV4 `BudgetGuard` (`src/orchestration/multiagent/chat.rs`):
    - Concurrent token & cost reservation tracker (`try_reserve`, `settle`).
    - Proves that `actual_tokens + reserved_tokens` never exceeds `max_tokens`
      on any successful reservation (`try_reserve_respects_token_cap`),
      cancelled/failed calls release their reservation without inflating
      `actual_tokens` (`abandoned_call_releases_reservation`), and unmetered
      provider responses under a cost cap latch `unmetered_cost = true` and
      fail closed on all subsequent reservations (`unmetered_cost_fails_closed`,
      `settle_without_cost_latches_unmetered`, `unmetered_cost_sticky`).

  `exportTable` emits a deterministic 3-column JSON table
  (`formal/supervision_table.json`) verified against the Rust implementations
  by unit conformance tests.
-/

namespace SupervisionBounds

/-! ## Part I — Supervision Circuit Breaker (`src/supervision/circuit_breaker.rs`) -/

inductive CircuitState where
  | closed
  | openState
  | halfOpen
  deriving DecidableEq, Repr

def CircuitState.name : CircuitState → String
  | .closed => "closed"
  | .openState => "open"
  | .halfOpen => "half_open"

structure CBConfig where
  failureThreshold : Nat
  successThreshold : Nat
  halfOpenMaxRequests : Nat
  deriving DecidableEq, Repr

structure CBState where
  state : CircuitState
  failureCount : Nat
  successCount : Nat
  halfOpenRequests : Nat
  deriving DecidableEq, Repr

def freshCB : CBState :=
  { state := .closed, failureCount := 0, successCount := 0, halfOpenRequests := 0 }

/-- `CircuitBreaker::transition_to` resets all three counters before publishing `new_state`. -/
def transitionTo (newState : CircuitState) : CBState :=
  { state := newState, failureCount := 0, successCount := 0, halfOpenRequests := 0 }

/-- `CircuitBreaker::on_success` in `src/supervision/circuit_breaker.rs`. -/
def onSuccess (cfg : CBConfig) (s : CBState) : CBState :=
  match s.state with
  | .halfOpen =>
    let sc := s.successCount + 1
    if sc ≥ cfg.successThreshold then
      transitionTo .closed
    else
      { s with successCount := sc, halfOpenRequests := s.halfOpenRequests - 1 }
  | .closed =>
    { s with successCount := s.successCount + 1, failureCount := 0 }
  | .openState =>
    s

/-- `CircuitBreaker::on_failure_with_reason` in `src/supervision/circuit_breaker.rs`. -/
def onFailure (cfg : CBConfig) (s : CBState) : CBState :=
  let fc := s.failureCount + 1
  if s.state == .halfOpen then
    transitionTo .openState
  else if fc ≥ cfg.failureThreshold then
    transitionTo .openState
  else
    { s with failureCount := fc }

inductive CallOutcome where
  | countedSuccess
  | countedFailure
  | uncountedFailure
  deriving DecidableEq, Repr

def CallOutcome.name : CallOutcome → String
  | .countedSuccess => "counted_success"
  | .countedFailure => "counted_failure"
  | .uncountedFailure => "uncounted_failure"

inductive CallResult where
  | ok
  | failedOp
  | circuitOpen
  deriving DecidableEq, Repr

def CallResult.name : CallResult → String
  | .ok => "ok"
  | .failedOp => "failed_op"
  | .circuitOpen => "circuit_open"

/-- Full `call_with_classifier` transition (admission + `HalfOpenSlotGuard` + completion). -/
def cbCall (cfg : CBConfig) (s : CBState) (resetElapsed : Bool) (out : CallOutcome) :
    CallResult × CBState :=
  let admittedState : Option (CBState × Bool) :=
    match s.state with
    | .openState =>
      if resetElapsed then
        let sHalf := { s with state := .halfOpen }
        if sHalf.halfOpenRequests < cfg.halfOpenMaxRequests then
          some ({ sHalf with halfOpenRequests := sHalf.halfOpenRequests + 1 }, true)
        else
          none
      else
        none
    | .halfOpen =>
      if s.halfOpenRequests < cfg.halfOpenMaxRequests then
        some ({ s with halfOpenRequests := s.halfOpenRequests + 1 }, true)
      else
        none
    | .closed =>
      some (s, false)
  match admittedState with
  | none =>
    let sRejected :=
      if s.state == .openState && resetElapsed then { s with state := .halfOpen } else s
    (.circuitOpen, sRejected)
  | some (sMid, acquiredSlot) =>
    match out with
    | .countedSuccess => (.ok, onSuccess cfg sMid)
    | .countedFailure => (.failedOp, onFailure cfg sMid)
    | .uncountedFailure =>
      -- `HalfOpenSlotGuard::drop` decrements `half_open_requests` when `acquiredSlot = true`.
      let sDropped :=
        if acquiredSlot then
          { sMid with halfOpenRequests := sMid.halfOpenRequests - 1 }
        else
          sMid
      (.failedOp, sDropped)

/-! ### SV1 Theorems -/

theorem transition_resets_counters (st : CircuitState) :
    (transitionTo st).failureCount = 0 ∧
    (transitionTo st).successCount = 0 ∧
    (transitionTo st).halfOpenRequests = 0 := ⟨rfl, rfl, rfl⟩

theorem open_late_success_noop (cfg : CBConfig) (fc sc ho : Nat) :
    onSuccess cfg { state := .openState, failureCount := fc, successCount := sc, halfOpenRequests := ho } =
      { state := .openState, failureCount := fc, successCount := sc, halfOpenRequests := ho } := rfl

theorem closed_success_resets_failures (cfg : CBConfig) (fc sc ho : Nat) :
    (onSuccess cfg { state := .closed, failureCount := fc, successCount := sc, halfOpenRequests := ho }).failureCount = 0 := rfl

theorem uncounted_failure_preserves_half_open (cfg : CBConfig) (fc sc ho : Nat)
    (radm : ho < cfg.halfOpenMaxRequests) (resetElapsed : Bool) :
    let s : CBState := { state := .halfOpen, failureCount := fc, successCount := sc, halfOpenRequests := ho }
    cbCall cfg s resetElapsed .uncountedFailure = (.failedOp, s) := by
  simp [cbCall, radm]

theorem uncounted_failure_preserves_closed (cfg : CBConfig) (fc sc ho : Nat) (resetElapsed : Bool) :
    let s : CBState := { state := .closed, failureCount := fc, successCount := sc, halfOpenRequests := ho }
    cbCall cfg s resetElapsed .uncountedFailure = (.failedOp, s) := rfl

/-- Iterating `k` uncounted failures in `HalfOpen` never consumes a permanent probe slot. -/
def runUncountedN (cfg : CBConfig) : Nat → CBState → CBState
  | 0, s => s
  | n + 1, s => (cbCall cfg (runUncountedN cfg n s) false .uncountedFailure).2

theorem half_open_no_deadlock_after_uncounted (cfg : CBConfig) (fc sc ho k : Nat)
    (radm : ho < cfg.halfOpenMaxRequests) :
    let s : CBState := { state := .halfOpen, failureCount := fc, successCount := sc, halfOpenRequests := ho }
    runUncountedN cfg k s = s := by
  intro s
  induction k with
  | zero => rfl
  | succ n ih =>
    simp [runUncountedN, ih, cbCall, s, radm]

theorem half_open_counted_failure_reopens (cfg : CBConfig) (fc sc ho : Nat)
    (radm : ho < cfg.halfOpenMaxRequests) (resetElapsed : Bool) :
    let s : CBState := { state := .halfOpen, failureCount := fc, successCount := sc, halfOpenRequests := ho }
    cbCall cfg s resetElapsed .countedFailure = (.failedOp, transitionTo .openState) := by
  simp [cbCall, radm, onFailure, transitionTo]

theorem half_open_closes_at_threshold (cfg : CBConfig) (fc sc ho : Nat)
    (radm : ho < cfg.halfOpenMaxRequests)
    (hthr : sc + 1 ≥ cfg.successThreshold) (resetElapsed : Bool) :
    let s : CBState := { state := .halfOpen, failureCount := fc, successCount := sc, halfOpenRequests := ho }
    cbCall cfg s resetElapsed .countedSuccess = (.ok, transitionTo .closed) := by
  simp [cbCall, radm, onSuccess, hthr, transitionTo]

/-! ## Part II — RunRegistry Sticky Terminal States & PID-Reuse Safety (`src/supervision/run_registry.rs`) -/

inductive RunStatus where
  | running
  | completed
  | failed
  | aborted
  deriving DecidableEq, Repr

def RunStatus.name : RunStatus → String
  | .running => "Running"
  | .completed => "Completed"
  | .failed => "Failed"
  | .aborted => "Aborted"

def allRunStatuses : List RunStatus :=
  [.running, .completed, .failed, .aborted]

def isTerminalStatus : RunStatus → Bool
  | .running => false
  | .completed | .failed | .aborted => true

/-- `RunRegistry::update_status` in `src/supervision/run_registry.rs`. -/
def updateStatus (current next : RunStatus) : RunStatus :=
  if isTerminalStatus current then current else next

/-- `owner_pid_alive` in `src/supervision/run_registry.rs`. -/
def ownerPidAlive (actualStart expectedStart : Option Nat) (isSelfware : Bool) : Bool :=
  match actualStart with
  | none => false
  | some act =>
    match expectedStart with
    | some exp => decide (act = exp) && isSelfware
    | none => isSelfware

inductive EffectiveStatus where
  | running
  | stale
  | completed
  | failed
  | aborted
  deriving DecidableEq, Repr

def EffectiveStatus.name : EffectiveStatus → String
  | .running => "Running"
  | .stale => "Stale"
  | .completed => "Completed"
  | .failed => "Failed"
  | .aborted => "Aborted"

/-- `RunRecord::effective_status` in `src/supervision/run_registry.rs`. -/
def effectiveStatus (status : RunStatus) (actualStart expectedStart : Option Nat) (isSelfware : Bool) :
    EffectiveStatus :=
  match status with
  | .running =>
    if ownerPidAlive actualStart expectedStart isSelfware then .running else .stale
  | .completed => .completed
  | .failed => .failed
  | .aborted => .aborted

inductive SignalResult where
  | ok
  | esrch
  | eperm
  | unsupported
  deriving DecidableEq, Repr

def SignalResult.name : SignalResult → String
  | .ok => "sig_ok"
  | .esrch => "sig_esrch"
  | .eperm => "sig_eperm"
  | .unsupported => "sig_unsupported"

inductive AbortDecision where
  | alreadyDone
  | wasStale
  | signalled
  | signalFailed
  | signalUnsupported
  deriving DecidableEq, Repr

def AbortDecision.name : AbortDecision → String
  | .alreadyDone => "already_done"
  | .wasStale => "was_stale"
  | .signalled => "signalled"
  | .signalFailed => "signal_failed"
  | .signalUnsupported => "signal_unsupported"

/-- `RunRegistry::abort` in `src/supervision/run_registry.rs`. -/
def abortRun (status : RunStatus) (actualStart expectedStart : Option Nat)
    (isSelfware : Bool) (sig : SignalResult) : AbortDecision × RunStatus :=
  if isTerminalStatus status then
    (.alreadyDone, status)
  else if ownerPidAlive actualStart expectedStart isSelfware then
    match sig with
    | .ok => (.signalled, .aborted)
    | .esrch => (.wasStale, .aborted)
    | .eperm => (.signalFailed, status)
    | .unsupported => (.signalUnsupported, status)
  else
    (.wasStale, .aborted)

/-! ### SV2 Theorems -/

theorem terminal_status_sticky (s next : RunStatus) (h : isTerminalStatus s = true) :
    updateStatus s next = s := by
  simp [updateStatus, h]

theorem terminal_sticky_foldl (s : RunStatus) (updates : List RunStatus)
    (h : isTerminalStatus s = true) :
    updates.foldl updateStatus s = s := by
  induction updates with
  | nil => rfl
  | cons u rest ih =>
    simp [List.foldl, terminal_status_sticky s u h, ih]

theorem aborted_never_becomes_completed (updates : List RunStatus) :
    updates.foldl updateStatus .aborted = .aborted :=
  terminal_sticky_foldl .aborted updates rfl

theorem pid_reuse_even_selfware_never_signalled
    (status : RunStatus) (act exp : Nat) (isSelfware : Bool) (sig : SignalResult)
    (hmismatch : act ≠ exp) :
    ownerPidAlive (some act) (some exp) isSelfware = false ∧
    (abortRun status (some act) (some exp) isSelfware sig).1 ≠ .signalled := by
  have hown : ownerPidAlive (some act) (some exp) isSelfware = false := by
    simp [ownerPidAlive, hmismatch]
  refine ⟨hown, ?_⟩
  cases status <;> simp [abortRun, isTerminalStatus, hown]

theorem signalled_requires_live_matching_selfware
    (status : RunStatus) (actualStart : Option Nat) (exp : Nat)
    (isSelfware : Bool) (sig : SignalResult)
    (hsig : (abortRun status actualStart (some exp) isSelfware sig).1 = .signalled) :
    status = .running ∧ actualStart = some exp ∧ isSelfware = true ∧ sig = .ok := by
  unfold abortRun at hsig
  cases status <;> simp [isTerminalStatus] at hsig
  cases actualStart with
  | none => simp [ownerPidAlive] at hsig
  | some act =>
    by_cases hmatch : act = exp
    · subst hmatch
      cases isSelfware <;> simp [ownerPidAlive] at hsig
      cases sig <;> simp at hsig
      exact ⟨rfl, rfl, rfl, rfl⟩
    · simp [ownerPidAlive, hmatch] at hsig

theorem failed_signal_preserves_running
    (actualStart expectedStart : Option Nat) (isSelfware : Bool) :
    (abortRun .running actualStart expectedStart isSelfware .eperm).2 ≠ .running →
      ownerPidAlive actualStart expectedStart isSelfware = false := by
  intro h
  cases hown : ownerPidAlive actualStart expectedStart isSelfware
  · rfl
  · simp [abortRun, isTerminalStatus, hown] at h

/-! ## Part III — Reasoning Step-Down (`src/api/client.rs`) -/

inductive Effort where
  | none
  | low
  | medium
  | high
  | xhigh
  deriving DecidableEq, Repr

def Effort.rank : Effort → Nat
  | .none => 0
  | .low => 1
  | .medium => 2
  | .high => 3
  | .xhigh => 4

def Effort.name : Effort → String
  | .none => "none"
  | .low => "low"
  | .medium => "medium"
  | .high => "high"
  | .xhigh => "xhigh"

/-- `lower_reasoning_effort` in `src/api/client.rs`. -/
def lowerEffort (isQwen : Bool) : Effort → Option Effort
  | .xhigh => some (if isQwen then .medium else .high)
  | .high => some .medium
  | .medium => some .low
  | .low | .none => none

theorem lowerEffort_rank_decreases (isQwen : Bool) (e e' : Effort)
    (h : lowerEffort isQwen e = some e') :
    e'.rank < e.rank ∧ 1 ≤ e'.rank := by
  cases e <;> cases isQwen <;> simp [lowerEffort] at h <;> subst h <;> decide

structure RequestReasoning where
  topEffort : Effort
  kwargsPresent : Bool
  kwargsThinking : Option Bool
  kwargsEffort : Effort
  openrouterEffort : Effort
  hasThinkingBudget : Bool
  deriving DecidableEq, Repr

/-- Whether any effort pin can be lowered one step. -/
def canLowerAnyPin (isQwen : Bool) (r : RequestReasoning) : Bool :=
  (lowerEffort isQwen r.topEffort).isSome ||
  (r.kwargsPresent && (lowerEffort isQwen r.kwargsEffort).isSome) ||
  (lowerEffort isQwen r.openrouterEffort).isSome

def lowerPinOrKeep (isQwen : Bool) (e : Effort) : Effort :=
  match lowerEffort isQwen e with
  | some e' => e'
  | none => e

def hasAnyPin (r : RequestReasoning) : Bool :=
  (r.topEffort != .none) ||
  (r.kwargsPresent && (r.kwargsThinking.isSome || r.kwargsEffort != .none)) ||
  (r.openrouterEffort != .none)

/-- `apply_reasoning_step_down` in `src/api/client.rs`. -/
def applyReasoningStepDown (isQwen effortHonored : Bool) (r : RequestReasoning) :
    Option RequestReasoning :=
  if !effortHonored && r.kwargsPresent then
    if r.kwargsThinking != some false then
      some { r with kwargsThinking := some false }
    else
      none
  else if canLowerAnyPin isQwen r then
    some { r with
      topEffort := lowerPinOrKeep isQwen r.topEffort
      kwargsEffort := if r.kwargsPresent then lowerPinOrKeep isQwen r.kwargsEffort else r.kwargsEffort
      openrouterEffort := lowerPinOrKeep isQwen r.openrouterEffort }
  else if r.kwargsPresent then
    if r.kwargsThinking != some false then
      some { r with kwargsThinking := some false }
    else
      none
  else if !hasAnyPin r && !r.hasThinkingBudget then
    some { r with topEffort := .low }
  else
    none

/-- Well-founded potential function for `RequestReasoning`:
- Each lowerable pin contributes `e.rank - 1` (0 when `.none` or `.low`, up to 3 when `.xhigh`).
- Enabled/unset `kwargsThinking` when `kwargsPresent = true` contributes 1.
- Completely unpinned request without `hasThinkingBudget` contributes 1 (can step once to `.low`). -/
def pinStepsLeft (e : Effort) : Nat :=
  e.rank - 1

def reasoningPotential (r : RequestReasoning) : Nat :=
  let pinPot :=
    pinStepsLeft r.topEffort +
    (if r.kwargsPresent then pinStepsLeft r.kwargsEffort else 0) +
    pinStepsLeft r.openrouterEffort
  let kwPot :=
    if r.kwargsPresent && r.kwargsThinking != some false then 1 else 0
  let unpinnedPot :=
    if !r.kwargsPresent && !hasAnyPin r && !r.hasThinkingBudget then 1 else 0
  pinPot + kwPot + unpinnedPot

/-! ### SV3 Theorems -/

theorem lowerPinOrKeep_le (isQwen : Bool) (e : Effort) :
    pinStepsLeft (lowerPinOrKeep isQwen e) ≤ pinStepsLeft e := by
  cases e <;> cases isQwen <;> decide

theorem lowerPinOrKeep_lt_of_isSome (isQwen : Bool) (e : Effort)
    (h : (lowerEffort isQwen e).isSome = true) :
    pinStepsLeft (lowerPinOrKeep isQwen e) < pinStepsLeft e := by
  cases e <;> cases isQwen <;> simp [lowerEffort] at h <;> decide

def stepDownDecreasesBool (isQwen effortHonored : Bool) (r : RequestReasoning) : Bool :=
  match applyReasoningStepDown isQwen effortHonored r with
  | none => true
  | some r' => decide (reasoningPotential r' < reasoningPotential r)

theorem step_down_strictly_decreases_potential_aux (isQwen effortHonored : Bool)
    (r : RequestReasoning) :
    stepDownDecreasesBool isQwen effortHonored r = true := by
  rcases r with ⟨topE, kwP, _ | (_ | _), kwE, orE, hasTB⟩ <;>
    cases isQwen <;> cases effortHonored <;> cases topE <;> cases kwP <;>
    cases kwE <;> cases orE <;> cases hasTB <;> rfl

theorem step_down_strictly_decreases_potential (isQwen effortHonored : Bool)
    (r r' : RequestReasoning)
    (h : applyReasoningStepDown isQwen effortHonored r = some r') :
    reasoningPotential r' < reasoningPotential r := by
  have hpot := step_down_strictly_decreases_potential_aux isQwen effortHonored r
  unfold stepDownDecreasesBool at hpot
  rw [h] at hpot
  exact of_decide_eq_true hpot

/-- Run `applyReasoningStepDown` up to `k` times. -/
def iterateStepDown (isQwen effortHonored : Bool) : Nat → RequestReasoning → RequestReasoning
  | 0, r => r
  | n + 1, r =>
    match applyReasoningStepDown isQwen effortHonored r with
    | none => r
    | some r' => iterateStepDown isQwen effortHonored n r'

/-- Every `RequestReasoning` reaches a fixed point (`applyReasoningStepDown = none`)
within at most 4 steps (3 effort step-downs `xhigh -> high -> medium -> low` + 1 thinking toggle). -/
theorem step_down_terminates_in_4 (isQwen effortHonored : Bool) (r : RequestReasoning) :
    applyReasoningStepDown isQwen effortHonored (iterateStepDown isQwen effortHonored 4 r) = none := by
  rcases r with ⟨topE, kwP, _ | (_ | _), kwE, orE, hasTB⟩ <;>
    cases isQwen <;> cases effortHonored <;> cases topE <;> cases kwP <;>
    cases kwE <;> cases orE <;> cases hasTB <;> rfl

theorem thinking_budget_never_injects_effort (isQwen effortHonored : Bool) :
    let r : RequestReasoning := {
      topEffort := .none
      kwargsPresent := false
      kwargsThinking := none
      kwargsEffort := .none
      openrouterEffort := .none
      hasThinkingBudget := true
    }
    applyReasoningStepDown isQwen effortHonored r = none := by
  cases isQwen <;> cases effortHonored <;> rfl

theorem unhonored_effort_disables_thinking_immediately (isQwen : Bool) (r : RequestReasoning)
    (hkw : r.kwargsPresent = true) (ht : r.kwargsThinking ≠ some false) :
    applyReasoningStepDown isQwen false r = some { r with kwargsThinking := some false } := by
  have hne : (r.kwargsThinking != some false) = true := by
    cases h : r.kwargsThinking with
    | none => rfl
    | some b =>
      cases b
      · exfalso; exact ht h
      · rfl
  simp [applyReasoningStepDown, hkw, hne]

/-! ## Part IV — Multi-Agent BudgetGuard (`src/orchestration/multiagent/chat.rs`) -/

structure BudgetLimits where
  maxBudgetTokens : Option Nat
  maxCostMicros : Option Nat
  estimate : Nat
  deriving DecidableEq, Repr

structure BudgetTracker where
  actualTokens : Nat
  reservedTokens : Nat
  actualCostMicros : Nat
  unmeteredCost : Bool
  deriving DecidableEq, Repr

def freshTracker : BudgetTracker :=
  { actualTokens := 0, reservedTokens := 0, actualCostMicros := 0, unmeteredCost := false }

/-- `BudgetGuard::try_reserve` in `src/orchestration/multiagent/chat.rs`. -/
def tryReserve (lim : BudgetLimits) (t : BudgetTracker) (promptTokens : Nat) :
    Option (Nat × BudgetTracker) :=
  let callEstimate := lim.estimate + promptTokens
  let tokenOk :=
    match lim.maxBudgetTokens with
    | none => true
    | some maxT => decide (t.actualTokens + t.reservedTokens + callEstimate ≤ maxT)
  let costOk :=
    match lim.maxCostMicros with
    | none => true
    | some maxC => !t.unmeteredCost && decide (t.actualCostMicros < maxC)
  if tokenOk && costOk then
    some (callEstimate, { t with reservedTokens := t.reservedTokens + callEstimate })
  else
    none

/-- `BudgetGuard::settle` in `src/orchestration/multiagent/chat.rs`. -/
def settle (lim : BudgetLimits) (t : BudgetTracker) (reserved : Nat)
    (usageTotalTokens : Option Nat) (usageCostMicros : Option Nat) (fallbackTokens : Nat) :
    BudgetTracker :=
  let r' := t.reservedTokens - reserved
  match usageTotalTokens with
  | none => { t with reservedTokens := r' }
  | some totalTok =>
    let tokAdd := if totalTok > 0 then totalTok else fallbackTokens
    let (cost', unmetered') :=
      match usageCostMicros with
      | some c => (t.actualCostMicros + c, t.unmeteredCost)
      | none => (t.actualCostMicros, t.unmeteredCost || lim.maxCostMicros.isSome)
    { actualTokens := t.actualTokens + tokAdd
      reservedTokens := r'
      actualCostMicros := cost'
      unmeteredCost := unmetered' }

/-! ### SV4 Theorems -/

theorem try_reserve_respects_token_cap (lim : BudgetLimits) (t t' : BudgetTracker)
    (promptTokens reserved maxT : Nat)
    (hlim : lim.maxBudgetTokens = some maxT)
    (hres : tryReserve lim t promptTokens = some (reserved, t')) :
    t'.actualTokens + t'.reservedTokens ≤ maxT ∧
    reserved = lim.estimate + promptTokens ∧
    t'.reservedTokens = t.reservedTokens + reserved := by
  unfold tryReserve at hres
  rw [hlim] at hres
  dsimp at hres
  by_cases htok : t.actualTokens + t.reservedTokens + (lim.estimate + promptTokens) ≤ maxT
  · simp [htok] at hres
    rcases hres with ⟨_, rfl, rfl⟩
    dsimp
    refine ⟨by omega, rfl, rfl⟩
  · simp [htok] at hres

theorem unmetered_cost_fails_closed (lim : BudgetLimits) (t : BudgetTracker)
    (promptTokens maxC : Nat)
    (hcost : lim.maxCostMicros = some maxC)
    (hunm : t.unmeteredCost = true) :
    tryReserve lim t promptTokens = none := by
  unfold tryReserve
  rw [hcost, hunm]
  simp

theorem settle_without_cost_latches_unmetered (lim : BudgetLimits) (t : BudgetTracker)
    (reserved totalTok fallbackTokens maxC : Nat)
    (hcost : lim.maxCostMicros = some maxC) :
    (settle lim t reserved (some totalTok) none fallbackTokens).unmeteredCost = true := by
  simp [settle, hcost]

theorem unmetered_cost_sticky_on_settle (lim : BudgetLimits) (t : BudgetTracker)
    (reserved : Nat) (usageTok usageCost : Option Nat) (fallbackTokens : Nat)
    (hunm : t.unmeteredCost = true) :
    (settle lim t reserved usageTok usageCost fallbackTokens).unmeteredCost = true := by
  unfold settle
  cases usageTok with
  | none => exact hunm
  | some tok =>
    cases usageCost <;> simp [hunm]

theorem abandoned_call_releases_reservation (lim : BudgetLimits) (t : BudgetTracker)
    (reserved fallbackTokens : Nat) :
    settle lim t reserved none none fallbackTokens =
      { t with reservedTokens := t.reservedTokens - reserved } := rfl

/-! ## Part V: RunEnd & Exit Code Classification (`src/errors.rs`) -/

inductive ShutdownReason where
  | userInterrupt
  | signalTerminate
  | timeout
  deriving DecidableEq, Repr

def ShutdownReason.optName : Option ShutdownReason → String
  | none => "none"
  | some .userInterrupt => "user_interrupt"
  | some .signalTerminate => "signal_terminate"
  | some .timeout => "timeout"

inductive ErrorCause where
  | cancelled
  | cancelledWithReason
  | terminated
  | selfwareInterrupted
  | shutdownApi
  | configErr
  | apiErr
  | safetyErr
  | confirmationErr
  | otherErr
  deriving DecidableEq, Repr

def ErrorCause.name : ErrorCause → String
  | .cancelled => "cancelled"
  | .cancelledWithReason => "cancelled_with_reason"
  | .terminated => "terminated"
  | .selfwareInterrupted => "selfware_interrupted"
  | .shutdownApi => "shutdown_api"
  | .configErr => "config"
  | .apiErr => "api"
  | .safetyErr => "safety"
  | .confirmationErr => "confirmation"
  | .otherErr => "other"

inductive RunOutcomeInput where
  | ok
  | err (cause : ErrorCause)
  deriving DecidableEq, Repr

def RunOutcomeInput.name : RunOutcomeInput → String
  | .ok => "ok"
  | .err c => s!"err:{c.name}"

inductive RunEnd where
  | completed
  | failed
  | interrupted
  | terminated
  deriving DecidableEq, Repr

def RunEnd.name : RunEnd → String
  | .completed => "completed"
  | .failed => "failed"
  | .interrupted => "interrupted"
  | .terminated => "terminated"

def classifyError (cause : ErrorCause) (shutdown : Option ShutdownReason) : RunEnd :=
  match cause with
  | .cancelled => .interrupted
  | .terminated => .terminated
  | .selfwareInterrupted => .interrupted
  | .shutdownApi =>
    match shutdown with
    | some .signalTerminate => .terminated
    | some .timeout => .failed
    | _ => .interrupted
  | _ => .failed

def classifyRunEnd (input : RunOutcomeInput) (shutdown : Option ShutdownReason) : RunEnd :=
  match input with
  | .ok =>
    match shutdown with
    | none => .completed
    | some .userInterrupt => .interrupted
    | some .signalTerminate => .terminated
    | some .timeout => .failed
  | .err cause => classifyError cause shutdown

def getExitCode (cause : ErrorCause) (shutdown : Option ShutdownReason) : Nat :=
  match cause with
  | .confirmationErr => 6
  | .shutdownApi =>
    match shutdown with
    | some .signalTerminate => 143
    | _ => 130
  | .configErr => 2
  | .apiErr => 4
  | .safetyErr => 5
  | .selfwareInterrupted => 130
  | .terminated => 143
  | .cancelled | .cancelledWithReason => 130
  | .otherErr => 1

def processExitCode (input : RunOutcomeInput) (shutdown : Option ShutdownReason) : Nat :=
  match input with
  | .err cause => getExitCode cause shutdown
  | .ok =>
    match shutdown with
    | some .signalTerminate => 143
    | some .userInterrupt | some .timeout => 130
    | none => 0

theorem completed_iff_exit_zero (input : RunOutcomeInput) (shutdown : Option ShutdownReason) :
    classifyRunEnd input shutdown = .completed ↔ processExitCode input shutdown = 0 := by
  cases input with
  | ok =>
    cases shutdown with
    | none => simp [classifyRunEnd, processExitCode]
    | some r => cases r <;> simp [classifyRunEnd, processExitCode]
  | err cause =>
    cases cause <;> simp [classifyRunEnd, classifyError, processExitCode, getExitCode]
    cases shutdown with
    | none => simp
    | some r => cases r <;> simp

theorem terminated_iff_exit_143 (input : RunOutcomeInput) (shutdown : Option ShutdownReason) :
    classifyRunEnd input shutdown = .terminated ↔ processExitCode input shutdown = 143 := by
  cases input with
  | ok =>
    cases shutdown with
    | none => simp [classifyRunEnd, processExitCode]
    | some r => cases r <;> simp [classifyRunEnd, processExitCode]
  | err cause =>
    cases cause <;> simp [classifyRunEnd, classifyError, processExitCode, getExitCode]
    cases shutdown with
    | none => simp
    | some r => cases r <;> simp

theorem interrupted_implies_exit_130 (input : RunOutcomeInput) (shutdown : Option ShutdownReason)
    (h : classifyRunEnd input shutdown = .interrupted) :
    processExitCode input shutdown = 130 := by
  cases input with
  | ok =>
    cases shutdown with
    | none => simp [classifyRunEnd] at h
    | some r => cases r <;> simp [classifyRunEnd, processExitCode] at h ⊢
  | err cause =>
    cases cause <;> simp [classifyRunEnd, classifyError, processExitCode, getExitCode] at h ⊢
    cases shutdown with
    | none => simp at h ⊢
    | some r => cases r <;> simp at h ⊢

theorem timeout_always_fails_run_end (shutdown : Option ShutdownReason) :
    classifyRunEnd .ok (some .timeout) = .failed ∧
    classifyRunEnd (.err .cancelledWithReason) shutdown = .failed ∧
    classifyRunEnd (.err .shutdownApi) (some .timeout) = .failed := by
  simp [classifyRunEnd, classifyError]

theorem error_always_nonzero_exit (cause : ErrorCause) (shutdown : Option ShutdownReason) :
    processExitCode (.err cause) shutdown > 0 := by
  cases cause <;> simp [processExitCode, getExitCode]
  cases shutdown with
  | none => decide
  | some r => cases r <;> decide

/-! ## Export deterministic 3-column table for `scripts/check_formal.sh` & Rust conformance tests -/

def exportTable : String :=
  let cbCfg : CBConfig := { failureThreshold := 2, successThreshold := 2, halfOpenMaxRequests := 1 }
  let cbStates : List (String × CBState × Bool) :=
    [ ("closed_fresh", freshCB, false)
    , ("closed_one_fail", { freshCB with failureCount := 1 }, false)
    , ("open_waiting", { freshCB with state := .openState }, false)
    , ("open_reset_ready", { freshCB with state := .openState }, true)
    , ("half_open_probe_0", { freshCB with state := .halfOpen, successCount := 0, halfOpenRequests := 0 }, false)
    , ("half_open_probe_1", { freshCB with state := .halfOpen, successCount := 1, halfOpenRequests := 0 }, false)
    , ("half_open_saturated", { freshCB with state := .halfOpen, successCount := 0, halfOpenRequests := 1 }, false)
    ]
  let cbOutcomes : List CallOutcome := [.countedSuccess, .countedFailure, .uncountedFailure]
  let cbRows := cbStates.foldr (fun (label, st, resetReady) acc =>
    cbOutcomes.foldr (fun out acc2 =>
      let (res, st') := cbCall cbCfg st resetReady out
      let summary := s!"{res.name}:{st'.state.name}:f{st'.failureCount}:s{st'.successCount}:h{st'.halfOpenRequests}"
      s!"[\"cb:{label}\",\"{out.name}\",\"{summary}\"]" :: acc2) acc) []

  let runUpdateRows := allRunStatuses.foldr (fun cur acc =>
    allRunStatuses.foldr (fun nxt acc2 =>
      let res := updateStatus cur nxt
      s!"[\"run_update:{cur.name}\",\"{nxt.name}\",\"{res.name}\"]" :: acc2) acc) []

  let abortScenarios : List (String × RunStatus × Option Nat × Option Nat × Bool) :=
    [ ("running_live_owner", .running, some 1000, some 1000, true)
    , ("running_pid_reused_other_selfware", .running, some 2000, some 1000, true)
    , ("running_pid_reused_non_selfware", .running, some 1000, some 1000, false)
    , ("running_pid_dead", .running, none, some 1000, false)
    , ("completed_already", .completed, some 1000, some 1000, true)
    , ("aborted_already", .aborted, some 1000, some 1000, true)
    ]
  let sigResults : List SignalResult := [.ok, .esrch, .eperm]
  let abortRows := abortScenarios.foldr (fun (label, st, act, exp, isSw) acc =>
    sigResults.foldr (fun sig acc2 =>
      let (dec, st') := abortRun st act exp isSw sig
      s!"[\"run_abort:{label}\",\"{sig.name}\",\"{dec.name}:{st'.name}\"]" :: acc2) acc) []

  let stepDownProfiles : List (String × Bool × Bool) :=
    [ ("standard_honored", false, true)
    , ("qwen_honored", true, true)
    , ("qwen_unhonored", true, false)
    ]
  let stepDownInputs : List (String × RequestReasoning) :=
    [ ("top_xhigh", { topEffort := .xhigh, kwargsPresent := false, kwargsThinking := none, kwargsEffort := .none, openrouterEffort := .none, hasThinkingBudget := false })
    , ("kwargs_xhigh_thinking", { topEffort := .none, kwargsPresent := true, kwargsThinking := some true, kwargsEffort := .xhigh, openrouterEffort := .none, hasThinkingBudget := false })
    , ("kwargs_low_thinking", { topEffort := .none, kwargsPresent := true, kwargsThinking := some true, kwargsEffort := .low, openrouterEffort := .none, hasThinkingBudget := false })
    , ("unpinned_default", { topEffort := .none, kwargsPresent := false, kwargsThinking := none, kwargsEffort := .none, openrouterEffort := .none, hasThinkingBudget := false })
    , ("thinking_budget_only", { topEffort := .none, kwargsPresent := false, kwargsThinking := none, kwargsEffort := .none, openrouterEffort := .none, hasThinkingBudget := true })
    ]
  let stepDownRows := stepDownProfiles.foldr (fun (profLabel, isQwen, honored) acc =>
    stepDownInputs.foldr (fun (inLabel, req) acc2 =>
      let outStr := match applyReasoningStepDown isQwen honored req with
        | none => "none"
        | some r' =>
          let kwT := match r'.kwargsThinking with
            | none => "none"
            | some true => "true"
            | some false => "false"
          s!"top={r'.topEffort.name},kwE={r'.kwargsEffort.name},kwT={kwT}"
      s!"[\"step_down:{profLabel}\",\"{inLabel}\",\"{outStr}\"]" :: acc2) acc) []

  let budgetLim : BudgetLimits := { maxBudgetTokens := some 1000, maxCostMicros := some 500, estimate := 200 }
  let budgetReserveStates : List (String × BudgetTracker × Nat) :=
    [ ("fresh_ok", freshTracker, 100)
    , ("token_cap_exceeded", { freshTracker with actualTokens := 500, reservedTokens := 300 }, 50)
    , ("cost_cap_reached", { freshTracker with actualCostMicros := 500 }, 50)
    , ("unmetered_latched", { freshTracker with unmeteredCost := true }, 50)
    ]
  let budgetRows := budgetReserveStates.foldr (fun (label, trk, promptTok) acc =>
    let outStr := match tryReserve budgetLim trk promptTok with
      | none => "rejected"
      | some (res, trk') => s!"reserved={res}:r={trk'.reservedTokens}"
    s!"[\"budget_reserve\",\"{label}\",\"{outStr}\"]" :: acc) []

  let runOutcomeInputs : List RunOutcomeInput :=
    [ .ok
    , .err .cancelled
    , .err .cancelledWithReason
    , .err .terminated
    , .err .selfwareInterrupted
    , .err .shutdownApi
    , .err .configErr
    , .err .apiErr
    , .err .safetyErr
    , .err .confirmationErr
    , .err .otherErr
    ]
  let shutdownOpts : List (Option ShutdownReason) :=
    [none, some .userInterrupt, some .signalTerminate, some .timeout]
  let runEndRows := runOutcomeInputs.foldr (fun inp acc =>
    shutdownOpts.foldr (fun sh acc2 =>
      let re := classifyRunEnd inp sh
      let code := processExitCode inp sh
      s!"[\"run_end:{inp.name}\",\"{ShutdownReason.optName sh}\",\"{re.name}:{code}\"]" :: acc2) acc) []

  "[" ++ ",".intercalate (cbRows ++ runUpdateRows ++ abortRows ++ stepDownRows ++ budgetRows ++ runEndRows) ++ "]"

end SupervisionBounds

#eval SupervisionBounds.exportTable
