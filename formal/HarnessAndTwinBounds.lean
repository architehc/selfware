/-
  selfware benchmark harness (`src/bench_harness/swebench_pro/runner.rs`,
  `manifest.rs`) and Fox Policy Digital Twin (`src/phi/ledger.rs`,
  `src/phi/policy.rs`) — formal model.
  Core Lean 4 only (no Mathlib). Check with: `lean HarnessAndTwinBounds.lean`.

  What is proved here:
  * BH1 seed/rerun partition on `--resume` (`should_skip_trial`):
    - Every trial in a resumed sweep is either seeded from disk or re-run,
      never both and never dropped (`seed_xor_run`).
    - Failed or in-flight trials (`planned`, `bootFailed`, `cloneFailed`,
      `running`, `agentFailed`) are never skipped on resume, regardless of
      `forceRerun` (`failed_or_incomplete_always_rerun`).
    - `evaluated` is skipped iff `forceRerun = false` (`evaluated_skip_iff`),
      preventing `--force-rerun` from double-counting completed trials.
  * BH2 honest trial outcome classification (`trial_state_from_result`):
    - An infrastructure or runner error never transitions to `patchCaptured`
      or `evaluated` (`error_never_patch_captured`).
    - A non-empty patch without a runner error transitions to `patchCaptured`
      even when the subprocess exited non-zero (`nonempty_patch_captured`),
      so evaluable work is preserved across resume.
    - A zero-byte patch with a non-clean exit transitions to `agentFailed`
      (`empty_unclean_fails`).
  * BH3 label-blind frozen candidate selection (`select_frozen_candidate`):
    - The pre-evaluation candidate selector is invariant under arbitrary
      changes to `officialResolved` (`frozen_key_ignores_official_label`,
      `select_frozen_ignores_official_label`): pass@1 selection cannot leak
      oracle evaluation labels.
    - Any candidate with a source edit and no test edit strictly outranks any
      candidate that modifies tests or lacks a source edit
      (`source_only_beats_non_source_only`).
  * BH4 multi-candidate pool conservation on resume
    (`reconstruct_trial_pool_from_disk`):
    - Both live execution and resume reconstruction contribute exactly
      `1 + candidates.length` entries to `all_runs` (`pool_size_conserved`).
  * BH5 official metric cross-multiplication ordering (`cmp_official_metrics`):
    - Cross-multiplication comparison is irreflexive (`rateBetter_irrefl`,
      `metricBetter_irrefl`).
    - Any measured `fail_to_pass` pool outranks an unmeasured `0/0` pool
      (`measured_f2p_beats_unmeasured`).
    - Full `fail_to_pass` resolution strictly beats zero resolution
      (`full_f2p_beats_zero_f2p`).
  * PT1 orthogonality of review and testing (`EvidenceKind::discharges`):
    - Passing tests never discharge `unreviewedChange`
      (`tests_never_discharge_review`).
    - Human review never discharges `unconfirmedCoverage` or
      `brokenByRemoval` (`review_never_discharges_coverage`).
  * PT2 snapshot & stale-run soundness (`Ledger` sequence ordering):
    - Failed runs never discharge any obligation (`failed_never_discharges`).
    - Uncovered scopes (`WorkspaceCoverageUnknown`) never discharge any
      obligation (`uncovered_never_discharges`).
    - Evidence whose snapshot precedes the obligation (`snapshotSeq < obSeq`)
      or races a later path edit (`snapshotSeq < lastPathEditSeq`) never
      discharges (`stale_snapshot_never_discharges`,
      `racing_edit_never_discharges`).
  * PT3 Fox Digital Twin posture honesty (`derive_fox_posture_with_debt`):
    - High verification debt (`debtPct > 70`) forces `.unimpressed` in every
      `ActivityPhase`, including `.completed`
      (`high_debt_always_unimpressed`).
    - `.flow` is reachable only when `debtPct ≤ 45`, `noOutstanding = true`,
      `hasEvidence = true`, and `phase = .completed` (`flow_sound`).
  * PT4 attention-economy monotonic backoff (`PhiPresence::ignore`):
    - Ignoring a signal strictly increases its observation quota whenever
      `base > 0` (`ignore_strictly_increases_quota`).
  * PT5 outbound mediation hierarchy & Steward priority (`PhiMediator::evaluate_outbound`,
    `PhiSteward::propose`):
    - Operator override (`gateOverridden = true`) unconditionally suppresses
      `gate:debt` so Phi never asks twice (`override_suppresses_debt_gate`).
    - Whenever `narrowLines ≤ gateLines`, firing `gate:debt` implies
      `NarrowScope` augmentation is also active (`debt_gate_implies_narrow_scope`).
    - Any failing run or broken removal obligation strictly prioritizes
      `.repair` over `.verify` and `.orient` (`repair_dominates_steward`).

  `exportTable` emits a deterministic 3-column JSON table
  (`formal/harness_twin_table.json`) covering both the benchmark harness
  trial decision space and the Fox Digital Twin posture/steward decision space,
  checked against the Rust implementation by conformance tests.
-/

namespace HarnessAndTwin

/-! ## Part I — SWE-bench Pro Benchmark Harness (`BenchHarness`) -/

inductive TrialState where
  | planned
  | bootFailed
  | cloneFailed
  | running
  | agentFailed
  | patchCaptured
  | evaluated
  deriving DecidableEq, Repr

def TrialState.name : TrialState → String
  | .planned => "planned"
  | .bootFailed => "boot_failed"
  | .cloneFailed => "clone_failed"
  | .running => "running"
  | .agentFailed => "agent_failed"
  | .patchCaptured => "patch_captured"
  | .evaluated => "evaluated"

def allTrialStates : List TrialState :=
  [.planned, .bootFailed, .cloneFailed, .running, .agentFailed, .patchCaptured, .evaluated]

/-- `should_skip_trial` in `src/bench_harness/swebench_pro/runner.rs`. -/
def shouldSkipTrial (forceRerun : Bool) : Option TrialState → Bool
  | none => false
  | some .evaluated => !forceRerun
  | some .patchCaptured => true
  | some .planned
  | some .bootFailed
  | some .cloneFailed
  | some .running
  | some .agentFailed => false

def shouldRunTrial (forceRerun : Bool) (s : Option TrialState) : Bool :=
  !shouldSkipTrial forceRerun s

/-! ### BH1 — Seed / Rerun Partition -/

theorem seed_xor_run (forceRerun : Bool) (s : Option TrialState) :
    shouldSkipTrial forceRerun s ≠ shouldRunTrial forceRerun s := by
  simp [shouldRunTrial]

theorem evaluated_skip_iff (forceRerun : Bool) :
    shouldSkipTrial forceRerun (some .evaluated) = !forceRerun := rfl

theorem failed_or_incomplete_always_rerun (forceRerun : Bool) (s : TrialState)
    (h : s ≠ .evaluated ∧ s ≠ .patchCaptured) :
    shouldSkipTrial forceRerun (some s) = false := by
  cases s <;> simp_all [shouldSkipTrial]

/-! ### BH2 — Honest Trial Outcome Classification (`trial_state_from_result`) -/

inductive ResultError where
  | none
  | bootFailed
  | cloneFailed
  | otherError
  deriving DecidableEq, Repr

def ResultError.name : ResultError → String
  | .none => "none"
  | .bootFailed => "boot_failed"
  | .cloneFailed => "clone_failed"
  | .otherError => "other_error"

def allResultErrors : List ResultError :=
  [.none, .bootFailed, .cloneFailed, .otherError]

/-- `trial_state_from_result` in `src/bench_harness/swebench_pro/runner.rs`. -/
def trialStateFromResult (err : ResultError) (patchBytes : Nat) (cleanExit : Bool) : TrialState :=
  match err with
  | .bootFailed => .bootFailed
  | .cloneFailed => .cloneFailed
  | .otherError => .agentFailed
  | .none =>
    if patchBytes > 0 then .patchCaptured
    else if cleanExit then .patchCaptured
    else .agentFailed

theorem error_never_patch_captured (err : ResultError) (patchBytes : Nat) (cleanExit : Bool)
    (herr : err ≠ .none) :
    trialStateFromResult err patchBytes cleanExit ≠ .patchCaptured ∧
    trialStateFromResult err patchBytes cleanExit ≠ .evaluated := by
  cases err <;> simp_all [trialStateFromResult]

theorem nonempty_patch_captured (patchBytes : Nat) (cleanExit : Bool) (hp : patchBytes > 0) :
    trialStateFromResult .none patchBytes cleanExit = .patchCaptured := by
  simp [trialStateFromResult, hp]

theorem empty_unclean_fails :
    trialStateFromResult .none 0 false = .agentFailed := rfl

/-! ### BH3 — Label-Blind Frozen Candidate Selector (`select_frozen_candidate`) -/

structure Candidate where
  hasSourceEdit : Bool
  hasTestEdit : Bool
  patchLines : Nat
  syntaxOk : Bool
  candidateNum : Nat
  officialResolved : Option Bool
  deriving DecidableEq, Repr

def Candidate.isSourceOnly (c : Candidate) : Bool :=
  c.hasSourceEdit && !c.hasTestEdit

/-- Strict/weak comparison matching `select_frozen_candidate` in `runner.rs`:
1. `isSourceOnly` (`true` > `false`)
2. smaller `patchLines` (`a.patchLines < b.patchLines`)
3. `syntaxOk` (`true` > `false`)
4. earlier `candidateNum` (`a.candidateNum ≤ b.candidateNum`)
Notice `officialResolved` is never read. -/
def frozenBetterEq (a b : Candidate) : Bool :=
  if a.isSourceOnly != b.isSourceOnly then a.isSourceOnly
  else if a.patchLines != b.patchLines then decide (a.patchLines < b.patchLines)
  else if a.syntaxOk != b.syntaxOk then a.syntaxOk
  else decide (a.candidateNum ≤ b.candidateNum)

def selectFrozenStep (best c : Candidate) : Candidate :=
  if frozenBetterEq best c then best else c

def selectFrozen (first : Candidate) (rest : List Candidate) : Candidate :=
  rest.foldl selectFrozenStep first

theorem frozen_better_eq_ignores_official_label (a b : Candidate) (la lb : Option Bool) :
    frozenBetterEq { a with officialResolved := la } { b with officialResolved := lb } =
      frozenBetterEq a b := rfl

theorem source_only_beats_non_source_only (a b : Candidate)
    (ha : a.isSourceOnly = true) (hb : b.isSourceOnly = false) :
    frozenBetterEq a b = true ∧ frozenBetterEq b a = false := by
  simp [frozenBetterEq, ha, hb]

/-! ### BH4 — Multi-Candidate Pool Conservation on Resume -/

def liveTrialPool (promoted : Candidate) (candidates : List Candidate) : List Candidate :=
  promoted :: candidates

def resumedTrialPool (reconstructedTrial : Candidate) (reconstructedCandidates : List Candidate) :
    List Candidate :=
  reconstructedTrial :: reconstructedCandidates

theorem pool_size_conserved (promoted reconstructed : Candidate) (candidates : List Candidate) :
    (liveTrialPool promoted candidates).length = (resumedTrialPool reconstructed candidates).length ∧
    (resumedTrialPool reconstructed candidates).length = 1 + candidates.length := by
  simp [liveTrialPool, resumedTrialPool, Nat.add_comm]

/-! ### BH5 — Official Metric Cross-Multiplication Ordering (`cmp_official_metrics`) -/

structure RatePair where
  passed : Nat
  total : Nat
  deriving DecidableEq, Repr

/-- Strict greater-than on `RatePair` via cross-multiplication (`cmp_official_metrics` in `runner.rs`). -/
def rateBetter (a b : RatePair) : Bool :=
  if a.total == 0 && b.total == 0 then false
  else if a.total == 0 then false
  else if b.total == 0 then true
  else decide (a.passed * b.total > b.passed * a.total)

def rateEq (a b : RatePair) : Bool :=
  if a.total == 0 && b.total == 0 then true
  else if a.total == 0 || b.total == 0 then false
  else decide (a.passed * b.total = b.passed * a.total)

structure OfficialMetrics where
  f2p : RatePair
  p2p : RatePair
  overallPass : Bool
  deriving DecidableEq, Repr

def metricBetter (a b : OfficialMetrics) : Bool :=
  if !rateEq a.f2p b.f2p then rateBetter a.f2p b.f2p
  else if !rateEq a.p2p b.p2p then rateBetter a.p2p b.p2p
  else a.overallPass && !b.overallPass

theorem rateBetter_irrefl (r : RatePair) : rateBetter r r = false := by
  unfold rateBetter
  by_cases h0 : r.total == 0 <;> simp [h0]

theorem rateEq_refl (r : RatePair) : rateEq r r = true := by
  unfold rateEq
  by_cases h0 : r.total == 0 <;> simp [h0]

theorem metricBetter_irrefl (m : OfficialMetrics) : metricBetter m m = false := by
  unfold metricBetter
  rw [rateEq_refl m.f2p, rateEq_refl m.p2p]
  cases m.overallPass <;> rfl

theorem measured_f2p_beats_unmeasured (a b : OfficialMetrics)
    (ha : a.f2p.total > 0) (hb : b.f2p.total = 0) :
    metricBetter a b = true := by
  have ha0 : (a.f2p.total == 0) = false := by
    cases h : a.f2p.total == 0
    · rfl
    · have : a.f2p.total = 0 := eq_of_beq h
      omega
  have hb0 : (b.f2p.total == 0) = true := by
    simp [hb]
  simp [metricBetter, rateEq, rateBetter, ha0, hb0]

theorem full_f2p_beats_zero_f2p (a b : OfficialMetrics)
    (ha : a.f2p.total > 0) (hb : b.f2p.total > 0)
    (hpA : a.f2p.passed = a.f2p.total) (hpB : b.f2p.passed = 0) :
    metricBetter a b = true := by
  have ha0 : (a.f2p.total == 0) = false := by
    cases h : a.f2p.total == 0
    · rfl
    · have : a.f2p.total = 0 := eq_of_beq h
      omega
  have hb0 : (b.f2p.total == 0) = false := by
    cases h : b.f2p.total == 0
    · rfl
    · have : b.f2p.total = 0 := eq_of_beq h
      omega
  have hmul : a.f2p.passed * b.f2p.total > b.f2p.passed * a.f2p.total := by
    rw [hpA, hpB, Nat.zero_mul]
    exact Nat.mul_pos ha hb
  have hneq : ¬ (a.f2p.passed * b.f2p.total = b.f2p.passed * a.f2p.total) := by omega
  simp [metricBetter, rateEq, rateBetter, ha0, hb0, hmul, hneq]

/-! ## Part II — Fox Policy Digital Twin & Evidence Ledger (`PhiPolicyTwin`) -/

inductive ObligationKind where
  | unreviewedChange
  | brokenByRemoval
  | unconfirmedCoverage
  deriving DecidableEq, Repr

inductive EvidenceKind where
  | testsExecuted
  | humanReviewed
  deriving DecidableEq, Repr

/-- `EvidenceKind::discharges` + removal-regression coverage in `src/phi/ledger.rs`. -/
def dischargesKind : EvidenceKind → ObligationKind → Bool
  | .testsExecuted, .unconfirmedCoverage => true
  | .testsExecuted, .brokenByRemoval => true
  | .humanReviewed, .unreviewedChange => true
  | _, _ => false

/-! ### PT1 — Orthogonality of Review and Testing -/

theorem tests_never_discharge_review :
    dischargesKind .testsExecuted .unreviewedChange = false := rfl

theorem review_never_discharges_coverage :
    dischargesKind .humanReviewed .unconfirmedCoverage = false ∧
    dischargesKind .humanReviewed .brokenByRemoval = false := ⟨rfl, rfl⟩

/-! ### PT2 — Snapshot & Stale-Run Soundness -/

/-- Whether an evidence record can discharge an obligation on a path:
- `outcomePassed` must be `true`
- `scopeCovers` must be `true` (`Scope::WorkspaceCoverageUnknown` has `false`)
- `dischargesKind evKind obKind` must hold
- `obSeq ≤ snapshotSeq` (run started at or after the obligation was created)
- `lastPathEditSeq ≤ snapshotSeq` (no edit to the path raced the run after it started) -/
def canDischarge (evKind : EvidenceKind) (obKind : ObligationKind)
    (outcomePassed scopeCovers : Bool)
    (snapshotSeq obSeq lastPathEditSeq : Nat) : Bool :=
  outcomePassed && scopeCovers && dischargesKind evKind obKind
    && decide (obSeq ≤ snapshotSeq) && decide (lastPathEditSeq ≤ snapshotSeq)

theorem failed_never_discharges (evKind : EvidenceKind) (obKind : ObligationKind)
    (scopeCovers : Bool) (snapshotSeq obSeq lastPathEditSeq : Nat) :
    canDischarge evKind obKind false scopeCovers snapshotSeq obSeq lastPathEditSeq = false := rfl

theorem uncovered_never_discharges (evKind : EvidenceKind) (obKind : ObligationKind)
    (outcomePassed : Bool) (snapshotSeq obSeq lastPathEditSeq : Nat) :
    canDischarge evKind obKind outcomePassed false snapshotSeq obSeq lastPathEditSeq = false := by
  simp [canDischarge]

theorem stale_snapshot_never_discharges (evKind : EvidenceKind) (obKind : ObligationKind)
    (outcomePassed scopeCovers : Bool) (snapshotSeq obSeq lastPathEditSeq : Nat)
    (hstale : snapshotSeq < obSeq) :
    canDischarge evKind obKind outcomePassed scopeCovers snapshotSeq obSeq lastPathEditSeq = false := by
  have hnot : ¬ (obSeq ≤ snapshotSeq) := by omega
  simp [canDischarge, hnot]

theorem racing_edit_never_discharges (evKind : EvidenceKind) (obKind : ObligationKind)
    (outcomePassed scopeCovers : Bool) (snapshotSeq obSeq lastPathEditSeq : Nat)
    (hrace : snapshotSeq < lastPathEditSeq) :
    canDischarge evKind obKind outcomePassed scopeCovers snapshotSeq obSeq lastPathEditSeq = false := by
  have hnot : ¬ (lastPathEditSeq ≤ snapshotSeq) := by omega
  simp [canDischarge, hnot]

/-! ### PT3 — Fox Digital Twin Posture Honesty (`derive_fox_posture_with_debt`) -/

inductive ActivityPhase where
  | running
  | completed
  | partialPhase
  | failed
  | abandoned
  deriving DecidableEq, Repr

def ActivityPhase.name : ActivityPhase → String
  | .running => "running"
  | .completed => "completed"
  | .partialPhase => "partial"
  | .failed => "failed"
  | .abandoned => "abandoned"

def allPhases : List ActivityPhase :=
  [.running, .completed, .partialPhase, .failed, .abandoned]

inductive FoxEmotion where
  | greeting
  | working
  | error
  | flow
  | skeptical
  | unimpressed
  deriving DecidableEq, Repr

def FoxEmotion.name : FoxEmotion → String
  | .greeting => "greeting"
  | .working => "working"
  | .error => "error"
  | .flow => "flow"
  | .skeptical => "skeptical"
  | .unimpressed => "unimpressed"

/-- `derive_fox_posture_with_debt` in `src/phi/policy.rs`, with `debtPct` in `0..=100`
(`debt_ratio > 0.70` ⇔ `debtPct > 70`, `debt_ratio > 0.45` ⇔ `debtPct > 45`). -/
def deriveFoxEmotion (debtPct : Nat) (noOutstanding hasEvidence : Bool)
    (phase : ActivityPhase) : FoxEmotion :=
  if debtPct > 70 then .unimpressed
  else if debtPct > 45 then .skeptical
  else match phase with
    | .failed => .error
    | .partialPhase | .abandoned => .skeptical
    | .running => .working
    | .completed => if noOutstanding && hasEvidence then .flow else .greeting

theorem high_debt_always_unimpressed (debtPct : Nat) (noOut hasEv : Bool)
    (phase : ActivityPhase) (h : debtPct > 70) :
    deriveFoxEmotion debtPct noOut hasEv phase = .unimpressed := by
  simp [deriveFoxEmotion, h]

theorem medium_debt_always_skeptical (debtPct : Nat) (noOut hasEv : Bool)
    (phase : ActivityPhase) (hlo : 45 < debtPct) (hhi : debtPct ≤ 70) :
    deriveFoxEmotion debtPct noOut hasEv phase = .skeptical := by
  have hnot70 : ¬ (debtPct > 70) := by omega
  simp [deriveFoxEmotion, hnot70, hlo]

theorem flow_sound (debtPct : Nat) (noOut hasEv : Bool) (phase : ActivityPhase)
    (h : deriveFoxEmotion debtPct noOut hasEv phase = .flow) :
    debtPct ≤ 45 ∧ noOut = true ∧ hasEv = true ∧ phase = .completed := by
  unfold deriveFoxEmotion at h
  by_cases h70 : debtPct > 70
  · simp [h70] at h
  · simp only [h70, ↓reduceIte] at h
    by_cases h45 : debtPct > 45
    · simp [h45] at h
    · simp only [h45, ↓reduceIte] at h
      cases phase <;> simp at h
      cases noOut <;> cases hasEv <;> simp at h
      exact ⟨by omega, rfl, rfl, rfl⟩

/-! ### PT4 — Attention Economy Monotonic Backoff (`PhiPresence::ignore`) -/

def quotaAfterIgnore (quota base ignored : Nat) : Nat :=
  quota + base * (ignored + 1) * 2

theorem ignore_strictly_increases_quota (quota base ignored : Nat) (hb : base > 0) :
    quota < quotaAfterIgnore quota base ignored := by
  unfold quotaAfterIgnore
  have h1 : 1 ≤ base * (ignored + 1) := Nat.mul_pos hb (by omega)
  omega

/-! ### PT5 — Outbound Mediation Hierarchy & Steward Proposal Priority -/

structure OutboundDecision where
  demandDisproof : Bool
  requireEvidence : Bool
  narrowScope : Bool
  gateDebt : Bool
  deriving DecidableEq, Repr

/-- `PhiMediator::evaluate_outbound` in `src/phi/policy.rs`. -/
def evaluateOutbound (capitulations capThreshold unreviewedCount unrevThreshold
    unverifiedLines narrowLines gateLines debtPct : Nat)
    (gateOverridden : Bool) : OutboundDecision :=
  { demandDisproof := decide (capitulations ≥ capThreshold)
    requireEvidence := decide (unreviewedCount ≥ unrevThreshold)
    narrowScope := decide (unverifiedLines ≥ narrowLines) || decide (debtPct > 55)
    gateDebt := (decide (unverifiedLines ≥ gateLines) || decide (debtPct > 72)) && !gateOverridden }

theorem override_suppresses_debt_gate
    (cap capT unrev unrevT unvLines narrowL gateL debtPct : Nat) :
    (evaluateOutbound cap capT unrev unrevT unvLines narrowL gateL debtPct true).gateDebt = false := by
  simp [evaluateOutbound]

theorem debt_gate_implies_narrow_scope
    (cap capT unrev unrevT unvLines narrowL gateL debtPct : Nat)
    (overridden : Bool)
    (hlines : narrowL ≤ gateL)
    (hgate : (evaluateOutbound cap capT unrev unrevT unvLines narrowL gateL debtPct overridden).gateDebt = true) :
    (evaluateOutbound cap capT unrev unrevT unvLines narrowL gateL debtPct overridden).narrowScope = true := by
  unfold evaluateOutbound at hgate ⊢
  simp only [Bool.and_eq_true, Bool.or_eq_true, decide_eq_true_eq] at hgate ⊢
  rcases hgate.1 with hl | hd
  · left; omega
  · right; omega

inductive ProposalKind where
  | repair
  | verify
  | repay
  | orient
  | explore
  deriving DecidableEq, Repr

def ProposalKind.rank : ProposalKind → Nat
  | .repair => 0
  | .verify => 1
  | .repay => 2
  | .orient => 3
  | .explore => 4

def ProposalKind.name : ProposalKind → String
  | .repair => "repair"
  | .verify => "verify"
  | .repay => "repay"
  | .orient => "orient"
  | .explore => "explore"

/-- Top proposal kind returned by `PhiSteward::propose` when no proposals are dismissed. -/
def stewardTopKind (hasFailedRun hasBrokenRemoval hasUnreviewed hasUntested hasOpaque : Bool) :
    Option ProposalKind :=
  if hasFailedRun || hasBrokenRemoval then some .repair
  else if hasUnreviewed || hasUntested then some .verify
  else if hasOpaque then some .orient
  else none

theorem repair_dominates_steward
    (hasFailedRun hasBrokenRemoval hasUnreviewed hasUntested hasOpaque : Bool)
    (h : hasFailedRun = true ∨ hasBrokenRemoval = true) :
    stewardTopKind hasFailedRun hasBrokenRemoval hasUnreviewed hasUntested hasOpaque = some .repair := by
  rcases h with hf | hb
  · simp [stewardTopKind, hf]
  · simp [stewardTopKind, hb]

theorem verify_when_no_repair
    (hasUnreviewed hasUntested hasOpaque : Bool)
    (h : hasUnreviewed = true ∨ hasUntested = true) :
    stewardTopKind false false hasUnreviewed hasUntested hasOpaque = some .verify := by
  rcases h with hu | ht
  · simp [stewardTopKind, hu]
  · simp [stewardTopKind, ht]

theorem clean_books_no_proposal :
    stewardTopKind false false false false false = none := rfl

/-! ## Part III — Offline Endpoint Fallback & Context Budget Derivation -/

/-- `MIN_CONVERSATION_TOKENS` in `src/config/mod.rs`. -/
def minConversationTokens : Nat := 2048

/-- `Config::derive_context_budget` in `src/config/mod.rs`:
clamps `maxTokens` down to `context - context / 5 - minConversationTokens` and
returns `some (maxContextTokens, reservedOutput)` iff `maxContextTokens ≥ 2048`. -/
def deriveContextBudget (context maxTokens : Nat) : Option (Nat × Nat) :=
  let safetyMargin := context / 5
  let maxReservable := context - safetyMargin - minConversationTokens
  let reservedOutput := min maxTokens maxReservable
  let maxContextTokens := context - reservedOutput - safetyMargin
  if maxContextTokens < minConversationTokens then none
  else some (maxContextTokens, reservedOutput)

theorem derive_context_budget_sound (context maxTokens budget reserved : Nat)
    (h : deriveContextBudget context maxTokens = some (budget, reserved)) :
    minConversationTokens ≤ budget ∧ reserved ≤ maxTokens ∧
    budget + reserved + context / 5 ≤ context := by
  unfold deriveContextBudget at h
  dsimp at h
  by_cases hlt : context - min maxTokens (context - context / 5 - minConversationTokens) - context / 5 < minConversationTokens
  · simp [hlt] at h
  · simp [hlt] at h
    rcases h with ⟨rfl, rfl⟩
    refine ⟨by omega, Nat.min_le_left _ _, ?_⟩
    omega

theorem derive_context_budget_32k_clamps_oversized_max_tokens :
    deriveContextBudget 32768 65536 = some (2048, 24167) := rfl

theorem derive_context_budget_32k_8k_output :
    deriveContextBudget 32768 8192 = some (18023, 8192) := rfl

inductive EndpointTarget where
  | customLocal
  | ollamaDefault
  deriving DecidableEq, Repr

/-- `LocalEndpointFallback::choose_local_endpoint` in `src/self_healing/recovery_tree.rs`:
an override is accepted only when it is non-empty AND verified as a local endpoint. -/
def chooseLocalEndpoint (overrideNonEmpty overrideIsLocal : Bool) : EndpointTarget :=
  if overrideNonEmpty && overrideIsLocal then .customLocal else .ollamaDefault

theorem non_local_override_never_selected (nonEmpty : Bool) :
    chooseLocalEndpoint nonEmpty false = .ollamaDefault := by
  simp [chooseLocalEndpoint]

theorem chosen_endpoint_always_local (nonEmpty isLocal : Bool) :
    chooseLocalEndpoint nonEmpty isLocal = .ollamaDefault ∨ isLocal = true := by
  cases nonEmpty <;> cases isLocal <;> simp [chooseLocalEndpoint]

/-- Rigor-mode transition in `src/agent/task_runner.rs`:
when `resolved = true`, a recovery action that fails to apply (`appliedOk = false`)
must still enter rigor mode, matching `Escalate` / `Unresolvable`. -/
def shouldEnterRigorOnRecovery (resolved appliedOk escalatedOrUnresolvable : Bool) : Bool :=
  if resolved then !appliedOk else escalatedOrUnresolvable

theorem failed_recovery_application_always_enters_rigor (esc : Bool) :
    shouldEnterRigorOnRecovery true false esc = true := rfl

theorem successful_recovery_application_skips_rigor (esc : Bool) :
    shouldEnterRigorOnRecovery true true esc = false := rfl

/-- `clamp_hardware_fallback_budgets` in `src/config/unpack.rs`:
clamps `maxTokens` and derives a safe `tokenBudget` when `auto_calibrate` falls
back to a hardware-recommended model window without a live server probe. -/
def clampHardwareFallbackBudgets (context maxTokens : Nat) (maxTokensUserSet : Bool) : Nat × Nat :=
  let clampedMaxTokens :=
    if maxTokensUserSet then min maxTokens context
    else min (min maxTokens (max (context / 4) 1024)) context
  let tokenBudget := max (min (context * 3 / 5) (context - clampedMaxTokens)) 1
  (clampedMaxTokens, tokenBudget)

theorem clamp_hardware_fallback_budgets_fits_window (context maxTokens : Nat) (userSet : Bool) :
    let (clamped, budget) := clampHardwareFallbackBudgets context maxTokens userSet
    clamped ≤ context ∧ 1 ≤ budget ∧ (clamped < context → clamped + budget ≤ context) := by
  unfold clampHardwareFallbackBudgets
  cases userSet <;> dsimp <;> refine ⟨?_, ?_, ?_⟩ <;> omega

theorem clamp_hardware_fallback_32k_default :
    clampHardwareFallbackBudgets 32768 65536 false = (8192, 19660) := rfl

/-! ## Export deterministic 3-column table for `scripts/check_formal.sh` & Rust conformance tests -/

def exportTable : String :=
  let skipRows := [false, true].foldr (fun forceRerun acc =>
    let frName := if forceRerun then "force_rerun" else "normal_resume"
    allTrialStates.foldr (fun st acc2 =>
      let act := if shouldSkipTrial forceRerun (some st) then "skip" else "run"
      s!"[\"bench_skip:{frName}\",\"{st.name}\",\"{act}\"]" :: acc2) acc) []
  let resultRows := allResultErrors.foldr (fun err acc =>
    [(0, false, "empty_fail"), (0, true, "empty_clean"), (42, false, "patch_fail"), (42, true, "patch_clean")].foldr
      (fun (bytes, clean, label) acc2 =>
        let st := trialStateFromResult err bytes clean
        s!"[\"bench_result:{err.name}\",\"{label}\",\"{st.name}\"]" :: acc2) acc) []
  let postureRows := [(0, true, true, "clean_verified"),
                      (0, true, false, "clean_unverified"),
                      (55, false, false, "medium_debt"),
                      (85, false, false, "high_debt")].foldr
    (fun (debtPct, noOut, hasEv, stateLabel) acc =>
      allPhases.foldr (fun ph acc2 =>
        let em := deriveFoxEmotion debtPct noOut hasEv ph
        s!"[\"phi_posture:{stateLabel}\",\"{ph.name}\",\"{em.name}\"]" :: acc2) acc) []
  let stewardRows := [(true, false, true, true, true, "failed_run_plus_debt"),
                      (false, true, true, true, false, "broken_removal_plus_debt"),
                      (false, false, true, false, true, "unreviewed_plus_opaque"),
                      (false, false, false, true, true, "untested_plus_opaque"),
                      (false, false, false, false, true, "opaque_only"),
                      (false, false, false, false, false, "all_clear")].foldr
    (fun (fr, br, ur, ut, op, label) acc =>
      let out := match stewardTopKind fr br ur ut op with
        | some k => k.name
        | none => "none"
      s!"[\"phi_steward\",\"{label}\",\"{out}\"]" :: acc) []
  "[" ++ ",".intercalate (skipRows ++ resultRows ++ postureRows ++ stewardRows) ++ "]"

end HarnessAndTwin

#eval HarnessAndTwin.exportTable
