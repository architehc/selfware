/-
  selfware W8b completion gate — formal model of the verification and
  readback part of `Agent::check_completion_gate` (src/agent/verification.rs)
  on a mixed task (a code change plus a task-named non-code artifact).
  Core Lean 4 only (no Mathlib). Check with: `lean VerificationGateBounds.lean`.

  The model follows the order the Rust gate checks in:

  1. `ArtifactReadbackRequired` — a task-named artifact written since its
     last read-back blocks, unless a fresh authoritative pass ran after that
     write (accept-with-proof), or `ARTIFACT_READBACK_REJECTION_BOUND`
     rejections were already sent (the harness then reads it back itself).
  2. "file written without a passing verification" / StaleVerification /
     FailingTestsAccepted — a code change after the last credited pass, or
     a failure after it, blocks; unless every check at this revision could
     not run or failed only with pre-existing errors (accepted WITHOUT
     credit: `acceptUncredited`, never `accept`).
  3. Otherwise the tree is accepted: `acceptWithProof` when doc-only writes
     followed the pass (W8b accept-with-proof), `accept` when the pass
     covers the current revision.

  Revisions are `mutation_sequence` values: every write advances `rev`, a
  pass is credited at the current `rev`. Not modelled: artifact-only tasks
  (their completion is the citation gate), a harness readback of an
  artifact that is not on disk (it keeps blocking), and the gates before and
  after this part (required tools, min steps, audit ledger, citations).

  What is proved here:
  * V1 unverified code never completes: with a code write after the last
    pass (or a failure since it), the verdict is a rejection, or
    `acceptUncredited` when the waiver holds — never `accept` or
    `acceptWithProof`.
  * V2 accept-with-proof soundness: `acceptWithProof` implies the last code
    write is covered by a pass, no failure is outstanding, and something was
    written after that pass.
  * V3 a failing run never advances the credited pass revision.
  * V4 bounded readback: a readback rejection happens only below
    `READBACK_BOUND` consecutive rejections; at the bound the gate reads the
    artifact itself and moves on (no livelock).
  * V5 bounded audit ledger: the 3rd rejected attempt steps aside.
  * V6 a write resets the readback-rejection count (a new artifact state
    gets a fresh readback).

  `exportTable` evaluates `applyAction` on four scenarios × three inputs;
  `formal/verification_gate_table.json` is that export, and the Rust
  conformance test replays each scenario through the real gate.
-/

namespace VerificationGate

/-- `ARTIFACT_READBACK_REJECTION_BOUND` -/
def READBACK_BOUND : Nat := 2
/-- `check_audit_ledger`: the 3rd rejected attempt steps aside. -/
def AUDIT_STEP_ASIDE_ATTEMPT : Nat := 3

structure GateState where
  rev : Nat
  lastCodeWriteRev : Nat
  /-- Last write of the task-named artifact (0: none). -/
  lastArtifactWriteRev : Nat
  /-- The artifact was read back after its last write. -/
  artifactRead : Bool
  /-- Revision the last authoritative pass was credited at (0: none). -/
  lastPassRev : Nat
  /-- A verification failed after the last pass. -/
  failureOutstanding : Bool
  /-- Every check at this revision was not-run or failed only with
      pre-existing errors. -/
  waiver : Bool
  readbackRejections : Nat
  auditAttempts : Nat
  deriving DecidableEq, Repr

def initState : GateState :=
  { rev := 0, lastCodeWriteRev := 0, lastArtifactWriteRev := 0, artifactRead := false,
    lastPassRev := 0, failureOutstanding := false, waiver := false,
    readbackRejections := 0, auditAttempts := 0 }

inductive GateVerdict where
  | accept
  | acceptWithProof
  | acceptUncredited
  | rejectUnverified
  | rejectUnread
  | auditReject
  | auditStepAside
  deriving DecidableEq, Repr

inductive Action where
  | writeCode
  | writeArtifact
  | readArtifact
  | testPass
  | testFail
  | noCheckCanRun
  | attemptComplete
  | auditFinding
  deriving DecidableEq, Repr

/-- `fresh_authoritative_pass` covering the artifact's last write: a pass,
no failure since, no code write after it, and it ran after the artifact. -/
def proofCoversArtifact (s : GateState) : Bool :=
  s.lastPassRev > 0 && !s.failureOutstanding && s.lastCodeWriteRev ≤ s.lastPassRev
    && s.lastArtifactWriteRev ≤ s.lastPassRev

def codeUnverified (s : GateState) : Bool :=
  s.lastCodeWriteRev > s.lastPassRev || s.failureOutstanding

/-- Steps 2 and 3 (after the readback part). -/
def verifyVerdict (s : GateState) : GateVerdict :=
  if codeUnverified s then
    if s.waiver then .acceptUncredited else .rejectUnverified
  else if s.rev > s.lastPassRev then .acceptWithProof
  else .accept

def applyAction (s : GateState) : Action → GateState × Option GateVerdict
  | .writeCode =>
    ({ s with rev := s.rev + 1, lastCodeWriteRev := s.rev + 1, waiver := false,
              readbackRejections := 0 }, none)
  | .writeArtifact =>
    ({ s with rev := s.rev + 1, lastArtifactWriteRev := s.rev + 1, artifactRead := false,
              waiver := false, readbackRejections := 0 }, none)
  | .readArtifact => ({ s with artifactRead := true }, none)
  | .testPass => ({ s with lastPassRev := s.rev, failureOutstanding := false }, none)
  | .testFail => ({ s with failureOutstanding := true }, none)
  | .noCheckCanRun => ({ s with waiver := true }, none)
  | .attemptComplete =>
    let pending := s.lastArtifactWriteRev > 0 && !s.artifactRead && !proofCoversArtifact s
    if pending && s.readbackRejections < READBACK_BOUND then
      ({ s with readbackRejections := s.readbackRejections + 1 }, some .rejectUnread)
    else
      -- at the bound the harness reads the artifact back itself
      let s' := if pending then { s with artifactRead := true } else s
      (s', some (verifyVerdict s'))
  | .auditFinding =>
    let n := s.auditAttempts + 1
    ({ s with auditAttempts := n },
     some (if n ≥ AUDIT_STEP_ASIDE_ATTEMPT then .auditStepAside else .auditReject))

def verdictOf (s : GateState) : Option GateVerdict := (applyAction s .attemptComplete).2

/-! ## Shape of a completion verdict -/

def pendingReadback (s : GateState) : Bool :=
  s.lastArtifactWriteRev > 0 && !s.artifactRead && !proofCoversArtifact s

/-- The verification verdict does not depend on whether the artifact was read. -/
theorem verify_verdict_ignores_read (s : GateState) (b : Bool) :
    verifyVerdict { s with artifactRead := b } = verifyVerdict s := rfl

theorem verdict_shape (s : GateState) :
    verdictOf s =
      if pendingReadback s && decide (s.readbackRejections < READBACK_BOUND) then
        some .rejectUnread
      else some (verifyVerdict s) := by
  unfold verdictOf applyAction pendingReadback
  simp only
  split
  · simp_all
  · split <;> rfl

/-! ## V1 — Unverified code never completes green -/

theorem verify_verdict_unverified (s : GateState) (h : codeUnverified s = true) :
    verifyVerdict s = .rejectUnverified ∨ verifyVerdict s = .acceptUncredited := by
  unfold verifyVerdict
  cases hw : s.waiver <;> simp [h]

theorem unverified_code_never_accepted (s : GateState) (h : codeUnverified s = true) :
    verdictOf s ≠ some .accept ∧ verdictOf s ≠ some .acceptWithProof := by
  rw [verdict_shape]
  split
  · simp
  · rcases verify_verdict_unverified s h with h1 | h1 <;> simp [h1]

theorem unverified_without_waiver_rejects (s : GateState) (h : codeUnverified s = true)
    (hw : s.waiver = false) :
    verdictOf s = some .rejectUnverified ∨ verdictOf s = some .rejectUnread := by
  rw [verdict_shape]
  split
  · exact Or.inr rfl
  · left
    simp [verifyVerdict, h, hw]

/-! ## V2 — Accept-with-proof soundness -/

theorem accept_with_proof_sound (s : GateState)
    (h : verifyVerdict s = .acceptWithProof) :
    s.lastCodeWriteRev ≤ s.lastPassRev ∧ s.failureOutstanding = false ∧ s.lastPassRev < s.rev := by
  unfold verifyVerdict at h
  by_cases hc : codeUnverified s = true
  · cases hw : s.waiver <;> simp [hc, hw] at h
  · simp only [hc] at h
    have hc' : ¬ (s.lastCodeWriteRev > s.lastPassRev) ∧ s.failureOutstanding = false := by
      simp [codeUnverified] at hc
      exact ⟨by omega, hc.2⟩
    by_cases hr : s.rev > s.lastPassRev
    · exact ⟨by omega, hc'.2, hr⟩
    · simp [hr] at h

/-! ## V3 — A failing run never advances pass credit -/

theorem test_failure_preserves_pass_revision (s : GateState) :
    (applyAction s .testFail).1.lastPassRev = s.lastPassRev := rfl

theorem test_failure_blocks_accept (s : GateState) :
    codeUnverified (applyAction s .testFail).1 = true := by
  simp [applyAction, codeUnverified]

/-! ## V4 — Bounded readback -/

theorem readback_rejection_only_below_bound (s : GateState)
    (h : verdictOf s = some .rejectUnread) : s.readbackRejections < READBACK_BOUND := by
  rw [verdict_shape] at h
  split at h
  · rename_i hp
    simp at hp
    exact hp.2
  · simp [verifyVerdict] at h
    split at h
    · split at h <;> simp at h
    · split at h <;> simp at h

theorem at_bound_no_readback_rejection (s : GateState)
    (h : READBACK_BOUND ≤ s.readbackRejections) : verdictOf s ≠ some .rejectUnread := by
  intro hr
  have := readback_rejection_only_below_bound s hr
  omega

theorem readback_rejections_step_bounded (s : GateState) :
    (applyAction s .attemptComplete).1.readbackRejections ≤ s.readbackRejections + 1 := by
  unfold applyAction
  simp only
  split
  · simp
  · split <;> simp

/-! ## V5 — Audit ledger steps aside -/

theorem audit_steps_aside_at_third_attempt (s : GateState)
    (h : AUDIT_STEP_ASIDE_ATTEMPT ≤ s.auditAttempts + 1) :
    (applyAction s .auditFinding).2 = some .auditStepAside := by
  simp [applyAction, h]

theorem audit_attempts_advance (s : GateState) :
    (applyAction s .auditFinding).1.auditAttempts = s.auditAttempts + 1 := rfl

/-! ## V6 — A write resets the readback count -/

theorem write_resets_readback (s : GateState) :
    (applyAction s .writeCode).1.readbackRejections = 0 ∧
    (applyAction s .writeArtifact).1.readbackRejections = 0 := ⟨rfl, rfl⟩

/-! ## Export: scenarios × inputs, evaluated with `applyAction` -/

def run (s : GateState) : List Action → GateState
  | [] => s
  | a :: rest => run (applyAction s a).1 rest

inductive Scenario where
  | clean
  | codeUnverified
  | docAfterPass
  | failedAfterDoc
  deriving DecidableEq, Repr

def Scenario.actions : Scenario → List Action
  | .clean          => [.writeCode, .writeArtifact, .testPass]
  | .codeUnverified => [.writeCode, .writeArtifact, .testPass, .writeCode]
  | .docAfterPass   => [.writeCode, .testPass, .writeArtifact]
  | .failedAfterDoc => [.writeCode, .testPass, .writeArtifact, .testFail]

def Scenario.name : Scenario → String
  | .clean => "clean"
  | .codeUnverified => "code_unverified"
  | .docAfterPass => "doc_after_pass"
  | .failedAfterDoc => "failed_after_doc"

inductive Input where
  | completeNoRead
  | completeWithRead
  | completeAtReadbackBound
  deriving DecidableEq, Repr

def Input.name : Input → String
  | .completeNoRead => "complete_no_read"
  | .completeWithRead => "complete_with_read"
  | .completeAtReadbackBound => "complete_at_readback_bound"

/-- The state the completion attempt sees. At the bound: the earlier
attempts were the `READBACK_BOUND` readback rejections. -/
def Input.prepare (s : GateState) : Input → GateState
  | .completeNoRead => s
  | .completeWithRead => (applyAction s .readArtifact).1
  | .completeAtReadbackBound => { s with readbackRejections := READBACK_BOUND }

def GateVerdict.name : GateVerdict → String
  | .accept => "accept"
  | .acceptWithProof => "accept_with_proof"
  | .acceptUncredited => "accept_uncredited"
  | .rejectUnverified => "reject_unverified"
  | .rejectUnread => "reject_unread"
  | .auditReject => "audit_reject"
  | .auditStepAside => "audit_step_aside"

def outcome (sc : Scenario) (i : Input) : String :=
  match verdictOf (i.prepare (run initState sc.actions)) with
  | some v => v.name
  | none => "none"

def allScenarios : List Scenario := [.clean, .codeUnverified, .docAfterPass, .failedAfterDoc]
def allInputs : List Input := [.completeNoRead, .completeWithRead, .completeAtReadbackBound]

def exportTable : String :=
  let rows := allScenarios.foldr (fun sc acc =>
    allInputs.foldr (fun i acc2 =>
      s!"[\"{sc.name}\",\"{i.name}\",\"{outcome sc i}\"]" :: acc2) acc) []
  "[" ++ ",".intercalate rows ++ "]"

end VerificationGate

#eval VerificationGate.exportTable
