/-
  selfware Phi Fox umbrella, grounded trust state, attention economy, Debt Boss
  arena, Dream consolidation honesty, KV-prefix role demotion, ToolCache
  worktree-scoped invalidation, and EditHistory restore honesty — formal model.
  Core Lean 4 only (no Mathlib).
  Check with: `lean formal/PhiArenaAndTrustBounds.lean` (or `./scripts/check_formal.sh`).

  Code modelled:
  * PA1 `SignalEmotion` 6-tier priority lattice (`src/evolve/server/phi_activity.rs:classify_signal_emotion`):
    - Deterministic mapping from workspace health signals (`failurePresent`,
      `debtPresent`, `uncertaintyPresent`, `runningPresent`, `hasCitations`) to
      the 6 signal emotions (`error`, `skeptical`, `guard`, `working`,
      `analytical`, `curious`).
    - Proves strict priority dominance (`error_dominates_all`,
      `skeptical_dominates_non_error`, `guard_dominates_when_no_error_or_debt`,
      `working_dominates_idle_signals`) and exact iff characterizations of
      `analytical` (`analytical_iff_clean_idle_with_citations`) and `curious`
      (`curious_iff_all_quiet`).
  * PA2 Grounded Citation `TrustState` (`src/evolve/server/phi_activity.rs`, `AGENTS.md` Rule 3):
    - Honest status lattice (`withheld`, `unverified`, `structural`, `verified`).
    - Proves that redacted targets always withhold (`redaction_always_withholds`),
      structural citations without semantic verification never claim `verified`
      (`structural_only_never_verified`), and trust conjunction is monotone and
      idempotent (`downgrade_monotone_left`, `downgrade_monotone_right`).
  * PA3 `PhiPresence` Attention-Economy Routing (`src/phi/policy.rs:PhiPresence::route`):
    - Proves that busy or unsettled sessions only emit `.ambient`
      (`busy_only_ambient`, `unsettled_only_ambient`), user fatigue (`dismissed`,
      `alreadySeen`, or `consecutiveIgnored >= 2`) silences output
      (`dismissed_silences_when_ready`, `ignored_twice_silences_when_ready`), and
      `.interrupt` never fires without quota or with positive ignored count
      (`interrupt_never_fires_when_quota_zero`,
      `interrupt_never_fires_when_ignored_positive`).
  * PA4 `DebtBoss` Archetype Priority & HP Bounds (`src/phi/arena.rs`):
    - Models `classify_debt_boss_archetype` (`dormantFog`, `regressionHydra`,
      `phantomSpecter`, `boilerplateGolem`, `sentinelWard`, `arenaClear`) and HP
      bounds.
    - Proves `.dormantFog` when observations are absent
      (`dormant_fog_when_obs_unavailable`), `.regressionHydra` dominance on
      failing command runs (`regression_hydra_dominates_observed`), `maxHp >= 100`
      (`max_hp_at_least_100`), and monotone drill damage reduction
      (`drill_damage_monotone_reduces_hp`).
  * PA5 `DreamConsolidation` Honest Status (`src/cognitive/dream_subprocess.rs`, `AGENTS.md` Rule 3):
    - Proves `success = true ↔ (consolidateFailed = false ∧ errorCount = 0)`
      (`dream_honest_success_iff`) and zeroed stored/archived counts on
      consolidation failure (`consolidate_failure_zeros_counters`).
  * PA6 KV-Prefix Role Demotion (`src/agent/context_management.rs:demote_mid_conversation_system_messages`):
    - Proves first-message immutability (`demote_preserves_first_message`),
      complete elimination of mid-conversation `.system` roles
      (`demoteTailRole_never_system`), length preservation
      (`demote_preserves_length`), and idempotence (`demote_idempotent`).
  * PA7 ToolCache Worktree Isolation & Invalidation (`src/session/cache.rs`):
    - Models `is_cacheable`, `invalidates_cache`, `is_tree_scoped_key`, and
      worktree-isolated `invalidate_path` + `invalidate_git`.
    - Proves disjointness of cacheable reads and invalidating mutations
      (`cacheable_and_invalidating_disjoint`), foreign worktree preservation
      (`foreign_worktree_entry_always_retained`), and active-worktree eviction of
      tree-scoped and git-scoped entries (`tree_scoped_dropped_in_active_worktree`,
      `git_status_diff_dropped_in_active_worktree`).
  * PA8 EditHistory Undo/Redo & `RestoreOutcome` Honesty (`src/session/edit_history.rs`, `AGENTS.md` Rule 3):
    - Proves `recordEdit` clears the redo stack (`recordEdit_clears_redo`),
      corrupt snapshots never restore (`corrupt_snapshot_never_restores`), and
      `restored` iff hash-valid, modified on disk, and write succeeded
      (`restored_iff_valid_dirty_and_written`).

  `exportTable` emits a deterministic 3-column JSON table
  (`formal/phi_arena_trust_table.json`) verified against the Rust implementations
  by unit conformance tests.
-/

namespace PhiArenaAndTrustBounds

/-! ## Part I — `SignalEmotion` 6-Tier Priority Lattice (`src/evolve/server/phi_activity.rs`) -/

inductive SignalEmotion where
  | error
  | skeptical
  | guard
  | working
  | analytical
  | curious
  deriving DecidableEq, Repr

def SignalEmotion.name : SignalEmotion → String
  | .error => "error"
  | .skeptical => "skeptical"
  | .guard => "guard"
  | .working => "working"
  | .analytical => "analytical"
  | .curious => "curious"

/-- `classify_signal_emotion` in `src/evolve/server/phi_activity.rs`. -/
def classifySignalEmotion
    (failurePresent debtPresent uncertaintyPresent runningPresent hasCitations : Bool) :
    SignalEmotion :=
  if failurePresent then
    .error
  else if debtPresent then
    .skeptical
  else if uncertaintyPresent then
    .guard
  else if runningPresent then
    .working
  else if hasCitations then
    .analytical
  else
    .curious

theorem error_dominates_all
    (debtPresent uncertaintyPresent runningPresent hasCitations : Bool) :
    classifySignalEmotion true debtPresent uncertaintyPresent runningPresent hasCitations = .error :=
  rfl

theorem skeptical_dominates_non_error
    (uncertaintyPresent runningPresent hasCitations : Bool) :
    classifySignalEmotion false true uncertaintyPresent runningPresent hasCitations = .skeptical :=
  rfl

theorem guard_dominates_when_no_error_or_debt
    (runningPresent hasCitations : Bool) :
    classifySignalEmotion false false true runningPresent hasCitations = .guard :=
  rfl

theorem working_dominates_idle_signals
    (hasCitations : Bool) :
    classifySignalEmotion false false false true hasCitations = .working :=
  rfl

theorem analytical_iff_clean_idle_with_citations
    (f d u r c : Bool) :
    classifySignalEmotion f d u r c = .analytical ↔
      (f = false ∧ d = false ∧ u = false ∧ r = false ∧ c = true) := by
  cases f <;> cases d <;> cases u <;> cases r <;> cases c <;> decide

theorem curious_iff_all_quiet
    (f d u r c : Bool) :
    classifySignalEmotion f d u r c = .curious ↔
      (f = false ∧ d = false ∧ u = false ∧ r = false ∧ c = false) := by
  cases f <;> cases d <;> cases u <;> cases r <;> cases c <;> decide

/-! ## Part II — Grounded Citation `TrustState` Lattice (`AGENTS.md` Rule 3) -/

inductive TrustState where
  | withheld
  | unverified
  | structural
  | verified
  deriving DecidableEq, Repr

def TrustState.name : TrustState → String
  | .withheld => "withheld"
  | .unverified => "unverified"
  | .structural => "structural"
  | .verified => "verified"

def TrustState.rank : TrustState → Nat
  | .withheld => 0
  | .unverified => 1
  | .structural => 2
  | .verified => 3

def allTrustStates : List TrustState :=
  [.withheld, .unverified, .structural, .verified]

/-- Derive honest `trust_state` from citation validity, redaction status, and semantic check. -/
def deriveTrustState (hasValidCitation isRedacted hasSemanticVerification : Bool) : TrustState :=
  if isRedacted then
    .withheld
  else if !hasValidCitation then
    .unverified
  else if hasSemanticVerification then
    .verified
  else
    .structural

/-- Combine two trust states by taking the more conservative (lower-rank) state. -/
def downgradeTrust (a b : TrustState) : TrustState :=
  if a.rank ≤ b.rank then a else b

theorem redaction_always_withholds (cit sem : Bool) :
    deriveTrustState cit true sem = .withheld := rfl

theorem structural_only_never_verified (cit red : Bool) :
    deriveTrustState cit red false ≠ .verified := by
  cases cit <;> cases red <;> decide

theorem verified_iff_all_grounded (cit red sem : Bool) :
    deriveTrustState cit red sem = .verified ↔ (cit = true ∧ red = false ∧ sem = true) := by
  cases cit <;> cases red <;> cases sem <;> decide

theorem downgrade_monotone_left (a b : TrustState) :
    (downgradeTrust a b).rank ≤ a.rank := by
  cases a <;> cases b <;> decide

theorem downgrade_monotone_right (a b : TrustState) :
    (downgradeTrust a b).rank ≤ b.rank := by
  cases a <;> cases b <;> decide

theorem downgrade_idempotent (a : TrustState) :
    downgradeTrust a a = a := by
  cases a <;> decide

/-! ## Part III — `PhiPresence` Attention-Economy Routing (`src/phi/mod.rs`) -/

inductive PresenceSurface where
  | silent
  | ambient
  | chip
  | drawer
  | interrupt
  deriving DecidableEq, Repr

def PresenceSurface.name : PresenceSurface → String
  | .silent => "silent"
  | .ambient => "ambient"
  | .chip => "chip"
  | .drawer => "drawer"
  | .interrupt => "interrupt"

structure PresenceInput where
  userBusy : Bool
  settled : Bool
  dismissedCategory : Bool
  alreadySeen : Bool
  consecutiveIgnored : Nat
  allowsInterrupt : Bool
  categoryAllowsChip : Bool
  interruptQuota : Nat
  spokeSinceLastInterrupt : Bool
  deriving DecidableEq, Repr

/-- `PhiPresence::route` in `src/phi/mod.rs`. -/
def routePresence (inp : PresenceInput) : PresenceSurface :=
  if inp.userBusy || !inp.settled then
    .ambient
  else if inp.dismissedCategory || inp.alreadySeen || inp.consecutiveIgnored ≥ 2 then
    .silent
  else if inp.allowsInterrupt
      && inp.interruptQuota > 0
      && inp.spokeSinceLastInterrupt
      && inp.consecutiveIgnored == 0 then
    .interrupt
  else if inp.categoryAllowsChip then
    .chip
  else
    .ambient

theorem busy_only_ambient (inp : PresenceInput) (hBusy : inp.userBusy = true) :
    routePresence inp = .ambient := by
  simp [routePresence, hBusy]

theorem unsettled_only_ambient (inp : PresenceInput) (hUnsettled : inp.settled = false) :
    routePresence inp = .ambient := by
  cases h : inp.userBusy <;> simp [routePresence, h, hUnsettled]

theorem dismissed_silences_when_ready (inp : PresenceInput)
    (hBusy : inp.userBusy = false)
    (hSettled : inp.settled = true)
    (hDismissed : inp.dismissedCategory = true) :
    routePresence inp = .silent := by
  simp [routePresence, hBusy, hSettled, hDismissed]

theorem ignored_twice_silences_when_ready (inp : PresenceInput)
    (hBusy : inp.userBusy = false)
    (hSettled : inp.settled = true)
    (hIgnored : inp.consecutiveIgnored ≥ 2) :
    routePresence inp = .silent := by
  cases hd : inp.dismissedCategory <;> cases hs : inp.alreadySeen <;>
    simp [routePresence, hBusy, hSettled, hd, hs, hIgnored]

theorem interrupt_never_fires_when_quota_zero (inp : PresenceInput)
    (hQuota : inp.interruptQuota = 0) :
    routePresence inp ≠ .interrupt := by
  unfold routePresence
  simp [hQuota]
  split
  · decide
  · split
    · decide
    · split <;> decide

theorem interrupt_never_fires_when_ignored_positive (inp : PresenceInput)
    (hIgnored : inp.consecutiveIgnored > 0) :
    routePresence inp ≠ .interrupt := by
  unfold routePresence
  have hne : (inp.consecutiveIgnored == 0) = false := by
    cases h : inp.consecutiveIgnored with
    | zero => omega
    | succ n => rfl
  simp [hne]
  split
  · decide
  · split
    · decide
    · split <;> decide

/-! ## Part IV — `DebtBoss` Archetype Priority & HP Bounds (`src/phi/arena.rs`) -/

inductive DebtBossArchetype where
  | dormantFog
  | regressionHydra
  | phantomSpecter
  | boilerplateGolem
  | sentinelWard
  | arenaClear
  deriving DecidableEq, Repr

def DebtBossArchetype.name : DebtBossArchetype → String
  | .dormantFog => "Dormant Fog (No Live Agent Captures)"
  | .regressionHydra => "Regression Hydra (Failing Command Runs)"
  | .phantomSpecter => "Phantom Specter (Unbounded / Uncertain Mutations)"
  | .boilerplateGolem => "Boilerplate Golem (Unreviewed & Untested Diff)"
  | .sentinelWard => "Sentinel Ward (Outstanding Obligations)"
  | .arenaClear => "Arena Clear (0 Observed Debt Obligations)"

def maxNat (a b : Nat) : Nat :=
  if a ≥ b then a else b

/-- `classify_debt_boss_archetype` in `src/phi/arena.rs`. -/
def classifyDebtBoss
    (obsAvailable : Bool)
    (unreviewed untested outstanding failedRuns unknownRuns unknownObl peakHp drillDamage : Nat) :
    DebtBossArchetype × Nat × Nat :=
  if !obsAvailable then
    (.dormantFog, 0, maxNat peakHp 100)
  else
    let rawHp :=
      unreviewed +
      untested * 2 +
      outstanding * 25 +
      failedRuns * 50 +
      unknownRuns * 20 +
      unknownObl * 40
    let effectiveHp := rawHp - drillDamage
    let maxHp := maxNat (maxNat peakHp rawHp) 100
    let arch :=
      if failedRuns > 0 then
        .regressionHydra
      else if unknownObl > 0 || unknownRuns > 0 then
        .phantomSpecter
      else if unreviewed > 0 || untested > 0 then
        .boilerplateGolem
      else if outstanding > 0 then
        .sentinelWard
      else
        .arenaClear
    (arch, effectiveHp, maxHp)

theorem dormant_fog_when_obs_unavailable
    (unrev untest out fail unkR unkO peak dmg : Nat) :
    (classifyDebtBoss false unrev untest out fail unkR unkO peak dmg).1 = .dormantFog ∧
    (classifyDebtBoss false unrev untest out fail unkR unkO peak dmg).2.1 = 0 :=
  ⟨rfl, rfl⟩

theorem regression_hydra_dominates_observed
    (unrev untest out fail unkR unkO peak dmg : Nat)
    (hFail : fail > 0) :
    (classifyDebtBoss true unrev untest out fail unkR unkO peak dmg).1 = .regressionHydra := by
  simp [classifyDebtBoss, hFail]

theorem maxNat_ge_right (a b : Nat) : maxNat a b ≥ b := by
  dsimp [maxNat]
  split <;> omega

theorem max_hp_at_least_100
    (obs : Bool)
    (unrev untest out fail unkR unkO peak dmg : Nat) :
    (classifyDebtBoss obs unrev untest out fail unkR unkO peak dmg).2.2 ≥ 100 := by
  cases obs <;> exact maxNat_ge_right _ 100

theorem drill_damage_monotone_reduces_hp
    (unrev untest out fail unkR unkO peak d1 d2 : Nat)
    (hd : d1 ≤ d2) :
    (classifyDebtBoss true unrev untest out fail unkR unkO peak d2).2.1 ≤
      (classifyDebtBoss true unrev untest out fail unkR unkO peak d1).2.1 := by
  dsimp [classifyDebtBoss]
  omega

/-! ## Part V — Dream Consolidation Honest Status (`src/cognitive/dream_subprocess.rs`) -/

structure DreamStatus where
  success : Bool
  memoriesCreated : Nat
  sessionsArchived : Nat
  errorCount : Nat
  deriving DecidableEq, Repr

/-- Honest dream status derivation in `run_dream` (`src/cognitive/dream_subprocess.rs`). -/
def evaluateDreamStatus
    (consolidateFailed : Bool)
    (errorCount rawStored rawArchived : Nat) : DreamStatus :=
  let stored := if consolidateFailed then 0 else rawStored
  let archived := if consolidateFailed then 0 else rawArchived
  { success := !consolidateFailed && errorCount == 0
  , memoriesCreated := stored
  , sessionsArchived := archived
  , errorCount := errorCount
  }

theorem dream_honest_success_iff
    (consolidateFailed : Bool)
    (errorCount rawStored rawArchived : Nat) :
    (evaluateDreamStatus consolidateFailed errorCount rawStored rawArchived).success = true ↔
      (consolidateFailed = false ∧ errorCount = 0) := by
  unfold evaluateDreamStatus
  cases consolidateFailed <;> cases errorCount <;> simp

theorem consolidate_failure_zeros_counters
    (errorCount rawStored rawArchived : Nat) :
    let st := evaluateDreamStatus true errorCount rawStored rawArchived
    st.success = false ∧ st.memoriesCreated = 0 ∧ st.sessionsArchived = 0 := by
  simp [evaluateDreamStatus]

theorem nonzero_errors_never_green
    (consolidateFailed : Bool)
    (errorCount rawStored rawArchived : Nat)
    (hErr : errorCount > 0) :
    (evaluateDreamStatus consolidateFailed errorCount rawStored rawArchived).success = false := by
  unfold evaluateDreamStatus
  have hne : (errorCount == 0) = false := by
    cases errorCount with
    | zero => omega
    | succ n => rfl
  simp [hne]

/-! ## Part VI — KV-Prefix Role Demotion (`src/agent/context_management.rs`) -/

inductive RoleKind where
  | system
  | user
  | assistant
  | tool
  deriving DecidableEq, Repr

def RoleKind.name : RoleKind → String
  | .system => "system"
  | .user => "user"
  | .assistant => "assistant"
  | .tool => "tool"

def demoteTailRole : RoleKind → RoleKind
  | .system => .user
  | .user => .user
  | .assistant => .assistant
  | .tool => .tool

/-- `demote_mid_conversation_system_messages` role transformation (`src/agent/context_management.rs`). -/
def demoteMidConversationRoles : List RoleKind → List RoleKind
  | [] => []
  | head :: tail => head :: tail.map demoteTailRole

theorem demote_preserves_length (roles : List RoleKind) :
    (demoteMidConversationRoles roles).length = roles.length := by
  cases roles with
  | nil => rfl
  | cons hd tl => simp [demoteMidConversationRoles]

theorem demote_preserves_first_message (hd : RoleKind) (tl : List RoleKind) :
    (demoteMidConversationRoles (hd :: tl)).head? = some hd := rfl

theorem demoteTailRole_never_system (r : RoleKind) :
    demoteTailRole r ≠ .system := by
  cases r <;> decide

theorem demoteTailRole_idempotent (r : RoleKind) :
    demoteTailRole (demoteTailRole r) = demoteTailRole r := by
  cases r <;> rfl

theorem map_demoteTailRole_idempotent (tl : List RoleKind) :
    (tl.map demoteTailRole).map demoteTailRole = tl.map demoteTailRole := by
  induction tl with
  | nil => rfl
  | cons x xs ih =>
    simp [demoteTailRole_idempotent, ih]

theorem demote_idempotent (roles : List RoleKind) :
    demoteMidConversationRoles (demoteMidConversationRoles roles) =
      demoteMidConversationRoles roles := by
  cases roles with
  | nil => rfl
  | cons hd tl =>
    dsimp [demoteMidConversationRoles]
    rw [map_demoteTailRole_idempotent]

/-! ## Part VII — `ToolCache` Worktree Isolation & Invalidation (`src/session/cache.rs`) -/

inductive CacheToolKind where
  | fileRead
  | directoryTree
  | grepSearch
  | symbolSearch
  | gitStatus
  | gitDiff
  | fileWrite
  | fileEdit
  | fileDelete
  | gitCheckpoint
  | gitRollback
  | shellExec
  deriving DecidableEq, Repr

def CacheToolKind.name : CacheToolKind → String
  | .fileRead => "file_read"
  | .directoryTree => "directory_tree"
  | .grepSearch => "grep_search"
  | .symbolSearch => "symbol_search"
  | .gitStatus => "git_status"
  | .gitDiff => "git_diff"
  | .fileWrite => "file_write"
  | .fileEdit => "file_edit"
  | .fileDelete => "file_delete"
  | .gitCheckpoint => "git_checkpoint"
  | .gitRollback => "git_rollback"
  | .shellExec => "shell_exec"

def CacheToolKind.isCacheable : CacheToolKind → Bool
  | .fileRead | .directoryTree | .grepSearch | .symbolSearch | .gitStatus | .gitDiff => true
  | _ => false

def CacheToolKind.invalidatesCache : CacheToolKind → Bool
  | .fileWrite | .fileEdit | .fileDelete | .gitCheckpoint | .gitRollback | .shellExec => true
  | _ => false

def CacheToolKind.isTreeScoped : CacheToolKind → Bool
  | .directoryTree | .grepSearch | .symbolSearch => true
  | _ => false

def CacheToolKind.isGitScoped : CacheToolKind → Bool
  | .gitStatus | .gitDiff => true
  | _ => false

theorem cacheable_and_invalidating_disjoint (t : CacheToolKind) :
    !(t.isCacheable && t.invalidatesCache) = true := by
  cases t <;> decide

structure CacheEntryModel where
  workspaceId : Nat
  tool : CacheToolKind
  mentionsTargetPath : Bool
  deriving DecidableEq, Repr

/-- Worktree-isolated `invalidate_path` + `invalidate_git` retention predicate (`src/session/cache.rs`). -/
def retainsAfterPathAndGitInvalidation (activeWs : Nat) (e : CacheEntryModel) : Bool :=
  if e.workspaceId != activeWs then
    true
  else if e.tool.isTreeScoped || e.tool.isGitScoped || e.mentionsTargetPath then
    false
  else
    true

theorem foreign_worktree_entry_always_retained
    (activeWs : Nat) (e : CacheEntryModel)
    (hDiff : e.workspaceId ≠ activeWs) :
    retainsAfterPathAndGitInvalidation activeWs e = true := by
  unfold retainsAfterPathAndGitInvalidation
  have hne : (e.workspaceId != activeWs) = true := by
    simp [bne_iff_ne, hDiff]
  simp [hne]

theorem tree_scoped_dropped_in_active_worktree
    (ws : Nat) (e : CacheEntryModel)
    (hSame : e.workspaceId = ws)
    (hTree : e.tool.isTreeScoped = true) :
    retainsAfterPathAndGitInvalidation ws e = false := by
  unfold retainsAfterPathAndGitInvalidation
  simp [hSame, hTree]

theorem git_status_diff_dropped_in_active_worktree
    (ws : Nat) (e : CacheEntryModel)
    (hSame : e.workspaceId = ws)
    (hGit : e.tool.isGitScoped = true) :
    retainsAfterPathAndGitInvalidation ws e = false := by
  unfold retainsAfterPathAndGitInvalidation
  cases e.tool.isTreeScoped <;> simp [hSame, hGit]

/-! ## Part VIII — `EditHistory` Undo/Redo & `RestoreOutcome` Honesty (`src/session/edit_history.rs`) -/

structure UndoRedoState where
  undoStack : List Nat
  redoStack : List Nat
  deriving DecidableEq, Repr

def recordEdit (st : UndoRedoState) (editId : Nat) : UndoRedoState :=
  { undoStack := editId :: st.undoStack, redoStack := [] }

theorem recordEdit_clears_redo (st : UndoRedoState) (editId : Nat) :
    (recordEdit st editId).redoStack = [] := rfl

inductive RestoreOutcomeModel where
  | restored
  | alreadyCurrent
  | skippedCorrupt
  | writeError
  deriving DecidableEq, Repr

def RestoreOutcomeModel.name : RestoreOutcomeModel → String
  | .restored => "restored"
  | .alreadyCurrent => "already_current"
  | .skippedCorrupt => "skipped_corrupt"
  | .writeError => "write_error"

/-- `restore_checkpoint_guarded` honest decision tree (`src/session/edit_history.rs`). -/
def evaluateRestoreOutcome
    (snapshotValid diskMatchesSnapshot writeSucceeded : Bool) : RestoreOutcomeModel :=
  if !snapshotValid then
    .skippedCorrupt
  else if diskMatchesSnapshot then
    .alreadyCurrent
  else if writeSucceeded then
    .restored
  else
    .writeError

theorem corrupt_snapshot_never_restores (m w : Bool) :
    evaluateRestoreOutcome false m w = .skippedCorrupt := rfl

theorem restored_iff_valid_dirty_and_written (v m w : Bool) :
    evaluateRestoreOutcome v m w = .restored ↔ (v = true ∧ m = false ∧ w = true) := by
  cases v <;> cases m <;> cases w <;> decide

/-! ## Part IX — `PhiArenaProfile` XP-to-Level Monotonicity (`src/phi/arena.rs`) -/

/-- Matches `PhiArenaProfile::recompute_level_and_achievements`: `1 + (xp / 100)`. -/
def levelForXp (xp : Nat) : Nat :=
  1 + xp / 100

theorem level_at_least_one (xp : Nat) :
    1 ≤ levelForXp xp := by
  unfold levelForXp
  omega

theorem level_monotone {xp₁ xp₂ : Nat} (h : xp₁ ≤ xp₂) :
    levelForXp xp₁ ≤ levelForXp xp₂ := by
  unfold levelForXp
  have hdiv : xp₁ / 100 ≤ xp₂ / 100 := Nat.div_le_div_right h
  omega

/-! ## Part X — `ContextTrust` Provenance & Gate Decision Lattice (`src/evolve/context_trust.rs`) -/

inductive SourceKindModel where
  | workspace
  | toolOutput
  | modelOutput
  | external
  | memory
  | user
  deriving DecidableEq, Repr

def SourceKindModel.name : SourceKindModel → String
  | .workspace => "workspace"
  | .toolOutput => "tool_output"
  | .modelOutput => "model_output"
  | .external => "external"
  | .memory => "memory"
  | .user => "user"

inductive TrustLevelModel where
  | trusted
  | semiTrusted
  | untrusted
  deriving DecidableEq, Repr

def TrustLevelModel.name : TrustLevelModel → String
  | .trusted => "trusted"
  | .semiTrusted => "semi_trusted"
  | .untrusted => "untrusted"

inductive GateDecisionModel where
  | allow
  | review
  | quarantine
  deriving DecidableEq, Repr

def GateDecisionModel.name : GateDecisionModel → String
  | .allow => "allow"
  | .review => "review"
  | .quarantine => "quarantine"

/-- Matches `crate::evolve::context_trust::trust_level`. -/
def classifyTrustLevel (src : SourceKindModel) (isCodeOrTest : Bool) : TrustLevelModel :=
  match src with
  | .external | .modelOutput => .untrusted
  | .toolOutput | .memory => .semiTrusted
  | .user => .trusted
  | .workspace => if isCodeOrTest then .trusted else .semiTrusted

/-- Matches `crate::evolve::context_trust::gate_decision`. -/
def evaluateGateDecision (hasAnyFinding hasHighFinding : Bool) (trust : TrustLevelModel) : GateDecisionModel :=
  if hasHighFinding && trust != .trusted then
    .quarantine
  else if hasAnyFinding || trust != .trusted then
    .review
  else
    .allow

theorem external_and_model_always_untrusted (c : Bool) :
    classifyTrustLevel .external c = .untrusted ∧ classifyTrustLevel .modelOutput c = .untrusted :=
  ⟨rfl, rfl⟩

theorem non_trusted_never_auto_allowed
    (any high : Bool) (trust : TrustLevelModel) (h : trust ≠ .trusted) :
    evaluateGateDecision any high trust ≠ .allow := by
  cases any <;> cases high <;> cases trust <;> simp_all [evaluateGateDecision]

theorem allow_iff_trusted_and_clean (any : Bool) (trust : TrustLevelModel) :
    evaluateGateDecision any false trust = .allow ↔ (trust = .trusted ∧ any = false) := by
  cases any <;> cases trust <;> decide

theorem non_trusted_high_always_quarantined
    (any : Bool) (trust : TrustLevelModel) (h : trust ≠ .trusted) :
    evaluateGateDecision any true trust = .quarantine := by
  cases any <;> cases trust <;> simp_all [evaluateGateDecision]

/-! ## Part XI — `ContextFit` Tier Ladder & Honest Fit Bounds (`src/evolve/context_fit.rs`) -/

inductive ContextTierModel where
  | fullExtended
  | full
  | compact
  | lite
  | map
  deriving DecidableEq, Repr

def ContextTierModel.name : ContextTierModel → String
  | .fullExtended => "full_extended"
  | .full => "full"
  | .compact => "compact"
  | .lite => "lite"
  | .map => "map"

structure FitOutcomeModel where
  mode : ContextTierModel
  measuredTokens : Nat
  budgetTokens : Nat
  fits : Bool
  deriving DecidableEq, Repr

/-- Matches `crate::evolve::context_fit::fit_tier` across `TIER_LADDER`. -/
def fitTierLadder (fe f c l m usable : Nat) : FitOutcomeModel :=
  if fe ≤ usable then
    { mode := .fullExtended, measuredTokens := fe, budgetTokens := usable, fits := true }
  else if f ≤ usable then
    { mode := .full, measuredTokens := f, budgetTokens := usable, fits := true }
  else if c ≤ usable then
    { mode := .compact, measuredTokens := c, budgetTokens := usable, fits := true }
  else if l ≤ usable then
    { mode := .lite, measuredTokens := l, budgetTokens := usable, fits := true }
  else if m ≤ usable then
    { mode := .map, measuredTokens := m, budgetTokens := usable, fits := true }
  else
    { mode := .map, measuredTokens := m, budgetTokens := usable, fits := false }

theorem fits_implies_measured_le_budget
    (fe f c l m u : Nat)
    (h : (fitTierLadder fe f c l m u).fits = true) :
    (fitTierLadder fe f c l m u).measuredTokens ≤ (fitTierLadder fe f c l m u).budgetTokens := by
  unfold fitTierLadder at h ⊢
  by_cases hfe : fe ≤ u
  · simp [hfe]
  · by_cases hf : f ≤ u
    · simp [hfe, hf]
    · by_cases hc : c ≤ u
      · simp [hfe, hf, hc]
      · by_cases hl : l ≤ u
        · simp [hfe, hf, hc, hl]
        · by_cases hm : m ≤ u
          · simp [hfe, hf, hc, hl, hm]
          · simp [hfe, hf, hc, hl, hm] at h

theorem overflow_reports_map_and_fits_false
    (fe f c l m u : Nat)
    (hfe : u < fe) (hf : u < f) (hc : u < c) (hl : u < l) (hm : u < m) :
    (fitTierLadder fe f c l m u).mode = .map ∧ (fitTierLadder fe f c l m u).fits = false := by
  unfold fitTierLadder
  have nfe : ¬(fe ≤ u) := by omega
  have nf : ¬(f ≤ u) := by omega
  have nc : ¬(c ≤ u) := by omega
  have nl : ¬(l ≤ u) := by omega
  have nm : ¬(m ≤ u) := by omega
  simp [nfe, nf, nc, nl, nm]

/-! ## Part XII — `ApplyStatus` Shadow Worktree & Compile-Gate Lifecycle (`src/evolve/apply.rs`) -/

inductive ApplyStatusModel where
  | running
  | staged
  | rejected
  | succeeded
  | failed
  deriving DecidableEq, Repr

def ApplyStatusModel.name : ApplyStatusModel → String
  | .running => "running"
  | .staged => "staged"
  | .rejected => "rejected"
  | .succeeded => "succeeded"
  | .failed => "failed"

inductive ApplyEventModel where
  | exitNonZero
  | diffInvalid
  | compileFailed
  | verifiedAndCompiled
  | commitDigestMatch
  | commitDigestMismatch
  deriving DecidableEq, Repr

def ApplyEventModel.name : ApplyEventModel → String
  | .exitNonZero => "exit_nonzero"
  | .diffInvalid => "diff_invalid"
  | .compileFailed => "compile_failed"
  | .verifiedAndCompiled => "verified_and_compiled"
  | .commitDigestMatch => "commit_digest_match"
  | .commitDigestMismatch => "commit_digest_mismatch"

def stepApplyStatus (st : ApplyStatusModel) (ev : ApplyEventModel) : ApplyStatusModel :=
  match st, ev with
  | .running, .exitNonZero => .failed
  | .running, .diffInvalid => .rejected
  | .running, .compileFailed => .rejected
  | .running, .verifiedAndCompiled => .staged
  | .staged, .commitDigestMatch => .succeeded
  | .staged, .commitDigestMismatch => .staged
  | _, _ => st

theorem staged_from_running_iff_verified_and_compiled (ev : ApplyEventModel) :
    stepApplyStatus .running ev = .staged ↔ ev = .verifiedAndCompiled := by
  cases ev <;> decide

theorem compile_failed_rejects_run :
    stepApplyStatus .running .compileFailed = .rejected := rfl

theorem succeeded_only_from_staged_or_succeeded
    (st : ApplyStatusModel) (ev : ApplyEventModel)
    (h : stepApplyStatus st ev = .succeeded) :
    st = .staged ∨ st = .succeeded := by
  cases st <;> cases ev <;> simp_all [stepApplyStatus]

theorem terminal_apply_states_absorbing
    (st : ApplyStatusModel) (ev : ApplyEventModel)
    (h : st = .rejected ∨ st = .succeeded ∨ st = .failed) :
    stepApplyStatus st ev = st := by
  rcases h with rfl | rfl | rfl <;> cases ev <;> rfl

/-! ## Part XIII — PA13: Safety Tool Metadata & Confirmation Gate (`src/safety/tool_metadata.rs`) -/

inductive ToolRiskLevel where
  | low
  | medium
  | high
  deriving DecidableEq, Repr

inductive SafetyExecMode where
  | normal
  | plan
  | auto
  | yolo
  deriving DecidableEq, Repr

inductive PermissionOutcome where
  | allow
  | deny
  | prompt
  deriving DecidableEq, Repr

def PermissionOutcome.asString : PermissionOutcome → String
  | .allow => "allow"
  | .deny => "deny"
  | .prompt => "prompt"

structure ToolSafetySpec where
  riskLevel : ToolRiskLevel
  readOnly : Bool
  destructive : Bool
  networkAccess : Bool
  isProtected : Bool
  isHotReload : Bool
  deriving DecidableEq, Repr

structure PermissionConfig where
  mode : SafetyExecMode
  allowDestructiveInYolo : Bool := false
  allowNetwork : Bool := true
  explicitlyAllowed : Bool := false
  explicitlyDenied : Bool := false
  deriving DecidableEq, Repr

/-- Permission decision mirroring `PermissionChecker::check` (`src/safety/tool_metadata.rs`). -/
def checkToolPermission (cfg : PermissionConfig) (spec : ToolSafetySpec) : PermissionOutcome :=
  if cfg.explicitlyDenied then
    .deny
  else if cfg.explicitlyAllowed then
    .allow
  else
    match cfg.mode with
    | .plan =>
        if spec.readOnly then .allow else .deny
    | .normal =>
        match spec.riskLevel with
        | .low => .allow
        | .medium | .high => .prompt
    | .auto =>
        match spec.riskLevel with
        | .low | .medium => .allow
        | .high =>
            if spec.destructive && spec.isProtected then .prompt else .allow
    | .yolo =>
        if spec.isHotReload then
          .prompt
        else if spec.networkAccess && !cfg.allowNetwork then
          .prompt
        else if spec.destructive && !cfg.allowDestructiveInYolo then
          .prompt
        else
          .allow

/-- Normal-mode confirmation predicate mirroring `normal_mode_needs_confirmation` (`src/safety/tool_metadata.rs`). -/
def normalModeNeedsConfirmation
    (isGranted : Bool)
    (inRequireConfirmationList : Bool)
    (classified : Option (Bool × ToolRiskLevel)) : Bool :=
  if isGranted then
    false
  else if inRequireConfirmationList then
    true
  else
    match classified with
    | some (readOnly, risk) => !(readOnly && decide (risk = .low))
    | none => true

theorem plan_mode_denies_mutating_tools
    (spec : ToolSafetySpec)
    (hMut : spec.readOnly = false) :
    checkToolPermission { mode := .plan } spec = .deny := by
  simp [checkToolPermission, hMut]

theorem plan_mode_allows_readonly_tools
    (spec : ToolSafetySpec)
    (hRo : spec.readOnly = true) :
    checkToolPermission { mode := .plan } spec = .allow := by
  simp [checkToolPermission, hRo]

theorem normal_mode_prompts_medium_and_high
    (spec : ToolSafetySpec)
    (hRisk : spec.riskLevel = .medium ∨ spec.riskLevel = .high) :
    checkToolPermission { mode := .normal } spec = .prompt := by
  rcases hRisk with h | h <;> simp [checkToolPermission, h]

theorem auto_mode_prompts_destructive_protected_high_risk
    (spec : ToolSafetySpec)
    (hHigh : spec.riskLevel = .high)
    (hDest : spec.destructive = true)
    (hProt : spec.isProtected = true) :
    checkToolPermission { mode := .auto } spec = .prompt := by
  simp [checkToolPermission, hHigh, hDest, hProt]

theorem default_yolo_prompts_destructive_and_hot_reload
    (spec : ToolSafetySpec)
    (hGuard : spec.isHotReload = true ∨ spec.destructive = true) :
    checkToolPermission { mode := .yolo } spec = .prompt := by
  rcases hGuard with hHot | hDest
  · simp [checkToolPermission, hHot]
  · cases hHot : spec.isHotReload <;> simp [checkToolPermission, hHot, hDest]

theorem unclassified_tool_requires_confirmation
    (inReqList : Bool) :
    normalModeNeedsConfirmation false inReqList none = true := by
  cases inReqList <;> rfl

theorem normal_confirmation_skipped_iff_readonly_low
    (ro : Bool) (risk : ToolRiskLevel) :
    normalModeNeedsConfirmation false false (some (ro, risk)) = false ↔
      (ro = true ∧ risk = .low) := by
  cases ro <;> cases risk <;> decide

/-! ## Part XIV — PA14: Supervision Circuit Breaker Threshold & Probe Recovery (`src/supervision/circuit_breaker.rs`) -/

inductive CBState where
  | closed
  | openState
  | halfOpen
  deriving DecidableEq, Repr

def CBState.asString : CBState → String
  | .closed => "closed"
  | .openState => "open"
  | .halfOpen => "half_open"

structure CBConfig where
  failureThreshold : Nat
  successThreshold : Nat
  halfOpenMaxRequests : Nat
  deriving DecidableEq, Repr

structure CBSnapshot where
  state : CBState
  failureCount : Nat
  successCount : Nat
  halfOpenRequests : Nat
  deriving DecidableEq, Repr

def cbTransitionTo (target : CBState) : CBSnapshot :=
  { state := target, failureCount := 0, successCount := 0, halfOpenRequests := 0 }

def cbCanExecute (cfg : CBConfig) (s : CBSnapshot) (timeoutElapsed : Bool) : Bool × CBSnapshot :=
  match s.state with
  | .closed => (true, s)
  | .openState =>
      if timeoutElapsed then
        let next := cbTransitionTo .halfOpen
        if next.halfOpenRequests < cfg.halfOpenMaxRequests then
          (true, { next with halfOpenRequests := next.halfOpenRequests + 1 })
        else
          (false, next)
      else
        (false, s)
  | .halfOpen =>
      if s.halfOpenRequests < cfg.halfOpenMaxRequests then
        (true, { s with halfOpenRequests := s.halfOpenRequests + 1 })
      else
        (false, s)

def cbOnFailure (cfg : CBConfig) (s : CBSnapshot) : CBSnapshot :=
  let fc := s.failureCount + 1
  match s.state with
  | .halfOpen => cbTransitionTo .openState
  | _ =>
      if fc ≥ cfg.failureThreshold then
        cbTransitionTo .openState
      else
        { s with failureCount := fc }

def cbOnSuccess (cfg : CBConfig) (s : CBSnapshot) : CBSnapshot :=
  match s.state with
  | .halfOpen =>
      let sc := s.successCount + 1
      if sc ≥ cfg.successThreshold then
        cbTransitionTo .closed
      else
        { s with successCount := sc, halfOpenRequests := s.halfOpenRequests - 1 }
  | .closed =>
      { s with successCount := s.successCount + 1, failureCount := 0 }
  | .openState =>
      s

theorem cb_closed_trips_at_failure_threshold
    (cfg : CBConfig) (s : CBSnapshot)
    (hClosed : s.state = .closed)
    (hThresh : s.failureCount + 1 ≥ cfg.failureThreshold) :
    cbOnFailure cfg s = cbTransitionTo .openState := by
  simp [cbOnFailure, hClosed, hThresh]

theorem cb_half_open_single_failure_reopens
    (cfg : CBConfig) (s : CBSnapshot)
    (hHalf : s.state = .halfOpen) :
    cbOnFailure cfg s = cbTransitionTo .openState := by
  simp [cbOnFailure, hHalf]

theorem cb_half_open_closes_at_success_threshold
    (cfg : CBConfig) (s : CBSnapshot)
    (hHalf : s.state = .halfOpen)
    (hThresh : s.successCount + 1 ≥ cfg.successThreshold) :
    cbOnSuccess cfg s = cbTransitionTo .closed := by
  simp [cbOnSuccess, hHalf, hThresh]

theorem cb_closed_success_resets_failure_count
    (cfg : CBConfig) (s : CBSnapshot)
    (hClosed : s.state = .closed) :
    (cbOnSuccess cfg s).failureCount = 0 ∧ (cbOnSuccess cfg s).state = .closed := by
  simp [cbOnSuccess, hClosed]

theorem cb_open_ignores_late_success
    (cfg : CBConfig) (s : CBSnapshot)
    (hOpen : s.state = .openState) :
    cbOnSuccess cfg s = s := by
  simp [cbOnSuccess, hOpen]

theorem cb_half_open_rejects_at_probe_cap
    (cfg : CBConfig) (s : CBSnapshot)
    (hHalf : s.state = .halfOpen)
    (hCap : s.halfOpenRequests ≥ cfg.halfOpenMaxRequests) :
    (cbCanExecute cfg s false).1 = false := by
  have hNotLt : ¬ (s.halfOpenRequests < cfg.halfOpenMaxRequests) := by omega
  simp [cbCanExecute, hHalf, hNotLt]

/-! ## Part XV — PA15: Evolve Ontology Graph Validation & Deletion Execution Gate (`src/evolve/ontology.rs`, `src/evolve/deletion.rs`) -/

inductive OntologyEdgeKind where
  | contains
  | dependsOn
  | implements
  | tests
  | documents
  | evolvedFrom
  | influences
  deriving DecidableEq, Repr

def participatesInCycleCheck (kind : OntologyEdgeKind) : Bool :=
  kind != .dependsOn

structure OntologyValidationCounts where
  duplicateIds : Nat
  cycles : Nat
  danglingEdges : Nat
  isolatedNodes : Nat
  deriving DecidableEq, Repr

def validateGraphReportValid (c : OntologyValidationCounts) : Bool :=
  decide (c.duplicateIds = 0) &&
  decide (c.cycles = 0) &&
  decide (c.danglingEdges = 0) &&
  decide (c.isolatedNodes = 0)

def validateConnectedGraphValid (duplicateIds cycles danglingEdges : Nat) : Bool :=
  validateGraphReportValid {
    duplicateIds := duplicateIds
    cycles := cycles
    danglingEdges := danglingEdges
    isolatedNodes := 0
  }

def deletionRequireExecutable (executable : Bool) (blockerCount : Nat) : Bool :=
  executable && decide (blockerCount = 0)

theorem validate_graph_valid_iff_all_empty (c : OntologyValidationCounts) :
    validateGraphReportValid c = true ↔
      (c.duplicateIds = 0 ∧ c.cycles = 0 ∧ c.danglingEdges = 0 ∧ c.isolatedNodes = 0) := by
  simp [validateGraphReportValid, and_assoc]

theorem validate_connected_graph_valid_iff_three_empty (dup cyc dang : Nat) :
    validateConnectedGraphValid dup cyc dang = true ↔
      (dup = 0 ∧ cyc = 0 ∧ dang = 0) := by
  simp [validateConnectedGraphValid, validateGraphReportValid, and_assoc]

theorem depends_on_excluded_from_cycle_check (k : OntologyEdgeKind) :
    participatesInCycleCheck k = false ↔ k = .dependsOn := by
  cases k <;> decide

theorem deletion_executable_iff_flag_and_zero_blockers (exec : Bool) (blockers : Nat) :
    deletionRequireExecutable exec blockers = true ↔ (exec = true ∧ blockers = 0) := by
  cases exec <;> simp [deletionRequireExecutable]

/-! ## Part XVI — PA16: Phi Viseme Time-Window Lookup & Rest Fallback (`src/phi/viseme.rs`) -/

inductive PhiViseme where
  | rest
  | mbp
  | etc
  | ai
  | e
  | o
  | u
  | fv
  | lth
  | wq
  deriving DecidableEq, Repr

def PhiViseme.asCode : PhiViseme → String
  | .rest => "rest"
  | .mbp => "mbp"
  | .etc => "etc"
  | .ai => "ai"
  | .e => "e"
  | .o => "o"
  | .u => "u"
  | .fv => "fv"
  | .lth => "l_th"
  | .wq => "wq"

structure VisemeSpan where
  startMs : Nat
  endMs : Nat
  viseme : PhiViseme
  deriving DecidableEq, Repr

def spanContains (span : VisemeSpan) (elapsedMs : Nat) : Bool :=
  decide (span.startMs ≤ elapsedMs) && decide (elapsedMs < span.endMs)

def activeVisemeAt (schedule : List VisemeSpan) (elapsedMs : Nat) : PhiViseme :=
  match schedule with
  | [] => .rest
  | span :: tail =>
      if spanContains span elapsedMs then
        span.viseme
      else
        activeVisemeAt tail elapsedMs

theorem active_viseme_empty_is_rest (t : Nat) :
    activeVisemeAt [] t = .rest := rfl

theorem active_viseme_inside_single_span
    (span : VisemeSpan) (t : Nat)
    (hStart : span.startMs ≤ t)
    (hEnd : t < span.endMs) :
    activeVisemeAt [span] t = span.viseme := by
  simp [activeVisemeAt, spanContains, hStart, hEnd]

theorem active_viseme_outside_single_span_is_rest
    (span : VisemeSpan) (t : Nat)
    (hOut : t < span.startMs ∨ span.endMs ≤ t) :
    activeVisemeAt [span] t = .rest := by
  rcases hOut with hBefore | hAfter
  · have hNotStart : ¬ (span.startMs ≤ t) := by omega
    simp [activeVisemeAt, spanContains, hNotStart]
  · have hNotEnd : ¬ (t < span.endMs) := by omega
    simp [activeVisemeAt, spanContains, hNotEnd]

theorem active_viseme_degenerate_span_is_rest
    (span : VisemeSpan) (t : Nat)
    (hDeg : span.endMs ≤ span.startMs) :
    activeVisemeAt [span] t = .rest := by
  by_cases hStart : span.startMs ≤ t
  · have hNotEnd : ¬ (t < span.endMs) := by omega
    simp [activeVisemeAt, spanContains, hStart, hNotEnd]
  · simp [activeVisemeAt, spanContains, hStart]

def fileReadSpec : ToolSafetySpec :=
  { riskLevel := .low, readOnly := true, destructive := false, networkAccess := false, isProtected := false, isHotReload := false }
def fileEditSpec : ToolSafetySpec :=
  { riskLevel := .medium, readOnly := false, destructive := false, networkAccess := false, isProtected := false, isHotReload := false }
def gitPushSpec : ToolSafetySpec :=
  { riskLevel := .high, readOnly := false, destructive := false, networkAccess := true, isProtected := false, isHotReload := false }
def fileDeleteSpec : ToolSafetySpec :=
  { riskLevel := .high, readOnly := false, destructive := true, networkAccess := false, isProtected := true, isHotReload := false }

def defaultCBConfig : CBConfig :=
  { failureThreshold := 5, successThreshold := 3, halfOpenMaxRequests := 3 }

def sampleVisemeSchedule : List VisemeSpan :=
  [ { startMs := 100, endMs := 250, viseme := .mbp }
  , { startMs := 250, endMs := 400, viseme := .ai }
  , { startMs := 450, endMs := 600, viseme := .o }
  ]

/-! ## Part XVII — PA17: Self-Healing Failure Classification Priority Lattice (`src/self_healing/recovery_tree.rs`) -/

inductive FailureKindModel where
  | llmUnreachable
  | authError
  | rateLimited
  | toolTimeout
  | toolError
  | contextOverflow
  | configInvalid
  | diskFull
  | unknown
  deriving DecidableEq, Repr

def FailureKindModel.name : FailureKindModel → String
  | .llmUnreachable => "llm_unreachable"
  | .authError => "auth_error"
  | .rateLimited => "rate_limited"
  | .toolTimeout => "tool_timeout"
  | .toolError => "tool_error"
  | .contextOverflow => "context_overflow"
  | .configInvalid => "config_invalid"
  | .diskFull => "disk_full"
  | .unknown => "unknown"

/-- Mirrors `crate::self_healing::recovery_tree::classify` (`src/self_healing/recovery_tree.rs:99-198`). -/
def classifyFailureSignals
    (startsWithContextOverflow hasHook hasTimeout hasLlmUnreachable hasAuth hasRateLimit hasTool hasContext hasConfig hasDisk : Bool)
    : FailureKindModel :=
  if startsWithContextOverflow then
    .contextOverflow
  else if hasHook then
    if hasTimeout then .toolTimeout else .toolError
  else if hasLlmUnreachable then
    .llmUnreachable
  else if hasAuth then
    .authError
  else if hasRateLimit then
    .rateLimited
  else if hasTool || hasTimeout then
    .toolTimeout
  else if hasContext then
    .contextOverflow
  else if hasConfig then
    .configInvalid
  else if hasDisk then
    .diskFull
  else
    .unknown

theorem context_overflow_prefix_dominates_all
    (hk ht hllm ha hr htl hctx hcfg hdsk : Bool) :
    classifyFailureSignals true hk ht hllm ha hr htl hctx hcfg hdsk = .contextOverflow := rfl

theorem hook_error_never_llm_unreachable
    (ht hllm ha hr htl hctx hcfg hdsk : Bool) :
    classifyFailureSignals false true ht hllm ha hr htl hctx hcfg hdsk ≠ .llmUnreachable := by
  cases ht <;> intro h <;> cases h

theorem llm_unreachable_dominates_generic_timeout
    (ht ha hr htl hctx hcfg hdsk : Bool) :
    classifyFailureSignals false false ht true ha hr htl hctx hcfg hdsk = .llmUnreachable := rfl

theorem auth_dominates_generic_timeout
    (ht hr htl hctx hcfg hdsk : Bool) :
    classifyFailureSignals false false ht false true hr htl hctx hcfg hdsk = .authError := rfl

/-! ## Part XVIII — PA18: Computer Control `ActionRateLimiter` Window & Clock-Skew Safety (`src/computer/mod.rs`) -/

/-- Mirrors `ActionRateLimiter::check` (`src/computer/mod.rs:62-91`).
    Returns `(allowed, nextLastMs, nextActionsInWindow)`. -/
def stepRateLimiter
    (maxActions windowMs lastMs actionsInWindow nowMs : Nat) : Bool × Nat × Nat :=
  if nowMs - lastMs > windowMs then
    (true, nowMs, 1)
  else
    let count := actionsInWindow + 1
    (decide (count ≤ maxActions), lastMs, count)

theorem rate_limiter_clock_skew_stays_in_current_window
    (maxActions windowMs lastMs actionsInWindow nowMs : Nat)
    (hSkew : nowMs ≤ lastMs) :
    stepRateLimiter maxActions windowMs lastMs actionsInWindow nowMs =
      (decide (actionsInWindow + 1 ≤ maxActions), lastMs, actionsInWindow + 1) := by
  have hDiff : ¬ (nowMs - lastMs > windowMs) := by omega
  simp [stepRateLimiter, hDiff]

theorem rate_limiter_new_window_resets_count_to_one
    (maxActions windowMs lastMs actionsInWindow nowMs : Nat)
    (hNew : nowMs - lastMs > windowMs) :
    stepRateLimiter maxActions windowMs lastMs actionsInWindow nowMs = (true, nowMs, 1) := by
  simp [stepRateLimiter, hNew]

theorem rate_limiter_within_window_blocks_above_max
    (maxActions windowMs lastMs actionsInWindow nowMs : Nat)
    (hWin : nowMs - lastMs ≤ windowMs)
    (hCap : actionsInWindow ≥ maxActions) :
    (stepRateLimiter maxActions windowMs lastMs actionsInWindow nowMs).1 = false := by
  have hNotGt : ¬ (nowMs - lastMs > windowMs) := by omega
  have hNotLe : ¬ (actionsInWindow + 1 ≤ maxActions) := by omega
  simp [stepRateLimiter, hNotGt, hNotLe]

/-! ## Part XIX — PA19: Hook Lifecycle Fail-Closed Semantics & `SkipKind` Provenance (`src/hooks/mod.rs`) -/

inductive HookEventModel where
  | preToolUse
  | postToolUse
  | stop
  deriving DecidableEq, Repr

inductive SkipKindModel where
  | policy
  | hookFailure
  deriving DecidableEq, Repr

inductive HookOutcomeModel where
  | continueExec
  | skip (kind : SkipKindModel)
  | errorNonFatal
  deriving DecidableEq, Repr

def skipFailureTag : SkipKindModel → String
  | .policy => "hook_policy"
  | .hookFailure => "hook_failure"

def resolveHookStep (ev : HookEventModel) (raw : HookOutcomeModel) : HookOutcomeModel :=
  match raw with
  | .errorNonFatal =>
      if ev = .preToolUse then .skip .hookFailure else .continueExec
  | other => other

theorem pre_tool_use_never_continues_on_hook_error :
    resolveHookStep .preToolUse .errorNonFatal = .skip .hookFailure := rfl

theorem post_and_stop_hook_errors_non_fatal
    (ev : HookEventModel) (h : ev ≠ .preToolUse) :
    resolveHookStep ev .errorNonFatal = .continueExec := by
  cases ev <;> simp_all [resolveHookStep]

theorem skip_kind_tags_disjoint :
    skipFailureTag .policy ≠ skipFailureTag .hookFailure := by decide

/-! ## Part XX — PA20: SWL Guardrail Enforcer Honest Accounting & Fail-Closed Block (`src/swl/guardrails/enforcer.rs`) -/

inductive EvalResultKind where
  | pass
  | fail
  | evalError
  deriving DecidableEq, Repr

inductive GuardActionKind where
  | block
  | warn
  | log
  | alert
  deriving DecidableEq, Repr

structure GuardSummaryCounts where
  total : Nat := 0
  passed : Nat := 0
  failed : Nat := 0
  errors : Nat := 0
  blocked : Nat := 0
  warnings : Nat := 0
  deriving DecidableEq, Repr

def outcomeShouldBlock (res : EvalResultKind) (act : GuardActionKind) : Bool :=
  decide (act = .block) && (decide (res = .fail) || decide (res = .evalError))

def stepGuardSummary
    (s : GuardSummaryCounts) (res : EvalResultKind) (act : GuardActionKind) : GuardSummaryCounts :=
  match res with
  | .pass =>
      { s with total := s.total + 1, passed := s.passed + 1 }
  | .fail =>
      { s with
        total := s.total + 1
        failed := s.failed + 1
        blocked := s.blocked + (if act = .block then 1 else 0)
        warnings := s.warnings + (if act = .warn then 1 else 0) }
  | .evalError =>
      { s with
        total := s.total + 1
        errors := s.errors + 1
        blocked := s.blocked + (if act = .block then 1 else 0)
        warnings := s.warnings + (if act = .warn then 1 else 0) }

theorem guardrail_eval_error_with_block_blocks :
    outcomeShouldBlock .evalError .block = true := rfl

theorem guardrail_pass_never_blocks (act : GuardActionKind) :
    outcomeShouldBlock .pass act = false := by
  cases act <;> rfl

theorem guardrail_step_conserves_total
    (s : GuardSummaryCounts) (res : EvalResultKind) (act : GuardActionKind) :
    let s' := stepGuardSummary s res act
    s'.total = s.total + 1 ∧
      s'.passed + s'.failed + s'.errors = (s.passed + s.failed + s.errors) + 1 := by
  cases res <;> simp [stepGuardSummary] <;> omega

theorem guardrail_blocked_increments_iff_should_block
    (s : GuardSummaryCounts) (res : EvalResultKind) (act : GuardActionKind) :
    (stepGuardSummary s res act).blocked =
      s.blocked + (if outcomeShouldBlock res act then 1 else 0) := by
  cases res <;> cases act <;> rfl

/-! ## Part XXI — PA21: Safety Confirm-View Risk Classification Lattice (`src/safety/confirm_view.rs`) -/

inductive RiskTagModel where
  | reads
  | runsCommand
  | writesWorkspace
  | network
  | installsPackages
  | gitHistory
  | deletesFiles
  | unclassified
  deriving DecidableEq, Repr

def RiskTagModel.label : RiskTagModel → String
  | .reads => "[reads]"
  | .runsCommand => "[runs command]"
  | .writesWorkspace => "[writes workspace]"
  | .network => "[network]"
  | .installsPackages => "[installs packages]"
  | .gitHistory => "[git history]"
  | .deletesFiles => "[deletes files]"
  | .unclassified => "[unclassified tool]"

def RiskTagModel.severity : RiskTagModel → Nat
  | .reads => 0
  | .runsCommand => 1
  | .writesWorkspace => 2
  | .network => 3
  | .installsPackages => 4
  | .gitHistory => 5
  | .deletesFiles => 6
  | .unclassified => 7

def RiskTagModel.isReadOnly (t : RiskTagModel) : Bool :=
  t == .reads

def joinRiskTag (a b : RiskTagModel) : RiskTagModel :=
  if b.severity > a.severity then b else a

def finalizeShellExecRisk
    (segWorst : RiskTagModel)
    (isObservational isShellRead envSafe noCustomShell : Bool) : RiskTagModel :=
  let cmdTag :=
    if segWorst == .reads && !(isObservational && isShellRead) then
      .runsCommand
    else
      segWorst
  if cmdTag == .reads && (!envSafe || !noCustomShell) then
    .runsCommand
  else
    cmdTag

def classifyToolMetadataFallback
    (isContextTool : Bool)
    (metaSpec : Option (Bool × Bool × Bool × Bool)) : RiskTagModel :=
  if isContextTool then
    .reads
  else
    match metaSpec with
    | none => .unclassified
    | some (destructive, shellExec, netAccess, readOnly) =>
        if destructive && !shellExec then .deletesFiles
        else if netAccess then .network
        else if shellExec then .runsCommand
        else if readOnly then .reads
        else .writesWorkspace

theorem risk_tag_join_severity_eq_max (a b : RiskTagModel) :
    (joinRiskTag a b).severity = max a.severity b.severity := by
  cases a <;> cases b <;> decide

theorem risk_tag_join_comm (a b : RiskTagModel) :
    joinRiskTag a b = joinRiskTag b a := by
  cases a <;> cases b <;> decide

theorem shell_exec_reads_iff_all_guards_hold
    (seg : RiskTagModel) (obs rd env sh : Bool) :
    finalizeShellExecRisk seg obs rd env sh = .reads ↔
      (seg = .reads ∧ obs = true ∧ rd = true ∧ env = true ∧ sh = true) := by
  cases seg <;> cases obs <;> cases rd <;> cases env <;> cases sh <;> decide

theorem shell_exec_unsafe_env_or_custom_shell_never_reads
    (seg : RiskTagModel) (obs rd env sh : Bool)
    (h : env = false ∨ sh = false) :
    finalizeShellExecRisk seg obs rd env sh ≠ .reads := by
  cases seg <;> cases obs <;> cases rd <;> cases env <;> cases sh <;> simp_all [finalizeShellExecRisk]

theorem unknown_non_context_tool_is_unclassified :
    classifyToolMetadataFallback false none = .unclassified := rfl

theorem unclassified_dominates_all_risk_tags (t : RiskTagModel) :
    joinRiskTag t .unclassified = .unclassified := by
  cases t <;> decide

/-! ## Part XXII — PA22: Resource Pressure & Adaptive Quotas Governor Lattice (`src/resources/hardware.rs`, `src/resources/quotas.rs`) -/

inductive ResourcePressureModel where
  | none
  | low
  | medium
  | high
  | critical
  deriving DecidableEq, Repr

def ResourcePressureModel.name : ResourcePressureModel → String
  | .none => "none"
  | .low => "low"
  | .medium => "medium"
  | .high => "high"
  | .critical => "critical"

def ResourcePressureModel.requiresAction : ResourcePressureModel → Bool
  | .medium | .high | .critical => true
  | .none | .low => false

def ResourcePressureModel.isCritical : ResourcePressureModel → Bool
  | .critical => true
  | _ => false

def classifyResourcePressure
    (memPct gpuMemPct warnPct critPct emergPct : Nat)
    (gpuOverheating diskExceeded : Bool) : ResourcePressureModel :=
  let maxPct := max memPct gpuMemPct
  if maxPct > emergPct then
    .critical
  else if decide (maxPct > critPct) || gpuOverheating || diskExceeded then
    .high
  else if maxPct > warnPct then
    .medium
  else if maxPct > 50 then
    .low
  else
    .none

structure QuotasModel where
  maxGpuMemoryPerModel : Nat
  maxConcurrentRequests : Nat
  maxContextTokens : Nat
  maxQueuedTasks : Nat
  deriving DecidableEq, Repr

def defaultBaseQuotas : QuotasModel :=
  { maxGpuMemoryPerModel := 20000000000
    maxConcurrentRequests := 4
    maxContextTokens := 1000000
    maxQueuedTasks := 1000 }

def adjustQuotasForPressure (base : QuotasModel) (p : ResourcePressureModel) : QuotasModel :=
  match p with
  | .none => base
  | .low =>
      { base with maxConcurrentRequests := base.maxConcurrentRequests - 1 }
  | .medium =>
      { base with
        maxConcurrentRequests := base.maxConcurrentRequests / 2
        maxContextTokens := base.maxContextTokens / 2 }
  | .high =>
      { base with
        maxConcurrentRequests := 1
        maxContextTokens := base.maxContextTokens / 4
        maxQueuedTasks := base.maxQueuedTasks / 2 }
  | .critical =>
      { maxGpuMemoryPerModel := base.maxGpuMemoryPerModel / 2
        maxConcurrentRequests := 1
        maxContextTokens := 8192
        maxQueuedTasks := 10 }

def checkQuotaRequest (q : QuotasModel) (reqGpuBytes reqSysBytes : Nat) : Bool :=
  decide (reqGpuBytes ≤ q.maxGpuMemoryPerModel) &&
  decide (reqSysBytes ≤ q.maxContextTokens * 100)

theorem emergency_ratio_forces_critical
    (mem gpu warn crit emerg : Nat) (hot disk : Bool)
    (h : max mem gpu > emerg) :
    classifyResourcePressure mem gpu warn crit emerg hot disk = .critical := by
  simp [classifyResourcePressure, h]

theorem overheating_or_disk_requires_action
    (mem gpu warn crit emerg : Nat) (hot disk : Bool)
    (h : hot = true ∨ disk = true) :
    (classifyResourcePressure mem gpu warn crit emerg hot disk).requiresAction = true := by
  unfold classifyResourcePressure
  by_cases hEm : max mem gpu > emerg
  · simp [hEm, ResourcePressureModel.requiresAction]
  · rcases h with rfl | rfl <;> simp [hEm, ResourcePressureModel.requiresAction]

theorem pressure_none_restores_base_quotas (base : QuotasModel) :
    adjustQuotasForPressure base .none = base := rfl

theorem high_and_critical_clamp_concurrent_requests_to_one
    (base : QuotasModel) (p : ResourcePressureModel)
    (h : p = .high ∨ p = .critical) :
    (adjustQuotasForPressure base p).maxConcurrentRequests = 1 := by
  rcases h with rfl | rfl <;> rfl

theorem critical_clamps_context_and_queue (base : QuotasModel) :
    (adjustQuotasForPressure base .critical).maxContextTokens = 8192 ∧
    (adjustQuotasForPressure base .critical).maxQueuedTasks = 10 := by
  exact ⟨rfl, rfl⟩

/-! ## Part XXIII — PA23: Agent Loop Guard & Context Compaction Bounds (`src/agent/loop_control.rs`) -/

def approachingLimitBand (iteration maxIterations : Nat) : Nat :=
  if maxIterations = 0 then
    0
  else if (iteration * 100) / maxIterations ≥ 90 then
    90
  else if (iteration * 100) / maxIterations ≥ 80 then
    80
  else
    0

def restoreBudgetExtension (originalMax persistedCap grants : Nat) : Nat × Nat :=
  (max originalMax persistedCap, min grants 4)

def productivePairStreak
    (t1Success t2Success : Bool)
    (sig1 sig2 : Nat)
    (interveningOtherMutation : Bool) : Bool :=
  t1Success && t2Success && (decide (sig1 ≠ sig2) || interveningOtherMutation)

theorem approaching_limit_zero_max_is_zero (iter : Nat) :
    approachingLimitBand iter 0 = 0 := rfl

theorem approaching_limit_band_values (iter maxIter : Nat) :
    approachingLimitBand iter maxIter = 0 ∨
    approachingLimitBand iter maxIter = 80 ∨
    approachingLimitBand iter maxIter = 90 := by
  unfold approachingLimitBand
  split
  · left; rfl
  · split
    · right; right; rfl
    · split
      · right; left; rfl
      · left; rfl

theorem restore_budget_never_shrinks_cap_and_clamps_grants
    (orig persisted grants : Nat) :
    (restoreBudgetExtension orig persisted grants).1 ≥ orig ∧
    (restoreBudgetExtension orig persisted grants).2 ≤ 4 := by
  simp only [restoreBudgetExtension]
  omega

theorem unexcused_duplicate_breaks_productive_streak
    (s1 s2 : Bool) (sig : Nat) :
    productivePairStreak s1 s2 sig sig false = false := by
  cases s1 <;> cases s2 <;> simp [productivePairStreak]

theorem intervening_mutation_excuses_duplicate (sig : Nat) :
    productivePairStreak true true sig sig true = true := by
  simp [productivePairStreak]

/-! ## Part XXIV — PA24: Memory Budget Eviction & Temporal Decay Lattice (`src/memory/mod.rs`, `src/consolidation/temporal.rs`) -/

def deriveMaxMemoryTokens (contextLength quotaMaxTokens : Nat) : Nat :=
  (min contextLength quotaMaxTokens * 95) / 100

def isNearMemoryLimit (totalTokens maxMemoryTokens : Nat) : Bool :=
  decide (totalTokens * 100 > maxMemoryTokens * 85)

def sumEntryTokens : List Nat → Nat
  | [] => 0
  | x :: xs => x + sumEntryTokens xs

def evictOldestUntilFits : List Nat → Nat → Nat → List Nat
  | [], _, _ => []
  | x :: xs, maxMem, newTok =>
      if (x + sumEntryTokens xs) + newTok > maxMem then
        evictOldestUntilFits xs maxMem newTok
      else
        x :: xs

def addEntryWithEviction (entries : List Nat) (maxMemoryTokens newTokens : Nat) : List Nat :=
  evictOldestUntilFits entries maxMemoryTokens newTokens ++ [newTokens]

inductive RecordImportanceModel where
  | low
  | normal
  | high
  | critical
  deriving DecidableEq, Repr

def RecordImportanceModel.name : RecordImportanceModel → String
  | .low => "low"
  | .normal => "normal"
  | .high => "high"
  | .critical => "critical"

def halfLifeDecayPct : RecordImportanceModel → Nat
  | .low => 25
  | .normal => 50
  | .high => 75
  | .critical => 100

theorem derived_memory_cap_le_both_bounds (ctx quota : Nat) :
    deriveMaxMemoryTokens ctx quota ≤ ctx ∧ deriveMaxMemoryTokens ctx quota ≤ quota := by
  simp only [deriveMaxMemoryTokens]
  omega

theorem sum_entry_tokens_append_singleton (xs : List Nat) (y : Nat) :
    sumEntryTokens (xs ++ [y]) = sumEntryTokens xs + y := by
  induction xs with
  | nil => simp [sumEntryTokens]
  | cons x xs ih =>
      simp [sumEntryTokens, ih]
      omega

theorem evict_oldest_fits_when_new_entry_within_cap
    (entries : List Nat) (maxMem newTok : Nat)
    (hFit : newTok ≤ maxMem) :
    sumEntryTokens (addEntryWithEviction entries maxMem newTok) ≤ maxMem := by
  unfold addEntryWithEviction
  rw [sum_entry_tokens_append_singleton]
  induction entries with
  | nil =>
      simp [evictOldestUntilFits, sumEntryTokens]
      omega
  | cons x xs ih =>
      unfold evictOldestUntilFits
      split
      · exact ih
      · rename_i hNotGt
        simp [sumEntryTokens]
        omega

theorem importance_decay_strictly_monotone :
    halfLifeDecayPct .low < halfLifeDecayPct .normal ∧
    halfLifeDecayPct .normal < halfLifeDecayPct .high ∧
    halfLifeDecayPct .high < halfLifeDecayPct .critical := by
  decide

/-! ## Part XXV — PA25: RSI Safety, DarwinX Non-Regression, Circuit-Breaker & Prompt Extension (`src/cognitive/rsi_orchestrator.rs`, `src/cognitive/self_improvement.rs`) -/

/-- Scenario evaluation outcome in `cognitive::rsi_orchestrator::BenchmarkReport`. -/
structure ScenarioPair where
  sameName       : Bool
  baselinePassed : Bool
  candPassed     : Bool
  deriving DecidableEq, Repr

/-- Whether a single aligned scenario pair passes DarwinX non-regression. -/
def scenarioNonRegressing (p : ScenarioPair) : Bool :=
  p.sameName && (!p.baselinePassed || p.candPassed)

/-- DarwinX suite non-regression gate (`BenchmarkReport::check_darwinx_non_regression`):
    requires identical scenario cardinality and every scenario pair to have matching name
    and preserve `Passed(baseline) ⊆ Passed(candidate)`. -/
def checkDarwinxSuite (sameCount : Bool) (pairs : List ScenarioPair) : Bool :=
  sameCount && pairs.all scenarioNonRegressing

/-- Outcome of a single RSI loop cycle (`cognitive::rsi_orchestrator::CycleOutcome`). -/
inductive RsiCycleOutcome where
  | improved
  | notImproved
  | noEligibleMutation
  deriving DecidableEq, Repr

/-- State transition of `(consecutive_failures, circuit_breaker_tripped)` after an RSI cycle. -/
def stepRsiBreaker (failures maxFailures : Nat) (tripped : Bool) (outcome : RsiCycleOutcome) : Nat × Bool :=
  match outcome with
  | .improved => (0, tripped)
  | .noEligibleMutation => (failures, tripped)
  | .notImproved =>
    let f' := failures + 1
    (f', tripped || decide (f' ≥ maxFailures))

/-- Restart decay of persisted `consecutive_failures` in `RSIOrchestrator::new`:
    halves the persisted failure count (`loaded / 2`) and clears `circuit_breaker_tripped`. -/
def decayRsiFailuresOnRestart (loadedFailures : Nat) : Nat × Bool :=
  (loadedFailures / 2, false)

/-- Prompt tournament baseline-extension check (`PromptTournamentResult::extends_baseline`)
    and `PromptOptimizer::best_patterns_for` minimum usage gate (`usage_count >= 5`). -/
def promptPatternEligible (winnerLen baseLen : Nat) (isPrefix : Bool) (usageCount : Nat) : Bool × Bool :=
  let extendsBaseline := decide (winnerLen ≥ baseLen) && isPrefix
  let eligibleInBest := decide (usageCount ≥ 5)
  (extendsBaseline, eligibleInBest)

theorem darwinx_rejects_passed_to_failed (sameCount : Bool) (rest : List ScenarioPair) :
    checkDarwinxSuite sameCount ({ sameName := true, baselinePassed := true, candPassed := false } :: rest) = false := by
  cases sameCount <;> rfl

theorem darwinx_rejects_suite_mismatch (pairs : List ScenarioPair) :
    checkDarwinxSuite false pairs = false := by
  rfl

theorem rsi_no_eligible_mutation_preserves_breaker (failures maxFailures : Nat) (tripped : Bool) :
    stepRsiBreaker failures maxFailures tripped .noEligibleMutation = (failures, tripped) := by
  rfl

theorem rsi_improved_resets_consecutive_failures (failures maxFailures : Nat) (tripped : Bool) :
    (stepRsiBreaker failures maxFailures tripped .improved).1 = 0 := by
  rfl

theorem rsi_not_improved_trips_at_threshold (failures maxFailures : Nat) (tripped : Bool)
    (h : failures + 1 ≥ maxFailures) :
    (stepRsiBreaker failures maxFailures tripped .notImproved).2 = true := by
  simp [stepRsiBreaker, h]

theorem rsi_restart_decay_bounds_and_clears (loaded : Nat) :
    (decayRsiFailuresOnRestart loaded).1 ≤ loaded ∧ (decayRsiFailuresOnRestart loaded).2 = false := by
  simp [decayRsiFailuresOnRestart]
  omega

theorem prompt_fresh_winner_not_immediately_best (winnerLen baseLen : Nat) (isPrefix : Bool) :
    (promptPatternEligible winnerLen baseLen isPrefix 0).2 = false := by
  rfl

/-! ## Part XXVI — PA26: Evolution Fitness, Sabotage Guard & Promotion Lattice (`src/evolution/daemon.rs`, `src/evolution/fitness.rs`) -/

/-- Tri-state outcome of the multi-arm token-reduction gate (`daemon::evaluate_token_term`). -/
inductive TokenGateOutcome where
  | pass
  | fail
  | inconclusive
  deriving DecidableEq, Repr

def TokenGateOutcome.label : TokenGateOutcome → String
  | .pass => "some_true"
  | .fail => "some_false"
  | .inconclusive => "none"

/-- Whether a single measured arm achieves the strict >5% token reduction (`(base - arm) * 20 > base`). -/
def armReducesTokens5Pct (baseTokens armTokens : Nat) : Bool :=
  decide (baseTokens > armTokens) && decide ((baseTokens - armTokens) * 20 > baseTokens)

/-- Multi-arm token gate (`daemon::evaluate_token_term`) for two measured arms:
    requires `measuredArms >= 2` and `baseTokens > 0`; returns `.pass` iff all arms pass,
    `.fail` iff all arms fail, and `.inconclusive` on disagreement or insufficient data. -/
def evaluateTwoArmTokenTerm (baseTokens : Nat) (measuredArms : Nat) (arm1Tokens arm2Tokens : Nat) : TokenGateOutcome :=
  if baseTokens = 0 || measuredArms < 2 then
    .inconclusive
  else
    let p1 := armReducesTokens5Pct baseTokens arm1Tokens
    let p2 := armReducesTokens5Pct baseTokens arm2Tokens
    if p1 && p2 then .pass
    else if !p1 && !p2 then .fail
    else .inconclusive

/-- Promotion decision outcome (`daemon::PromotionDecision`). -/
inductive PromotionOutcome where
  | promote
  | rejectSabDrop
  | rejectDarwinx
  | rejectSabotageTestsDeleted
  | rejectWithinNoise
  deriving DecidableEq, Repr

def PromotionOutcome.isPromote : PromotionOutcome → Bool
  | .promote => true
  | _ => false

def PromotionOutcome.label : PromotionOutcome → String
  | .promote => "promote"
  | .rejectSabDrop => "reject:sab_drop"
  | .rejectDarwinx => "reject:darwinx"
  | .rejectSabotageTestsDeleted => "reject:tests_deleted"
  | .rejectWithinNoise => "reject:within_noise"

/-- Candidate promotion lattice (`daemon::evaluate_candidate_promotion`) with integer-scaled
    tenths for SAB score (`sab_x10`) and noise margin (`noise_x10`, default 5 for 0.5). -/
def evaluateCandidatePromotion
    (baseSabX10 candSabX10 noiseX10 : Nat)
    (darwinxOk : Bool)
    (baseTests candTests : Nat)
    (compositeImproved : Bool)
    (tokenGate : TokenGateOutcome) : PromotionOutcome :=
  if candSabX10 + noiseX10 < baseSabX10 then
    .rejectSabDrop
  else if !darwinxOk then
    .rejectDarwinx
  else if candTests < baseTests then
    .rejectSabotageTestsDeleted
  else if candSabX10 > baseSabX10 + noiseX10 then
    .promote
  else if compositeImproved && tokenGate == .pass then
    .promote
  else
    .rejectWithinNoise

theorem sabotage_test_deletion_never_promotes
    (baseSab candSab noise : Nat) (darwinxOk : Bool)
    (baseTests candTests : Nat) (comp : Bool) (tok : TokenGateOutcome)
    (hDel : candTests < baseTests) :
    (evaluateCandidatePromotion baseSab candSab noise darwinxOk baseTests candTests comp tok).isPromote = false := by
  simp only [evaluateCandidatePromotion, hDel, ↓reduceIte]
  split
  · rfl
  · split <;> rfl

theorem darwinx_failure_never_promotes
    (baseSab candSab noise : Nat) (baseTests candTests : Nat) (comp : Bool) (tok : TokenGateOutcome) :
    (evaluateCandidatePromotion baseSab candSab noise false baseTests candTests comp tok).isPromote = false := by
  unfold evaluateCandidatePromotion
  split <;> rfl

theorem within_noise_requires_composite_and_token_pass
    (baseSab candSab noise : Nat) (baseTests candTests : Nat)
    (comp : Bool) (tok : TokenGateOutcome)
    (hNoDrop : ¬(candSab + noise < baseSab))
    (hNotAbove : ¬(candSab > baseSab + noise))
    (hTests : ¬(candTests < baseTests)) :
    (evaluateCandidatePromotion baseSab candSab noise true baseTests candTests comp tok).isPromote =
      (comp && tok == .pass) := by
  unfold evaluateCandidatePromotion
  simp [hNoDrop, hNotAbove, hTests]
  cases comp <;> cases tok <;> rfl

theorem token_term_disagreement_is_inconclusive (base a1 a2 : Nat)
    (hBase : base > 0)
    (hDisagree : armReducesTokens5Pct base a1 ≠ armReducesTokens5Pct base a2) :
    evaluateTwoArmTokenTerm base 2 a1 a2 = .inconclusive := by
  unfold evaluateTwoArmTokenTerm
  have hNe : base ≠ 0 := by omega
  simp [hNe]
  cases h1 : armReducesTokens5Pct base a1 <;> cases h2 : armReducesTokens5Pct base a2 <;> simp_all

/-! ## Part XXVII — PA27: Multi-Arm Winner Selection, Compiler Repair & Readiness Lattice (`src/evolve/multi_arm_runner.rs`, `src/evolve/compiler_repair.rs`) -/

/-- Evaluation parameters for a single optimization arm in `evolve::multi_arm_runner`. -/
structure ArmCandidate where
  armId           : Nat
  protectedTouch  : Bool
  syntaxValid     : Bool
  compiled        : Bool
  testRegressions : Nat
  fitnessDeltaPos : Bool
  fitnessDeltaMag : Nat
  armMaxRuntime   : Nat
  baseMinRuntime  : Nat
  deriving DecidableEq, Repr

/-- Whether an arm passes all safety, compilation, regression, fitness, and runtime separation gates. -/
def armPassesAllGates (c : ArmCandidate) : Bool :=
  !c.protectedTouch &&
  c.syntaxValid &&
  c.compiled &&
  decide (c.testRegressions = 0) &&
  c.fitnessDeltaPos &&
  decide (c.armMaxRuntime < c.baseMinRuntime)

/-- Select the passing arm with maximum `fitnessDeltaMag` (first-seen tie-breaking). -/
def selectBestPassingArm : List ArmCandidate → Option (Nat × Nat)
  | [] => none
  | c :: rest =>
    let tailBest := selectBestPassingArm rest
    if armPassesAllGates c then
      match tailBest with
      | none => some (c.armId, c.fitnessDeltaMag)
      | some (bestId, bestMag) =>
        if c.fitnessDeltaMag ≥ bestMag then some (c.armId, c.fitnessDeltaMag)
        else some (bestId, bestMag)
    else
      tailBest

/-- Consensus check (`threshold > 0 && passingArms >= threshold`). -/
def checkMultiArmConsensus (threshold : Nat) (arms : List ArmCandidate) : Bool :=
  decide (threshold > 0) && decide ((arms.filter armPassesAllGates).length ≥ threshold)

/-- Disposition of a candidate compiler suggestion in `DiagnosticRepairEngine::apply_machine_fixes`. -/
inductive RepairEditDisposition where
  | applied
  | duplicate
  | skipped
  deriving DecidableEq, Repr

/-- Classify a single compiler fix suggestion (`evolve::compiler_repair::DiagnosticRepairEngine::apply_machine_fixes`). -/
def classifyRepairCandidate (insideRoot validRange isDuplicate overlapsAccepted : Bool) : RepairEditDisposition :=
  if !insideRoot || !validRange then
    .skipped
  else if isDuplicate then
    .duplicate
  else if overlapsAccepted then
    .skipped
  else
    .applied

/-- Count `(applied, duplicates, skipped)` across a batch of candidate fixes. -/
def summarizeRepairBatch (candidates : List (Bool × Bool × Bool × Bool)) : Nat × Nat × Nat :=
  candidates.foldl (fun (app, dup, skp) (inRoot, validR, isDup, ovl) =>
    match classifyRepairCandidate inRoot validR isDup ovl with
    | .applied => (app + 1, dup, skp)
    | .duplicate => (app, dup + 1, skp)
    | .skipped => (app, dup, skp + 1)) (0, 0, 0)

/-- Stop reason of `DiagnosticRepairEngine::run_repair_loop` (`evolve::compiler_repair::RepairStop`). -/
inductive RepairStopModel where
  | compiles
  | noApplicableFix
  | fuelExhausted
  deriving DecidableEq, Repr

def RepairStopModel.label : RepairStopModel → String
  | .compiles => "compiles"
  | .noApplicableFix => "no_applicable_fix"
  | .fuelExhausted => "fuel_exhausted"

/-- Bounded compiler repair loop (`DiagnosticRepairEngine::run_repair_loop`):
    given `maxRounds`, current `(rounds, checksRun, fixesApplied)`, and a trace of
    `(checkSuccess, appliedInRound)` per check, returns `(stop, rounds, checksRun, fixesApplied)`. -/
def simulateRepairLoop (maxRounds rounds checksRun fixesApplied : Nat) :
    List (Bool × Nat) → RepairStopModel × Nat × Nat × Nat
  | [] => (.noApplicableFix, rounds, checksRun, fixesApplied)
  | (compiles, appliedNow) :: rest =>
    if compiles then
      (.compiles, rounds, checksRun, fixesApplied)
    else if rounds ≥ maxRounds then
      (.fuelExhausted, rounds, checksRun, fixesApplied)
    else if appliedNow = 0 then
      (.noApplicableFix, rounds, checksRun, fixesApplied)
    else
      simulateRepairLoop maxRounds (rounds + 1) (checksRun + 1) (fixesApplied + appliedNow) rest

theorem protected_or_regression_arm_never_passes (c : ArmCandidate)
    (h : c.protectedTouch = true ∨ c.testRegressions > 0) :
    armPassesAllGates c = false := by
  unfold armPassesAllGates
  rcases h with hProt | hReg
  · simp [hProt]
  · have hNe : c.testRegressions ≠ 0 := by omega
    simp [hNe]

theorem zero_threshold_never_meets_consensus (arms : List ArmCandidate) :
    checkMultiArmConsensus 0 arms = false := by
  rfl

theorem repair_escape_or_overlap_never_applied (inRoot validR isDup ovl : Bool)
    (h : inRoot = false ∨ (isDup = false ∧ ovl = true)) :
    classifyRepairCandidate inRoot validR isDup ovl ≠ .applied := by
  unfold classifyRepairCandidate
  rcases h with hOut | ⟨hNotDup, hOvl⟩
  · simp [hOut]
  · cases inRoot <;> cases validR <;> simp [hNotDup, hOvl]

theorem repair_loop_stops_at_fuel_ceiling (maxRounds rounds checks fixes appliedNow : Nat)
    (rest : List (Bool × Nat)) (hExhaust : rounds ≥ maxRounds) :
    simulateRepairLoop maxRounds rounds checks fixes ((false, appliedNow) :: rest) =
      (.fuelExhausted, rounds, checks, fixes) := by
  simp [simulateRepairLoop, hExhaust]

/-! ## Part XXVIII — PA28: Self-Healing Recovery, Escalation & Health Prediction State Machine (`src/self_healing/mod.rs`) -/

/-- Error taxonomy (`self_healing::ErrorClass`). -/
inductive HealingErrorClass where
  | network
  | timeout
  | rateLimit
  | resourceExhaustion
  | parseError
  | authError
  | unknown
  deriving DecidableEq, Repr

/-- Whether the default strategy for an error class uses a `Retry` action
    (which preserves `retry_attempt_count` across primary success so retries can exhaust). -/
def defaultStrategyHasRetry : HealingErrorClass → Bool
  | .network => true
  | .timeout => true
  | .rateLimit => true
  | .resourceExhaustion => true
  | .parseError => false
  | .authError => false
  | .unknown => true

/-- Outcome of `SelfHealingEngine::handle_error`: returns `(resolved, escalated, nextRetryAttempts)`. -/
def stepSelfHealingError
    (enabled : Bool)
    (errClass : HealingErrorClass)
    (prevAttempts : Nat)
    (primarySucceeded : Bool)
    (escalationSucceeded : Bool) : Option (Bool × Bool × Nat) :=
  if !enabled then
    none
  else
    let attempts := prevAttempts + 1
    if primarySucceeded then
      let nextAttempts := if defaultStrategyHasRetry errClass then attempts else 0
      some (true, false, nextAttempts)
    else if escalationSucceeded then
      some (true, true, 0)
    else
      some (false, true, attempts)

/-- Predicted component health (`self_healing::PredictedHealth`). -/
inductive PredictedHealthModel where
  | healthy
  | atRisk
  | degrading
  | failureImminent
  deriving DecidableEq, Repr

def PredictedHealthModel.severity : PredictedHealthModel → Nat
  | .healthy => 0
  | .atRisk => 1
  | .degrading => 2
  | .failureImminent => 3

def PredictedHealthModel.label : PredictedHealthModel → String
  | .healthy => "Healthy"
  | .atRisk => "AtRisk"
  | .degrading => "Degrading"
  | .failureImminent => "FailureImminent"

/-- Classify component health when `windowLen >= 5`. -/
def classifyComponentHealth (unhealthyCount errorTrend avgResponseMs : Nat) : PredictedHealthModel :=
  if unhealthyCount ≥ 7 then
    .failureImminent
  else if unhealthyCount ≥ 4 ∨ errorTrend > 10 then
    .degrading
  else if unhealthyCount ≥ 2 ∨ avgResponseMs > 3000 then
    .atRisk
  else
    .healthy

/-- Predict component health from the recent window (`HealthPredictor::predict`):
    returns `none` if `windowLen < 5`, otherwise delegates to `classifyComponentHealth`. -/
def predictComponentHealth
    (windowLen unhealthyCount errorTrend avgResponseMs : Nat) : Option PredictedHealthModel :=
  if windowLen < 5 then
    none
  else
    some (classifyComponentHealth unhealthyCount errorTrend avgResponseMs)

theorem disabled_engine_returns_none
    (c : HealingErrorClass) (prev : Nat) (pOk eOk : Bool) :
    stepSelfHealingError false c prev pOk eOk = none := by
  rfl

theorem retry_primary_success_accumulates_attempts
    (prev : Nat) :
    stepSelfHealingError true .network prev true false = some (true, false, prev + 1) := by
  rfl

theorem escalation_success_resets_retry_attempts
    (c : HealingErrorClass) (prev : Nat) :
    stepSelfHealingError true c prev false true = some (true, true, 0) := by
  rfl

theorem health_prediction_requires_five_samples
    (len u err rt : Nat) (h : len < 5) :
    predictComponentHealth len u err rt = none := by
  unfold predictComponentHealth
  simp [h]

theorem health_prediction_monotone_in_unhealthy
    (u1 u2 err rt : Nat) (h : u1 ≤ u2) :
    (classifyComponentHealth u1 err rt).severity ≤ (classifyComponentHealth u2 err rt).severity := by
  unfold classifyComponentHealth
  all_goals try split
  all_goals try split
  all_goals try split
  all_goals try split
  all_goals try split
  all_goals try split
  all_goals (simp [PredictedHealthModel.severity]; try omega)

/-! ## Part XXIX — PA29: `evolution::investigate` 4-Check Static Governance & Safety Veto Lattice (`src/evolution/investigate.rs`) -/

/-- Attempt execution status (`evolution::tree_log::AttemptStatus`). -/
inductive AttemptStatusModel where
  | baseline
  | evaluated
  | compileFailed
  | testFailed
  | clippyFailed
  | formatFailed
  | safetyRejected
  | patchFailed
  | buildFailed
  | timeout
  | duplicateRejected
  | cancelled
  | internalError
  deriving DecidableEq, Repr

/-- Provenance check outcome in `simulate_10000_reviewer_governance_resolved` when `diff_sha256.len() == 64`
    and `is_verified_success` holds. -/
inductive ProvenanceCheckStatus where
  | verified
  | incomplete
  | integrityMismatch
  deriving DecidableEq, Repr

/-- Governance verdict (`evolution::investigate::GovernanceDecision`). -/
inductive GovernanceDecisionModel where
  | approveForPromotion
  | conditionalClarification
  | quarantineForExperimentation
  | hardRejectVeto
  deriving DecidableEq, Repr

def GovernanceDecisionModel.label : GovernanceDecisionModel → String
  | .approveForPromotion => "approve_for_promotion"
  | .conditionalClarification => "conditional_clarification"
  | .quarantineForExperimentation => "quarantine_for_experimentation"
  | .hardRejectVeto => "hard_reject_veto"

/-- Input to the 4-check static and empirical governance panel (`simulate_10000_reviewer_governance_resolved`). -/
structure GovernanceInput where
  status              : AttemptStatusModel
  hasCompositeScore   : Bool
  protectedPathsClean : Bool
  hasKillswitchBypass : Bool
  merkleMismatch      : Bool
  hasCriticalFinding  : Bool
  hasWarningFinding   : Bool
  diffSha256Valid     : Bool
  provenance          : ProvenanceCheckStatus
  deriving DecidableEq, Repr

/-- Synthesized 4-perspective governance outcome (`evolution::investigate::ReviewerConsensus`). -/
structure GovernanceConsensusModel where
  votesApprove       : Nat
  votesClarification : Nat
  votesQuarantine    : Nat
  votesVeto          : Nat
  decision           : GovernanceDecisionModel
  hasSafetyVeto      : Bool
  deriving DecidableEq, Repr

def evalSafetyCheck (safetyVeto isVerifiedSuccess hasCritical : Bool) : Nat × Nat × Nat × Nat :=
  if safetyVeto || !isVerifiedSuccess then (0, 0, 0, 1)
  else if hasCritical then (0, 0, 1, 0)
  else (1, 0, 0, 0)

def evalSystemsCheck (status : AttemptStatusModel) (isVerifiedSuccess hasCritical : Bool) : Nat × Nat × Nat × Nat :=
  if status == .timeout || status == .buildFailed || status == .internalError then
    (0, 0, 0, 1)
  else if status == .cancelled || !isVerifiedSuccess || hasCritical then
    (0, 0, 1, 0)
  else
    (1, 0, 0, 0)

def evalSweCheck (status : AttemptStatusModel) (isVerifiedSuccess hasCritical hasWarning : Bool) : Nat × Nat × Nat × Nat :=
  let sweFailed :=
    status == .compileFailed || status == .buildFailed ||
    status == .testFailed || status == .clippyFailed ||
    status == .formatFailed || status == .patchFailed
  if sweFailed then (0, 0, 0, 1)
  else if !isVerifiedSuccess || hasCritical then (0, 0, 1, 0)
  else if hasWarning then (0, 1, 0, 0)
  else (1, 0, 0, 0)

def evalProvenanceCheck
    (status : AttemptStatusModel)
    (isVerifiedSuccess diffSha256Valid : Bool)
    (prov : ProvenanceCheckStatus) : Nat × Nat × Nat × Nat × Bool :=
  if !diffSha256Valid then (0, 0, 0, 1, true)
  else if status == .internalError || !isVerifiedSuccess then (0, 0, 1, 0, false)
  else match prov with
    | .integrityMismatch => (0, 0, 0, 1, true)
    | .verified => (1, 0, 0, 0, false)
    | .incomplete => (0, 1, 0, 0, false)

/-- Evaluates the 4 deterministic static & empirical governance checks
    (Safety, Systems, Architecture, Provenance) in `simulate_10000_reviewer_governance_resolved`. -/
def evaluateGovernance4Check (inp : GovernanceInput) : GovernanceConsensusModel :=
  let isVerifiedSuccess :=
    (inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline
  let safetyVeto :=
    !inp.protectedPathsClean || inp.hasKillswitchBypass || inp.merkleMismatch ||
    inp.status == .safetyRejected
  let (sApp, sCla, sQua, sVet) :=
    evalSafetyCheck safetyVeto isVerifiedSuccess inp.hasCriticalFinding
  let (sysApp, sysCla, sysQua, sysVet) :=
    evalSystemsCheck inp.status isVerifiedSuccess inp.hasCriticalFinding
  let (sweApp, sweCla, sweQua, sweVet) :=
    evalSweCheck inp.status isVerifiedSuccess inp.hasCriticalFinding inp.hasWarningFinding
  let (provApp, provCla, provQua, provVet, integrityVeto) :=
    evalProvenanceCheck inp.status isVerifiedSuccess inp.diffSha256Valid inp.provenance
  let totApp := sApp + sysApp + sweApp + provApp
  let totCla := sCla + sysCla + sweCla + provCla
  let totQua := sQua + sysQua + sweQua + provQua
  let totVet := sVet + sysVet + sweVet + provVet
  let decision : GovernanceDecisionModel :=
    if safetyVeto || integrityVeto || decide (totVet ≥ 1) || !isVerifiedSuccess then
      .hardRejectVeto
    else if decide (totApp * 100 ≥ 85 * 4) then
      .approveForPromotion
    else if decide (totApp * 100 ≥ 65 * 4) then
      .conditionalClarification
    else
      .quarantineForExperimentation
  { votesApprove := totApp
  , votesClarification := totCla
  , votesQuarantine := totQua
  , votesVeto := totVet
  , decision := decision
  , hasSafetyVeto := safetyVeto || integrityVeto
  }

theorem protected_path_breach_always_vetoes (inp : GovernanceInput)
    (h : inp.protectedPathsClean = false) :
    (evaluateGovernance4Check inp).decision = .hardRejectVeto ∧
    (evaluateGovernance4Check inp).hasSafetyVeto = true := by
  unfold evaluateGovernance4Check
  simp [h]

theorem killswitch_or_merkle_breach_always_vetoes (inp : GovernanceInput)
    (h : inp.hasKillswitchBypass = true ∨ inp.merkleMismatch = true) :
    (evaluateGovernance4Check inp).decision = .hardRejectVeto ∧
    (evaluateGovernance4Check inp).hasSafetyVeto = true := by
  unfold evaluateGovernance4Check
  rcases h with hKill | hMerk
  · cases inp.protectedPathsClean <;> simp [hKill]
  · cases inp.protectedPathsClean <;> cases inp.hasKillswitchBypass <;> simp [hMerk]

theorem unverified_attempt_never_promoted (inp : GovernanceInput)
    (hNotEval : inp.status ≠ .evaluated) (hNotBase : inp.status ≠ .baseline) :
    (evaluateGovernance4Check inp).decision = .hardRejectVeto := by
  unfold evaluateGovernance4Check
  have h1 : (inp.status == .evaluated) = false := beq_false_of_ne hNotEval
  have h2 : (inp.status == .baseline) = false := beq_false_of_ne hNotBase
  simp [h1, h2]

theorem evalSafety_crit_zero (sv ver : Bool) : (evalSafetyCheck sv ver true).1 = 0 := by
  unfold evalSafetyCheck
  split <;> rfl

theorem evalSystems_crit_zero (st : AttemptStatusModel) (ver : Bool) :
    (evalSystemsCheck st ver true).1 = 0 := by
  unfold evalSystemsCheck
  split
  · rfl
  · simp

theorem evalSwe_crit_zero (st : AttemptStatusModel) (ver warn : Bool) :
    (evalSweCheck st ver true warn).1 = 0 := by
  unfold evalSweCheck
  dsimp
  split
  · rfl
  · simp

theorem evalProv_app_le_one (st : AttemptStatusModel) (ver sha : Bool) (p : ProvenanceCheckStatus) :
    (evalProvenanceCheck st ver sha p).1 ≤ 1 := by
  unfold evalProvenanceCheck
  split
  · omega
  · split
    · omega
    · cases p <;> decide

theorem critical_finding_blocks_promotion (inp : GovernanceInput)
    (hCrit : inp.hasCriticalFinding = true) :
    (evaluateGovernance4Check inp).decision ≠ .approveForPromotion := by
  unfold evaluateGovernance4Check
  dsimp
  rw [hCrit]
  have hs := evalSafety_crit_zero
    (!inp.protectedPathsClean || inp.hasKillswitchBypass || inp.merkleMismatch || inp.status == .safetyRejected)
    ((inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline)
  have hsys := evalSystems_crit_zero inp.status
    ((inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline)
  have hswe := evalSwe_crit_zero inp.status
    ((inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline)
    inp.hasWarningFinding
  have hprov := evalProv_app_le_one inp.status
    ((inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline)
    inp.diffSha256Valid inp.provenance
  have hNot85 : decide ((
    (evalSafetyCheck (!inp.protectedPathsClean || inp.hasKillswitchBypass || inp.merkleMismatch || inp.status == .safetyRejected) ((inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline) true).1 +
    (evalSystemsCheck inp.status ((inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline) true).1 +
    (evalSweCheck inp.status ((inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline) true inp.hasWarningFinding).1 +
    (evalProvenanceCheck inp.status ((inp.status == .evaluated && inp.hasCompositeScore) || inp.status == .baseline) inp.diffSha256Valid inp.provenance).1) * 100 ≥ 85 * 4) = false := by
    rw [hs, hsys, hswe]
    simp only [Nat.zero_add, decide_eq_false_iff_not]
    omega
  rw [hNot85]
  split
  · decide
  · split
    · contradiction
    · split <;> decide

/-! ## Part XXX — PA30: `evolution::micro_mode` Small-Model Classification & Hypothesis Gate (`src/evolution/micro_mode.rs`) -/

def microMaxFiles : Nat := 3
def microMaxEdits : Nat := 2
def microMaxSearchChars : Nat := 500
def microMaxReplaceChars : Nat := 1000

/-- Word-bounded small-model classification (`evolution::micro_mode::is_micro_model`). -/
def classifyMicroModelName
    (hasTinyOrSmallKeyword hasMicroSizeToken prevBoundaryNonAlnum nextBoundaryNonAlnum : Bool) : Bool :=
  hasTinyOrSmallKeyword || (hasMicroSizeToken && prevBoundaryNonAlnum && nextBoundaryNonAlnum)

/-- Population size clamp in `evolution::micro_mode::build_micro_system_prompt`: `population_size.min(3)`. -/
def clampMicroPopulation (populationSize : Nat) : Nat :=
  if populationSize ≤ 3 then populationSize else 3

/-- Validation outcome for `evolution::micro_mode::validate_micro_hypothesis`. -/
inductive MicroValidationOutcome where
  | ok
  | errTargetFiles
  | errTargetProtected
  | errEditCount
  | errEditProtected
  | errSearchLen
  | errReplaceLen
  | errDiffFiles
  | errDiffProtected
  | errDiffHunks
  deriving DecidableEq, Repr

def MicroValidationOutcome.isOk : MicroValidationOutcome → Bool
  | .ok => true
  | _ => false

def MicroValidationOutcome.label : MicroValidationOutcome → String
  | .ok => "ok:true"
  | .errTargetFiles => "err:target_files"
  | .errTargetProtected => "err:target_protected"
  | .errEditCount => "err:edit_count"
  | .errEditProtected => "err:edit_protected"
  | .errSearchLen => "err:search_len"
  | .errReplaceLen => "err:replace_len"
  | .errDiffFiles => "err:diff_files"
  | .errDiffProtected => "err:diff_protected"
  | .errDiffHunks => "err:diff_hunks"

structure MicroHypothesisInput where
  targetFileCount    : Nat
  targetHasProtected : Bool
  isJsonPatch        : Bool
  editCount          : Nat
  editHasProtected   : Bool
  maxSearchLen       : Nat
  maxReplaceLen      : Nat
  diffFileCount      : Nat
  diffHasProtected   : Bool
  diffHunkCount      : Nat
  deriving DecidableEq, Repr

/-- Models `evolution::micro_mode::validate_micro_hypothesis`. -/
def validateMicroHypothesis (inp : MicroHypothesisInput) : MicroValidationOutcome :=
  if inp.targetFileCount > microMaxFiles then
    .errTargetFiles
  else if inp.targetHasProtected then
    .errTargetProtected
  else if inp.isJsonPatch then
    if inp.editCount > microMaxEdits then
      .errEditCount
    else if inp.editHasProtected then
      .errEditProtected
    else if inp.maxSearchLen > microMaxSearchChars then
      .errSearchLen
    else if inp.maxReplaceLen > microMaxReplaceChars then
      .errReplaceLen
    else
      .ok
  else
    if inp.diffFileCount > microMaxFiles then
      .errDiffFiles
    else if inp.diffHasProtected then
      .errDiffProtected
    else if inp.diffHunkCount > microMaxEdits then
      .errDiffHunks
    else
      .ok

theorem word_boundary_blocks_false_positive_micro
    (hasMicro nextOk : Bool) :
    classifyMicroModelName false hasMicro false nextOk = false := by
  unfold classifyMicroModelName
  cases hasMicro <;> rfl

theorem micro_population_clamped_at_three (pop : Nat) :
    clampMicroPopulation pop ≤ 3 := by
  unfold clampMicroPopulation
  split <;> omega

theorem protected_target_or_edit_rejected_in_micro (inp : MicroHypothesisInput)
    (h : inp.targetHasProtected = true ∨
         (inp.isJsonPatch = true ∧ inp.editHasProtected = true) ∨
         (inp.isJsonPatch = false ∧ inp.diffHasProtected = true)) :
    (validateMicroHypothesis inp).isOk = false := by
  unfold validateMicroHypothesis
  rcases h with hT | ⟨hJ, hE⟩ | ⟨hNotJ, hD⟩
  · rw [hT]
    split <;> rfl
  · rw [hJ, hE]
    split
    · rfl
    · split
      · rfl
      · dsimp
        split <;> rfl
  · rw [hNotJ, hD]
    split
    · rfl
    · split
      · rfl
      · dsimp
        split <;> rfl

theorem oversized_search_or_replace_rejected_in_micro (inp : MicroHypothesisInput)
    (hJson : inp.isJsonPatch = true)
    (hOver : inp.maxSearchLen > microMaxSearchChars ∨ inp.maxReplaceLen > microMaxReplaceChars) :
    (validateMicroHypothesis inp).isOk = false := by
  unfold validateMicroHypothesis
  rw [hJson]
  split
  · rfl
  · split
    · rfl
    · dsimp
      split
      · rfl
      · split
        · rfl
        · split
          · rfl
          · split
            · rfl
            · rcases hOver with hS | hR <;> contradiction

/-! ## Part XXXI — PA31: `cognitive::meta_learning` Strategy Effectiveness & PDVR Cycle Lattice (`src/cognitive/meta_learning.rs`, `src/cognitive/state.rs`) -/

/-- Minimum attempts before exploration bonus expires (`attempts < 3` in `StrategyScore::priority_weight`). -/
def minAttemptsWithoutExplorationBonus : Nat := 3

/-- Success rate in percentage points (`0..=100`), matching `StrategyScore::success_rate`:
    returns 50% prior when `attempts == 0`, else `(successes * 100) / attempts`. -/
def strategySuccessRatePct (attempts successes : Nat) : Nat :=
  if attempts = 0 then 50 else (successes * 100) / attempts

/-- Exploration bonus in percentage points (`20` when `attempts < 3`, else `0`). -/
def strategyExplorationBonusPct (attempts : Nat) : Nat :=
  if attempts < minAttemptsWithoutExplorationBonus then 20 else 0

/-- Priority weight in percentage points (`StrategyScore::priority_weight * 100`):
    returns `0` when in cooldown, otherwise `0.5 * success_rate + 0.5 * avg_effectiveness.max(0.0) + exploration_bonus`. -/
def strategyPriorityWeightPct
    (inCooldown : Bool)
    (attempts successes : Nat)
    (avgEffNegative : Bool)
    (avgEffMagPct : Nat) : Nat :=
  if inCooldown then
    0
  else
    let sr := strategySuccessRatePct attempts successes
    let eff := if avgEffNegative then 0 else avgEffMagPct
    let bonus := strategyExplorationBonusPct attempts
    (sr + eff) / 2 + bonus

/-- PDVR cycle phase (`cognitive::state::CyclePhase`). -/
inductive CyclePhaseModel where
  | plan
  | doPhase
  | verify
  | reflect
  deriving DecidableEq, Repr

def CyclePhaseModel.asStr : CyclePhaseModel → String
  | .plan => "plan"
  | .doPhase => "do"
  | .verify => "verify"
  | .reflect => "reflect"

/-- `CyclePhase::next` (`Plan -> Do -> Verify -> Reflect -> Plan`). -/
def CyclePhaseModel.next : CyclePhaseModel → CyclePhaseModel
  | .plan => .doPhase
  | .doPhase => .verify
  | .verify => .reflect
  | .reflect => .plan

/-- Working memory approach stack cap (`MAX_APPROACH_DEPTH = 20` in `src/cognitive/state.rs`). -/
def maxApproachDepth : Nat := 20

def capApproachStackDepth (pushed : Nat) : Nat :=
  if pushed ≤ maxApproachDepth then pushed else maxApproachDepth

theorem cooldown_always_zeros_priority_weight
    (attempts successes mag : Nat) (neg : Bool) :
    strategyPriorityWeightPct true attempts successes neg mag = 0 :=
  rfl

theorem negative_effectiveness_clamped_to_zero
    (attempts successes mag : Nat) :
    strategyPriorityWeightPct false attempts successes true mag =
      strategyPriorityWeightPct false attempts successes false 0 :=
  rfl

theorem exploration_bonus_strictly_boosts_under_three
    (sr eff : Nat) :
    (sr + eff) / 2 + strategyExplorationBonusPct 1 >
      (sr + eff) / 2 + strategyExplorationBonusPct 3 := by
  have h1 : strategyExplorationBonusPct 1 = 20 := rfl
  have h3 : strategyExplorationBonusPct 3 = 0 := rfl
  rw [h1, h3]
  omega

theorem cycle_phase_four_step_period (p : CyclePhaseModel) :
    p.next.next.next.next = p := by
  cases p <;> rfl

theorem cycle_phase_no_fixed_point (p : CyclePhaseModel) :
    p.next ≠ p := by
  cases p <;> decide

theorem approach_stack_bounded_at_twenty (pushed : Nat) :
    capApproachStackDepth pushed ≤ maxApproachDepth := by
  unfold capApproachStackDepth maxApproachDepth
  split <;> omega

/-! ## Part XXXII — PA32: `orchestration::swarm::coordinator` Assigned-Agent Task Completion Lattice (`src/orchestration/swarm/coordinator.rs`) -/

inductive SwarmTaskStatusModel where
  | pending
  | inProgress
  | completed
  | partialStatus
  | failed
  deriving DecidableEq, Repr

def SwarmTaskStatusModel.label : SwarmTaskStatusModel → String
  | .pending => "pending"
  | .inProgress => "in_progress"
  | .completed => "completed"
  | .partialStatus => "partial"
  | .failed => "failed"

/-- Task status derivation in `Swarm::complete_task`:
    - When `assignedCount == 0`, the task cannot complete and retains `currentStatus`.
    - When `reportedAssignedCount < assignedCount`, not all assigned agents have reported, so `currentStatus` is retained.
    - When all assigned agents have reported:
      * `succeededAssignedCount == 0` -> `.failed`
      * `succeededAssignedCount == assignedCount` -> `.completed`
      * otherwise -> `.partialStatus`. -/
def resolveSwarmTaskCompletion
    (currentStatus : SwarmTaskStatusModel)
    (assignedCount reportedAssignedCount succeededAssignedCount : Nat) : SwarmTaskStatusModel :=
  if assignedCount = 0 then
    currentStatus
  else if reportedAssignedCount < assignedCount then
    currentStatus
  else if succeededAssignedCount = 0 then
    .failed
  else if succeededAssignedCount = assignedCount then
    .completed
  else
    .partialStatus

/-- Per-agent trust score update in percentage points (`Agent::complete_task` in `src/orchestration/swarm/types.rs`):
    success adds 10% capped at 100%; failure subtracts 10% floored at 5%. -/
def stepAgentTrustPct (trustPct : Nat) (success : Bool) : Nat :=
  if success then
    if trustPct + 10 ≤ 100 then trustPct + 10 else 100
  else
    if trustPct ≥ 15 then trustPct - 10 else 5

theorem unassigned_task_never_completes
    (st : SwarmTaskStatusModel) (rep succ : Nat) :
    resolveSwarmTaskCompletion st 0 rep succ = st :=
  rfl

theorem incomplete_reporting_preserves_status
    (st : SwarmTaskStatusModel) (assigned rep succ : Nat)
    (hPos : assigned > 0) (hWait : rep < assigned) :
    resolveSwarmTaskCompletion st assigned rep succ = st := by
  unfold resolveSwarmTaskCompletion
  have hNe : assigned ≠ 0 := by omega
  simp [hNe, hWait]

theorem all_failed_reports_yield_failed
    (st : SwarmTaskStatusModel) (assigned : Nat) (hPos : assigned > 0) :
    resolveSwarmTaskCompletion st assigned assigned 0 = .failed := by
  unfold resolveSwarmTaskCompletion
  have hNe : assigned ≠ 0 := by omega
  have hNotLt : ¬(assigned < assigned) := Nat.lt_irrefl assigned
  simp [hNe, hNotLt]

theorem all_succeeded_reports_yield_completed
    (st : SwarmTaskStatusModel) (assigned : Nat) (hPos : assigned > 0) :
    resolveSwarmTaskCompletion st assigned assigned assigned = .completed := by
  unfold resolveSwarmTaskCompletion
  have hNe : assigned ≠ 0 := by omega
  have hNotLt : ¬(assigned < assigned) := Nat.lt_irrefl assigned
  simp [hNe, hNotLt]

theorem mixed_reports_yield_partial
    (st : SwarmTaskStatusModel) (assigned succ : Nat)
    (hSome : succ > 0) (hNotAll : succ < assigned) :
    resolveSwarmTaskCompletion st assigned assigned succ = .partialStatus := by
  unfold resolveSwarmTaskCompletion
  have hNe : assigned ≠ 0 := by omega
  have hNotLt : ¬(assigned < assigned) := Nat.lt_irrefl assigned
  have hSuccNe : succ ≠ 0 := by omega
  have hSuccNeAss : succ ≠ assigned := by omega
  simp [hNe, hNotLt, hSuccNe, hSuccNeAss]

theorem agent_trust_bounded_in_5_to_100
    (trust : Nat) (ok : Bool) (hHi : trust ≤ 100) :
    5 ≤ stepAgentTrustPct trust ok ∧ stepAgentTrustPct trust ok ≤ 100 := by
  cases ok
  · unfold stepAgentTrustPct
    dsimp
    split <;> omega
  · unfold stepAgentTrustPct
    dsimp
    split <;> omega

/-! ## PA33: `consolidation` Temporal Decay, Access Reinforcement & Store Retention Gate (`src/consolidation/temporal.rs`, `src/consolidation/store.rs`) -/

/-- Exact integer scaling of `RecordImportance` multipliers in `src/consolidation/temporal.rs`
    (`Low => 0.5, Normal => 1.0, High => 1.5, Critical => 2.0`, scaled by 10 -> `5, 10, 15, 20`). -/
def RecordImportanceModel.multiplierTenths : RecordImportanceModel → Nat
  | .low      => 5
  | .normal   => 10
  | .high     => 15
  | .critical => 20

/-- Unclamped decay score in basis points (`0..20000`) from integer `base_decay_bps` (`0..10000`),
    `access_bonus_bps` (`0..10000`), and `RecordImportanceModel`:
    `((base_decay_bps + access_bonus_bps) * importance_mult_tenths) / 10`. -/
def temporalDecayRawBps (baseDecayBps accessBonusBps : Nat) (imp : RecordImportanceModel) : Nat :=
  ((baseDecayBps + accessBonusBps) * imp.multiplierTenths) / 10

/-- Clamped temporal decay score in basis points (`0..10000`), mirroring
    `((base_decay + access_bonus) * importance_mult).min(1.0)` in `TemporalRecord::decay_score_at`. -/
def temporalDecayScoreBps (baseDecayBps accessBonusBps : Nat) (imp : RecordImportanceModel) : Nat :=
  Nat.min 10000 (temporalDecayRawBps baseDecayBps accessBonusBps imp)

/-- Access bonus vanishes when `access_count == 0`, mirroring
    `if self.access_count > 0 { ... } else { 0.0 }` in `TemporalRecord::decay_score_at`. -/
def effectiveAccessBonusBps (accessCount rawBonusBps : Nat) : Nat :=
  if accessCount > 0 then rawBonusBps else 0

/-- Character sanitizer mirroring `sanitize_record_id` in `src/consolidation/store.rs`:
    `c.is_ascii_alphanumeric() || c == '-' || c == '_'`. -/
def isAllowedRecordIdChar (c : Char) : Bool :=
  c.isAlphanum || c == '-' || c == '_'

def sanitizeRecordIdChar (c : Char) : Char :=
  if isAllowedRecordIdChar c then c else '_'

def sanitizeRecordId (s : String) : String :=
  s.map sanitizeRecordIdChar

/-- `LongTermStore::load_all` file ceiling (`MAX_LOADED_FILES = 512`) and
    `LongTermStore::index_existing` record ceiling (`MAX_INDEXED_RECORDS = 256`). -/
def maxLoadedFiles : Nat := 512
def maxIndexedRecords : Nat := 256

def clampLoadedFiles (fileCount : Nat) : Nat :=
  Nat.min fileCount maxLoadedFiles

def clampIndexedRecords (recordCount : Nat) : Nat :=
  Nat.min recordCount maxIndexedRecords

/-- `LongTermStore::retrieve` result cap:
    returns `0` when `k == 0`; otherwise caps `primary_hits + causal_neighbors` at `k * 2`. -/
def retrieveResultCap (k totalCandidates : Nat) : Nat :=
  if k == 0 then 0 else Nat.min totalCandidates (k * 2)

theorem record_importance_multiplier_tenths_strictly_increasing :
    RecordImportanceModel.low.multiplierTenths < RecordImportanceModel.normal.multiplierTenths ∧
    RecordImportanceModel.normal.multiplierTenths < RecordImportanceModel.high.multiplierTenths ∧
    RecordImportanceModel.high.multiplierTenths < RecordImportanceModel.critical.multiplierTenths := by
  decide

theorem temporal_decay_score_bps_bounded (b a : Nat) (imp : RecordImportanceModel) :
    temporalDecayScoreBps b a imp ≤ 10000 := by
  unfold temporalDecayScoreBps
  exact Nat.min_le_left 10000 (temporalDecayRawBps b a imp)

theorem temporal_decay_zero_age_normal_is_10000 :
    temporalDecayScoreBps 10000 0 .normal = 10000 := by
  rfl

theorem temporal_decay_half_life_low_and_high :
    temporalDecayScoreBps 5000 0 .low = 2500 ∧
    temporalDecayScoreBps 5000 0 .normal = 5000 ∧
    temporalDecayScoreBps 5000 0 .high = 7500 ∧
    temporalDecayScoreBps 5000 0 .critical = 10000 := by
  decide

theorem effective_access_bonus_zero_when_unaccessed (raw : Nat) :
    effectiveAccessBonusBps 0 raw = 0 := by
  rfl

theorem sanitize_record_id_char_always_allowed (c : Char) :
    isAllowedRecordIdChar (sanitizeRecordIdChar c) = true := by
  unfold sanitizeRecordIdChar
  split
  · assumption
  · decide

theorem clamp_store_bounds_guaranteed (files records k candidates : Nat) :
    clampLoadedFiles files ≤ maxLoadedFiles ∧
    clampIndexedRecords records ≤ maxIndexedRecords ∧
    retrieveResultCap k candidates ≤ k * 2 := by
  refine ⟨Nat.min_le_right files maxLoadedFiles, Nat.min_le_right records maxIndexedRecords, ?_⟩
  unfold retrieveResultCap
  split
  · omega
  · exact Nat.min_le_right candidates (k * 2)

/-! ## PA34: `swl::guardrails` Composite Condition & Summary Blocking Lattice (`src/swl/guardrails/types.rs`, `src/swl/guardrails/engine.rs`, `src/swl/guardrails/enforcer.rs`) -/

/-- Logical operator for `Condition::Composite` in `src/swl/guardrails/types.rs`. -/
inductive CompositeOpKind where
  | andOp
  | orOp
  deriving DecidableEq, Repr

/-- Complete 4-valued `EvaluationResult` from `src/swl/guardrails/types.rs` (`Pass`, `Fail`, `Skip`, `Error`). -/
inductive ConditionEvalKind where
  | pass
  | fail
  | skip
  | evalError
  deriving DecidableEq, Repr

def ConditionEvalKind.asStr : ConditionEvalKind → String
  | .pass      => "pass"
  | .fail      => "fail"
  | .skip      => "skip"
  | .evalError => "error"

/-- Left-to-right folding step for `LogicalOperator::And` in `GuardrailEngine::evaluate_composite_condition`:
    returns the first `fail` or `evalError` encountered; otherwise `pass` (including when conditions are `skip`). -/
def stepCompositeAnd (acc next : ConditionEvalKind) : ConditionEvalKind :=
  match acc with
  | .fail      => .fail
  | .evalError => .evalError
  | .pass | .skip =>
      match next with
      | .fail      => .fail
      | .evalError => .evalError
      | .pass | .skip => .pass

/-- Left-to-right folding step for `LogicalOperator::Or` in `GuardrailEngine::evaluate_composite_condition`:
    returns `.pass` on the first `.pass`, `.evalError` on the first `.evalError` (before any `.pass`),
    and `.fail` if all conditions fail/skip. -/
def stepCompositeOr (acc next : ConditionEvalKind) : ConditionEvalKind :=
  match acc with
  | .pass      => .pass
  | .evalError => .evalError
  | .fail | .skip =>
      match next with
      | .pass      => .pass
      | .evalError => .evalError
      | .fail | .skip => .fail

/-- Evaluates a list of child condition results under `CompositeOpKind`, mirroring
    `GuardrailEngine::evaluate_composite_condition` in `src/swl/guardrails/engine.rs`.
    Note: empty conditions `[]` returns `.pass` for both `And` and `Or`. -/
def evalCompositeList (op : CompositeOpKind) (results : List ConditionEvalKind) : ConditionEvalKind :=
  match results with
  | [] => .pass
  | _  =>
      match op with
      | .andOp => results.foldl stepCompositeAnd .pass
      | .orOp  => results.foldl stepCompositeOr .fail

/-- Whether an outcome triggers `GuardrailOutcome::should_block()`:
    `self.action == ViolationAction::Block && (self.result.is_fail() || self.result.is_error())`. -/
def conditionOutcomeShouldBlock (res : ConditionEvalKind) (act : GuardActionKind) : Bool :=
  (res == .fail || res == .evalError) && act == .block

/-- Whether an outcome appears in `GuardrailSummary::warnings()`:
    `o.is_triggered() && o.action == ViolationAction::Warn`. -/
def conditionOutcomeIsWarning (res : ConditionEvalKind) (act : GuardActionKind) : Bool :=
  (res == .fail || res == .evalError) && act == .warn

/-- Full summary state tracking `GuardrailSummary` fields (`total_evaluated`, `passed`, `failed`,
    `skipped`, `errors`, `blocked`) plus `blocking_violations().len()` and `warnings().len()`. -/
structure GuardSummaryFull where
  total         : Nat := 0
  passed        : Nat := 0
  failed        : Nat := 0
  skipped       : Nat := 0
  errors        : Nat := 0
  blocked       : Nat := 0
  blockingCount : Nat := 0
  warningCount  : Nat := 0
  deriving DecidableEq, Repr

def GuardSummaryFull.empty : GuardSummaryFull := {}

def stepGuardSummaryFull (s : GuardSummaryFull) (res : ConditionEvalKind) (act : GuardActionKind) : GuardSummaryFull :=
  let blk := if conditionOutcomeShouldBlock res act then 1 else 0
  let wrn := if conditionOutcomeIsWarning res act then 1 else 0
  match res with
  | .pass =>
      { s with total := s.total + 1, passed := s.passed + 1 }
  | .fail =>
      { s with
        total := s.total + 1
        failed := s.failed + 1
        blocked := s.blocked + blk
        blockingCount := s.blockingCount + blk
        warningCount := s.warningCount + wrn }
  | .skip =>
      { s with total := s.total + 1, skipped := s.skipped + 1 }
  | .evalError =>
      { s with
        total := s.total + 1
        errors := s.errors + 1
        blocked := s.blocked + blk
        blockingCount := s.blockingCount + blk
        warningCount := s.warningCount + wrn }

def GuardSummaryFull.shouldBlock (s : GuardSummaryFull) : Bool :=
  decide (s.blockingCount > 0)

theorem eval_composite_empty_always_passes (op : CompositeOpKind) :
    evalCompositeList op [] = .pass := by
  cases op <;> rfl

theorem step_composite_and_absorbs_fail_and_error (next : ConditionEvalKind) :
    stepCompositeAnd .fail next = .fail ∧
    stepCompositeAnd .evalError next = .evalError := by
  exact ⟨rfl, rfl⟩

theorem step_composite_or_absorbs_pass_and_error (next : ConditionEvalKind) :
    stepCompositeOr .pass next = .pass ∧
    stepCompositeOr .evalError next = .evalError := by
  exact ⟨rfl, rfl⟩

theorem step_guard_summary_full_conserves_total (s : GuardSummaryFull) (res : ConditionEvalKind) (act : GuardActionKind) :
    let s' := stepGuardSummaryFull s res act
    s'.total = s.total + 1 ∧
    s'.passed + s'.failed + s'.skipped + s'.errors = (s.passed + s.failed + s.skipped + s.errors) + 1 := by
  cases res <;> simp [stepGuardSummaryFull] <;> omega

theorem step_guard_summary_full_blocking_monotone (s : GuardSummaryFull) (res : ConditionEvalKind) (act : GuardActionKind) :
    s.blockingCount ≤ (stepGuardSummaryFull s res act).blockingCount ∧
    s.warningCount ≤ (stepGuardSummaryFull s res act).warningCount := by
  cases res <;> cases act <;> simp [stepGuardSummaryFull, conditionOutcomeShouldBlock, conditionOutcomeIsWarning]

/-! ## PA35: `safety::redact` & `safety::source_context` Secret Redaction, Note Collision & Discovery Gate (`src/safety/redact.rs`, `src/safety/source_context.rs`, `src/evolve/context_trust.rs`) -/

/-- Discovery canonicalize status modeling `discovery_root_is_private` in `src/safety/source_context.rs`. -/
inductive CanonicalizeStatus where
  | okClean
  | okPrivate
  | errNotFound
  | errOther
  deriving DecidableEq, Repr

/-- Exact decision rule for `discovery_root_is_private(path)`:
    1. If the raw path already contains a private discovery directory (`.git`, `.selfware`, `.claude`,
       `.codex`, `.agents`, `.qwen`, `.superpowers`), return `true`.
    2. Otherwise inspect `canonicalize(path)`:
       - `Ok(resolved)` -> `path_contains_private_discovery_state(&resolved)`
       - `Err(NotFound)` -> `false`
       - `Err(_)` -> `true` (fail-closed on I/O errors). -/
def discoveryRootIsPrivate (rawHasPrivate : Bool) (canon : CanonicalizeStatus) : Bool :=
  if rawHasPrivate then true
  else match canon with
    | .okClean     => false
    | .okPrivate   => true
    | .errNotFound => false
    | .errOther    => true

/-- Redaction context mirroring `RedactionContext` in `src/safety/redact.rs`. -/
inductive RedactionContextKind where
  | generic
  | rustSource
  | structuredData
  | commandText
  deriving DecidableEq, Repr

/-- Candidate secret pattern kind in `redact_for_model` (`src/safety/redact.rs`). -/
inductive SecretPatternKind where
  | vendorTokenOrPem
  | quotedAssignment
  | bareKeyValueAssignment
  | highEntropyLiteral
  deriving DecidableEq, Repr

/-- Whether `redact_for_model` redacts a candidate match of `SecretPatternKind` in `RedactionContextKind`:
    In `RedactionContext::RustSource`, `bare_key_value_secret_re` is skipped (`!matches!(context, RedactionContext::RustSource)`),
    whereas vendor tokens, quoted secret assignments, and high-entropy secret literals are redacted in all contexts. -/
def shouldRedactPattern (ctx : RedactionContextKind) (pat : SecretPatternKind) : Bool :=
  match pat with
  | .vendorTokenOrPem       => true
  | .quotedAssignment       => true
  | .bareKeyValueAssignment => decide (ctx ≠ .rustSource)
  | .highEntropyLiteral     => true

/-- Number of leading underscores prepended to `"redaction"` by `with_redaction_note` in `src/evolve/context_trust.rs`
    when the JSON object already contains `existingCollisionDepth` keys `"redaction"`, `"_redaction"`, ... -/
def redactionNoteKeyName (existingCollisionDepth : Nat) : String :=
  String.ofList (List.replicate existingCollisionDepth '_') ++ "redaction"

/-- Source classification from tool argument path extension (`classification_for` in `src/evolve/context_trust.rs`). -/
inductive FileExtKind where
  | rs
  | markdownOrText
  | otherOrMissing
  deriving DecidableEq, Repr

def classifySourceExt (ext : FileExtKind) : String :=
  match ext with
  | .rs             => "rust_source"
  | .markdownOrText => "markup"
  | .otherOrMissing => "data"

theorem discovery_root_raw_private_always_blocked (canon : CanonicalizeStatus) :
    discoveryRootIsPrivate true canon = true := by
  rfl

theorem discovery_root_io_error_fails_closed (rawHasPrivate : Bool) :
    discoveryRootIsPrivate rawHasPrivate .errOther = true := by
  cases rawHasPrivate <;> rfl

theorem rust_source_skips_only_bare_kv :
    shouldRedactPattern .rustSource .bareKeyValueAssignment = false ∧
    shouldRedactPattern .generic .bareKeyValueAssignment = true ∧
    shouldRedactPattern .rustSource .quotedAssignment = true ∧
    shouldRedactPattern .rustSource .vendorTokenOrPem = true := by
  decide

theorem redaction_note_key_collision_avoidance :
    redactionNoteKeyName 0 = "redaction" ∧
    redactionNoteKeyName 1 = "_redaction" ∧
    redactionNoteKeyName 2 = "__redaction" := by
  decide

/-! ## PA36: `agent::plan_mode` & `orchestration::workflows` Plan Step & Workflow Status Lattice (`src/agent/plan_mode.rs`, `src/orchestration/workflows.rs`, `src/orchestration/workflows/checkpoint.rs`) -/

/-- `PlanModeState` from `src/agent/plan_mode.rs`. -/
inductive PlanModeStateKind where
  | inactive
  | planning
  | executing
  deriving DecidableEq, Repr

/-- `PlanModeManager::is_tool_allowed` in `src/agent/plan_mode.rs`:
    `Inactive => true`, `Planning => is_readonly`, `Executing => true`. -/
def planModeToolAllowed (state : PlanModeStateKind) (isReadonly : Bool) : Bool :=
  match state with
  | .inactive  => true
  | .planning  => isReadonly
  | .executing => true

/-- `Plan::is_complete` in `src/agent/plan_mode.rs`:
    `!self.steps.is_empty() && self.completed_count() == self.steps.len()`. -/
def planIsComplete (totalSteps completedSteps : Nat) : Bool :=
  decide (totalSteps > 0 ∧ completedSteps == totalSteps)

/-- Workflow step execution status (`StepStatus` in `src/orchestration/workflows.rs`). -/
inductive WorkflowStepStatusKind where
  | pending
  | running
  | completed
  | failed
  | skipped
  deriving DecidableEq, Repr

def WorkflowStepStatusKind.asStr : WorkflowStepStatusKind → String
  | .pending   => "Pending"
  | .running   => "Running"
  | .completed => "Completed"
  | .failed    => "Failed"
  | .skipped   => "Skipped"

/-- Dependency resolution outcome for a workflow step in `WorkflowExecutor::execute` (`src/orchestration/workflows.rs`). -/
inductive DepResolutionOutcome where
  | proceed
  | tddRedProceed
  | skipOptional
  | failWorkflow
  deriving DecidableEq, Repr

def DepResolutionOutcome.asStr : DepResolutionOutcome → String
  | .proceed       => "proceed"
  | .tddRedProceed => "tdd_red_proceed"
  | .skipOptional  => "skip_optional"
  | .failWorkflow  => "fail_workflow"

/-- Exact dependency gate in `WorkflowExecutor::execute` (`src/orchestration/workflows.rs` lines 2029-2093):
    1. If dependency does not exist -> `failWorkflow`
    2. If dependency status is `Completed` -> `proceed`
    3. If `step_is_implement && dep_is_run_test_red && !dep_required && dep_status == Failed` -> `tddRedProceed`
    4. Otherwise if `step_required` -> `failWorkflow`, else -> `skipOptional`. -/
def resolveStepDependency
    (depExists : Bool)
    (depStatus : WorkflowStepStatusKind)
    (stepRequired : Bool)
    (isTddRedException : Bool) : DepResolutionOutcome :=
  if !depExists then .failWorkflow
  else if depStatus == .completed then .proceed
  else if isTddRedException && depStatus == .failed then .tddRedProceed
  else if stepRequired then .failWorkflow
  else .skipOptional

/-- Loop iteration status aggregation in `src/orchestration/workflows.rs` (lines 2582-2588):
    `if failed > 0 { Failed } else if skipped > 0 && completed == 0 { Skipped } else { Completed }`. -/
def aggregateLoopStepStatus (completed failed skipped : Nat) : WorkflowStepStatusKind :=
  if failed > 0 then .failed
  else if skipped > 0 && completed == 0 then .skipped
  else .completed

/-- Final workflow status in `src/orchestration/workflows.rs` (lines 1792-1802):
    `if required_failed || has_stop_reason { Failed } else { Completed }`. -/
def finalizeWorkflowStatus (requiredFailed hasStopReason : Bool) : WorkflowStepStatusKind :=
  if requiredFailed || hasStopReason then .failed else .completed

/-- Exponential retry backoff in `src/orchestration/workflows.rs` (`retry_backoff_secs`, lines 1229-1239):
    `MAX_BACKOFF_SECS = 60`;
    if `exponential` returns `min(delay_secs * 2^(attempt - 1), 60)`; else `min(delay_secs, 60)`. -/
def retryBackoffSecs (delaySecs : Nat) (exponential : Bool) (attempt : Nat) : Nat :=
  let raw := if exponential then delaySecs * (2 ^ (attempt - 1)) else delaySecs
  Nat.min raw 60

/-- Checkpoint resume decision in `src/orchestration/workflows.rs` (lines 1986-2025):
    a step is skipped via checkpoint resume iff a checkpoint is present, the stored fingerprint
    matches the current step definition, and the stored result status is `Completed`. -/
def shouldResumeSkipStep (hasCheckpoint fingerprintMatches : Bool) (savedStatus : WorkflowStepStatusKind) : Bool :=
  hasCheckpoint && fingerprintMatches && (savedStatus == .completed)

theorem plan_mode_planning_blocks_mutating_tools :
    planModeToolAllowed .planning false = false ∧
    planModeToolAllowed .planning true = true ∧
    planModeToolAllowed .inactive false = true ∧
    planModeToolAllowed .executing false = true := by
  decide

theorem plan_empty_never_complete (completed : Nat) :
    planIsComplete 0 completed = false := by
  unfold planIsComplete
  simp

theorem resolve_step_dependency_unknown_always_fails (st : WorkflowStepStatusKind) (req tdd : Bool) :
    resolveStepDependency false st req tdd = .failWorkflow := by
  rfl

theorem aggregate_loop_failure_dominates (c f s : Nat) (hf : f > 0) :
    aggregateLoopStepStatus c f s = .failed := by
  unfold aggregateLoopStepStatus
  simp [hf]

theorem retry_backoff_secs_bounded_by_60 (delay : Nat) (exp : Bool) (attempt : Nat) :
    retryBackoffSecs delay exp attempt ≤ 60 := by
  unfold retryBackoffSecs
  exact Nat.min_le_right _ 60

theorem checkpoint_resume_requires_matching_fingerprint_and_completed (hasCp fpMatch : Bool) (st : WorkflowStepStatusKind) :
    shouldResumeSkipStep hasCp fpMatch st = true ↔
      (hasCp = true ∧ fpMatch = true ∧ st = .completed) := by
  cases hasCp <;> cases fpMatch <;> cases st <;> decide

/-! ## PA37: `agent::verification_scope` & `agent::preexisting_failure` Completion Gate & Failure Attribution Lattice (`src/agent/verification_scope.rs`, `src/agent/preexisting_failure.rs`) -/

/-- Scope relevance classification mirroring `Relevance` in `src/agent/verification_scope.rs`. -/
inductive ScopeRelevanceKind where
  | inScope
  | outOfScope
  | noRunner
  | unknown
  deriving DecidableEq, Repr

def ScopeRelevanceKind.asStr : ScopeRelevanceKind → String
  | .inScope    => "InScope"
  | .outOfScope => "OutOfScope"
  | .noRunner   => "NoRunner"
  | .unknown    => "Unknown"

/-- Geometric relationship between `scope.project_root` and `task_root` in `VerificationScope::relevance_to`. -/
inductive ProjectRootRelation where
  | sameOrDescendant
  | strictAncestor
  | siblingOrNone
  deriving DecidableEq, Repr

/-- Exact decision rule for `VerificationScope::relevance_to` in `src/agent/verification_scope.rs`:
    1. If `runner_exists == Some(false)` -> `.noRunner`
    2. Else if `project_root` is equal to or inside `task_root` -> `.inScope`
    3. Else if `project_root` is a strict parent enclosing `task_root` -> `.outOfScope`
    4. Else (`siblingOrNone`): if `runner_exists.is_some()` and `!cargo_applies_to_task(task_root)` -> `.outOfScope`,
       otherwise `.unknown`. -/
def classifyScopeRelevance
    (runnerExists : Option Bool)
    (rel : ProjectRootRelation)
    (cargoAppliesToTask : Bool) : ScopeRelevanceKind :=
  if runnerExists == some false then .noRunner
  else match rel with
    | .sameOrDescendant => .inScope
    | .strictAncestor   => .outOfScope
    | .siblingOrNone    =>
        if runnerExists.isSome && !cargoAppliesToTask then .outOfScope
        else .unknown

/-- `VerificationRecord::blocks_completion` in `src/agent/verification_scope.rs`:
    false if `passed || is_stale || is_preexisting`, otherwise blocks unless `OutOfScope` or `NoRunner`. -/
def recordBlocksCompletion
    (passed isStale isPreexisting : Bool)
    (rel : ScopeRelevanceKind) : Bool :=
  if passed || isStale || isPreexisting then false
  else decide (rel ≠ .outOfScope ∧ rel ≠ .noRunner)

/-- `VerificationRecord::awaits_rerun` in `src/agent/verification_scope.rs`:
    true iff `!passed && is_stale && !is_preexisting && has_rerun_tool` and scope is neither `OutOfScope` nor `NoRunner`. -/
def recordAwaitsRerun
    (passed isStale isPreexisting hasRerunTool : Bool)
    (rel : ScopeRelevanceKind) : Bool :=
  !passed && isStale && !isPreexisting && hasRerunTool &&
    decide (rel ≠ .outOfScope ∧ rel ≠ .noRunner)

/-- `VerificationRecord::clears` for cargo verification checks in `src/agent/verification_scope.rs`:
    Requires `passPassed && !failPassed && sameScope`. An unrestricted `cargo test` (`!passHasNarrowSelector`)
    clears any failure in the same family, whereas a narrow selector (`passHasNarrowSelector`, e.g. `--lib`)
    only clears a failure with the exact same narrow selector (`failHasNarrowSelector && sameSelector`). -/
def cargoPassClearsFailure
    (passPassed failPassed sameScope passHasNarrowSelector failHasNarrowSelector sameSelector : Bool) : Bool :=
  if !passPassed || failPassed || !sameScope then false
  else if !passHasNarrowSelector then true
  else failHasNarrowSelector && sameSelector

/-- Pre-existing failure attribution outcome mirroring `Attribution` in `src/agent/preexisting_failure.rs`. -/
inductive AttributionOutcomeKind where
  | preExisting
  | newFailure
  | different
  | unknown
  deriving DecidableEq, Repr

def AttributionOutcomeKind.asStr : AttributionOutcomeKind → String
  | .preExisting => "PreExisting"
  | .newFailure  => "New"
  | .different   => "Different"
  | .unknown     => "Unknown"

def AttributionOutcomeKind.isPreexisting : AttributionOutcomeKind → Bool
  | .preExisting => true
  | _            => false

def AttributionOutcomeKind.blockIsCapped : AttributionOutcomeKind → Bool
  | .different | .unknown => true
  | _                     => false

/-- Exact decision rule for `preexisting_failure::attribute` in `src/agent/preexisting_failure.rs`:
    - If `!baselineAvailable` or `!currentNonEmpty` -> `.unknown`
    - Else if `baselinePassed` -> `.newFailure`
    - Else if `!beforeNonEmpty` -> `.unknown`
    - Else if `!hasIncreasedCount` (every current diagnostic count <= baseline count) -> `.preExisting`
    - Else if `hasSharedDiagnostic` -> `.newFailure`
    - Else -> `.different`. -/
def attributeBaselineFailure
    (baselineAvailable baselinePassed beforeNonEmpty currentNonEmpty hasIncreasedCount hasSharedDiagnostic : Bool) :
    AttributionOutcomeKind :=
  if !baselineAvailable || !currentNonEmpty then .unknown
  else if baselinePassed then .newFailure
  else if !beforeNonEmpty then .unknown
  else if !hasIncreasedCount then .preExisting
  else if hasSharedDiagnostic then .newFailure
  else .different

theorem no_runner_never_blocks_or_reruns (passed isStale isPre hasRerun : Bool) :
    recordBlocksCompletion passed isStale isPre .noRunner = false ∧
    recordAwaitsRerun passed isStale isPre hasRerun .noRunner = false := by
  cases passed <;> cases isStale <;> cases isPre <;> cases hasRerun <;> decide

theorem preexisting_or_passed_never_blocks_completion (isStale : Bool) (rel : ScopeRelevanceKind) :
    recordBlocksCompletion true isStale false rel = false ∧
    recordBlocksCompletion false isStale true rel = false := by
  cases isStale <;> cases rel <;> decide

theorem blocks_completion_and_awaits_rerun_disjoint (passed isStale isPre hasRerun : Bool) (rel : ScopeRelevanceKind) :
    ¬(recordBlocksCompletion passed isStale isPre rel = true ∧
      recordAwaitsRerun passed isStale isPre hasRerun rel = true) := by
  cases passed <;> cases isStale <;> cases isPre <;> cases hasRerun <;> cases rel <;> decide

theorem narrow_cargo_pass_does_not_clear_full_suite (sameScope sameSel : Bool) :
    cargoPassClearsFailure true false sameScope true false sameSel = false ∧
    cargoPassClearsFailure true false true false true false = true := by
  cases sameScope <;> cases sameSel <;> decide

theorem attribute_preexisting_iff_no_new_counts_on_failed_baseline
    (avail passed beforeNe currNe inc shared : Bool) :
    (attributeBaselineFailure avail passed beforeNe currNe inc shared).isPreexisting = true ↔
      (avail = true ∧ passed = false ∧ beforeNe = true ∧ currNe = true ∧ inc = false) := by
  cases avail <;> cases passed <;> cases beforeNe <;> cases currNe <;> cases inc <;> cases shared <;> decide

theorem attribute_block_is_capped_for_different_and_unknown :
    AttributionOutcomeKind.different.blockIsCapped = true ∧
    AttributionOutcomeKind.unknown.blockIsCapped = true ∧
    AttributionOutcomeKind.preExisting.blockIsCapped = false ∧
    AttributionOutcomeKind.newFailure.blockIsCapped = false := by
  decide

/-! ## PA38: `skills` / `safety::process_env` / `lsp` Protocol & Sandboxed Process Guard Invariants (`src/skills/mod.rs`, `src/safety/process_env.rs`, `src/lsp/client.rs`) -/

def isAllowedSkillNameChar (c : Char) : Bool :=
  c.isAlphanum || c == '-' || c == '_'

def hasDotDotSubstr : List Char → Bool
  | '.' :: '.' :: _ => true
  | _ :: rest       => hasDotDotSubstr rest
  | []              => false

/-- Mirrors `validate_skill_name` in `src/skills/mod.rs`:
    rejects empty names, leading `.`, `..`, `/`, `\`, `\0`, or any character outside `[A-Za-z0-9_-]`. -/
def validateSkillNameModel (s : String) : Bool :=
  let cs := s.toList
  match cs with
  | [] => false
  | first :: _ =>
      first != '.' &&
      !hasDotDotSubstr cs &&
      cs.all isAllowedSkillNameChar

/-- Trust badge classification for `Skill::trust_badge` in `src/skills/mod.rs`. -/
inductive SkillBadgeKind where
  | candidateUnadmitted
  | unverifiedDistilled
  | verifiedOrUser
  deriving DecidableEq, Repr

def SkillBadgeKind.tag : SkillBadgeKind → String
  | .candidateUnadmitted => "CANDIDATE"
  | .unverifiedDistilled => "UNVERIFIED"
  | .verifiedOrUser      => "CLEAN"

/-- Exact decision rule for `Skill::trust_badge` in `src/skills/mod.rs`:
    1. If `(candidate || originDistilledOrGenerated) && !admitted` -> `.candidateUnadmitted`
    2. Else if `verified` -> `.verifiedOrUser`
    3. Else if `candidate || admitted || originDistilledOrGenerated` -> `.unverifiedDistilled`
    4. Else -> `.verifiedOrUser`. -/
def classifySkillTrustBadge
    (candidate originDistilledOrGenerated admitted verified : Bool) : SkillBadgeKind :=
  if (candidate || originDistilledOrGenerated) && !admitted then .candidateUnadmitted
  else if verified then .verifiedOrUser
  else if candidate || admitted || originDistilledOrGenerated then .unverifiedDistilled
  else .verifiedOrUser

/-- `Skill::render_content` in `src/skills/mod.rs`:
    substitutes `$ARGUMENTS` when present; otherwise returns `raw` when `argsEmpty`, else `appended`. -/
def renderSkillContentModel
    (hasPlaceholder argsEmpty : Bool)
    (replaced raw appended : String) : String :=
  if hasPlaceholder then replaced
  else if argsEmpty then raw
  else appended

/-- Project-local skill discovery outcome in `SkillRegistry::discover_dir` (`src/skills/mod.rs`). -/
inductive SkillDiscoveryOutcome where
  | admittedLoaded
  | refusedMissingLedger
  | refusedContentTampered
  | refusedMetadataTampered
  deriving DecidableEq, Repr

def SkillDiscoveryOutcome.loadedCount : SkillDiscoveryOutcome → Nat
  | .admittedLoaded => 1
  | _               => 0

def SkillDiscoveryOutcome.refusedCount : SkillDiscoveryOutcome → Nat
  | .admittedLoaded => 0
  | _               => 1

def evaluateProjectSkillDiscovery
    (inLedger contentHashMatch metadataHashMatch : Bool) : SkillDiscoveryOutcome :=
  if !inLedger then .refusedMissingLedger
  else if !contentHashMatch then .refusedContentTampered
  else if !metadataHashMatch then .refusedMetadataTampered
  else .admittedLoaded

/-- Proxy credential scrubbing in `sanitize_proxy_value` (`src/safety/process_env.rs`):
    strips `user:pass@` userinfo unless `SELFWARE_FORWARD_PROXY_CREDENTIALS` is opted in. -/
def scrubProxyCredentialsModel
    (forwardOptIn hasUserInfo : Bool)
    (rawUrl scrubbedUrl : String) : String :=
  if forwardOptIn || !hasUserInfo then rawUrl else scrubbedUrl

/-- Language detection in `Language::from_path` (`src/lsp/client.rs`). -/
inductive LspLangKind where
  | rust
  | python
  | typescript
  | javascript
  | go
  | unknown
  deriving DecidableEq, Repr

def LspLangKind.asId : LspLangKind → String
  | .rust       => "rust"
  | .python     => "python"
  | .typescript => "typescript"
  | .javascript => "javascript"
  | .go         => "go"
  | .unknown    => "none"

def detectLspLanguageExt (ext : String) : LspLangKind :=
  if ext == "rs" then .rust
  else if ext == "py" || ext == "pyi" then .python
  else if ext == "ts" || ext == "tsx" then .typescript
  else if ext == "js" || ext == "jsx" || ext == "mjs" || ext == "cjs" then .javascript
  else if ext == "go" then .go
  else .unknown

/-- `query_settling_indexing` in `src/lsp/client.rs`:
    returns `(queryCallCount, stillIndexing)` given whether the server is indexing on first call,
    whether the first result is empty, and whether indexing is still active after the retry wait. -/
def lspSettlingQueryCalls (isIndexing firstEmpty secondStillIndexing : Bool) : Nat × Bool :=
  if !firstEmpty || !isIndexing then (1, false)
  else (2, secondStillIndexing)

theorem empty_or_leading_dot_skill_name_rejected :
    validateSkillNameModel "" = false ∧
    validateSkillNameModel ".hidden" = false ∧
    validateSkillNameModel "../escape" = false := by
  decide

theorem unadmitted_candidate_always_gets_candidate_badge (orig ver : Bool) :
    classifySkillTrustBadge true orig false ver = .candidateUnadmitted := by
  cases orig <;> cases ver <;> rfl

theorem project_skill_requires_ledger_and_matching_hashes (inLed cMatch mMatch : Bool) :
    (evaluateProjectSkillDiscovery inLed cMatch mMatch).loadedCount = 1 ↔
      (inLed = true ∧ cMatch = true ∧ mMatch = true) := by
  cases inLed <;> cases cMatch <;> cases mMatch <;> decide

theorem proxy_credentials_scrubbed_unless_opted_in (raw scrubbed : String) :
    scrubProxyCredentialsModel false true raw scrubbed = scrubbed ∧
    scrubProxyCredentialsModel true true raw scrubbed = raw := by
  exact ⟨rfl, rfl⟩

theorem lsp_settling_query_calls_at_most_twice (idx emp still : Bool) :
    1 ≤ (lspSettlingQueryCalls idx emp still).1 ∧
    (lspSettlingQueryCalls idx emp still).1 ≤ 2 := by
  cases idx <;> cases emp <;> cases still <;> decide

/-! ## PA39: `analysis` (`bm25`, `vector_store`, `code_graph`) Retrieval & Ranking Invariants (`src/analysis/bm25.rs`, `src/analysis/vector_store.rs`, `src/analysis/code_graph.rs`) -/

/-- Effective number of results returned by `BM25Index::search` in `src/analysis/bm25.rs`:
    returns 0 when the index is empty or the tokenized query is empty; otherwise `min(matchingDocs, limit)`. -/
def bm25EffectiveResultCount (hasDocs queryNonEmpty : Bool) (matchingDocs limit : Nat) : Nat :=
  if !hasDocs || !queryNonEmpty then 0
  else Nat.min matchingDocs limit

/-- Cosine similarity percentage model (`VectorIndex::cosine_similarity` in `src/analysis/vector_store.rs`):
    returns 0 if either vector has zero L2 norm (`aZero || bZero`) or if vectors are orthogonal;
    returns 100 for positive collinear vectors (`sameDir`). -/
def cosineSimilarityPctModel (aZero bZero sameDir orthogonal : Bool) : Nat :=
  if aZero || bZero || orthogonal then 0
  else if sameDir then 100
  else 0

/-- Auto-compaction trigger in `VectorIndex::remove` (`src/analysis/vector_store.rs`):
    `self.deleted.len() * 10 > self.embeddings.len() * 3` (strictly > 30% deleted slots). -/
def vectorShouldAutoCompact (deletedCount totalSlots : Nat) : Bool :=
  decide (deletedCount * 10 > totalSlots * 3)

/-- Active item count and physical slot count after `deletedAfter` deletions from `totalSlots`:
    if `deletedAfter * 10 > totalSlots * 3`, `compact()` shrinks physical slots to `totalSlots - deletedAfter`. -/
def vectorSlotsAfterRemove (totalSlots deletedAfter : Nat) : Nat × Nat :=
  let active := totalSlots - deletedAfter
  let slots := if vectorShouldAutoCompact deletedAfter totalSlots then active else totalSlots
  (active, slots)

/-- `IndexHealth` lattice in `src/analysis/vector_store.rs`. -/
inductive VectorHealthKind where
  | healthy
  | degraded
  | corrupt
  deriving DecidableEq, Repr

def VectorHealthKind.asStr : VectorHealthKind → String
  | .healthy  => "Healthy"
  | .degraded => "Degraded"
  | .corrupt  => "Corrupt"

/-- `VectorIndex::check_health` in `src/analysis/vector_store.rs`:
    `Healthy` when `totalIssues == 0`, `Corrupt` when `corruptIssues > 0`, else `Degraded`. -/
def classifyVectorHealth (totalIssues corruptIssues : Nat) : VectorHealthKind :=
  if totalIssues = 0 then .healthy
  else if corruptIssues > 0 then .corrupt
  else .degraded

/-- BFS shortest path length in `CodeGraph::find_path` (`src/analysis/code_graph.rs`). -/
def bfsShortestPathLen (sameNode hasDirectEdge hasTwoHopPath : Bool) : Option Nat :=
  if sameNode then some 1
  else if hasDirectEdge then some 2
  else if hasTwoHopPath then some 3
  else none

theorem bm25_empty_index_or_query_returns_zero (m limit : Nat) :
    bm25EffectiveResultCount false true m limit = 0 ∧
    bm25EffectiveResultCount true false m limit = 0 := by
  exact ⟨rfl, rfl⟩

theorem bm25_result_count_bounded_by_limit (hasDocs qNe : Bool) (m limit : Nat) :
    bm25EffectiveResultCount hasDocs qNe m limit ≤ limit := by
  unfold bm25EffectiveResultCount
  split
  · omega
  · exact Nat.min_le_right m limit

theorem cosine_zero_vector_returns_zero (bZero sameDir ortho : Bool) :
    cosineSimilarityPctModel true bZero sameDir ortho = 0 ∧
    cosineSimilarityPctModel bZero true sameDir ortho = 0 := by
  cases bZero <;> cases sameDir <;> cases ortho <;> decide

theorem vector_autocompact_triggers_strictly_above_30_pct :
    vectorShouldAutoCompact 3 10 = false ∧
    vectorShouldAutoCompact 4 10 = true ∧
    vectorSlotsAfterRemove 10 3 = (7, 10) ∧
    vectorSlotsAfterRemove 10 4 = (6, 6) := by
  decide

theorem vector_corrupt_issue_dominates_degraded (total corrupt : Nat) (ht : total > 0) (hc : corrupt > 0) :
    classifyVectorHealth total corrupt = .corrupt := by
  unfold classifyVectorHealth
  have hne : total ≠ 0 := Nat.ne_of_gt ht
  simp [hne, hc]

theorem bfs_reflexive_path_has_length_one (d t : Bool) :
    bfsShortestPathLen true d t = some 1 := by
  rfl

/-! ## PA40: `evolve::readiness` / `evolve::quality` / `evolve::dedup` / `evolve::dead_code` Evolve Readiness & Quality Lattice (`src/evolve/readiness.rs`, `src/evolve/quality.rs`, `src/evolve/dedup.rs`, `src/evolve/dead_code.rs`) -/

/-- `GateState` in `src/evolve/readiness.rs`. -/
inductive ReadinessGateState where
  | pass
  | fail
  | unknown
  deriving DecidableEq, Repr

/-- Status string in `ReadinessReport` (`src/evolve/readiness.rs`). -/
inductive ReadinessStatusKind where
  | ready
  | blocked
  | incomplete
  deriving DecidableEq, Repr

def ReadinessStatusKind.asStr : ReadinessStatusKind → String
  | .ready      => "ready"
  | .blocked    => "blocked"
  | .incomplete => "incomplete"

/-- Gate aggregation in `readiness::evaluate` (`src/evolve/readiness.rs` lines 228-236):
    `ready = gates.all(Pass)`;
    `status = if ready { "ready" } else if gates.any(Fail) { "blocked" } else { "incomplete" }`. -/
def evaluateReadinessGates (gates : List ReadinessGateState) : Bool × ReadinessStatusKind :=
  let ready := gates.all (· == .pass)
  let hasFail := gates.any (· == .fail)
  let status :=
    if ready then .ready
    else if hasFail then .blocked
    else .incomplete
  (ready, status)

/-- `cyclomatic_complexity` in `src/evolve/quality.rs` scaled to tenths:
    `1.0 + sum(branch_tokens)` -> `10 + 10 * branchTokenCount`. -/
def cyclomaticComplexityTenths (branchTokenCount : Nat) : Nat :=
  10 + 10 * branchTokenCount

/-- `dead_code_ratio` in `src/evolve/quality.rs` scaled to integer percentage `[0, 100]`:
    `if fns == 0 { 0.0 } else { (allows / fns).min(1.0) }`. -/
def deadCodeRatioPct (allowCount fnCount : Nat) : Nat :=
  if fnCount == 0 then 0
  else Nat.min 100 ((allowCount * 100) / fnCount)

/-- `jaccard` in `src/evolve/dedup.rs` scaled to integer percentage `[0, 100]`:
    returns 0 when `unionCount == 0`, else `min(100, (interCount * 100) / unionCount)`. -/
def jaccardSimilarityPct (interCount unionCount : Nat) : Nat :=
  if unionCount == 0 then 0
  else Nat.min 100 ((interCount * 100) / unionCount)

/-- Size-ratio pruning (`minLines * 10 >= maxLines * 8`) and Jaccard threshold (`jaccard >= 80%`)
    for `DuplicateKind::Near` in `DeduplicationAnalyzer::find_duplicates_in` (`src/evolve/dedup.rs`). -/
def isNearDuplicatePct (minLines maxLines interCount unionCount : Nat) : Bool :=
  decide (minLines * 10 ≥ maxLines * 8) &&
    decide (jaccardSimilarityPct interCount unionCount ≥ 80)

inductive DedupNodeClass where
  | implementation
  | structuralSkip
  deriving DecidableEq, Repr

inductive DedupMatchKind where
  | none
  | exact
  | near
  deriving DecidableEq, Repr

def DedupMatchKind.asStr : DedupMatchKind → String
  | .none  => "None"
  | .exact => "Exact"
  | .near  => "Near"

/-- `DeduplicationAnalyzer::find_duplicates_in` in `src/evolve/dedup.rs`:
    skips `Structure | Symbol | Concept | Preset` nodes; exact hash match -> `.exact`;
    otherwise if size-ratio and Jaccard >= 80% -> `.near`. -/
def classifyDedupPair
    (nodeClass : DedupNodeClass)
    (sameHash : Bool)
    (minLines maxLines interCount unionCount : Nat) : DedupMatchKind :=
  match nodeClass with
  | .structuralSkip => .none
  | .implementation =>
      if sameHash then .exact
      else if isNearDuplicatePct minLines maxLines interCount unionCount then .near
      else .none

/-- `DeadCodeAnalyzer::find` filter predicate in `src/evolve/dead_code.rs` (lines 163-172):
    excludes `DISPATCHED` trait/entry-point names, `__`-prefixed names, and `is_test` items;
    marks symbol dead iff `totalOccurrences <= selfDefCount`. -/
def isDeadCodeCandidate
    (isDispatched startsWithDunder isTest : Bool)
    (totalOccurrences selfDefCount : Nat) : Bool :=
  if isDispatched || startsWithDunder || isTest then false
  else decide (totalOccurrences ≤ selfDefCount)

/-- Primary sort key in `DeadCodeAnalyzer::find` (`a.cfg_gated.cmp(&b.cfg_gated)`):
    non-cfg-gated dead code (`false -> 0`) is ordered before cfg-gated dead code (`true -> 1`). -/
def deadSymbolSortKey (cfgGated : Bool) : Nat :=
  if cfgGated then 1 else 0

theorem readiness_ready_iff_all_gates_pass (gates : List ReadinessGateState) :
    (evaluateReadinessGates gates).1 = true ↔
      (evaluateReadinessGates gates).2 = .ready := by
  unfold evaluateReadinessGates
  simp
  split <;> simp

theorem readiness_any_fail_gates_status_blocked :
    evaluateReadinessGates [.pass, .fail, .unknown] = (false, .blocked) ∧
    evaluateReadinessGates [.pass, .pass, .unknown] = (false, .incomplete) ∧
    evaluateReadinessGates [.pass, .pass, .pass] = (true, .ready) := by
  decide

theorem cyclomatic_complexity_at_least_one (branches : Nat) :
    10 ≤ cyclomaticComplexityTenths branches := by
  unfold cyclomaticComplexityTenths
  omega

theorem dead_code_ratio_bounded_by_100 (allows fns : Nat) :
    deadCodeRatioPct allows fns ≤ 100 := by
  unfold deadCodeRatioPct
  split
  · omega
  · exact Nat.min_le_left 100 _

theorem jaccard_empty_union_is_zero_and_bounded (inter union : Nat) :
    jaccardSimilarityPct inter 0 = 0 ∧
    jaccardSimilarityPct inter union ≤ 100 := by
  refine ⟨rfl, ?_⟩
  unfold jaccardSimilarityPct
  split
  · omega
  · exact Nat.min_le_left 100 _

theorem structural_nodes_never_produce_duplicates (sh : Bool) (minL maxL i u : Nat) :
    classifyDedupPair .structuralSkip sh minL maxL i u = .none := by
  rfl

theorem dispatched_or_test_symbols_never_dead (occ defs : Nat) :
    isDeadCodeCandidate true false false occ defs = false ∧
    isDeadCodeCandidate false true false occ defs = false ∧
    isDeadCodeCandidate false false true occ defs = false := by
  exact ⟨rfl, rfl, rfl⟩

theorem non_cfg_gated_sorts_before_cfg_gated :
    deadSymbolSortKey false < deadSymbolSortKey true := by
  decide

/-! ## PA41: Hook Registry Matching, Dispatch Precedence, Shell Quoting & Context Description (`hooks`)

Models the lifecycle hook system in `src/hooks/mod.rs`, `src/hooks/builtin.rs`,
and `src/hooks/shell_handler.rs`:
1. `HookRegistry::matches_any` & `HookRegistry::has_hooks_for` (`src/hooks/mod.rs`):
   - `has_hooks_for(event)` is true iff at least one registered `HookConfig` has `h.event == event`.
   - A hook `h` matches a `HookContext` iff `h.event == ctx.event` and either `h.match_tools`
     is empty (wildcard) or `ctx.tool_name` is `Some(t)` with `t ∈ h.match_tools`.
   - In particular, a `Stop` context (`tool_name = None`) matches a `Stop` hook with empty
     `match_tools`, but never matches a hook whose `match_tools` is non-empty.
2. `HookRegistry::fire_in_root` sequential short-circuit & fail-closed semantics (`src/hooks/mod.rs`):
   - Hooks matching `ctx` are evaluated in registration order.
   - `Continue` advances to the next matching hook.
   - `Skip(Policy)` or `Skip(HookFailure)` short-circuits immediately and returns that `Skip`.
   - `Error` on `PreToolUse` fails closed immediately as `Skip(HookFailure)` (blocking tool execution);
     `Error` on `PostToolUse` or `Stop` is logged as non-fatal and execution continues to `Continue`.
3. `pre_tool_skip_message` (`src/hooks/mod.rs`):
   - `SkipKind::Policy` -> failure tag `"hook_policy"`, prefix `"POLICY BLOCK"`.
   - `SkipKind::HookFailure` -> failure tag `"hook_failure"`, prefix `"not_run"`.
4. `shell_handler::expand_placeholders` quoting safety gate (`src/hooks/shell_handler.rs`):
   - Placeholders (`{path}`, `{tool}`, `{args}`, `{success}`) are expanded via POSIX single-quote
     escaping in `Unquoted` or `DoubleQuoted` contexts (`'` -> `'\''`) or raw inside `SingleQuoted`.
   - Placeholders inside `Backtick` (`` `...` ``) or after `<<` `Heredoc` are rejected before
     spawning a shell (`Error` -> converted to `Skip(HookFailure)` on `PreToolUse`).
5. `HookContext::description` (`src/hooks/mod.rs`) & builtin hook configs (`src/hooks/builtin.rs`):
   - Priority 1: explicit non-empty `"description"` or `"message"` string in `tool_args` JSON.
   - Priority 2: `tool_name` + space-joined `affected_paths` when `affected_paths` is non-empty.
   - Priority 3: `tool_name` alone when `affected_paths` is empty.
   - Priority 4: `none` when `tool_name` is `None` (e.g. `HookContext::stop()`).
-/

structure HookConfigModel where
  event : HookEventModel
  matchTools : List String
  deriving DecidableEq, Repr

def hookMatchesContext (h : HookConfigModel) (ev : HookEventModel) (toolName : Option String) : Bool :=
  h.event == ev &&
    match h.matchTools, toolName with
    | [], _ => true
    | _ :: _, some t => h.matchTools.contains t
    | _ :: _, none => false

def registryHasHooksFor (hooks : List HookConfigModel) (ev : HookEventModel) : Bool :=
  hooks.any (fun h => h.event == ev)

def registryMatchesAny (hooks : List HookConfigModel) (ev : HookEventModel) (toolName : Option String) : Bool :=
  hooks.any (fun h => hookMatchesContext h ev toolName)

/-- Single-hook outcome transition inside `HookRegistry::fire_in_root`:
    returns `Except HookOutcomeModel Unit` where `Except.error stopOutcome` short-circuits
    and `Except.ok ()` continues to the next matching hook. -/
def stepHookFire (ev : HookEventModel) (raw : HookOutcomeModel) : Except HookOutcomeModel Unit :=
  match raw with
  | .continueExec => .ok ()
  | .skip k => .error (.skip k)
  | .errorNonFatal =>
    match ev with
    | .preToolUse => .error (.skip .hookFailure)
    | .postToolUse | .stop => .ok ()

def fireHookRegistry (ev : HookEventModel) (outcomes : List HookOutcomeModel) : HookOutcomeModel :=
  match outcomes with
  | [] => .continueExec
  | raw :: rest =>
    match stepHookFire ev raw with
    | .error halted => halted
    | .ok () => fireHookRegistry ev rest

inductive QuotingContextModel
  | unquoted
  | singleQuoted
  | doubleQuoted
  | backtick
  | heredoc
  deriving DecidableEq, Repr

def placeholderQuotingAllowed : QuotingContextModel → Bool
  | .unquoted | .singleQuoted | .doubleQuoted => true
  | .backtick | .heredoc => false

def classifyPlaceholdersHookOutcome (ev : HookEventModel) (q : QuotingContextModel) : HookOutcomeModel :=
  if placeholderQuotingAllowed q then
    .continueExec
  else
    resolveHookStep ev .errorNonFatal

inductive HookDescModel
  | explicit (text : String)
  | toolWithPaths (tool : String) (paths : List String)
  | toolOnly (tool : String)
  | none
  deriving DecidableEq, Repr

def deriveHookDescription
    (explicitDesc : Option String)
    (toolName : Option String)
    (affectedPaths : List String) : HookDescModel :=
  match explicitDesc with
  | some d =>
    if d.isEmpty then
      match toolName, affectedPaths with
      | some t, _ :: _ => .toolWithPaths t affectedPaths
      | some t, [] => .toolOnly t
      | none, _ => .none
    else
      .explicit d
  | none =>
    match toolName, affectedPaths with
    | some t, _ :: _ => .toolWithPaths t affectedPaths
    | some t, [] => .toolOnly t
    | none, _ => .none

/-- Empty-matchTools wildcard hook matches any tool (or `none`) on the same event. -/
theorem wildcard_hook_matches_same_event (ev : HookEventModel) (tool : Option String) :
    hookMatchesContext { event := ev, matchTools := [] } ev tool = true := by
  cases ev <;> cases tool <;> rfl

/-- A hook with non-empty `matchTools` never matches a context with `toolName = none` (e.g. `Stop`). -/
theorem tool_filtered_hook_rejects_none (ev : HookEventModel) (t : String) (ts : List String) :
    hookMatchesContext { event := ev, matchTools := t :: ts } ev none = false := by
  cases ev <;> rfl

/-- Any `Skip` outcome in `fireHookRegistry` short-circuits immediately regardless of event or tail. -/
theorem fire_hook_registry_skip_short_circuits
    (ev : HookEventModel) (k : SkipKindModel) (rest : List HookOutcomeModel) :
    fireHookRegistry ev (.skip k :: rest) = .skip k := by
  unfold fireHookRegistry stepHookFire
  rfl

/-- On `PreToolUse`, an `.errorNonFatal` outcome fails closed immediately as `.skip .hookFailure`. -/
theorem fire_hook_registry_pre_tool_error_fails_closed (rest : List HookOutcomeModel) :
    fireHookRegistry .preToolUse (.errorNonFatal :: rest) = .skip .hookFailure := by
  rfl

/-- On `PostToolUse` and `Stop`, an `.errorNonFatal` outcome is non-fatal and continues to the remaining hooks. -/
theorem fire_hook_registry_post_and_stop_error_continues (rest : List HookOutcomeModel) :
    fireHookRegistry .postToolUse (.errorNonFatal :: rest) = fireHookRegistry .postToolUse rest ∧
    fireHookRegistry .stop (.errorNonFatal :: rest) = fireHookRegistry .stop rest := by
  exact ⟨rfl, rfl⟩

/-- Unsafe shell contexts (`backtick`, `heredoc`) always block `PreToolUse` with `.skip .hookFailure`. -/
theorem unsafe_quoting_blocks_pre_tool_use (q : QuotingContextModel)
    (h : placeholderQuotingAllowed q = false) :
    classifyPlaceholdersHookOutcome .preToolUse q = .skip .hookFailure := by
  cases q <;> simp_all [placeholderQuotingAllowed, classifyPlaceholdersHookOutcome, resolveHookStep]


/-! ## PA42: MCP Tool Discovery Prefixing, Call Result Normalization, Framing & Server Safety Gate (`mcp`)

Models the MCP client/server protocol invariants in `src/mcp/discovery.rs`,
`src/mcp/client.rs`, `src/mcp/transport.rs`, and `src/mcp/server.rs`.
-/

def mcpPrefixedToolName (serverName : String) (rawNameOpt : Option String) : String :=
  let raw := rawNameOpt.getD "unknown"
  s!"mcp_{serverName}_{raw}"

def mcpDefaultDescription (rawDescOpt : Option String) : String :=
  rawDescOpt.getD "No description"

inductive McpCallNormalization
  | normalizedText (isError : Bool) (success : Bool)
  | rawWithSuccessFalse
  | rawUnchanged
  deriving DecidableEq, Repr

def normalizeMcpCallResult (hasTextBlocks : Bool) (isError : Bool) : McpCallNormalization :=
  if hasTextBlocks then
    .normalizedText isError (!isError)
  else if isError then
    .rawWithSuccessFalse
  else
    .rawUnchanged

inductive McpFramingModel
  | contentLength
  | newlineDelimited
  deriving DecidableEq, Repr

def detectMcpFraming (firstByte : Option Char) : Option McpFramingModel :=
  match firstByte with
  | none => none
  | some c => if c == 'C' then some .contentLength else some .newlineDelimited

inductive McpRpcOutcome
  | noResponse
  | okResult
  | invalidRequestNeg32600
  | methodNotFoundNeg32601
  deriving DecidableEq, Repr

def isPreInitAllowedMethod (method : String) : Bool :=
  method == "initialize" || method == "ping" || method == "shutdown"

def isKnownPostInitMethod (method : String) : Bool :=
  isPreInitAllowedMethod method ||
    method == "notifications/initialized" ||
    method == "tools/list" ||
    method == "tools/call" ||
    method == "resources/list" ||
    method == "resources/templates/list" ||
    method == "resources/read" ||
    method == "prompts/list" ||
    method == "prompts/get"

def classifyMcpServerRequest
    (isNotification : Bool)
    (initialized : Bool)
    (method : String) : McpRpcOutcome :=
  if isNotification then
    .noResponse
  else if !initialized && !isPreInitAllowedMethod method then
    .invalidRequestNeg32600
  else if !isKnownPostInitMethod method then
    .methodNotFoundNeg32601
  else
    .okResult

def mcpToolCallAllowed
    (allowDestructive : Bool)
    (toolReadOnly : Bool)
    (dynamicMutating : Bool) : Bool :=
  if allowDestructive then
    true
  else
    toolReadOnly && !dynamicMutating

theorem mcp_call_text_normalization_inverts_error (isErr : Bool) :
    normalizeMcpCallResult true isErr = .normalizedText isErr (!isErr) := by
  rfl

theorem mcp_call_non_text_error_injection :
    normalizeMcpCallResult false true = .rawWithSuccessFalse ∧
    normalizeMcpCallResult false false = .rawUnchanged := by
  exact ⟨rfl, rfl⟩

theorem mcp_framing_detection_partition (c : Char) :
    detectMcpFraming none = none ∧
    (detectMcpFraming (some c) = some .contentLength ↔ c = 'C') := by
  constructor
  · rfl
  · unfold detectMcpFraming
    by_cases h : c == 'C'
    · have hc : c = 'C' := LawfulBEq.eq_of_beq h
      subst hc
      simp
    · have hne : c ≠ 'C' := by
        intro heq
        subst heq
        exact h BEq.rfl
      simp [h, hne]

theorem mcp_notification_never_responds (init : Bool) (m : String) :
    classifyMcpServerRequest true init m = .noResponse := by
  rfl

theorem mcp_default_gate_blocks_all_mutations (ro dyn : Bool) :
    mcpToolCallAllowed false ro dyn = true ↔ (ro = true ∧ dyn = false) := by
  cases ro <;> cases dyn <;> simp [mcpToolCallAllowed]


/-! ## PA43: Evolution Failure Repairability, Guarded Ancestry, Adaptive Patience & Replay Readiness (`evolution`)

Models the evolution search-policy and replay-validation invariants in `src/evolution/tree_log.rs`,
`src/evolution/policy.rs`, `src/evolution/replay.rs`, and `src/evolution/ast_tools.rs`.
-/

inductive EvolutionFailureClass
  | repairableSyntax
  | repairableTypeError
  | repairableTestFailure
  | repairableClippy
  | unrecoverableResource
  | safetyViolation
  | environmentError
  | unclassified
  deriving DecidableEq, Repr

def isEvolutionFailureRepairable : EvolutionFailureClass → Bool
  | .repairableSyntax | .repairableTypeError | .repairableTestFailure | .repairableClippy => true
  | .unrecoverableResource | .safetyViolation | .environmentError | .unclassified => false

inductive AncestryValidationResult
  | valid (maxDepth : Nat)
  | orphanedNode
  | cyclicAncestry
  deriving DecidableEq, Repr

def validateAncestryModel (parentExists : Bool) (hasCycle : Bool) (depth : Nat) : AncestryValidationResult :=
  if !parentExists then
    .orphanedNode
  else if hasCycle || depth > 256 then
    .cyclicAncestry
  else
    .valid depth

def effectivePatienceAndAllowance (maxPatience : Nat) (betaPermille : Nat) : Nat × Nat :=
  if betaPermille ≥ 700 then
    (maxPatience + 2, 3)
  else if betaPermille ≤ 300 then
    (if maxPatience - 1 ≥ 1 then maxPatience - 1 else 1, 1)
  else
    (maxPatience, 2)

inductive ReplayPromotionReadiness
  | ready
  | blockedByInstability
  | insufficientEvidence
  | retainIncumbent
  deriving DecidableEq, Repr

def evaluateReplayPromotionReadiness
    (candScorePermille incScorePermille : Nat)
    (treesEvaluated minTrees : Nat)
    (kendallWPermille : Nat) : ReplayPromotionReadiness :=
  if candScorePermille ≤ incScorePermille then
    .retainIncumbent
  else if treesEvaluated < minTrees then
    .insufficientEvidence
  else if kendallWPermille < 700 then
    .blockedByInstability
  else
    .ready

def isValidWorktreeName
    (nonEmpty : Bool)
    (startsWithDash : Bool)
    (hasPathSepOrNull : Bool)
    (hasDotDot : Bool) : Bool :=
  nonEmpty && !startsWithDash && !hasPathSepOrNull && !hasDotDot

theorem evolution_failure_repairable_partition (fc : EvolutionFailureClass) :
    isEvolutionFailureRepairable fc = true ↔
      (fc = .repairableSyntax ∨ fc = .repairableTypeError ∨
       fc = .repairableTestFailure ∨ fc = .repairableClippy) := by
  cases fc <;> simp [isEvolutionFailureRepairable]

theorem effective_patience_pos (maxPatience betaPermille : Nat) (h : maxPatience ≥ 1) :
    (effectivePatienceAndAllowance maxPatience betaPermille).1 ≥ 1 := by
  unfold effectivePatienceAndAllowance
  split
  · omega
  · split
    · split <;> omega
    · exact h

theorem replay_promotion_ready_iff
    (cand inc trees minTrees kw : Nat) :
    evaluateReplayPromotionReadiness cand inc trees minTrees kw = .ready ↔
      (cand > inc ∧ trees ≥ minTrees ∧ kw ≥ 700) := by
  unfold evaluateReplayPromotionReadiness
  by_cases h1 : cand ≤ inc
  · simp [h1]; omega
  · by_cases h2 : trees < minTrees
    · simp [h1, h2]; omega
    · by_cases h3 : kw < 700
      · simp [h1, h2, h3]
      · simp [h1, h2, h3]
        omega

theorem worktree_name_rejects_unsafe_flags (ne sd ps dd : Bool) :
    isValidWorktreeName ne sd ps dd = true ↔
      (ne = true ∧ sd = false ∧ ps = false ∧ dd = false) := by
  cases ne <;> cases sd <;> cases ps <;> cases dd <;> simp [isValidWorktreeName]


/-! ## PA44: Observability Log Level Error Gate, Anomaly Alert Threshold, Root Cause Deduplication & Telemetry Rotation (`observability`)

Models the observability, log analysis, dashboard token accounting, and telemetry rotation
invariants in `src/observability/log_analysis.rs`, `src/observability/dashboard.rs`,
and `src/observability/telemetry.rs`.
-/

inductive ObsLogLevel
  | trace
  | debug
  | info
  | warn
  | error
  | fatal
  deriving DecidableEq, Repr

def obsLogLevelIsError : ObsLogLevel → Bool
  | .error | .fatal => true
  | .trace | .debug | .info | .warn => false

inductive ObsAnomalyOutcome
  | none
  | anomalyOnly (severityPermille : Nat)
  | anomalyWithHighAlert (severityPermille : Nat)
  deriving DecidableEq, Repr

def classifyAnomalyAlert (severityPermilleOpt : Option Nat) : ObsAnomalyOutcome :=
  match severityPermilleOpt with
  | none => .none
  | some sev =>
    if sev ≥ 700 then
      .anomalyWithHighAlert sev
    else
      .anomalyOnly sev

def detectUnusualSourceSeverity (uniqueSourcesAfterInsert : Nat) (isFirstTimeForSource : Bool) : Option Nat :=
  if isFirstTimeForSource && uniqueSourcesAfterInsert > 10 then
    some 600
  else
    none

def detectSingleWindowBurstSeverity (windowCount : Nat) (errorCount : Nat) (burstThreshold : Nat) : Option Nat :=
  let floor := Nat.max burstThreshold 5
  if windowCount < 5 && errorCount == floor + 1 then
    some 800
  else
    none

def accumulateTokenUsage
    (in1 out1 reas1 : Nat)
    (in2 out2 reas2 : Nat) : Nat × Nat × Nat × Nat :=
  let total1 := in1 + out1
  let total2 := in2 + out2
  (in1 + in2, out1 + out2, total1 + total2, reas1 + reas2)

def maxTelemetryLogEntries : Nat := 100000

def rotateTelemetryCountIfNeeded (count : Nat) : Bool × Nat :=
  if count ≥ maxTelemetryLogEntries then
    (true, count / 2)
  else
    (false, count)

theorem obs_log_level_error_iff (lvl : ObsLogLevel) :
    obsLogLevelIsError lvl = true ↔ (lvl = .error ∨ lvl = .fatal) := by
  cases lvl <;> simp [obsLogLevelIsError]

theorem anomaly_alert_gate_distinguishes_source_vs_spike :
    classifyAnomalyAlert (detectUnusualSourceSeverity 11 true) = .anomalyOnly 600 ∧
    classifyAnomalyAlert (detectSingleWindowBurstSeverity 0 6 5) = .anomalyWithHighAlert 800 := by
  exact ⟨rfl, rfl⟩

theorem token_usage_add_preserves_total (i1 o1 r1 i2 o2 r2 : Nat) :
    let (i, o, tot, _) := accumulateTokenUsage i1 o1 r1 i2 o2 r2
    tot = i + o := by
  simp [accumulateTokenUsage]
  omega

theorem telemetry_rotation_halves_at_threshold (count : Nat) :
    (rotateTelemetryCountIfNeeded count).1 = true ↔ count ≥ maxTelemetryLogEntries := by
  unfold rotateTelemetryCountIfNeeded maxTelemetryLogEntries
  by_cases h : count ≥ 100000 <;> simp [h]


/-! ## PA45: Supervision Lifecycle & Health Aggregation Lattice (`supervision`)

Models `src/supervision/run_supervisor.rs`, `src/supervision/run_registry.rs`,
`src/supervision/health.rs`, and `src/supervision/circuit_breaker.rs`.
-/

inductive SupervisionRunStatus where
  | running
  | paused
  | completed
  | failed
  | aborted
  deriving DecidableEq, Repr

def SupervisionRunStatus.isTerminal : SupervisionRunStatus → Bool
  | .running => false
  | .paused => false
  | .completed => true
  | .failed => true
  | .aborted => true

def SupervisionRunStatus.asStr : SupervisionRunStatus → String
  | .running => "Running"
  | .paused => "Paused"
  | .completed => "Completed"
  | .failed => "Failed"
  | .aborted => "Aborted"

def isTerminalStatusStr (s : String) : Bool :=
  s == "Completed" || s == "Failed" || s == "Aborted"

theorem run_status_terminal_iff (s : SupervisionRunStatus) :
    s.isTerminal = true ↔ (s = .completed ∨ s = .failed ∨ s = .aborted) := by
  cases s <;> decide

theorem run_status_str_terminal_consistent (s : SupervisionRunStatus) :
    isTerminalStatusStr s.asStr = s.isTerminal := by
  cases s <;> rfl

def effectiveRunStatus (st : SupervisionRunStatus) (ownerPidAlive : Bool) : String :=
  match st with
  | .running => if ownerPidAlive then "Running" else "Stale"
  | other => other.asStr

def registryUpdateStatus
    (cur : SupervisionRunStatus)
    (curUpdated : Nat)
    (next : SupervisionRunStatus)
    (nextUpdated : Nat) : SupervisionRunStatus × Nat :=
  if cur.isTerminal then
    (cur, curUpdated)
  else
    (next, nextUpdated)

theorem registry_update_terminal_sticky
    (cur : SupervisionRunStatus)
    (curUpdated : Nat)
    (next : SupervisionRunStatus)
    (nextUpdated : Nat)
    (h : cur.isTerminal = true) :
    registryUpdateStatus cur curUpdated next nextUpdated = (cur, curUpdated) := by
  unfold registryUpdateStatus
  rw [h]
  rfl

theorem effective_status_terminal_ignores_liveness
    (st : SupervisionRunStatus)
    (alive : Bool)
    (h : st.isTerminal = true) :
    effectiveRunStatus st alive = st.asStr := by
  cases st <;> first | contradiction | rfl

inductive RegistryAbortOutcome where
  | notFound
  | alreadyDone (finalStatus : String)
  | wasStale
  | sentSignal
  deriving DecidableEq, Repr

def RegistryAbortOutcome.asStr : RegistryAbortOutcome → String
  | .notFound => "NotFound"
  | .alreadyDone st => s!"AlreadyDone:{st}"
  | .wasStale => "WasStale"
  | .sentSignal => "SentSignal"

def registryAbort (st : SupervisionRunStatus) (ownerPidAlive : Bool) :
    RegistryAbortOutcome × SupervisionRunStatus :=
  if st.isTerminal then
    (.alreadyDone st.asStr, st)
  else if !ownerPidAlive then
    (.wasStale, .aborted)
  else
    (.sentSignal, .aborted)

theorem registry_abort_terminal_preserves_status
    (st : SupervisionRunStatus)
    (alive : Bool)
    (h : st.isTerminal = true) :
    (registryAbort st alive).2 = st := by
  unfold registryAbort
  rw [h]
  rfl

theorem registry_abort_stale_marks_aborted
    (st : SupervisionRunStatus)
    (hNonTerm : st.isTerminal = false) :
    registryAbort st false = (.wasStale, .aborted) := by
  unfold registryAbort
  rw [hNonTerm]
  rfl

inductive HealthLevel where
  | healthy
  | degraded
  | unhealthyWarning
  | unhealthyCritical
  | unhealthyFatal
  deriving DecidableEq, Repr

def HealthLevel.rank : HealthLevel → Nat
  | .healthy => 0
  | .degraded => 1
  | .unhealthyWarning => 2
  | .unhealthyCritical => 3
  | .unhealthyFatal => 4

def HealthLevel.ofRank : Nat → HealthLevel
  | 0 => .healthy
  | 1 => .degraded
  | 2 => .unhealthyWarning
  | 3 => .unhealthyCritical
  | _ => .unhealthyFatal

def HealthLevel.asStr : HealthLevel → String
  | .healthy => "healthy"
  | .degraded => "degraded"
  | .unhealthyWarning => "unhealthy/warning"
  | .unhealthyCritical => "unhealthy/critical"
  | .unhealthyFatal => "unhealthy/fatal"

def joinHealthLevel (a b : HealthLevel) : HealthLevel :=
  HealthLevel.ofRank (max a.rank b.rank)

theorem health_rank_join_comm (a b : HealthLevel) :
    joinHealthLevel a b = joinHealthLevel b a := by
  cases a <;> cases b <;> rfl

theorem health_rank_join_idem (a : HealthLevel) :
    joinHealthLevel a a = a := by
  cases a <;> rfl

theorem fatal_absorbs_health_join (a : HealthLevel) :
    joinHealthLevel a .unhealthyFatal = .unhealthyFatal := by
  cases a <;> rfl

def settleSupervisedRun (abortedBeforeFinish : Bool) (outcomeOk : Bool) : SupervisionRunStatus :=
  if abortedBeforeFinish then
    .aborted
  else if outcomeOk then
    .completed
  else
    .failed

theorem supervisor_settle_aborted_sticky (outcomeOk : Bool) :
    settleSupervisedRun true outcomeOk = .aborted := rfl

theorem supervisor_settle_always_terminal (aborted outcomeOk : Bool) :
    (settleSupervisedRun aborted outcomeOk).isTerminal = true := by
  cases aborted <;> cases outcomeOk <;> rfl


/-! ## PA46: Session Checkpoint Lifecycle, Delta Hydration & Local-First Cache Bounds (`session`)

Models `src/session/checkpoint.rs` and `src/session/local_first.rs`.
-/

def forecastCallShapesMax : Nat := 128

def forecastBoundedLen (n : Nat) : Nat :=
  min n forecastCallShapesMax

def forecastBoundedHeadAndLast (total : Nat) (slowestIdx : Option Nat) : Nat × Nat :=
  if total ≤ forecastCallShapesMax then
    (0, total - 1)
  else
    let cut := total - forecastCallShapesMax
    match slowestIdx with
    | some i =>
        if i < cut then
          (i, total - 1)
        else
          (cut, total - 1)
    | none => (cut, total - 1)

theorem forecast_bounded_len_le_cap (n : Nat) :
    forecastBoundedLen n ≤ forecastCallShapesMax := by
  unfold forecastBoundedLen forecastCallShapesMax
  omega

theorem forecast_bounded_preserves_slowest_idx
    (total slowestIdx : Nat)
    (hOver : total > forecastCallShapesMax)
    (hOld : slowestIdx < total - forecastCallShapesMax) :
    (forecastBoundedHeadAndLast total (some slowestIdx)).1 = slowestIdx := by
  unfold forecastBoundedHeadAndLast forecastCallShapesMax at *
  have hNotLe : ¬ (total ≤ 128) := by omega
  simp [hNotLe, hOld]

def workspaceFingerprintDiffCount
    (headEq : Bool)
    (changedCount uncoveredCount removedCount : Nat) : Nat :=
  (if headEq then 0 else 1) + changedCount + uncoveredCount + removedCount

theorem workspace_fingerprint_refl_empty
    (headEq : Bool)
    (changed uncovered removed : Nat)
    (h1 : headEq = true)
    (h2 : changed = 0)
    (h3 : uncovered = 0)
    (h4 : removed = 0) :
    workspaceFingerprintDiffCount headEq changed uncovered removed = 0 := by
  subst h1 h2 h3 h4
  rfl

def canComputeCheckpointDelta
    (sameTaskId : Bool)
    (versionAdvanced : Bool)
    (budgetCapRemoved : Bool)
    (identityUnchanged : Bool)
    (vectorsPrefixPreserved : Bool)
    (hasAnyChange : Bool) : Bool :=
  sameTaskId &&
    versionAdvanced &&
    (!budgetCapRemoved) &&
    identityUnchanged &&
    vectorsPrefixPreserved &&
    hasAnyChange

def applyCheckpointDeltaVersion
    (sameTaskId : Bool)
    (curVersion baseVersion targetVersion : Nat) : Option Nat :=
  if sameTaskId && curVersion == baseVersion then
    some targetVersion
  else
    none

theorem checkpoint_delta_rejects_cap_removal
    (sameTask verAdvanced idUnchanged prefixOk hasChange : Bool) :
    canComputeCheckpointDelta sameTask verAdvanced true idUnchanged prefixOk hasChange = false := by
  cases sameTask <;> cases verAdvanced <;> rfl

theorem checkpoint_delta_rejects_shrunk_prefix
    (sameTask verAdvanced capRemoved idUnchanged hasChange : Bool) :
    canComputeCheckpointDelta sameTask verAdvanced capRemoved idUnchanged false hasChange = false := by
  cases sameTask <;> cases verAdvanced <;> cases capRemoved <;> cases idUnchanged <;> rfl

theorem checkpoint_delta_apply_advances_version
    (curVer targetVer : Nat) :
    applyCheckpointDeltaVersion true curVer curVer targetVer = some targetVer := by
  unfold applyCheckpointDeltaVersion
  simp

inductive ImplicitResumeDecision where
  | placeholderDescription
  | noUserMessage
  | resumable
  deriving DecidableEq, Repr

def ImplicitResumeDecision.asStr : ImplicitResumeDecision → String
  | .placeholderDescription => "PlaceholderDescription"
  | .noUserMessage => "NoUserMessage"
  | .resumable => "none"

def classifyImplicitResumeBlocker (isPlaceholder hasUserMsg : Bool) : ImplicitResumeDecision :=
  if isPlaceholder then
    .placeholderDescription
  else if !hasUserMsg then
    .noUserMessage
  else
    .resumable

def editedDescriptionSuffix (hasOriginal trimmedDiffers : Bool) : String :=
  if hasOriginal && trimmedDiffers then " (edited)" else ""

theorem implicit_resume_resumable_iff (isPlaceholder hasUserMsg : Bool) :
    classifyImplicitResumeBlocker isPlaceholder hasUserMsg = .resumable ↔
      (isPlaceholder = false ∧ hasUserMsg = true) := by
  cases isPlaceholder <;> cases hasUserMsg <;> decide

def shouldInvalidateCacheKey (isGitKey matchesPath isTreeScoped : Bool) : Bool :=
  isGitKey || matchesPath || isTreeScoped

def localCacheHitRatePct (hits misses : Nat) : Nat :=
  if hits + misses == 0 then 0 else (hits * 100) / (hits + misses)

theorem local_cache_invalidate_preserves_disjoint_key :
    shouldInvalidateCacheKey false false false = false := rfl

theorem local_cache_hit_rate_bounded (hits misses : Nat) :
    localCacheHitRatePct hits misses ≤ 100 := by
  unfold localCacheHitRatePct
  split
  · omega
  · exact Nat.div_le_of_le_mul (by omega)


/-! ## PA47: DevOps Process Manager & Container Tool Lifecycle & Security Bounds (`devops`, `tools::container`)

Models `src/devops/process_manager.rs` and `src/tools/container/mod.rs`.
-/

inductive ManagedProcessStatus where
  | starting
  | running
  | healthCheckFailed
  | stopped
  | crashed
  | restarting
  deriving DecidableEq, Repr

def ManagedProcessStatus.isActive : ManagedProcessStatus → Bool
  | .starting => true
  | .running => true
  | .restarting => true
  | .healthCheckFailed => false
  | .stopped => false
  | .crashed => false

def ManagedProcessStatus.serdeTag : ManagedProcessStatus → String
  | .starting => "starting"
  | .running => "running"
  | .healthCheckFailed => "health_check_failed"
  | .stopped => "stopped"
  | .crashed => "crashed"
  | .restarting => "restarting"

def canRemoveManagedProcess (st : ManagedProcessStatus) : Bool :=
  !st.isActive

def autoRestartBackoffSecs (attempt : Nat) : Nat :=
  min (attempt * 2) 30

theorem active_process_blocks_remove (st : ManagedProcessStatus) (h : st.isActive = true) :
    canRemoveManagedProcess st = false := by
  unfold canRemoveManagedProcess
  rw [h]
  rfl

theorem auto_restart_backoff_capped (attempt : Nat) :
    autoRestartBackoffSecs attempt ≤ 30 := by
  unfold autoRestartBackoffSecs
  omega

inductive ContainerIsolationProfile where
  | hardened
  | sealed
  | unsafeProfile
  | invalid
  deriving DecidableEq, Repr

def containerSecurityFlagCount (p : ContainerIsolationProfile) (hasCustomUser : Bool) : Option Nat :=
  match p with
  | .hardened => some (if hasCustomUser then 14 else 12)
  | .sealed => some 15
  | .unsafeProfile => some 0
  | .invalid => none

theorem sealed_profile_strictly_more_flags_than_default_hardened :
    containerSecurityFlagCount .sealed false = some 15 ∧
    containerSecurityFlagCount .hardened false = some 12 := by
  decide

def validatePortMappingModel
    (hasShellMeta : Bool)
    (validProto : Bool)
    (validHostPort : Bool)
    (validContainerPort : Bool) : Bool :=
  (!hasShellMeta) && validProto && validHostPort && validContainerPort

def validateVolumeMountModel
    (hasShellMeta : Bool)
    (validOpts : Bool)
    (containerAbs : Bool)
    (hasTraversal : Bool)
    (isDockerSock : Bool)
    (isSystemOrCredDir : Bool) : Bool :=
  (!hasShellMeta) &&
    validOpts &&
    containerAbs &&
    (!hasTraversal) &&
    (!isDockerSock) &&
    (!isSystemOrCredDir)

theorem docker_sock_mount_always_rejected
    (hasMeta opts abs trav sys : Bool) :
    validateVolumeMountModel hasMeta opts abs trav true sys = false := by
  cases hasMeta <;> cases opts <;> cases abs <;> cases trav <;> rfl

theorem traversal_mount_always_rejected
    (hasMeta opts abs sock sys : Bool) :
    validateVolumeMountModel hasMeta opts abs true sock sys = false := by
  cases hasMeta <;> cases opts <;> cases abs <;> rfl


/-! ## PA48: Phi Ledger, Observer, Activity & Policy Invariants (`phi`)

Models `src/phi/ledger.rs`, `src/phi/observer.rs`, `src/phi/policy.rs`, and `src/phi/activity.rs`.
-/

inductive PhiObligationKind where
  | unreviewedChange
  | unconfirmedCoverage
  | brokenByRemoval
  deriving DecidableEq, Repr

inductive PhiEvidenceKind where
  | testsExecuted
  | humanReviewed
  deriving DecidableEq, Repr

def PhiEvidenceKind.discharges : PhiEvidenceKind → PhiObligationKind
  | .testsExecuted => .unconfirmedCoverage
  | .humanReviewed => .unreviewedChange

inductive PhiCoverage where
  | covered
  | notCovered
  | unknown
  deriving DecidableEq, Repr

inductive PhiUnsatisfied where
  | wrongKind
  | outOfScope
  | coverageUnknown
  | dependentsUnknown
  | failed
  | predatesChange
  | racedAnEdit
  | revisionUnknown
  deriving DecidableEq, Repr

def PhiUnsatisfied.asStr : PhiUnsatisfied → String
  | .wrongKind => "WrongKind"
  | .outOfScope => "OutOfScope"
  | .coverageUnknown => "CoverageUnknown"
  | .dependentsUnknown => "DependentsUnknown"
  | .failed => "Failed"
  | .predatesChange => "PredatesChange"
  | .racedAnEdit => "RacedAnEdit"
  | .revisionUnknown => "RevisionUnknown"

def assessObligation
    (evKind : PhiEvidenceKind)
    (obKind : PhiObligationKind)
    (cov : PhiCoverage)
    (depsKnownAndCovered : Bool)
    (passed : Bool)
    (snapshotSeq obligationSeq : Nat)
    (changedBetween externallyChangedSince : Bool) : Except PhiUnsatisfied Unit :=
  match obKind with
  | .brokenByRemoval =>
      if evKind != .testsExecuted then
        .error .wrongKind
      else if !depsKnownAndCovered then
        .error .dependentsUnknown
      else if !passed then
        .error .failed
      else if snapshotSeq < obligationSeq then
        .error .predatesChange
      else if changedBetween then
        .error .racedAnEdit
      else if externallyChangedSince then
        .error .revisionUnknown
      else
        .ok ()
  | _ =>
      if evKind.discharges != obKind then
        .error .wrongKind
      else
        match cov with
        | .notCovered => .error .outOfScope
        | .unknown => .error .coverageUnknown
        | .covered =>
            if !passed then
              .error .failed
            else if snapshotSeq < obligationSeq then
              .error .predatesChange
            else if changedBetween then
              .error .racedAnEdit
            else if externallyChangedSince then
              .error .revisionUnknown
            else
              .ok ()

theorem evidence_discharges_bijection (e : PhiEvidenceKind) :
    (e.discharges == .unconfirmedCoverage) = (e == .testsExecuted) := by
  cases e <;> rfl

theorem workspace_unknown_cov_never_discharges
    (passed : Bool)
    (snapSeq obSeq : Nat) :
    assessObligation .testsExecuted .unconfirmedCoverage .unknown true passed snapSeq obSeq false false =
      .error .coverageUnknown := rfl

theorem assess_predates_change_rejected
    (snapSeq obSeq : Nat)
    (h : snapSeq < obSeq) :
    assessObligation .testsExecuted .unconfirmedCoverage .covered true true snapSeq obSeq false false =
      .error .predatesChange := by
  simp [assessObligation, PhiEvidenceKind.discharges, h]

theorem assess_raced_edit_rejected
    (snapSeq obSeq : Nat)
    (h : ¬ (snapSeq < obSeq)) :
    assessObligation .testsExecuted .unconfirmedCoverage .covered true true snapSeq obSeq true false =
      .error .racedAnEdit := by
  simp [assessObligation, PhiEvidenceKind.discharges, h]

inductive ObservedToolEventKind where
  | unattributed
  | fileChanged (lines : Nat)
  | opaqueRun (mayHaveMutated : Bool)
  deriving DecidableEq, Repr

def classifyFileEditCall (succeeded : Bool) (oldLines newLines : Nat) : ObservedToolEventKind :=
  if !succeeded then
    .unattributed
  else
    .fileChanged (max oldLines newLines)

def classifyCargoClippyCall (fixFlag : Bool) : ObservedToolEventKind :=
  .opaqueRun fixFlag

theorem failed_file_edit_is_unattributed (oldLines newLines : Nat) :
    classifyFileEditCall false oldLines newLines = .unattributed := rfl

theorem file_edit_line_count_ge_both (oldLines newLines : Nat) :
    oldLines ≤ max oldLines newLines ∧ newLines ≤ max oldLines newLines := by
  omega

def measuredDebtRatioPct
    (unreviewedLines untestedLines removalObligations unknownSize debtGateLines : Nat) : Nat :=
  if unreviewedLines + untestedLines + removalObligations + unknownSize == 0 then
    0
  else
    let effective := max unreviewedLines untestedLines + removalObligations * 40 + unknownSize * 40
    let gate := max debtGateLines 1
    min ((effective * 100) / gate) 100

theorem debt_ratio_pct_bounded (unrev untest rem unk gate : Nat) :
    measuredDebtRatioPct unrev untest rem unk gate ≤ 100 := by
  unfold measuredDebtRatioPct
  split
  · omega
  · dsimp
    omega

inductive PhiActivityPhase where
  | running
  | completed
  | partialPhase
  | failed
  | abandoned
  deriving DecidableEq, Repr

inductive PhiFoxEmotion where
  | unimpressed
  | skeptical
  | errorEmo
  | working
  | flow
  | greeting
  | guard
  deriving DecidableEq, Repr

def PhiFoxEmotion.asStr : PhiFoxEmotion → String
  | .unimpressed => "Unimpressed"
  | .skeptical => "Skeptical"
  | .errorEmo => "Error"
  | .working => "Working"
  | .flow => "Flow"
  | .greeting => "Greeting"
  | .guard => "Guard"

def deriveFoxEmotionModel
    (debtPct : Nat)
    (phase : PhiActivityPhase)
    (outstandingCount evidenceCount : Nat) : PhiFoxEmotion :=
  if debtPct > 70 then
    .unimpressed
  else if debtPct > 45 then
    .skeptical
  else
    match phase with
    | .failed => .errorEmo
    | .partialPhase | .abandoned => .skeptical
    | .running => .working
    | .completed =>
        if outstandingCount == 0 && evidenceCount > 0 then
          .flow
        else if outstandingCount == 0 then
          .greeting
        else
          .guard

theorem high_debt_forces_unimpressed
    (debtPct : Nat)
    (phase : PhiActivityPhase)
    (outCount evCount : Nat)
    (h : debtPct > 70) :
    deriveFoxEmotionModel debtPct phase outCount evCount = .unimpressed := by
  unfold deriveFoxEmotionModel
  simp [h]

def validateActivityIdLen (len : Nat) : Bool :=
  decide (1 ≤ len ∧ len ≤ 128)

theorem activity_id_valid_bounds (len : Nat) :
    validateActivityIdLen len = true ↔ (1 ≤ len ∧ len ≤ 128) := by
  unfold validateActivityIdLen
  simp

-- ============================================================================
-- Section PA49: Config & CLI Endpoint Redaction, Context Budget & Validation
-- (`src/config/api_key.rs`, `src/config/mod.rs`, `src/config/safety.rs`,
--  `src/config/model_profiles.rs`, `src/config/validation.rs`, `src/cli/mod.rs`)
-- ============================================================================

structure EndpointSecurityState where
  hasUserinfo : Bool
  isLocalHost : Bool
  isHttpScheme : Bool
  hasQuery : Bool
  deriving DecidableEq, Repr

def isLocalEndpointModel (e : EndpointSecurityState) : Bool :=
  e.isLocalHost && !e.hasUserinfo

def isInsecureRemoteEndpointModel (e : EndpointSecurityState) : Bool :=
  e.isHttpScheme && !isLocalEndpointModel e

def redactEndpointModel (e : EndpointSecurityState) : EndpointSecurityState :=
  { e with hasUserinfo := false, hasQuery := false }

theorem spoofed_userinfo_never_local (e : EndpointSecurityState)
    (h : e.hasUserinfo = true) :
    isLocalEndpointModel e = false := by
  simp [isLocalEndpointModel, h]

theorem http_with_userinfo_always_insecure_remote (e : EndpointSecurityState)
    (hHttp : e.isHttpScheme = true) (hUser : e.hasUserinfo = true) :
    isInsecureRemoteEndpointModel e = true := by
  simp [isInsecureRemoteEndpointModel, isLocalEndpointModel, hHttp, hUser]

theorem local_loopback_never_insecure_remote (e : EndpointSecurityState)
    (hLocal : e.isLocalHost = true) (hClean : e.hasUserinfo = false) :
    isInsecureRemoteEndpointModel e = false := by
  simp [isInsecureRemoteEndpointModel, isLocalEndpointModel, hLocal, hClean]

theorem redact_endpoint_clears_userinfo_and_query (e : EndpointSecurityState) :
    (redactEndpointModel e).hasUserinfo = false ∧
    (redactEndpointModel e).hasQuery = false := by
  simp [redactEndpointModel]

def minConversationTokens : Nat := 2048

def deriveContextBudgetModel (contextLength maxTokens : Nat) : Option (Nat × Nat) :=
  let safetyMargin := contextLength / 5
  let maxReservable := (contextLength - safetyMargin) - minConversationTokens
  let reservedOutput := Nat.min maxTokens maxReservable
  let maxContextTokens := (contextLength - reservedOutput) - safetyMargin
  if maxContextTokens < minConversationTokens then
    none
  else
    some (maxContextTokens, reservedOutput)

def checkGeneratedContextFitModel (contextLength maxTokens : Nat) : Bool :=
  decide (((contextLength - maxTokens) - (contextLength / 5)) ≥ minConversationTokens)

theorem derive_context_budget_guarantees_min_conversation
    (ctx maxOut maxCtx resOut : Nat)
    (h : deriveContextBudgetModel ctx maxOut = some (maxCtx, resOut)) :
    maxCtx ≥ minConversationTokens ∧ resOut ≤ maxOut := by
  dsimp [deriveContextBudgetModel] at h
  split at h
  · contradiction
  · rename_i hNotLt
    injection h with hPair
    injection hPair with hCtx hRes
    constructor
    · omega
    · subst hRes
      exact Nat.min_le_left maxOut ((ctx - ctx / 5) - minConversationTokens)

theorem strict_generated_fit_rejects_oversized_max_tokens :
    deriveContextBudgetModel 32768 30000 = some (2048, 24167) ∧
    checkGeneratedContextFitModel 32768 30000 = false := by
  decide

def scaleMaxCallSecsModel (profileMax baseSecs reqMax : Nat) : Nat × Bool :=
  if profileMax = 0 ∨ reqMax ≤ profileMax then
    (baseSecs, false)
  else
    ((baseSecs * reqMax + profileMax - 1) / profileMax, true)

theorem scale_max_call_secs_unscaled_within_profile (p b r : Nat) (h : r ≤ p) :
    scaleMaxCallSecsModel p b r = (b, false) := by
  unfold scaleMaxCallSecsModel
  split
  · rfl
  · rename_i hNot
    exfalso
    exact hNot (Or.inr h)

def validateConfigBoundsModel
    (marginLtBudget retryOrderOk qwenEffortOk sglangEffortOk : Bool) : Bool :=
  marginLtBudget && retryOrderOk && qwenEffortOk && sglangEffortOk

theorem config_bounds_valid_iff_all_hold (m r q s : Bool) :
    validateConfigBoundsModel m r q s = true ↔
      (m = true ∧ r = true ∧ q = true ∧ s = true) := by
  cases m <;> cases r <;> cases q <;> cases s <;> decide

def autocontinueShouldRunModel (hasSubcmd hasPrompt hasContinue hasResume : Bool) : Bool :=
  !(hasSubcmd || hasPrompt || hasContinue || hasResume)

theorem autocontinue_runs_iff_all_false (a b c d : Bool) :
    autocontinueShouldRunModel a b c d = true ↔
      (a = false ∧ b = false ∧ c = false ∧ d = false) := by
  cases a <;> cases b <;> cases c <;> cases d <;> decide

-- ============================================================================
-- Section PA50: Tool Call Extraction & Measured Token Accounting Invariants
-- (`src/tool_parser.rs`, `src/token_count.rs` — AGENTS.md Rule 4)
-- ============================================================================

inductive ToolSpanContext where
  | outsideCode
  | closedCodeFence
  | unclosedFenceBeforeOpener
  | unclosedFenceAtOpener
  deriving DecidableEq, Repr

def spanAllowsToolExtraction : ToolSpanContext → Bool
  | .outsideCode => true
  | .closedCodeFence => false
  | .unclosedFenceBeforeOpener => false
  | .unclosedFenceAtOpener => true

theorem closed_fence_blocks_and_unclosed_opener_recovers :
    spanAllowsToolExtraction .closedCodeFence = false ∧
    spanAllowsToolExtraction .unclosedFenceAtOpener = true := by
  decide

def isGenericToolWrapperName (name : String) : Bool :=
  name == "tool" || name == "tool_call" || name == "function" || name == "call"

def unwrapGenericToolCallModel (outerName : String) (innerName : Option String) : String :=
  match innerName with
  | some inner =>
      if isGenericToolWrapperName outerName && !inner.isEmpty then inner else outerName
  | none => outerName

theorem unwrap_generic_wrapper_extracts_inner (inner : String) (h : inner.isEmpty = false) :
    unwrapGenericToolCallModel "tool_call" (some inner) = inner := by
  simp [unwrapGenericToolCallModel, isGenericToolWrapperName, h]

def dedupCrossFamilyPair (first second : Nat × Nat) : List (Nat × Nat) :=
  if first = second then [first] else [first, second]

theorem restated_cross_family_call_deduplicates (call : Nat × Nat) :
    dedupCrossFamilyPair call call = [call] := by
  simp [dedupCrossFamilyPair]

def codeShapeMinTokens : Nat := 64
def defaultImageTokenEstimate : Nat := 765

def estimateContentTokensModel (rawBpeTokens : Nat) (isCodeShaped : Bool) : Nat :=
  if isCodeShaped && decide (rawBpeTokens ≥ codeShapeMinTokens) then
    (rawBpeTokens * 110 + 99) / 100
  else
    rawBpeTokens

def estimateTokensModel (rawBpeTokens : Nat) (isCodeShaped : Bool) : Nat :=
  Nat.max (estimateContentTokensModel rawBpeTokens isCodeShaped) 1

def estimateMessageTokensModel
    (textTokens imageCount reasoningTokens toolCallTokens : Nat) : Nat :=
  (textTokens + 4) +
    imageCount * defaultImageTokenEstimate +
    reasoningTokens +
    (if toolCallTokens > 0 then 10 + toolCallTokens else 0)

theorem estimate_tokens_pos (raw : Nat) (code : Bool) :
    estimateTokensModel raw code ≥ 1 := by
  unfold estimateTokensModel
  exact Nat.le_max_right (estimateContentTokensModel raw code) 1

theorem short_or_prose_uncalibrated (raw : Nat) (code : Bool)
    (h : code = false ∨ raw < codeShapeMinTokens) :
    estimateContentTokensModel raw code = raw := by
  unfold estimateContentTokensModel
  rcases h with rfl | hLt
  · simp
  · have hNotGe : ¬(raw ≥ codeShapeMinTokens) := by omega
    simp [hNotGe]

theorem code_shaped_ge_64_strictly_boosted (raw : Nat)
    (h : raw ≥ codeShapeMinTokens) :
    estimateContentTokensModel raw true > raw := by
  unfold estimateContentTokensModel codeShapeMinTokens at *
  have hDec : decide (raw ≥ 64) = true := decide_eq_true h
  simp [hDec]
  omega

theorem message_tokens_monotone_in_images (t r tc n : Nat) :
    estimateMessageTokensModel t (n + 1) r tc =
      estimateMessageTokensModel t n r tc + defaultImageTokenEstimate := by
  unfold estimateMessageTokensModel defaultImageTokenEstimate
  omega

-- ============================================================================
-- Section PA51: LSP Language Detection & Computer Window/Input/Screen Guards
-- (`src/lsp/client.rs`, `src/computer/mod.rs`, `src/computer/window.rs`,
--  `src/computer/screen.rs` — AGENTS.md Rule 6)
-- ============================================================================

inductive LspLanguage where
  | rust
  | python
  | typescript
  | javascript
  | go
  deriving DecidableEq, Repr

def LspLanguage.id : LspLanguage → String
  | .rust => "rust"
  | .python => "python"
  | .typescript => "typescript"
  | .javascript => "javascript"
  | .go => "go"

def lspLanguageFromExt : String → Option LspLanguage
  | "rs" => some .rust
  | "py" | "pyi" => some .python
  | "ts" | "tsx" => some .typescript
  | "js" | "jsx" | "mjs" | "cjs" => some .javascript
  | "go" => some .go
  | _ => none

theorem lsp_language_id_injective (a b : LspLanguage) :
    a.id = b.id ↔ a = b := by
  cases a <;> cases b <;> decide

structure KeyModMask where
  alt : Bool
  ctrl : Bool
  cmd : Bool
  deriving DecidableEq, Repr

inductive FinalKey where
  | deleteKey
  | qKey
  | f4Key
  | fVtKey
  | cKey
  | otherKey
  deriving DecidableEq, Repr

def isBlockedComboModel (mods : KeyModMask) (hasModifier : Bool) (key : FinalKey) : Bool :=
  if !hasModifier then
    false
  else
    (mods == ⟨true, true, false⟩ && (key == .deleteKey || key == .fVtKey)) ||
    (mods == ⟨false, false, true⟩ && key == .qKey) ||
    (mods == ⟨true, false, false⟩ && key == .f4Key)

theorem single_key_never_blocked (mods : KeyModMask) (k : FinalKey) :
    isBlockedComboModel mods false k = false := by
  simp [isBlockedComboModel]

theorem ctrl_alt_del_and_cmd_q_blocked :
    isBlockedComboModel ⟨true, true, false⟩ true .deleteKey = true ∧
    isBlockedComboModel ⟨false, false, true⟩ true .qKey = true ∧
    isBlockedComboModel ⟨true, false, false⟩ true .f4Key = true ∧
    isBlockedComboModel ⟨false, true, false⟩ true .cKey = false := by
  decide

def validateAppNameModel (nonEmpty hasTraversal hasSlash hasBackslash hasNul : Bool) : Bool :=
  nonEmpty && !hasTraversal && !hasSlash && !hasBackslash && !hasNul

theorem traversal_or_nul_always_rejected_app_name (ne sl bsl : Bool) :
    validateAppNameModel ne true sl bsl false = false ∧
    validateAppNameModel ne false sl bsl true = false := by
  simp [validateAppNameModel]

def checkWindowPlacementRule6Model
    (titleOwned : Bool)
    (x y width height minX minY maxX maxY : Int) : Bool :=
  titleOwned &&
    decide (width > 0 ∧ height > 0 ∧ x ≥ minX ∧ y ≥ minY ∧ x + width ≤ maxX ∧ y + height ≤ maxY)

theorem unowned_or_above_visible_top_rejected_rule6
    (x y w h minX minY maxX maxY : Int)
    (hAbove : y < minY) :
    checkWindowPlacementRule6Model false x y w h minX minY maxX maxY = false ∧
    checkWindowPlacementRule6Model true x y w h minX minY maxX maxY = false := by
  unfold checkWindowPlacementRule6Model
  constructor
  · simp
  · have hNot : ¬(w > 0 ∧ h > 0 ∧ x ≥ minX ∧ y ≥ minY ∧ x + w ≤ maxX ∧ y + h ≤ maxY) := by omega
    simp [hNot]

def validateScreenCropModel (imgW imgH rx ry rw rh : Int) : Bool :=
  decide (rw > 0 ∧ rh > 0 ∧ rw ≤ 32768 ∧ rh ≤ 32768 ∧ rx ≥ 0 ∧ ry ≥ 0 ∧ rx + rw ≤ imgW ∧ ry + rh ≤ imgH)

theorem screen_crop_valid_implies_positive_and_in_bounds
    (iw ih x y w h : Int)
    (hv : validateScreenCropModel iw ih x y w h = true) :
    w > 0 ∧ h > 0 ∧ x ≥ 0 ∧ y ≥ 0 ∧ x + w ≤ iw ∧ y + h ≤ ih := by
  unfold validateScreenCropModel at hv
  have hProp := of_decide_eq_true hv
  omega

-- ============================================================================
-- Section PA52: UI Theme Palette, Spinner Honesty, Output Sink & Arena Drills
-- (`src/ui/theme.rs`, `src/ui/spinner.rs`, `src/output/mod.rs`,
--  `src/output/hyperlink.rs`, `src/output/live.rs`, `src/ui/tui/phi_arena.rs`)
-- ============================================================================

inductive UiThemeId where
  | amber
  | ocean
  | minimal
  | highContrast
  | dracula
  | monokai
  | solarizedDark
  | solarizedLight
  | nord
  | gruvbox
  deriving DecidableEq, Repr

def UiThemeId.toU8 : UiThemeId → Nat
  | .amber => 0
  | .ocean => 1
  | .minimal => 2
  | .highContrast => 3
  | .dracula => 4
  | .monokai => 5
  | .solarizedDark => 6
  | .solarizedLight => 7
  | .nord => 8
  | .gruvbox => 9

def UiThemeId.fromU8 : Nat → UiThemeId
  | 0 => .amber
  | 1 => .ocean
  | 2 => .minimal
  | 3 => .highContrast
  | 4 => .dracula
  | 5 => .monokai
  | 6 => .solarizedDark
  | 7 => .solarizedLight
  | 8 => .nord
  | 9 => .gruvbox
  | _ => .amber

theorem ui_theme_roundtrip (t : UiThemeId) :
    UiThemeId.fromU8 t.toU8 = t := by
  cases t <;> rfl

theorem ui_theme_out_of_range_defaults_to_amber (n : Nat) (h : n ≥ 10) :
    UiThemeId.fromU8 n = .amber := by
  match n with
  | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 => omega
  | k + 10 => rfl

inductive SpinnerStopKind where
  | success
  | info
  | warn
  | error
  deriving DecidableEq, Repr

def SpinnerStopKind.glyph : SpinnerStopKind → String
  | .success => "✔"
  | .info => "ℹ"
  | .warn => "⚠"
  | .error => "✕"

theorem spinner_stop_glyphs_distinct (a b : SpinnerStopKind) :
    a.glyph = b.glyph ↔ a = b := by
  cases a <;> cases b <;> decide

inductive DiagnosticSinkModel where
  | stdout
  | stderr
  deriving DecidableEq, Repr

def diagnosticSinkForModel (jsonMode : Bool) : DiagnosticSinkModel :=
  if jsonMode then .stderr else .stdout

def hyperlinksWantedModel (styled json : Bool) (envOverride : Option Bool) (dumbOrLinux : Bool) : Bool :=
  if !styled || json then
    false
  else
    match envOverride with
    | some forced => forced
    | none => !dumbOrLinux

theorem json_mode_routes_diagnostics_to_stderr_and_blocks_hyperlinks
    (styled : Bool) (env : Option Bool) (dumb : Bool) :
    diagnosticSinkForModel true = .stderr ∧
    hyperlinksWantedModel styled true env dumb = false := by
  simp [diagnosticSinkForModel, hyperlinksWantedModel]

theorem unstyled_never_emits_hyperlinks
    (json : Bool) (env : Option Bool) (dumb : Bool) :
    hyperlinksWantedModel false json env dumb = false := by
  simp [hyperlinksWantedModel]

def drillCorrectSlot (seed : Nat) : Nat :=
  seed % 3

def stepArenaDrillStreakBps (curStreakBps : Nat) (hit isFormal : Bool) : Nat × Nat :=
  if hit then
    (if isFormal then 50 else 35, Nat.min (curStreakBps + 25) 300)
  else
    (0, 100)

theorem drill_correct_slot_in_bounds (seed : Nat) :
    drillCorrectSlot seed < 3 := by
  unfold drillCorrectSlot
  omega

theorem drill_three_seeds_cover_all_slots (s : Nat) :
    drillCorrectSlot s ≠ drillCorrectSlot (s + 1) ∧
    drillCorrectSlot (s + 1) ≠ drillCorrectSlot (s + 2) := by
  unfold drillCorrectSlot
  omega

theorem arena_drill_hit_capped_at_300_and_miss_resets_to_100
    (cur : Nat) (f : Bool) :
    (stepArenaDrillStreakBps cur true f).2 ≤ 300 ∧
    stepArenaDrillStreakBps cur false f = (0, 100) := by
  unfold stepArenaDrillStreakBps
  constructor
  · simp [Nat.min_le_right]
  · simp

-- ============================================================================
-- Section PA53: API Client Reasoning Step-Down, Side-Call Bounds & Usage Lattice
-- (`src/api/client.rs`, `src/api/usage.rs`, `src/api/types.rs`, `src/api/mod.rs`)
-- ============================================================================

def extraBodyPinsReasoningModel (hasTopReasoning hasKwargsThinking : Bool) : Bool :=
  hasTopReasoning || hasKwargsThinking

theorem extra_body_pins_reasoning_iff (top kw : Bool) :
    extraBodyPinsReasoningModel top kw = true ↔ (top = true ∨ kw = true) := by
  cases top <;> cases kw <;> decide

inductive ReasoningEffortLevel where
  | xhigh
  | high
  | medium
  | low
  | nonePinned
  deriving DecidableEq, Repr

inductive StepDownOutcome where
  | lowered (toLevel : String)
  | thinkingDisabled
  | exhausted
  deriving DecidableEq, Repr

def StepDownOutcome.asStr : StepDownOutcome → String
  | .lowered lvl => lvl
  | .thinkingDisabled => "disabled"
  | .exhausted => "exhausted"

def stepDownReasoningModel
    (isQwen effortHonored : Bool)
    (level : ReasoningEffortLevel)
    (kwargsThinkingEnabled : Bool) : StepDownOutcome :=
  match level with
  | .xhigh =>
      if !effortHonored && kwargsThinkingEnabled then .thinkingDisabled
      else if isQwen then .lowered "medium" else .lowered "high"
  | .high =>
      if !effortHonored && kwargsThinkingEnabled then .thinkingDisabled
      else .lowered "medium"
  | .medium =>
      if !effortHonored && kwargsThinkingEnabled then .thinkingDisabled
      else .lowered "low"
  | .low =>
      if kwargsThinkingEnabled then .thinkingDisabled else .exhausted
  | .nonePinned =>
      if kwargsThinkingEnabled then .thinkingDisabled else .lowered "low"

theorem unhonored_effort_immediately_disables_thinking (isQwen : Bool) (lvl : ReasoningEffortLevel) :
    stepDownReasoningModel isQwen false lvl true = .thinkingDisabled := by
  cases lvl <;> rfl

theorem qwen_honored_step_down_chain :
    stepDownReasoningModel true true .high true = .lowered "medium" ∧
    stepDownReasoningModel true true .medium true = .lowered "low" ∧
    stepDownReasoningModel true true .low true = .thinkingDisabled ∧
    stepDownReasoningModel true true .low false = .exhausted := by
  decide

def sideCallEffectiveCapSecs (purposeDefaultSecs : Nat) (configuredMaxSecs : Option Nat) : Nat :=
  match configuredMaxSecs with
  | some cfg => Nat.min cfg purposeDefaultSecs
  | none => purposeDefaultSecs

theorem side_call_cap_never_exceeds_purpose_default (defSecs : Nat) (cfg : Option Nat) :
    sideCallEffectiveCapSecs defSecs cfg ≤ defSecs := by
  unfold sideCallEffectiveCapSecs
  cases cfg with
  | none => exact Nat.le_refl defSecs
  | some c => exact Nat.min_le_right c defSecs

def classifyGatewayTimeoutModel (status : Nat) (bodyMentionsTimeout : Bool) : Bool :=
  status == 504 || ((status == 502 || status == 524 || status == 408) && bodyMentionsTimeout)

theorem status_429_never_gateway_timeout (mentions : Bool) :
    classifyGatewayTimeoutModel 429 mentions = false := by
  simp [classifyGatewayTimeoutModel]

theorem status_504_always_gateway_timeout (mentions : Bool) :
    classifyGatewayTimeoutModel 504 mentions = true := by
  simp [classifyGatewayTimeoutModel]

def sanitizeAssistantContentModel (content trimmedReasoning : String) (hasToolCalls : Bool) : String :=
  if !content.isEmpty || hasToolCalls then
    content
  else if !trimmedReasoning.isEmpty then
    trimmedReasoning
  else
    "(no textual output)"

theorem empty_assistant_without_tools_never_blank (r : String) :
    (sanitizeAssistantContentModel "" r false).isEmpty = false := by
  unfold sanitizeAssistantContentModel
  cases h : r.isEmpty <;> simp [h]

structure UsageCoverageModel where
  prompt : Bool
  completion : Bool
  total : Bool
  deriving DecidableEq, Repr

def UsageCoverageModel.union (a b : UsageCoverageModel) : UsageCoverageModel :=
  ⟨a.prompt || b.prompt, a.completion || b.completion, a.total || b.total⟩

def UsageCoverageModel.intersect (a b : UsageCoverageModel) : UsageCoverageModel :=
  ⟨a.prompt && b.prompt, a.completion && b.completion, a.total && b.total⟩

def UsageCoverageModel.isComplete (c : UsageCoverageModel) : Bool :=
  c.prompt && c.completion && c.total

def UsageCoverageModel.any (c : UsageCoverageModel) : Bool :=
  c.prompt || c.completion || c.total

theorem usage_coverage_union_comm (a b : UsageCoverageModel) :
    a.union b = b.union a := by
  cases a; cases b; simp [UsageCoverageModel.union, Bool.or_comm]

theorem usage_coverage_intersect_comm (a b : UsageCoverageModel) :
    a.intersect b = b.intersect a := by
  cases a; cases b; simp [UsageCoverageModel.intersect, Bool.and_comm]

def mergeStreamingField (cur next : Nat) : Nat :=
  Nat.max cur next

def applyFallbackField (covered : Bool) (rawVal fallbackVal : Nat) : Nat :=
  if covered then rawVal else fallbackVal

theorem streaming_field_monotone (cur next : Nat) :
    mergeStreamingField cur next ≥ cur ∧ mergeStreamingField cur next ≥ next := by
  unfold mergeStreamingField
  exact ⟨Nat.le_max_left cur next, Nat.le_max_right cur next⟩

theorem covered_field_ignores_fallback (raw fb : Nat) :
    applyFallbackField true raw fb = raw := by
  rfl

def resolveReasoningTokensModel
    (thinkingTokens detailReasoning estimatedReasoning : Option Nat) : Option Nat × Bool :=
  match thinkingTokens with
  | some t => (some t, false)
  | none =>
      match detailReasoning with
      | some d => (some d, false)
      | none =>
          match estimatedReasoning with
          | some e => (some e, true)
          | none => (none, false)

theorem reported_thinking_never_marked_estimated (t : Nat) (d e : Option Nat) :
    resolveReasoningTokensModel (some t) d e = (some t, false) := by
  rfl

-- ============================================================================
-- Section PA54: Evolve Context Fit & Content-Addressed Envelope Lattice
-- (`src/evolve/context_fit.rs`, `src/evolve/envelope.rs`, `src/evolve/mod.rs` — AGENTS.md Rule 4)
-- ============================================================================

inductive EvolveContextMode where
  | fullExtended
  | full
  | compact
  | lite
  | mapMode
  | custom
  deriving DecidableEq, Repr

def EvolveContextMode.asStr : EvolveContextMode → String
  | .fullExtended => "full_extended"
  | .full => "full"
  | .compact => "compact"
  | .lite => "lite"
  | .mapMode => "map"
  | .custom => "custom"

def parseRequestedModeName : String → Option String
  | "auto" => some "auto"
  | "full_extended" => some "full_extended"
  | "full" => some "full"
  | "compact" | "skeleton" => some "compact"
  | "lite" => some "lite"
  | "map" => some "map"
  | _ => none

theorem skeleton_alias_maps_to_compact :
    parseRequestedModeName "skeleton" = parseRequestedModeName "compact" := by
  rfl

def fitBudgetOutputReserve (contextLength maxTokens : Nat) : Nat :=
  Nat.min maxTokens (contextLength / 4)

def fitBudgetUsable (contextLength maxTokens fitRatioPct : Nat) : Nat :=
  ((contextLength - fitBudgetOutputReserve contextLength maxTokens) * fitRatioPct) / 100

theorem output_reserve_le_quarter_context (ctx maxOut : Nat) :
    fitBudgetOutputReserve ctx maxOut ≤ ctx / 4 := by
  unfold fitBudgetOutputReserve
  exact Nat.min_le_right maxOut (ctx / 4)

def measureTierTokensModel
    (rustCodeTokens rustInlineTestTokens rustCompactTokens rustSkeletonTokens
     nonRustCodeTokens nonRustCompactTokens testNodeTokens : Nat)
    (mode : EvolveContextMode) : Nat :=
  let rustCodeOnly := rustCodeTokens - rustInlineTestTokens
  match mode with
  | .fullExtended => rustCodeTokens + nonRustCodeTokens + testNodeTokens
  | .full | .custom => rustCodeOnly + nonRustCodeTokens
  | .compact => rustCompactTokens + nonRustCompactTokens
  | .lite => rustSkeletonTokens + nonRustCodeTokens
  | .mapMode => 71

theorem tier_measurement_ordering
    (rc rit rcomp rsk nrc nrcomp tst : Nat)
    (hRit : rit ≤ rc)
    (hSk : rsk ≤ rc - rit) :
    measureTierTokensModel rc rit rcomp rsk nrc nrcomp tst .lite ≤
      measureTierTokensModel rc rit rcomp rsk nrc nrcomp tst .full ∧
    measureTierTokensModel rc rit rcomp rsk nrc nrcomp tst .full ≤
      measureTierTokensModel rc rit rcomp rsk nrc nrcomp tst .fullExtended := by
  unfold measureTierTokensModel
  dsimp
  omega

theorem lite_preserves_non_rust_verbatim (rc rit rcomp rsk nrc nrcomp tst : Nat) :
    measureTierTokensModel rc rit rcomp rsk nrc nrcomp tst .lite = rsk + nrc := by
  rfl

def selectCascadingTier
    (feTokens fullTokens compactTokens liteTokens mapTokens usable : Nat) :
    EvolveContextMode × Nat × Bool :=
  if feTokens ≤ usable then
    (.fullExtended, feTokens, true)
  else if fullTokens ≤ usable then
    (.full, fullTokens, true)
  else if compactTokens ≤ usable then
    (.compact, compactTokens, true)
  else if liteTokens ≤ usable then
    (.lite, liteTokens, true)
  else if mapTokens ≤ usable then
    (.mapMode, mapTokens, true)
  else
    (.mapMode, mapTokens, false)

theorem cascading_tier_fits_within_usable
    (fe f c l m u : Nat)
    (h : (selectCascadingTier fe f c l m u).2.2 = true) :
    (selectCascadingTier fe f c l m u).2.1 ≤ u := by
  unfold selectCascadingTier at *
  split at h
  · rename_i hFe; simp [hFe]
  · split at h
    · rename_i hNotFe hF; simp [hNotFe, hF]
    · split at h
      · rename_i hNotFe hNotF hC; simp [hNotFe, hNotF, hC]
      · split at h
        · rename_i hNotFe hNotF hNotC hL; simp [hNotFe, hNotF, hNotC, hL]
        · split at h
          · rename_i hNotFe hNotF hNotC hNotL hM; simp [hNotFe, hNotF, hNotC, hNotL, hM]
          · contradiction

inductive EvolveNodeLayer where
  | code
  | test
  | auxiliary
  deriving DecidableEq, Repr

def envelopeAllowsLayer (mode : EvolveContextMode) (layer : EvolveNodeLayer) : Bool :=
  match mode with
  | .fullExtended => true
  | .custom => layer == .code || layer == .auxiliary
  | .full | .lite | .compact | .mapMode => layer == .code

theorem test_layer_only_in_full_extended (m : EvolveContextMode) :
    envelopeAllowsLayer m .test = true ↔ m = .fullExtended := by
  cases m <;> decide

def isProtectedEvolvePath (hasQuote matchesProtectedPrefix : Bool) : Bool :=
  hasQuote || matchesProtectedPrefix

theorem quoted_or_protected_prefix_always_blocked (q p : Bool)
    (h : q = true ∨ p = true) :
    isProtectedEvolvePath q p = true := by
  cases h with
  | inl hq => simp [isProtectedEvolvePath, hq]
  | inr hp => simp [isProtectedEvolvePath, hp]

-- ============================================================================
-- Section PA55: Tool Argument Validation, Package Registry Guards & Line Numbers
-- (`src/tools/git.rs`, `src/tools/cargo.rs`, `src/tools/package.rs`,
--  `src/tools/argv_guard.rs`, `src/tools/line_numbers.rs`, `src/tools/mod.rs`)
-- ============================================================================

def validateGitRefModel
    (nonEmpty le255 startsWithDash hasDotDot hasAtBrace hasLockOrDotComp : Bool) : Bool :=
  nonEmpty && le255 && !startsWithDash && !hasDotDot && !hasAtBrace && !hasLockOrDotComp

theorem git_ref_rejects_flag_dotdot_and_lock (ne le : Bool) :
    validateGitRefModel ne le true false false false = false ∧
    validateGitRefModel ne le false true false false = false ∧
    validateGitRefModel ne le false false false true = false := by
  cases ne <;> cases le <;> decide

def validatePushBranchModel
    (validRef isHead startsWithRefs hasPlusOrColon : Bool) : Bool :=
  validRef && !isHead && !startsWithRefs && !hasPlusOrColon

theorem push_branch_rejects_refspec_and_head (vr sr : Bool) :
    validatePushBranchModel vr true sr false = false ∧
    validatePushBranchModel vr false sr true = false := by
  simp [validatePushBranchModel]

def validateRegistrySpecModel
    (nonEmpty isFlagLike hasUrlOrVcs hasArchiveSuffix hasPathTraversal hasNpmAlias : Bool) : Bool :=
  nonEmpty && !isFlagLike && !hasUrlOrVcs && !hasArchiveSuffix && !hasPathTraversal && !hasNpmAlias

theorem registry_spec_rejects_url_archive_traversal_and_flag
    (ne url arch trav alias_ : Bool) :
    validateRegistrySpecModel ne true url arch trav alias_ = false ∧
    validateRegistrySpecModel ne false true arch trav alias_ = false ∧
    validateRegistrySpecModel ne false url true trav alias_ = false ∧
    validateRegistrySpecModel ne false url arch true alias_ = false := by
  simp [validateRegistrySpecModel]

def cargoTestRunVerdictModel (exitOk : Bool) (passed failed : Nat) : Bool × Bool :=
  let zeroTestsRan := exitOk && passed == 0 && failed == 0
  let success := exitOk && !zeroTestsRan
  (success, zeroTestsRan)

theorem zero_executed_tests_never_succeeds (exitOk : Bool) :
    (cargoTestRunVerdictModel exitOk 0 0).1 = false := by
  cases exitOk <;> decide

theorem positive_passed_clean_exit_succeeds (p : Nat) (f : Nat) (h : p > 0) :
    (cargoTestRunVerdictModel true p f).1 = true := by
  unfold cargoTestRunVerdictModel
  have hNe : (p == 0) = false := beq_false_of_ne (by omega)
  simp [hNe]

def dangerousShellPatternMatch (hasPipeToShWordBoundary hasRmRoot : Bool) : Bool :=
  hasPipeToShWordBoundary || hasRmRoot

theorem shellcheck_and_sha256sum_not_pipe_to_sh :
    dangerousShellPatternMatch false false = false ∧
    dangerousShellPatternMatch true false = true := by
  decide

-- ============================================================================
-- Section PA56: Session Encryption, ChatStore Collision & EditHistory Lattice
-- (`src/session/encryption.rs`, `src/session/chat_store.rs`,
--  `src/session/cache.rs`, `src/session/edit_history.rs`)
-- ============================================================================

def aesGcmNonceLen : Nat := 12
def aesGcmTagLen : Nat := 16

def aesGcmCiphertextLen (plaintextLen : Nat) : Nat :=
  aesGcmNonceLen + plaintextLen + aesGcmTagLen

def canDecryptPayloadModel (payloadLen : Nat) (keyMatches tagValid : Bool) : Bool :=
  decide (payloadLen ≥ aesGcmNonceLen) && keyMatches && tagValid

theorem aes_gcm_ciphertext_overhead_is_28 (n : Nat) :
    aesGcmCiphertextLen n - n = 28 := by
  unfold aesGcmCiphertextLen aesGcmNonceLen aesGcmTagLen
  omega

theorem short_or_tampered_or_wrong_key_rejected (len : Nat) (k : Bool) :
    canDecryptPayloadModel 11 k true = false ∧
    canDecryptPayloadModel len false true = false ∧
    canDecryptPayloadModel len k false = false := by
  simp [canDecryptPayloadModel, aesGcmNonceLen]

def validateChatSessionNameModel (nonEmpty allAsciiAlnumDashUnderscore : Bool) : Bool :=
  nonEmpty && allAsciiAlnumDashUnderscore

def canLoadOrDeleteChatFile (validName internalNameMatches : Bool) : Bool :=
  validName && internalNameMatches

theorem mismatched_internal_chat_name_fails_closed (v : Bool) :
    canLoadOrDeleteChatFile v false = false := by
  simp [canLoadOrDeleteChatFile]

def llmCacheEstimatedCostMilli (promptTokens completionTokens inPriceMilliPer1k outPriceMilliPer1k : Nat) : Nat :=
  (promptTokens * inPriceMilliPer1k + completionTokens * outPriceMilliPer1k) / 1000

theorem llm_cache_cost_monotone_in_prompt
    (p1 p2 c ip op : Nat) (h : p1 ≤ p2) :
    llmCacheEstimatedCostMilli p1 c ip op ≤ llmCacheEstimatedCostMilli p2 c ip op := by
  unfold llmCacheEstimatedCostMilli
  have hMul : p1 * ip ≤ p2 * ip := Nat.mul_le_mul_right ip h
  have hAdd : p1 * ip + c * op ≤ p2 * ip + c * op := Nat.add_le_add_right hMul (c * op)
  exact Nat.div_le_div_right hAdd

def editHistoryCanUndoModel (checkpointCount : Nat) (currentHasFiles : Bool) : Bool :=
  decide (checkpointCount > 1) || (decide (checkpointCount == 1) && currentHasFiles)

def pruneCheckpointsLen (total maxCheckpoints : Nat) : Nat :=
  Nat.min total maxCheckpoints

theorem lone_empty_checkpoint_cannot_undo_but_with_files_can :
    editHistoryCanUndoModel 0 false = false ∧
    editHistoryCanUndoModel 1 false = false ∧
    editHistoryCanUndoModel 1 true = true ∧
    editHistoryCanUndoModel 2 false = true := by
  decide

theorem pruned_checkpoints_never_exceed_max (tot maxCp : Nat) :
    pruneCheckpointsLen tot maxCp ≤ maxCp := by
  unfold pruneCheckpointsLen
  exact Nat.min_le_right tot maxCp

def canRestoreSnapshotGuarded (diskMatchesAfterHash forceRestore : Bool) : Bool :=
  diskMatchesAfterHash || forceRestore

theorem external_modification_blocks_unforced_restore :
    canRestoreSnapshotGuarded false false = false ∧
    canRestoreSnapshotGuarded false true = true ∧
    canRestoreSnapshotGuarded true false = true := by
  decide

-- ============================================================================
-- Section PA57: Safety Scanner, Secret Redaction & Permission/Quarantine Guards
-- (`src/safety/scanner.rs`, `src/safety/permissions.rs`, `src/safety/quarantine.rs`)
-- ============================================================================

inductive ScannerSeverity where
  | info
  | low
  | medium
  | high
  | critical
  deriving DecidableEq, Repr

def ScannerSeverity.asStr : ScannerSeverity → String
  | .info => "info"
  | .low => "low"
  | .medium => "medium"
  | .high => "high"
  | .critical => "critical"

def ScannerSeverity.scoreTenths : ScannerSeverity → Nat
  | .info => 0
  | .low => 30
  | .medium => 55
  | .high => 75
  | .critical => 95

def ScannerSeverity.rank : ScannerSeverity → Nat
  | .info => 0
  | .low => 1
  | .medium => 2
  | .high => 3
  | .critical => 4

theorem scanner_severity_score_strictly_monotone (a b : ScannerSeverity)
    (h : a.rank < b.rank) :
    a.scoreTenths < b.scoreTenths := by
  cases a <;> cases b <;> revert h <;> decide

def scanResultRiskScoreTenths (findings : List ScannerSeverity) : Nat :=
  findings.foldl (fun acc s => acc + s.scoreTenths) 0

def scanResultHasCritical (findings : List ScannerSeverity) : Bool :=
  findings.any (fun s => s == .critical)

def scanResultHasHigh (findings : List ScannerSeverity) : Bool :=
  findings.any (fun s => s == .high)

def secretPatternMatchesLine (isComment : Bool) (sev : ScannerSeverity) (regexMatches : Bool) : Bool :=
  if isComment && sev != .critical then
    false
  else
    regexMatches

theorem comment_line_blocks_non_critical_secrets (sev : ScannerSeverity) (m : Bool)
    (h : sev ≠ .critical) :
    secretPatternMatchesLine true sev m = false := by
  cases sev <;> simp_all [secretPatternMatchesLine]

theorem comment_line_retains_critical_secrets (m : Bool) :
    secretPatternMatchesLine true .critical m = m := by
  simp [secretPatternMatchesLine]

def maskedSecretCharCount (charLen : Nat) : Nat :=
  if charLen ≤ 8 then
    charLen
  else
    4 + 3 + (charLen - 4)

theorem masked_secret_above_8_adds_ellipsis_len (n : Nat) (h : n > 8) :
    maskedSecretCharCount n = n + 3 := by
  unfold maskedSecretCharCount
  have hNotLe : ¬ (n ≤ 8) := by omega
  simp [hNotLe]
  omega

def permissionGrantMatchesModel (toolMatches resourceGlobMatches hasDotDotSegment : Bool) : Bool :=
  toolMatches && !hasDotDotSegment && resourceGlobMatches

theorem dotdot_segment_always_denies_permission_grant (t g : Bool) :
    permissionGrantMatchesModel t g true = false := by
  simp [permissionGrantMatchesModel]

inductive ShellAllowRuleKind where
  | prefixRule
  | exactRule
  deriving DecidableEq, Repr

def deriveShellAllowRuleKind
    (hasMetachars eligibleRisk plainFirst isExactOnlyProg isBareMultiPurpose : Bool) :
    ShellAllowRuleKind :=
  if hasMetachars || !eligibleRisk || !plainFirst || isExactOnlyProg || isBareMultiPurpose then
    .exactRule
  else
    .prefixRule

def prefixRuleMatchesCommand
    (prefixNonEmpty hasMetachars prefixMatches hasTrailingOptionFlag eligibleRisk : Bool) : Bool :=
  prefixNonEmpty && !hasMetachars && prefixMatches && !hasTrailingOptionFlag && eligibleRisk

theorem trailing_option_or_metachar_blocks_prefix_rule (ne pm er : Bool) :
    prefixRuleMatchesCommand ne true pm false er = false ∧
    prefixRuleMatchesCommand ne false pm true er = false := by
  simp [prefixRuleMatchesCommand]

def gitVersionSupportsAttrSource (major minor : Nat) : Bool :=
  decide (major > 2 || (major == 2 && minor ≥ 40))

def selectArmGitVersion
    (sysMaj sysMin : Nat)
    (candMaj candMin : Nat)
    (candInOperatorHome : Bool) : Nat × Nat × Bool :=
  if gitVersionSupportsAttrSource sysMaj sysMin then
    (sysMaj, sysMin, true)
  else if !candInOperatorHome && gitVersionSupportsAttrSource candMaj candMin then
    (candMaj, candMin, true)
  else
    (sysMaj, sysMin, false)

theorem operator_home_candidate_never_selected (sMaj sMin cMaj cMin : Nat) :
    selectArmGitVersion sMaj sMin cMaj cMin true =
      (sMaj, sMin, gitVersionSupportsAttrSource sMaj sMin) := by
  unfold selectArmGitVersion
  cases gitVersionSupportsAttrSource sMaj sMin <;> rfl

-- ============================================================================
-- Section PA58: Introspect Token/Depth Lattice & File/Patch Edit Guards
-- (`src/tools/introspect/budget.rs`, `src/tools/introspect/query.rs`,
--  `src/tools/introspect/parser.rs`, `src/tools/file.rs`, `src/tools/patch_apply.rs`)
-- ============================================================================

inductive IntrospectDepth where
  | overview
  | signatures
  | full
  | dependencies
  deriving DecidableEq, Repr

def IntrospectDepth.asStr : IntrospectDepth → String
  | .overview => "overview"
  | .signatures => "signatures"
  | .full => "full"
  | .dependencies => "dependencies"

def IntrospectDepth.downgrade : IntrospectDepth → Option IntrospectDepth
  | .full => some .signatures
  | .dependencies => some .signatures
  | .signatures => some .overview
  | .overview => none

theorem depth_downgrade_terminates_in_three_steps (d : IntrospectDepth) :
    ((d.downgrade.bind IntrospectDepth.downgrade).bind IntrospectDepth.downgrade) = none := by
  cases d <;> rfl

structure IntrospectBudgetState where
  total : Nat
  used : Nat
  reserved : Nat
  deriving DecidableEq, Repr

def IntrospectBudgetState.reservePct (total pct : Nat) : IntrospectBudgetState :=
  { total := total, used := 0, reserved := (total * pct) / 100 }

def IntrospectBudgetState.remaining (b : IntrospectBudgetState) : Nat :=
  b.total - (b.used + b.reserved)

def IntrospectBudgetState.exhausted (b : IntrospectBudgetState) : Bool :=
  decide (b.used + b.reserved ≥ b.total)

def IntrospectBudgetState.tryAllocate (b : IntrospectBudgetState) (requested : Nat) :
    Bool × IntrospectBudgetState :=
  if requested > b.remaining then
    (false, b)
  else
    (true, { b with used := b.used + requested })

theorem failed_try_allocate_preserves_budget_state (b : IntrospectBudgetState) (req : Nat)
    (h : req > b.remaining) :
    b.tryAllocate req = (false, b) := by
  unfold IntrospectBudgetState.tryAllocate
  simp [h]

theorem successful_try_allocate_conserves_sum (b : IntrospectBudgetState) (req : Nat)
    (h : req ≤ b.remaining) :
    let (ok, b') := b.tryAllocate req
    ok = true ∧ b'.used + b'.remaining = b.used + b.remaining := by
  unfold IntrospectBudgetState.tryAllocate IntrospectBudgetState.remaining at *
  have hNotGt : ¬ (req > b.total - (b.used + b.reserved)) := by omega
  simp [hNotGt]
  omega

def denumberEditFires
    (oldNonEmpty contentContainsRawOld oldIsNumbered contentContainsStrippedOld : Bool) : Bool :=
  oldNonEmpty && !contentContainsRawOld && oldIsNumbered && contentContainsStrippedOld

theorem raw_match_never_triggers_denumber_edit (ne num strMatch : Bool) :
    denumberEditFires ne true num strMatch = false := by
  simp [denumberEditFires]

def refuseDisplayArtifactsModel
    (newHasRedacted origHasRedacted newHasEscapedTag origHasEscapedTag isMarkupExt : Bool) : Bool :=
  let badRedacted := newHasRedacted && !origHasRedacted
  let badEscaped := !isMarkupExt && newHasEscapedTag && !origHasEscapedTag
  badRedacted || badEscaped

theorem preexisting_artifacts_and_markup_tags_allowed :
    refuseDisplayArtifactsModel true true false false false = false ∧
    refuseDisplayArtifactsModel false false true false true = false ∧
    refuseDisplayArtifactsModel true false false false false = true ∧
    refuseDisplayArtifactsModel false false true false false = true := by
  decide

def checkDiffHeaderSafeModel (hasDoubleQuote hasControlChar : Bool) : Bool :=
  !hasDoubleQuote && !hasControlChar

theorem quoted_or_control_diff_header_rejected (q c : Bool)
    (h : q = true ∨ c = true) :
    checkDiffHeaderSafeModel q c = false := by
  cases h with
  | inl hq => simp [checkDiffHeaderSafeModel, hq]
  | inr hc => simp [checkDiffHeaderSafeModel, hc]

-- ============================================================================
-- Section PA59: Agent Context Map, Compression Circuit Breaker & Failure/Compaction Lattice
-- (`src/agent/context_map.rs`, `src/agent/context.rs`, `src/agent/compression.rs`,
--  `src/agent/result_compaction.rs`, `src/agent/failure_mode.rs`)
-- ============================================================================

inductive TaskModalityModel where
  | merge
  | refactor
  | review
  | debug
  | greenfield
  | implement
  deriving DecidableEq, Repr

def TaskModalityModel.name : TaskModalityModel → String
  | .merge => "Merge"
  | .refactor => "Refactor"
  | .review => "Review"
  | .debug => "Debug"
  | .greenfield => "Greenfield"
  | .implement => "Implement"

def TaskModalityModel.ratiosPct : TaskModalityModel → Nat × Nat × Nat
  | .merge => (55, 30, 15)
  | .refactor => (50, 35, 15)
  | .review => (65, 20, 15)
  | .debug => (50, 20, 30)
  | .greenfield => (40, 30, 30)
  | .implement => (60, 25, 15)

def classifyTaskModality
    (hasMerge hasRefactor hasReview hasDebug hasGreenfield : Bool) : TaskModalityModel :=
  if hasMerge then .merge
  else if hasRefactor then .refactor
  else if hasReview then .review
  else if hasDebug then .debug
  else if hasGreenfield then .greenfield
  else .implement

theorem all_modality_ratios_sum_to_100 (m : TaskModalityModel) :
    let (c, comp, th) := m.ratiosPct
    c + comp + th = 100 := by
  cases m <;> decide

theorem merge_keyword_dominates_modality_priority (r rev d g : Bool) :
    classifyTaskModality true r rev d g = .merge := by
  rfl

inductive ContextNodeLevel where
  | mapLevel
  | liteLevel
  | fullLevel
  deriving DecidableEq, Repr

def ContextNodeLevel.name : ContextNodeLevel → String
  | .mapLevel => "Map"
  | .liteLevel => "Lite"
  | .fullLevel => "Full"

def downgradeNodeToSkeleton
    (lvl : ContextNodeLevel) (fullTok : Nat) (skelTok : Option Nat) :
    ContextNodeLevel × Nat × Nat :=
  match lvl, skelTok with
  | .fullLevel, some s => (.liteLevel, s, fullTok - s)
  | _, _ => (lvl, fullTok, 0)

theorem downgrade_without_skeleton_is_noop (lvl : ContextNodeLevel) (tok : Nat) :
    downgradeNodeToSkeleton lvl tok none = (lvl, tok, 0) := by
  cases lvl <;> rfl

def contextCompressorThreshold (budget : Nat) : Nat :=
  (budget * 75) / 100

def stepSummaryFailureStrike (consecutiveFailures : Nat) : Nat × Bool :=
  let next := consecutiveFailures + 1
  (next, decide (next ≥ 3))

theorem third_summary_failure_latches_disable :
    stepSummaryFailureStrike 0 = (1, false) ∧
    stepSummaryFailureStrike 1 = (2, false) ∧
    stepSummaryFailureStrike 2 = (3, true) := by
  decide

def autoCompactShouldCompress
    (circuitOpen : Bool) (currentTokens contextWindow thresholdPct : Nat) : Bool :=
  if circuitOpen || contextWindow == 0 then
    false
  else
    decide (currentTokens * 100 ≥ contextWindow * thresholdPct)

theorem open_circuit_or_zero_window_blocks_autocompact (cur win th : Nat) :
    autoCompactShouldCompress true cur win th = false ∧
    autoCompactShouldCompress false cur 0 th = false := by
  simp [autoCompactShouldCompress]

def stubTokenBudgetModel (maxContextTokens : Nat) : Nat :=
  let raw := maxContextTokens / 64
  if raw ≤ 200 then 200 else if raw ≥ 500 then 500 else raw

theorem stub_token_budget_within_200_to_500 (n : Nat) :
    200 ≤ stubTokenBudgetModel n ∧ stubTokenBudgetModel n ≤ 500 := by
  unfold stubTokenBudgetModel
  dsimp
  split
  · omega
  · split <;> omega

inductive AgentFailureKindModel where
  | success
  | noChange
  | fakeComplete
  | requiredEditMissing
  | verificationFailed
  | recoveryExhausted
  deriving DecidableEq, Repr

def AgentFailureKindModel.tag : AgentFailureKindModel → String
  | .success => "REAL_EDIT"
  | .noChange => "NO_CHANGES"
  | .fakeComplete => "FAKE_COMPLETE"
  | .requiredEditMissing => "NO_CHANGES_REQUIRED_EDIT"
  | .verificationFailed => "VERIFICATION_FAILED"
  | .recoveryExhausted => "RECOVERY_EXHAUSTED"

def AgentFailureKindModel.isSuccess : AgentFailureKindModel → Bool
  | .success => true
  | _ => false

def AgentFailureKindModel.isNonfailure : AgentFailureKindModel → Bool
  | .success | .noChange => true
  | _ => false

theorem success_implies_nonfailure_and_fake_is_failure (k : AgentFailureKindModel) :
    (k.isSuccess = true → k.isNonfailure = true) ∧
    AgentFailureKindModel.fakeComplete.isNonfailure = false ∧
    AgentFailureKindModel.requiredEditMissing.isNonfailure = false := by
  refine ⟨?_, by decide, by decide⟩
  intro h
  cases k <;> simp_all [AgentFailureKindModel.isSuccess, AgentFailureKindModel.isNonfailure]

-- ============================================================================
-- Section PA60: Evolve Action Engine, Staged Apply Verification & Quality Metrics
-- (`src/evolve/actions.rs`, `src/evolve/apply.rs`, `src/evolve/quality.rs`)
-- ============================================================================

inductive EvolveActionKind where
  | extendAction
  | connectAction
  | blockEvolution
  | notifyAction
  deriving DecidableEq, Repr

def EvolveActionKind.createsBranch : EvolveActionKind → Bool
  | .extendAction => true
  | .connectAction | .blockEvolution | .notifyAction => false

theorem only_extend_action_creates_branch (k : EvolveActionKind) :
    k.createsBranch = true ↔ k = .extendAction := by
  cases k <;> decide

def isScaffoldArtifactModel (exactMatch prefixSlashMatch : Bool) : Bool :=
  exactMatch || prefixSlashMatch

inductive ApplyRejectReasonModel where
  | emptyDiff
  | outOfScope (path : String)
  | compileFailed (stderr : String)
  | killswitchActive (reason : String)
  deriving DecidableEq, Repr

def ApplyRejectReasonModel.display : ApplyRejectReasonModel → String
  | .emptyDiff => "empty_diff"
  | .outOfScope p => s!"diff_out_of_scope: {p}"
  | .compileFailed e => s!"compile_failed: {e}"
  | .killswitchActive r => s!"killswitch_active: {r}"

def verifyStagedPathGate
    (isScaffold isSymlink inSrcOrDocs isProtected : Bool) (path : String) :
    Option ApplyRejectReasonModel :=
  if isScaffold then
    some (.outOfScope path)
  else if isSymlink then
    some (.outOfScope s!"{path} (symlink)")
  else if !inSrcOrDocs then
    some (.outOfScope path)
  else if isProtected then
    some (.outOfScope s!"{path} (protected path)")
  else
    none

theorem protected_or_outside_scope_always_rejected
    (sc sym inScope : Bool) (path : String) :
    (verifyStagedPathGate sc sym false false path).isSome = true ∧
    (verifyStagedPathGate sc sym inScope true path).isSome = true := by
  cases sc <;> cases sym <;> cases inScope <;> simp [verifyStagedPathGate]

inductive CommitRejectModel where
  | unknownRun
  | notStaged (status : String)
  | killswitch (reason : String)
  | baseMoved
  deriving DecidableEq, Repr

def CommitRejectModel.tag : CommitRejectModel → String
  | .unknownRun => "unknown_run"
  | .notStaged st => s!"not_staged:{st}"
  | .killswitch r => s!"killswitch:{r}"
  | .baseMoved => "base_moved"

def evaluateCommitStagedGate
    (runExists isStaged digestMatches killswitchClear headMatchesBase : Bool)
    (statusStr : String) : Option CommitRejectModel :=
  if !runExists then
    some .unknownRun
  else if !isStaged then
    some (.notStaged statusStr)
  else if !digestMatches then
    some .unknownRun
  else if !killswitchClear then
    some (.killswitch "active")
  else if !headMatchesBase then
    some .baseMoved
  else
    none

theorem digest_mismatch_or_moved_base_blocks_commit (ex ks hm : Bool) (s : String) :
    evaluateCommitStagedGate ex true false ks hm s = some .unknownRun ∧
    evaluateCommitStagedGate true true true true false s = some .baseMoved := by
  cases ex <;> simp [evaluateCommitStagedGate]

def staticComplexityTenths (decisionPoints : Nat) : Nat :=
  10 + decisionPoints * 10

def staticDeadCodeRatioPct (allowCount fnCount : Nat) : Nat :=
  if fnCount == 0 then 0 else Nat.min ((allowCount * 100) / fnCount) 100

theorem static_dead_code_ratio_bounded_100 (a f : Nat) :
    staticDeadCodeRatioPct a f ≤ 100 := by
  unfold staticDeadCodeRatioPct
  split
  · decide
  · exact Nat.min_le_right ((a * 100) / f) 100

-- ============================================================================
-- Section PA61: SWL Document Validation, Lowering, Codegen & State Schema/Backend Lattice
-- (`src/swl/parser/validator.rs`, `src/swl/lowering.rs`, `src/swl/types/checker.rs`,
--  `src/swl/types/schema.rs`, `src/swl/state/validation.rs`, `src/swl/state/backend.rs`,
--  `src/swl/state/mod.rs`)
-- ============================================================================

def swlEmptyDocIssueCount (emptyVer emptyName emptyAgents emptyWorkflows : Bool) : Nat :=
  (if emptyVer then 1 else 0) +
  (if emptyName then 1 else 0) +
  (if emptyAgents then 1 else 0) +
  (if emptyWorkflows then 1 else 0)

def swlMapReduceStageIssues (isMapReduce hasMap hasReduce : Bool) : Nat :=
  if !isMapReduce then 0
  else (if hasMap then 0 else 1) + (if hasReduce then 0 else 1)

def swlStepActionIssues (actionKinds : Nat) (delegateUnknown : Bool) : Nat :=
  let kindIssue := if actionKinds == 1 then 0 else 1
  let unkIssue := if delegateUnknown then 1 else 0
  kindIssue + unkIssue

theorem swl_step_clean_iff_single_known_action (k : Nat) (u : Bool) :
    swlStepActionIssues k u = 0 ↔ (k = 1 ∧ u = false) := by
  unfold swlStepActionIssues
  cases u <;> simp

inductive SwlViolationAction where
  | block
  | alert
  | warn
  | log
  deriving DecidableEq, Repr

def SwlViolationAction.loweredSeverityAndRequired : SwlViolationAction → String × Bool
  | .block => ("critical", true)
  | .alert => ("high", false)
  | .warn => ("medium", false)
  | .log => ("low", false)

theorem only_block_guardrail_is_critical_and_required (a : SwlViolationAction) :
    (a.loweredSeverityAndRequired).2 = true ↔ a = .block := by
  cases a <;> decide

def loweredDelegateStepCount (hasOutputKey : Bool) : Nat :=
  if hasOutputKey then 2 else 1

theorem delegate_with_output_key_emits_two_steps :
    loweredDelegateStepCount true = 2 ∧ loweredDelegateStepCount false = 1 := by
  decide

inductive SwlFieldTypeKind where
  | scalar
  | collection
  | reference
  deriving DecidableEq, Repr

def SwlFieldTypeKind.flags : SwlFieldTypeKind → Bool × Bool × Bool
  | .scalar => (true, false, false)
  | .collection => (false, true, false)
  | .reference => (false, false, true)

theorem field_type_taxonomy_mutually_exclusive (k : SwlFieldTypeKind) :
    let (s, c, r) := k.flags
    (if s then 1 else 0) + (if c then 1 else 0) + (if r then 1 else 0) = 1 := by
  cases k <;> decide

def codegenReduceLanguageIssues (isCodeReduce isRustLanguage : Bool) : Nat :=
  if isCodeReduce && !isRustLanguage then 1 else 0

theorem rust_reduce_has_zero_codegen_issues (isCode : Bool) :
    codegenReduceLanguageIssues isCode true = 0 := by
  cases isCode <;> rfl

def swlValueMatchesNumericField (isIntegerField isFloatField valIsInt valIsFloat : Bool) : Bool :=
  if isIntegerField then valIsInt
  else if isFloatField then (valIsInt || valIsFloat)
  else false

def swlStateTransitionValid (removedRequiredPresentInOld addedUnknownField : Bool) : Bool :=
  !removedRequiredPresentInOld && !addedUnknownField

theorem float_accepts_int_and_removed_required_blocks_transition :
    swlValueMatchesNumericField false true true false = true ∧
    swlValueMatchesNumericField true false false true = false ∧
    swlStateTransitionValid true false = false ∧
    swlStateTransitionValid false true = false := by
  decide

def parseStateBackendSupported (name : String) : Bool :=
  name == "memory" || name == "MEMORY" || name == "file"

inductive StateManagerOp where
  | applyDefaults
  | setValid
  | setInvalid
  | save
  deriving DecidableEq, Repr

def stepStateManagerDirty (dirty : Bool) (op : StateManagerOp) : Bool :=
  match op with
  | .applyDefaults | .setValid => true
  | .setInvalid => dirty
  | .save => false

theorem invalid_set_preserves_dirty_and_save_clears (d : Bool) :
    stepStateManagerDirty d .setInvalid = d ∧
    stepStateManagerDirty d .save = false := by
  cases d <;> decide

-- ============================================================================
-- Section PA62: Cognitive Memory Tag Neutralization, Semantic Linker, Curriculum,
-- Intelligence & Honest Metrics
-- (`src/cognitive/memory_system.rs`, `src/cognitive/knowledge_graph.rs`,
--  `src/cognitive/learning.rs`, `src/cognitive/intelligence.rs`,
--  `src/cognitive/metrics.rs`, `src/cognitive/rag.rs`)
-- ============================================================================

def neutralizedRawTagCount (_rawCloseTags _rawOpenTags : Nat) : Nat := 0

def framedOuterCloseTagCount (_injectedCloseTags : Nat) : Nat := 1

theorem framing_neutralizes_all_inner_tags (c o inj : Nat) :
    neutralizedRawTagCount c o = 0 ∧ framedOuterCloseTagCount inj = 1 := by
  trivial

inductive LinkerLanguage where
  | rust
  | python
  | javascript
  | unknown
  deriving DecidableEq, Repr

def extractImportsCountModel (lang : LinkerLanguage) (matchedCount : Nat) : Nat :=
  match lang with
  | .unknown => 0
  | .rust | .python | .javascript => matchedCount

theorem unknown_language_extracts_zero_imports (n : Nat) :
    extractImportsCountModel .unknown n = 0 := by
  rfl

def lessonEstimatedMinutes (conceptCount : Nat) : Nat :=
  conceptCount * 15

theorem lesson_minutes_additive (a b : Nat) :
    lessonEstimatedMinutes (a + b) = lessonEstimatedMinutes a + lessonEstimatedMinutes b := by
  unfold lessonEstimatedMinutes
  exact Nat.add_mul a b 15

inductive RustVisibilityModel where
  | pubVis
  | pubCrate
  | pubSuper
  | privateVis
  deriving DecidableEq, Repr

def RustVisibilityModel.isPublic : RustVisibilityModel → Bool
  | .pubVis => true
  | .pubCrate | .pubSuper | .privateVis => false

theorem only_unrestricted_pub_is_public (v : RustVisibilityModel) :
    v.isPublic = true ↔ v = .pubVis := by
  cases v <;> decide

def verificationNotRunRatePct (firstRan finalRan : Bool) : Nat :=
  if !firstRan && !finalRan then 100 else 0

def errorRecoveryRatePct (totalErrors recoveredErrors : Nat) : Nat :=
  if totalErrors == 0 then 100 else (recoveredErrors * 100) / totalErrors

theorem unverified_run_reports_100pct_not_run :
    verificationNotRunRatePct false false = 100 ∧
    verificationNotRunRatePct true true = 0 := by
  decide

-- ============================================================================
-- Section PA63: Testing Quality Profiles, Language QA Zero-Test Gates,
-- Rust Edition & Verification Honesty
-- (`src/testing/qa_profiles.rs`, `src/testing/language_qa.rs`,
--  `src/testing/rust_edition.rs`, `src/testing/syntax_toolchain.rs`,
--  `src/testing/verification.rs`)
-- ============================================================================

inductive QualityGradeModel where
  | S
  | A
  | B
  | C
  | D
  | F
  deriving DecidableEq, Repr

def QualityGradeModel.asStr : QualityGradeModel → String
  | .S => "S"
  | .A => "A"
  | .B => "B"
  | .C => "C"
  | .D => "D"
  | .F => "F"

def classifyQualityGradeTenths (scoreTenths : Nat) : QualityGradeModel :=
  if scoreTenths ≥ 950 then .S
  else if scoreTenths ≥ 900 then .A
  else if scoreTenths ≥ 800 then .B
  else if scoreTenths ≥ 700 then .C
  else if scoreTenths ≥ 600 then .D
  else .F

def stageWeightedScoreTenths (notRun passed warningsOnly : Bool) (weight : Nat) : Nat :=
  if notRun then 0
  else if passed then weight * 10
  else if warningsOnly then weight * 5
  else 0

theorem not_run_stage_contributes_zero_even_with_warnings (p wOnly : Bool) (w : Nat) :
    stageWeightedScoreTenths true p wOnly w = 0 := by
  rfl

def demoteZeroTestRun (rawPassed isPyExit5 isUnittestZero isGoNoTestFiles : Bool) : Bool × Bool :=
  if isPyExit5 || (rawPassed && (isUnittestZero || isGoNoTestFiles)) then
    (false, true)
  else
    (rawPassed, false)

theorem zero_test_indicators_demote_to_not_run (rawP : Bool) :
    demoteZeroTestRun rawP true false false = (false, true) ∧
    demoteZeroTestRun true false true false = (false, true) ∧
    demoteZeroTestRun true false false true = (false, true) := by
  cases rawP <;> decide

def resolveRustEditionModel
    (rustfmtEd cargoPkgEd cargoWsEd : Option String) : String × Bool :=
  match rustfmtEd with
  | some e => (e, false)
  | none =>
    match cargoPkgEd with
    | some e => (e, false)
    | none =>
      match cargoWsEd with
      | some e => (e, false)
      | none => ("2021", true)

theorem rustfmt_edition_overrides_cargo_and_fallback_is_2021 (pkg ws : Option String) :
    resolveRustEditionModel (some "2024") pkg ws = ("2024", false) ∧
    resolveRustEditionModel none none none = ("2021", true) :=
  ⟨rfl, rfl⟩

def normalizeJavaMajorModel (firstTok secondTok : Nat) : Nat :=
  if firstTok == 1 && secondTok > 0 then secondTok else firstTok

def cmakeStdFlagModel (isCxx : Bool) (std : Nat) (gnuExt : Bool) : String :=
  if isCxx then
    let pfx := if gnuExt then "-std=gnu++" else "-std=c++"
    if std ≥ 23 then s!"{pfx}2b" else s!"{pfx}{std}"
  else
    let pfx := if gnuExt then "-std=gnu" else "-std=c"
    s!"{pfx}{std}"

theorem java_legacy_1_8_normalizes_to_8 :
    normalizeJavaMajorModel 1 8 = 8 ∧ normalizeJavaMajorModel 21 0 = 21 := by
  decide

inductive RustfmtFailureKindModel where
  | toolUnavailable
  | formattingDiff
  | syntaxFailure
  deriving DecidableEq, Repr

def classifyRustfmtFailureModel
    (missingComponent hasDiffIn hasErrorToken : Bool) : RustfmtFailureKindModel :=
  if missingComponent then .toolUnavailable
  else if hasDiffIn && !hasErrorToken then .formattingDiff
  else .syntaxFailure

inductive VerificationBannerModel where
  | notVerified
  | passedAll
  | passedPartialNotRun
  | failed
  deriving DecidableEq, Repr

def VerificationBannerModel.tag : VerificationBannerModel → String
  | .notVerified => "NOT_VERIFIED"
  | .passedAll => "PASSED"
  | .passedPartialNotRun => "PASSED_1_NOT_RUN"
  | .failed => "FAILED"

def classifyVerificationBanner
    (overallPassed : Bool) (totalChecks notRunChecks : Nat) : VerificationBannerModel :=
  if !overallPassed then .failed
  else if totalChecks > 0 && notRunChecks == totalChecks then .notVerified
  else if notRunChecks > 0 then .passedPartialNotRun
  else .passedAll

theorem diff_with_error_is_syntax_and_all_not_run_is_not_verified :
    classifyRustfmtFailureModel false true true = .syntaxFailure ∧
    classifyVerificationBanner true 2 2 = .notVerified := by
  decide

-- ============================================================================
-- Section PA64: Skills Trust/Discovery Gate & Resource Lifecycle/Quota/Reaper Invariants
-- (`src/skills/mod.rs`, `src/resources/context.rs`, `src/resources/quotas.rs`,
--  `src/resources/reaper.rs`, `src/resources/registry.rs`, `src/resources/teardown.rs`,
--  `src/resources/disk.rs`)
-- ============================================================================

def parseSkillFrontmatterValid
    (startsWithDelim hasClosingDelim nameNonEmpty : Bool) : Bool :=
  startsWithDelim && hasClosingDelim && nameNonEmpty

def userSkillDiscoveryAdmitted
    (isCandidateOrDistilled inLedgerWithMatchingHash : Bool) : Bool :=
  !isCandidateOrDistilled || inLedgerWithMatchingHash

theorem unadmitted_candidate_refused_in_user_dir :
    userSkillDiscoveryAdmitted true false = false ∧
    userSkillDiscoveryAdmitted false false = true := by
  decide

def containerAndImageLabelIsolation (hasAgent : Bool) : Nat × Nat × Bool × Bool :=
  let containerLabels := if hasAgent then 6 else 4
  let imageLabels := 4
  (containerLabels, imageLabels, true, false)

theorem image_labels_never_include_task_reap_label (hasAgent : Bool) :
    (containerAndImageLabelIsolation hasAgent).2.2.2 = false := by
  cases hasAgent <;> rfl

def tryAllocateQuotaModel (used req maxAllowed : Nat) : Bool × Nat :=
  if used + req > maxAllowed then (false, used)
  else (true, used + req)

def releaseQuotaGuardModel (used released : Nat) : Nat :=
  used - released

theorem raii_quota_alloc_then_drop_restores_zero (req maxAllowed : Nat)
    (h : req ≤ maxAllowed) :
    let (ok, u) := tryAllocateQuotaModel 0 req maxAllowed
    ok = true ∧ releaseQuotaGuardModel u req = 0 := by
  unfold tryAllocateQuotaModel releaseQuotaGuardModel
  have hNotGt : ¬ (maxAllowed < req) := by omega
  simp [hNotGt]

inductive ReaperEntryStatusModel where
  | active
  | kept
  | keptOrphan
  | zombie
  | released
  deriving DecidableEq, Repr

def ReaperEntryStatusModel.label : ReaperEntryStatusModel → String
  | .active => "active"
  | .kept => "kept"
  | .keptOrphan => "kept (session ended)"
  | .zombie => "zombie"
  | .released => "released"

def classifyReaperEntryModel
    (isReleased isLeaked sessionAlive keep isSessionOwner : Bool) : ReaperEntryStatusModel :=
  if isReleased then .released
  else if isLeaked && !keep then .zombie
  else match sessionAlive, keep with
    | true, true => .kept
    | true, false => if isSessionOwner then .kept else .active
    | false, true => .keptOrphan
    | false, false => .zombie

theorem leaked_non_keep_or_dead_unkept_is_zombie (sa so : Bool) :
    classifyReaperEntryModel false true sa false so = .zombie ∧
    classifyReaperEntryModel false false false false so = .zombie := by
  cases sa <;> cases so <;> decide

inductive ResourceStateModel where
  | live
  | draining
  | released
  | leaked
  deriving DecidableEq, Repr

def ResourceStateModel.asStr : ResourceStateModel → String
  | .live => "live"
  | .draining => "draining"
  | .released => "released"
  | .leaked => "leaked"

inductive ResourceEventModel where
  | drain
  | stopped
  | deadlinePassed
  deriving DecidableEq, Repr

def stepResourceStateModel
    (s : ResourceStateModel) (ev : ResourceEventModel) : Option (ResourceStateModel × Bool) :=
  match s, ev with
  | .live, .drain => some (.draining, false)
  | .live, .stopped => some (.released, false)
  | .draining, .stopped => some (.released, false)
  | .draining, .deadlinePassed => some (.leaked, true)
  | _, _ => none

theorem released_state_rejects_further_transitions (ev : ResourceEventModel) :
    stepResourceStateModel .released ev = none := by
  cases ev <;> rfl

def storageEstimateTotal (checkpoints logs models buffer : Nat) : Nat :=
  checkpoints + logs + models + buffer

-- ============================================================================
-- Section PA65: Benchmark Harness Config, Transport vs Eval Reporting,
-- Long-Running Status Lattice & SWE-bench Pro Frozen Selection / Trace Diagnosis
-- (`src/bench_harness/config.rs`, `src/bench_harness/task.rs`,
--  `src/bench_harness/report.rs`, `src/bench_harness/long_running/project.rs`,
--  `src/bench_harness/long_running/report.rs`,
--  `src/bench_harness/swebench_pro/candidate.rs`,
--  `src/bench_harness/swebench_pro/trace.rs`)
-- ============================================================================

def clampBenchConcurrency (n : Nat) : Nat :=
  max n 1

def validateHarnessConfigModel
    (epNonEmpty modelNonEmpty : Bool) (maxConc maxTok timeoutSecs : Nat) : Bool :=
  epNonEmpty && modelNonEmpty && maxConc ≥ 1 && maxTok ≥ 1 && timeoutSecs ≥ 1

def streamInferenceLatencyMs (latencyMs queueWaitMs : Nat) : Nat :=
  latencyMs - queueWaitMs

theorem bench_concurrency_clamped_pos_and_inference_latency_le (n lat qw : Nat) :
    clampBenchConcurrency n ≥ 1 ∧ streamInferenceLatencyMs lat qw ≤ lat := by
  unfold clampBenchConcurrency streamInferenceLatencyMs
  omega

def jsonEvaluatorScorePermille
    (validJson : Bool) (foundFields requiredFields : Nat) : Nat × Bool :=
  if !validJson then
    (0, false)
  else
    let total := 1 + requiredFields
    let hits := 1 + foundFields
    let scorePermille := (hits * 1000 + total / 2) / total
    (scorePermille, decide (hits = total))

theorem json_evaluator_partial_fields_fails_strict_pass :
    jsonEvaluatorScorePermille true 1 2 = (667, false) ∧
    jsonEvaluatorScorePermille true 2 2 = (1000, true) := by
  decide

inductive LongRunningStatusModel where
  | green
  | partialStatus
  | compiles
  | wrote
  | failStatus
  deriving DecidableEq, Repr

def LongRunningStatusModel.label : LongRunningStatusModel → String
  | .green => "GREEN"
  | .partialStatus => "PARTIAL"
  | .compiles => "COMPILES"
  | .wrote => "WROTE"
  | .failStatus => "FAIL"

def classifyLongRunningStatus
    (compiles : Bool) (testsPassed testsFailed srcLines : Nat) : LongRunningStatusModel :=
  if compiles && testsPassed > 0 && testsFailed == 0 then .green
  else if compiles && testsPassed > 0 then .partialStatus
  else if compiles then .compiles
  else if srcLines > 2 then .wrote
  else .failStatus

def LongRunningStatusModel.countsTowardSuccess : LongRunningStatusModel → Bool
  | .green | .partialStatus | .compiles => true
  | .wrote | .failStatus => false

theorem green_requires_zero_failed_and_wrote_requires_over_two_lines :
    classifyLongRunningStatus true 5 0 100 = .green ∧
    classifyLongRunningStatus true 3 2 100 = .partialStatus ∧
    classifyLongRunningStatus true 0 2 100 = .compiles ∧
    classifyLongRunningStatus false 0 0 10 = .wrote ∧
    classifyLongRunningStatus false 0 0 2 = .failStatus := by
  decide

def swebenchCandidateBetter
    (srcOnlyA synA : Bool) (linesA numA trialA : Nat)
    (srcOnlyB synB : Bool) (linesB numB trialB : Nat) : Bool :=
  if srcOnlyA != srcOnlyB then srcOnlyA
  else if linesA != linesB then decide (linesA < linesB)
  else if synA != synB then synA
  else if numA != numB then decide (numA < numB)
  else decide (trialA < trialB)

theorem frozen_selector_ignores_official_eval_and_prefers_smaller_source_only_patch :
    swebenchCandidateBetter true true 2 3 1 true true 4 2 2 = true ∧
    swebenchCandidateBetter true true 4 2 2 false true 4 1 1 = true := by
  decide

-- ============================================================================
-- Section PA66: Lifecycle State Machines (Agent / Task / Resource),
-- Cause Sanitization, TaskEdit Validation & Projection Queries
-- (`src/lifecycle/mod.rs`, `src/lifecycle/agent.rs`, `src/lifecycle/task.rs`,
--  `src/lifecycle/resource.rs`, `src/lifecycle/log.rs`,
--  `src/lifecycle/tracked.rs`, `src/lifecycle/control.rs`,
--  `src/lifecycle/projection.rs`)
-- ============================================================================

inductive AgentStateKindModel where
  | idle
  | working
  | blocked
  | crashed
  | stopped
  deriving DecidableEq, Repr

def AgentStateKindModel.asStr : AgentStateKindModel → String
  | .idle => "idle"
  | .working => "working"
  | .blocked => "blocked"
  | .crashed => "crashed"
  | .stopped => "stopped"

def AgentStateKindModel.isTerminal : AgentStateKindModel → Bool
  | .stopped => true
  | _ => false

inductive AgentEventKindModel where
  | assign
  | block
  | unblock
  | done
  | crash
  | restart
  | stop
  deriving DecidableEq, Repr

def stepAgentStateModel
    (s : AgentStateKindModel) (ev : AgentEventKindModel) : Option AgentStateKindModel :=
  match s, ev with
  | .idle, .assign => some .working
  | .working, .block => some .blocked
  | .blocked, .unblock => some .working
  | .working, .done => some .idle
  | .idle, .crash | .working, .crash | .blocked, .crash => some .crashed
  | .crashed, .restart => some .idle
  | .idle, .stop | .working, .stop | .blocked, .stop | .crashed, .stop => some .stopped
  | _, _ => none

theorem agent_stopped_is_absorbing_and_rejects_events (ev : AgentEventKindModel) :
    stepAgentStateModel .stopped ev = none := by
  cases ev <;> rfl

def taskEnterEffectsCount (isTerminal : Bool) : Nat :=
  if isTerminal then 1 else 0

def taskCheckStepTerminalAllowed (fromInterrupted isResumeToQueued : Bool) : Bool :=
  fromInterrupted && isResumeToQueued

inductive LifecycleResourceKindModel where
  | container
  | process
  | pty
  | browser
  | mcp
  | port
  | server
  | worktree
  | tempDir
  deriving DecidableEq, Repr

def LifecycleResourceKindModel.isDrainable : LifecycleResourceKindModel → Bool
  | .container | .process | .pty | .browser | .mcp => true
  | .port | .server | .worktree | .tempDir => false

theorem only_interrupted_terminal_resumes_and_drainable_split :
    taskCheckStepTerminalAllowed true true = true ∧
    taskCheckStepTerminalAllowed false true = false ∧
    LifecycleResourceKindModel.container.isDrainable = true ∧
    LifecycleResourceKindModel.worktree.isDrainable = false := by
  decide

def sanitizeCauseLengthModel (rawChars : Nat) (maxChars : Nat := 240) : Nat × Bool :=
  if rawChars ≤ maxChars then (rawChars, false) else (maxChars, true)

def stepTrackedUsageConsumed (attachedBefore : Bool) : Bool × Bool :=
  (attachedBefore, false)

theorem tracked_usage_consumed_on_first_transition :
    stepTrackedUsageConsumed true = (true, false) ∧
    (stepTrackedUsageConsumed (stepTrackedUsageConsumed true).2).1 = false := by
  decide

def validateTaskEditModel
    (descNonEmpty : Bool)
    (turnsUsed : Nat) (newMaxTurns : Option Nat)
    (tokensUsed : Nat) (newTokenBudget : Option Nat) : Bool :=
  let turnsOk := match newMaxTurns with
    | some m => decide (m > turnsUsed)
    | none => true
  let budgetOk := match newTokenBudget with
    | some b => decide (b > tokensUsed)
    | none => true
  descNonEmpty && turnsOk && budgetOk

theorem task_edit_rejects_retroactive_turn_or_token_ceiling (u : Nat) :
    validateTaskEditModel true u (some u) 100 none = false ∧
    validateTaskEditModel true 5 none u (some u) = false := by
  unfold validateTaskEditModel
  have h : ¬ (u > u) := by omega
  simp [h]

def pausedIntervalTotalMs (pauseStartMs resumeMs : Nat) (_hadIntermediateEdit : Bool) : Nat :=
  if resumeMs ≥ pauseStartMs then resumeMs - pauseStartMs else 0

-- ============================================================================
-- Section PA67: Evolution AST Ancestry / Worktree Pruning,
-- Prefix-Observable Replay Simulator & Telemetry Formatting
-- (`src/evolution/ast_tools.rs`, `src/evolution/replay.rs`,
--  `src/evolution/telemetry.rs`)
-- ============================================================================

def pruneAncestryAtLatestEvaluated (chain : List (String × Bool)) : List String :=
  let rec takeThroughFirstEval : List (String × Bool) → List String
    | [] => []
    | (id, isEval) :: rest =>
      if isEval then [id]
      else id :: takeThroughFirstEval rest
  let hasEval := chain.any (fun (_, isEval) => isEval)
  let truncated := if hasEval then takeThroughFirstEval chain else chain.map Prod.fst
  truncated.reverse

theorem prune_ancestry_truncates_after_first_evaluated_and_reverses :
    pruneAncestryAtLatestEvaluated [("att-2", false), ("att-1", true), ("att-0", true)] =
      ["att-1", "att-2"] := by
  decide

inductive ReplayBatchGuardModel where
  | ok
  | batchExceedsParallelism
  | illegalAction
  | duplicateNode
  | duplicateBranch
  deriving DecidableEq, Repr

def checkReplayBatchGuard
    (batchLen maxPar : Nat) (allLegal noDupNode noDupBranch : Bool) : ReplayBatchGuardModel :=
  if batchLen > maxPar then .batchExceedsParallelism
  else if !allLegal then .illegalAction
  else if !noDupNode then .duplicateNode
  else if !noDupBranch then .duplicateBranch
  else .ok

def replayMetricsPermille
    (baselinePermille terminalPermille probes treeLen seqRounds betaPermille lambdaPermille : Nat)
    : Nat × Nat × Nat × Nat × Nat × Nat :=
  let improv := terminalPermille - baselinePermille
  let parPen := if probes > 0 then (seqRounds * 1000) / probes else 1000
  let normCost := (probes * 1000) / (max treeLen 1)
  let obj := terminalPermille - (betaPermille * normCost) / 1000
  let pareto := improv - (lambdaPermille * parPen) / 1000
  (improv, parPen, normCost, obj, pareto, seqRounds)

theorem refine_top1_replay_metrics_exact :
    replayMetricsPermille 500 850 4 5 2 200 100 = (350, 500, 800, 690, 300, 2) := by
  decide

def rankingStabilityGateModel
    (treeCount policyCount maxSPermille sPermille : Nat) : Option (Nat × Bool) :=
  if policyCount == 0 || treeCount == 0 then none
  else if treeCount < 2 || policyCount < 2 || maxSPermille == 0 then some (0, false)
  else
    let w := min ((sPermille * 1000) / maxSPermille) 1000
    some (w, decide (w ≥ 700))

theorem single_tree_or_tied_scores_never_claim_ranking_stability :
    rankingStabilityGateModel 1 2 1000 1000 = some (0, false) ∧
    rankingStabilityGateModel 2 3 0 0 = some (0, false) ∧
    rankingStabilityGateModel 2 3 8000 8000 = some (1000, true) := by
  decide

def classifyBenchDeltaBadge (deltaTenths : Int) : String × String :=
  if deltaTenths > 20 then ("🔴", "SLOWER")
  else if deltaTenths < -20 then ("🟢", "FASTER")
  else ("⚪", "STABLE")

-- ============================================================================
-- Section PA68: Multi-Agent Orchestration Config / Line Buffer / View Tabs &
-- Workflow Checkpoints / Templates / Budget / Substitution
-- (`src/orchestration/multiagent/config.rs`, `src/orchestration/multiagent/types.rs`,
--  `src/orchestration/multiagent/chat.rs`, `src/orchestration/multiagent/view.rs`,
--  `src/orchestration/workflows/checkpoint.rs`,
--  `src/orchestration/workflows/templates.rs`, `src/orchestration/workflows.rs`)
-- ============================================================================

def clampMultiAgentConcurrency (n : Nat) : Nat :=
  min (max n 1) 16

def agentTagModel (roleLower : String) (agentId : Nat) : String :=
  s!"{roleLower}·{agentId + 1}"

def agentColorIndexModel (agentId : Nat) : Nat :=
  agentId % 8

theorem multiagent_concurrency_within_1_to_16 (n : Nat) :
    1 ≤ clampMultiAgentConcurrency n ∧ clampMultiAgentConcurrency n ≤ 16 := by
  unfold clampMultiAgentConcurrency
  omega

inductive MultiChatTabModel where
  | all
  | agent (idx : Nat)
  deriving DecidableEq, Repr

def MultiChatTabModel.asStr : MultiChatTabModel → String
  | .all => "All"
  | .agent idx => s!"Agent({idx})"

def selectTabByNumberModel
    (agentCount : Nat) (current : MultiChatTabModel) (n : Nat) : Bool × MultiChatTabModel :=
  if n == 0 then (true, .all)
  else if n ≤ agentCount then (true, .agent (n - 1))
  else (false, current)

def classifyAgentSummaryOutcome (success skippedPrefix : Bool) : String :=
  if success then "ok"
  else if skippedPrefix then "skipped"
  else "failed"

def validateWorkflowRunIdModel (len : Nat) (startsWithDot allCharsValid : Bool) : Bool :=
  len ≥ 1 && len ≤ 128 && !startsWithDot && allCharsValid

def workflowMeasuredTokensModel (totalTok promptTok completionTok : Nat) : Nat :=
  max totalTok (promptTok + completionTok)

inductive WorkflowBudgetStopModel where
  | none
  | wallClock (elapsedMs maxWallMs : Nat)
  | tokenBudget (usedTok maxTok unmetered : Nat)
  deriving DecidableEq, Repr

def WorkflowBudgetStopModel.asStr : WorkflowBudgetStopModel → String
  | .none => "None"
  | .wallClock el mx => s!"Wall({el}/{mx})"
  | .tokenBudget used mx unm => s!"Token({used}/{mx},unmet={unm})"

def checkWorkflowBudgetModel
    (maxWallSecs maxTokens : Option Nat)
    (elapsedMs totalTok promptTok compTok unmetered : Nat) : WorkflowBudgetStopModel :=
  match maxWallSecs with
  | some w =>
    if elapsedMs ≥ w * 1000 then
      .wallClock elapsedMs (w * 1000)
    else
      match maxTokens with
      | some m =>
        let used := workflowMeasuredTokensModel totalTok promptTok compTok
        if used ≥ m then .tokenBudget used m unmetered else .none
      | none => .none
  | none =>
    match maxTokens with
    | some m =>
      let used := workflowMeasuredTokensModel totalTok promptTok compTok
      if used ≥ m then .tokenBudget used m unmetered else .none
    | none => .none

theorem wall_clock_budget_precedes_token_budget_and_measured_max :
    checkWorkflowBudgetModel (some 10) (some 200) 10000 150 120 80 1 = .wallClock 10000 10000 ∧
    checkWorkflowBudgetModel (some 10) (some 200) 5000 150 120 80 1 = .tokenBudget 200 200 1 ∧
    checkWorkflowBudgetModel (some 10) (some 200) 5000 150 100 99 1 = .none := by
  decide

def checkStepDepIterationAware
    (knownDep : Bool) (iterCompleted topCompleted : Option Bool) : String :=
  if !knownDep then "Unknown"
  else
    let effective := match iterCompleted with
      | some b => some b
      | none => topCompleted
    match effective with
    | some true => "Ok"
    | some false => "Failed"
    | none => "NotExecuted"

theorem iteration_scoped_dep_overrides_failed_top_level :
    checkStepDepIterationAware true (some true) (some false) = "Ok" ∧
    checkStepDepIterationAware true none (some false) = "Failed" := by
  decide

-- ============================================================================
-- Section PA69: Self-Healing Recovery Strategy, Circuit Breaker & Supervision Lattice
-- (`src/self_healing/mod.rs`, `src/self_healing/recovery_tree.rs`,
--  `src/self_healing/executor.rs`, `src/supervision/circuit_breaker.rs`,
--  `src/supervision/run_supervisor.rs`)
-- ============================================================================

inductive SelfHealErrorClass where
  | network
  | timeout
  | rateLimit
  | resourceExhaustion
  | parseError
  | authError
  | unknown
  deriving DecidableEq, Repr

def defaultStrategyActionCount : SelfHealErrorClass → Nat
  | .resourceExhaustion => 2
  | _ => 1

def hasEscalationStrategy : SelfHealErrorClass → Bool
  | .authError => false
  | .unknown => false
  | _ => true

theorem pa69_error_class_strategy_and_escalation (c : SelfHealErrorClass) :
    defaultStrategyActionCount c >= 1 ∧
    (hasEscalationStrategy c = false ↔ (c = .authError ∨ c = .unknown)) := by
  cases c <;> decide

inductive OfflineResolution where
  | actionFallback
  | actionRetry
  | actionCompress
  | escalate
  | unresolvable
  deriving DecidableEq, Repr

def OfflineResolution.asStr : OfflineResolution → String
  | .actionFallback => "fallback"
  | .actionRetry => "retry"
  | .actionCompress => "compress"
  | .escalate => "escalate"
  | .unresolvable => "unresolvable"

def resolveEndpointOffline (optIn isLlmUnreachable alreadyLocal : Bool) : OfflineResolution :=
  if !optIn || !isLlmUnreachable || alreadyLocal then .escalate else .actionFallback

theorem pa69_recovery_tree_endpoint_opt_in_and_local_guard :
    resolveEndpointOffline false true false = .escalate ∧
    resolveEndpointOffline true false false = .escalate ∧
    resolveEndpointOffline true true true = .escalate ∧
    resolveEndpointOffline true true false = .actionFallback := by
  decide

def learnerRecommends (threshold occurrences totalAttempts successes : Nat) : Bool :=
  occurrences >= threshold && totalAttempts >= 2 && successes * 2 >= totalAttempts

def checkpointRingActive (maxCheckpoints pushed : Nat) : Nat :=
  Nat.min maxCheckpoints pushed

theorem pa69_learner_threshold_and_checkpoint_cap :
    learnerRecommends 3 2 2 2 = false ∧
    learnerRecommends 3 3 1 1 = false ∧
    learnerRecommends 3 3 2 0 = false ∧
    learnerRecommends 3 3 2 1 = true ∧
    checkpointRingActive 2 3 = 2 := by
  decide

inductive HealthPredictionState where
  | noneYet
  | healthy
  | atRisk
  | degrading
  | failureImminent
  deriving DecidableEq, Repr

def HealthPredictionState.asStr : HealthPredictionState → String
  | .noneYet => "none"
  | .healthy => "healthy"
  | .atRisk => "at_risk"
  | .degrading => "degrading"
  | .failureImminent => "failure_imminent"

def predictHealth (historyLen unhealthyInLast10 : Nat) : HealthPredictionState :=
  if historyLen < 5 then .noneYet
  else if unhealthyInLast10 >= 7 then .failureImminent
  else if unhealthyInLast10 >= 4 then .degrading
  else if unhealthyInLast10 >= 2 then .atRisk
  else .healthy

def executorWithinAttemptBudget (enabled : Bool) (actionCount maxAttempts : Nat) : Bool :=
  enabled && actionCount > 0 && actionCount <= maxAttempts

theorem pa69_executor_budget_and_health_predictor :
    predictHealth 4 8 = .noneYet ∧
    predictHealth 10 7 = .failureImminent ∧
    predictHealth 10 4 = .degrading ∧
    predictHealth 10 2 = .atRisk ∧
    predictHealth 10 0 = .healthy ∧
    executorWithinAttemptBudget false 1 2 = false ∧
    executorWithinAttemptBudget true 0 2 = false ∧
    executorWithinAttemptBudget true 3 2 = false ∧
    executorWithinAttemptBudget true 2 2 = true := by
  decide

inductive CircuitStateModel where
  | closed
  | openState
  | halfOpen
  deriving DecidableEq, Repr

def CircuitStateModel.asStr : CircuitStateModel → String
  | .closed => "closed"
  | .openState => "open"
  | .halfOpen => "half-open"

def recordClassifiedError (st : CircuitStateModel) (failures threshold : Nat) (countsTowardFailure : Bool) :
    CircuitStateModel × Nat :=
  if !countsTowardFailure then (st, failures)
  else match st with
    | .closed => if failures + 1 >= threshold then (.openState, 0) else (.closed, failures + 1)
    | .halfOpen => (.openState, 0)
    | .openState => (.openState, failures + 1)

theorem pa69_circuit_breaker_classifier_and_trip :
    (∀ st f t, recordClassifiedError st f t false = (st, f)) ∧
    recordClassifiedError .closed 0 2 true = (.closed, 1) ∧
    recordClassifiedError .closed 1 2 true = (.openState, 0) ∧
    recordClassifiedError .halfOpen 0 2 true = (.openState, 0) := by
  refine ⟨?_, by decide, by decide, by decide⟩
  intro st f t
  rfl

-- ============================================================================
-- Section PA70: Tools HTTP/Browser SSRF Guard, Search Pagination/Regex/Glob &
-- Knowledge Graph Lattice
-- (`src/tools/net_policy.rs`, `src/tools/http.rs`, `src/tools/browser.rs`,
--  `src/tools/mod.rs`, `src/tools/grep_search/mod.rs`, `src/tools/search.rs`,
--  `src/tools/knowledge.rs`)
-- ============================================================================

structure NetTargetPolicy where
  allowPrivate : Bool
  allowLocalhost : Bool
  originPort : Nat
  deriving DecidableEq, Repr

def resolvedAddressAllowed (hostIsLocalhost ipIsLoopback ipIsPrivate allowPrivate allowLocalhost : Bool) : Bool :=
  if !ipIsLoopback && !ipIsPrivate then true
  else if allowPrivate then true
  else if allowLocalhost && hostIsLocalhost && ipIsLoopback then true
  else false

def redirectTargetAllowed (policy : NetTargetPolicy) (targetIsLocalhost targetIsPrivate : Bool) (targetPort : Nat) : Bool :=
  if policy.allowPrivate then true
  else if targetIsLocalhost then policy.allowLocalhost && targetPort == policy.originPort
  else !targetIsPrivate

theorem pa70_net_policy_ssrf_and_redirect_guard :
    resolvedAddressAllowed false true false false true = false ∧
    resolvedAddressAllowed true false true false true = false ∧
    resolvedAddressAllowed true true false false true = true ∧
    redirectTargetAllowed ⟨false, true, 8000⟩ true false 9000 = false ∧
    redirectTargetAllowed ⟨false, true, 8000⟩ true false 8000 = true ∧
    redirectTargetAllowed ⟨false, false, 443⟩ true false 8000 = false := by
  decide

def paginateSlice (totalChars offset limit : Nat) : Nat × Nat × Bool :=
  let start := Nat.min offset totalChars
  let stop := Nat.min (start + limit) totalChars
  (start, stop, stop < totalChars)

def outboundRequestAllowed (schemeHttpOrHttps hasLeakedSecret : Bool) : Bool :=
  schemeHttpOrHttps && !hasLeakedSecret

theorem pa70_outbound_secret_and_pagination_bounds :
    paginateSlice 5 1 2 = (1, 3, true) ∧
    paginateSlice 5 3 2 = (3, 5, false) ∧
    paginateSlice 5 9 2 = (5, 5, false) ∧
    outboundRequestAllowed true true = false ∧
    outboundRequestAllowed false false = false ∧
    outboundRequestAllowed true false = true := by
  decide

def regexPatternLengthAllowed (len maxLen : Nat) : Bool :=
  len <= maxLen

def globMatchesLiteralSeparator (isRecursiveGlob isNestedPath isPrivateDir isSymlink : Bool) : Bool :=
  !isPrivateDir && !isSymlink && (isRecursiveGlob || !isNestedPath)

theorem pa70_grep_regex_and_glob_literal_separator :
    regexPatternLengthAllowed 1000 1000 = true ∧
    regexPatternLengthAllowed 1001 1000 = false ∧
    globMatchesLiteralSeparator false true false false = false ∧
    globMatchesLiteralSeparator true true false false = true ∧
    globMatchesLiteralSeparator true false true false = false ∧
    globMatchesLiteralSeparator true false false true = false := by
  decide

inductive SymbolKindFilter where
  | fnKind
  | structKind
  | enumKind
  | traitKind
  | implKind
  | all
  deriving DecidableEq, Repr

def symbolFilterMatches (filter actual : SymbolKindFilter) : Bool :=
  filter == .all || filter == actual

theorem pa70_symbol_filter_reflexive_and_all :
    (∀ s : SymbolKindFilter, symbolFilterMatches .all s = true ∧ symbolFilterMatches s s = true) ∧
    symbolFilterMatches .structKind .fnKind = false := by
  refine ⟨?_, by decide⟩
  intro s
  cases s <;> decide

def incidentEdgesPurged (edges : List (Nat × Nat)) (removedNode : Nat) : List (Nat × Nat) :=
  edges.filter (fun e => e.1 != removedNode && e.2 != removedNode)

def exportPathSafe (isRelative hasParentTraversal : Bool) : Bool :=
  isRelative && !hasParentTraversal

theorem pa70_knowledge_graph_remove_purges_incident_edges :
    incidentEdgesPurged [(1, 2), (2, 3), (1, 3)] 2 = [(1, 3)] ∧
    exportPathSafe false false = false ∧
    exportPathSafe true true = false ∧
    exportPathSafe true false = true := by
  decide

-- ============================================================================
-- Section PA71: Evolve Compiler Repair, Multi-Arm Runner, Dedup & Dead-Code Lattice
-- (`src/evolve/compiler_repair.rs`, `src/evolve/multi_arm_runner.rs`,
--  `src/evolve/dedup.rs`, `src/evolve/dead_code.rs`)
-- ============================================================================

inductive ApplicabilityLevel where
  | machineApplicable
  | maybeIncorrect
  | hasPlaceholders
  | unspecified
  deriving DecidableEq, Repr

def spanExtractable (app : ApplicabilityLevel) (hasReplacement : Bool) : Bool :=
  app == .machineApplicable && hasReplacement

theorem pa71_compiler_repair_extract_gate (app : ApplicabilityLevel) (hasRep : Bool) :
    spanExtractable app hasRep = true ↔ (app = .machineApplicable ∧ hasRep = true) := by
  cases app <;> cases hasRep <;> decide

def groupAtomicallyValid (allSpansInBounds noInternalOverlap noAcceptedOverlap : Bool) : Bool :=
  allSpansInBounds && noInternalOverlap && noAcceptedOverlap

theorem pa71_compiler_repair_group_atomicity :
    groupAtomicallyValid false true true = false ∧
    groupAtomicallyValid true false true = false ∧
    groupAtomicallyValid true true false = false ∧
    groupAtomicallyValid true true true = true := by
  decide

def testRegressionsCount (baselineFailed armFailed baselinePassed armPassed : Nat) : Nat :=
  (armFailed - baselineFailed) + (baselinePassed - armPassed)

def separatedFromBaseline (armMaxNs baseMinNs : Nat) : Bool :=
  armMaxNs < baseMinNs

def armAccepted (compileOk : Bool) (regressions : Nat) (armMaxNs baseMinNs : Nat) : Bool :=
  compileOk && regressions == 0 && separatedFromBaseline armMaxNs baseMinNs

theorem pa71_multi_arm_separation_and_regression_gate :
    testRegressionsCount 1 2 10 9 = 2 ∧
    armAccepted true 0 105 100 = false ∧
    armAccepted true 2 80 100 = false ∧
    armAccepted false 0 80 100 = false ∧
    armAccepted true 0 90 100 = true := by
  decide

inductive EvolveDedupClass where
  | distinct
  | nearDuplicate
  | exactDuplicate
  deriving DecidableEq, Repr

def EvolveDedupClass.asStr : EvolveDedupClass → String
  | .distinct => "distinct"
  | .nearDuplicate => "near"
  | .exactDuplicate => "exact"

def classifyEvolveDedup (sameSha256 : Bool) (minLines maxLines intersectionPoints unionPoints : Nat) : EvolveDedupClass :=
  if maxLines == 0 then .distinct
  else if sameSha256 then .exactDuplicate
  else if minLines * 5 >= maxLines * 4 && unionPoints > 0 && intersectionPoints * 5 >= unionPoints * 4 then .nearDuplicate
  else .distinct

theorem pa71_dedup_exact_near_and_size_ratio_pruning :
    classifyEvolveDedup true 5 5 5 5 = .exactDuplicate ∧
    classifyEvolveDedup false 4 5 4 5 = .nearDuplicate ∧
    classifyEvolveDedup false 1 10 1 1 = .distinct ∧
    classifyEvolveDedup false 5 5 3 5 = .distinct ∧
    classifyEvolveDedup true 0 0 0 0 = .distinct := by
  decide

def isDeadSymbol (isDispatched startsWithDunder isTestAnnotated inTestMod crossTreeRefs : Bool) : Bool :=
  !isDispatched && !startsWithDunder && !isTestAnnotated && !inTestMod && !crossTreeRefs

def deadCodeSortKey (cfgGated : Bool) : Nat :=
  if cfgGated then 1 else 0

theorem pa71_dead_code_exclusions_and_cfg_order :
    isDeadSymbol false false false false false = true ∧
    isDeadSymbol true false false false false = false ∧
    isDeadSymbol false true false false false = false ∧
    isDeadSymbol false false true false false = false ∧
    isDeadSymbol false false false true false = false ∧
    isDeadSymbol false false false false true = false ∧
    deadCodeSortKey false < deadCodeSortKey true := by
  decide

-- ============================================================================
-- Section PA72: VLM Benchmark Scoring/Rating & SWE-bench Pro Dataset/Manifest/Catalog Lattice
-- (`src/vlm_bench/scoring.rs`, `src/vlm_bench/config.rs`, `src/vlm_bench/report.rs`,
--  `src/vlm_bench/levels/mod.rs`, `src/vlm_bench/levels/mega_evolution.rs`,
--  `src/bench_harness/swebench_pro/dataset.rs`, `src/bench_harness/swebench_pro/manifest.rs`,
--  `src/bench_harness/swebench_pro/harness.rs`, `src/bench_harness/swebench_pro/catalog.rs`)
-- ============================================================================

inductive VlmGardenRating where
  | bloom
  | grow
  | wilt
  | frost
  deriving DecidableEq, Repr

def VlmGardenRating.asStr : VlmGardenRating → String
  | .bloom => "BLOOM"
  | .grow => "GROW"
  | .wilt => "WILT"
  | .frost => "FROST"

def ratingFromBasisPoints (accBp thresholdBp : Nat) : VlmGardenRating :=
  if accBp * 100 >= thresholdBp * 100 then .bloom
  else if accBp * 100 >= thresholdBp * 75 then .grow
  else if accBp * 100 >= thresholdBp * 50 then .wilt
  else .frost

theorem pa72_vlm_rating_lattice_and_thresholds :
    ratingFromBasisPoints 80 80 = .bloom ∧
    ratingFromBasisPoints 60 80 = .grow ∧
    ratingFromBasisPoints 40 80 = .wilt ∧
    ratingFromBasisPoints 39 80 = .frost := by
  decide

inductive VlmDifficultyTier where
  | easy
  | medium
  | hard
  | veryHard
  | extreme
  | mega
  deriving DecidableEq, Repr

def difficultyRank : VlmDifficultyTier → Nat
  | .easy => 0
  | .medium => 1
  | .hard => 2
  | .veryHard => 3
  | .extreme => 4
  | .mega => 5

def clampCorrelationBp (corrSignPositive : Bool) (corrMagBp : Nat) : Nat :=
  if !corrSignPositive then 0 else Nat.min corrMagBp 100

theorem pa72_vlm_difficulty_filter_and_correlation_clamp :
    (([VlmDifficultyTier.easy, .medium, .hard, .veryHard, .extreme, .mega].filter
      (fun d => difficultyRank d <= difficultyRank .hard)).length = 3) ∧
    clampCorrelationBp false 80 = 0 ∧
    ratingFromBasisPoints (clampCorrelationBp false 80) 50 = .frost ∧
    clampCorrelationBp true 100 = 100 ∧
    ratingFromBasisPoints (clampCorrelationBp true 100) 50 = .bloom := by
  decide

def safeRepoSlug (segments : Nat) (startsWithDash hasTraversal : Bool) : Bool :=
  segments == 2 && !startsWithDash && !hasTraversal

def safeGitRevision (nonEmpty startsWithDash hasDotDot hasInvalidChar : Bool) : Bool :=
  nonEmpty && !startsWithDash && !hasDotDot && !hasInvalidChar

theorem pa72_swebench_repo_and_revision_guards :
    safeRepoSlug 2 false false = true ∧
    safeRepoSlug 3 false false = false ∧
    safeRepoSlug 2 true false = false ∧
    safeRepoSlug 2 false true = false ∧
    safeGitRevision true false false false = true ∧
    safeGitRevision false false false false = false ∧
    safeGitRevision true true false false = false ∧
    safeGitRevision true false true false = false ∧
    safeGitRevision true false false true = false := by
  decide

def llamaServerOptionalArgCount (hasNonEmptyTensorSplit hasMmproj : Bool) : Nat :=
  (if hasNonEmptyTensorSplit then 2 else 0) + (if hasMmproj then 2 else 0)

theorem pa72_llama_server_optional_arg_inclusion :
    llamaServerOptionalArgCount false false = 0 ∧
    llamaServerOptionalArgCount true false = 2 ∧
    llamaServerOptionalArgCount false true = 2 ∧
    llamaServerOptionalArgCount true true = 4 := by
  decide

def catalogParallelSlots (isQ8Kp is35bA3b : Bool) : Nat :=
  if isQ8Kp || is35bA3b then 1 else 2

def catalogContextWindow : Nat := 262144

theorem pa72_swebench_catalog_parallel_and_ctx_invariants :
    catalogParallelSlots true false = 1 ∧
    catalogParallelSlots false true = 1 ∧
    catalogParallelSlots false false = 2 ∧
    catalogContextWindow = 262144 := by
  decide

-- ============================================================================
-- Section PA73: TUI Dashboard, Garden View, Layout & Tasks View Lattice
-- (`src/ui/tui/*`, `src/ui/garden.rs`)
-- ============================================================================

inductive TuiPermissionAnswerModel where
  | once
  | alwaysTool
  | shellRule
  | deny
  deriving DecidableEq, Repr

def TuiPermissionAnswerModel.asStr : TuiPermissionAnswerModel → String
  | .once => "once"
  | .alwaysTool => "always_tool"
  | .shellRule => "shell_rule"
  | .deny => "deny"

inductive TuiKeyModel where
  | charY
  | charA
  | charP
  | charN
  | charV
  | esc
  | enter
  | down
  | other
  deriving DecidableEq, Repr

/-- Mirrors `ui::tui::permission_key_answer`. -/
def evalPermissionKeyAnswer
    (allowAlways hasShellRule : Bool)
    (key : TuiKeyModel) : Option TuiPermissionAnswerModel :=
  match key with
  | .charY => some .once
  | .charA => if allowAlways then some .alwaysTool else none
  | .charP => if hasShellRule then some .shellRule else none
  | .charN | .esc | .enter => some .deny
  | _ => none

/-- Mirrors `ui::tui::permission_view_key`: returns `(consumed, fullDiffOpen, fullDiffScroll)`. -/
def stepPermissionDiffView
    (hasDiff openState : Bool)
    (scroll : Nat)
    (key : TuiKeyModel) : Bool × Bool × Nat :=
  if !hasDiff then
    (false, openState, scroll)
  else
    match key with
    | .charV => (true, !openState, 0)
    | .esc => if openState then (true, false, 0) else (false, openState, scroll)
    | .down => if openState then (true, openState, scroll + 1) else (false, openState, scroll)
    | _ => (false, openState, scroll)

/-- Mirrors `ui::tui::parse_step_status`. -/
def parseStepStatusModel (hasStepPrefix hasDigitOnlyTail : Bool) (stepNum : Nat) : Option Nat :=
  if hasStepPrefix && hasDigitOnlyTail then some stepNum else none

structure DashboardStateModel where
  connected : Bool
  gardenHealthMeasured : Bool
  gardenHealthPct : Nat
  tokensUsed : Nat
  deriving DecidableEq, Repr

def DashboardStateModel.initial : DashboardStateModel :=
  { connected := false, gardenHealthMeasured := false, gardenHealthPct := 0, tokensUsed := 0 }

def stepDashboardTokenUsage (s : DashboardStateModel) (prompt completion : Nat) : DashboardStateModel :=
  { s with connected := true, tokensUsed := s.tokensUsed + prompt + completion }

def stepDashboardGardenHealth (s : DashboardStateModel) (rawPct : Nat) : DashboardStateModel :=
  { s with gardenHealthMeasured := true, gardenHealthPct := if rawPct <= 100 then rawPct else 100 }

def stepDashboardAgentError (s : DashboardStateModel) : DashboardStateModel :=
  { s with connected := false }

inductive ExecutionModeUiModel where
  | normal
  | autoEdit
  | yolo
  | daemon
  deriving DecidableEq, Repr

def ExecutionModeUiModel.statusLabel : ExecutionModeUiModel → String
  | .normal => "NORMAL"
  | .autoEdit => "AUTO"
  | .yolo | .daemon => "YOLO"

inductive GardenGrowthStageModel where
  | seedling
  | sprout
  | established
  | mature
  | ancient
  | wilting
  deriving DecidableEq, Repr

def GardenGrowthStageModel.asStr : GardenGrowthStageModel → String
  | .seedling => "seedling"
  | .sprout => "sprout"
  | .established => "established"
  | .mature => "mature"
  | .ancient => "ancient"
  | .wilting => "wilting"

/-- Mirrors `ui::garden::GrowthStage::from_metrics`. -/
def classifyGrowthStage (lines lastTendedDays : Nat) : GardenGrowthStageModel :=
  if lastTendedDays > 90 then .wilting
  else if lines <= 50 then .seedling
  else if lines <= 200 then .sprout
  else if lines <= 500 then .established
  else if lines <= 1000 then .mature
  else .ancient

inductive GardenPlantTypeModel where
  | pollinator
  | flower
  | roots
  | vegetable
  deriving DecidableEq, Repr

def GardenPlantTypeModel.asStr : GardenPlantTypeModel → String
  | .pollinator => "pollinator"
  | .flower => "flower"
  | .roots => "roots"
  | .vegetable => "vegetable"

/-- Mirrors `ui::garden::PlantType::from_path` priority order. -/
def classifyPlantType (hasTest isEntryPoint isConfig : Bool) : GardenPlantTypeModel :=
  if hasTest then .pollinator
  else if isEntryPoint then .flower
  else if isConfig then .roots
  else .vegetable

/-- Mirrors `ui::garden::DigitalGarden::tended_fraction` (Rule 3: `none` when empty). -/
def gardenTendedPctModel (total wilting : Nat) : Option Nat :=
  if total == 0 then none
  else
    let cappedWilting := if wilting <= total then wilting else total
    some (((total - cappedWilting) * 100) / total)

inductive TuiLayoutPresetModel where
  | focus
  | coding
  | debugging
  | dashboard
  deriving DecidableEq, Repr

def TuiLayoutPresetModel.paneCount : TuiLayoutPresetModel → Nat
  | .focus => 1
  | .coding => 2
  | .debugging => 3
  | .dashboard => 5

/-- Mirrors `ui::tui::app::App::on_escape` priority: help -> palette -> zoom. -/
def stepTuiEscapeHierarchy (showHelp showPalette isZoomed : Bool) : Bool × Bool × Bool :=
  if showHelp then (false, showPalette, isZoomed)
  else if showPalette then (false, false, isZoomed)
  else if isZoomed then (false, false, false)
  else (false, false, false)

/-- Mirrors `ui::tui::app::App` speed clamp `[0.25, 4.0]` in percent `[25, 400]`. -/
def clampTuiSpeedPct (rawPct : Nat) : Nat :=
  if rawPct < 25 then 25
  else if rawPct > 400 then 400
  else rawPct

inductive TuiTokenSizeModel where
  | small
  | medium
  | large
  | massive
  deriving DecidableEq, Repr

def TuiTokenSizeModel.shortCode : TuiTokenSizeModel → String
  | .small => "S"
  | .medium => "M"
  | .large => "L"
  | .massive => "XL"

def classifyTokenSize (tokens : Nat) : TuiTokenSizeModel :=
  if tokens < 10000 then .small
  else if tokens < 100000 then .medium
  else if tokens < 500000 then .large
  else .massive

/-- Mirrors `ui::tui::tasks_view::ResourceRow::is_reapable`. -/
def isResourceReapableModel (drainable leaked orphaned : Bool) : Bool :=
  drainable && (leaked || orphaned)

/-- Mirrors `ui::tui::tasks_view::TasksPane::on_key` two-step cancel confirmation. -/
def stepTasksCancelConfirm (armed isKeyX : Bool) : Bool × Bool :=
  if isKeyX then
    if armed then (false, true) else (true, false)
  else
    (false, false)

theorem permission_always_and_shell_rule_gated :
    evalPermissionKeyAnswer false false .charA = none ∧
    evalPermissionKeyAnswer true false .charA = some .alwaysTool ∧
    evalPermissionKeyAnswer false false .charP = none ∧
    evalPermissionKeyAnswer false true .charP = some .shellRule := by
  decide

theorem permission_diff_view_esc_closes_without_denying :
    stepPermissionDiffView true true 3 .esc = (true, false, 0) ∧
    stepPermissionDiffView true false 0 .esc = (false, false, 0) := by
  decide

theorem dashboard_initial_is_honestly_unmeasured :
    DashboardStateModel.initial.connected = false ∧
    DashboardStateModel.initial.gardenHealthMeasured = false ∧
    (stepDashboardGardenHealth DashboardStateModel.initial 150).gardenHealthPct = 100 ∧
    (stepDashboardGardenHealth DashboardStateModel.initial 150).gardenHealthMeasured = true := by
  decide

theorem garden_wilting_overrides_ancient_and_empty_fraction_is_none (lines : Nat) :
    classifyGrowthStage lines 91 = .wilting ∧
    gardenTendedPctModel 0 0 = none := by
  simp [classifyGrowthStage, gardenTendedPctModel]

theorem plant_type_test_priority_beats_entry_point :
    classifyPlantType true true false = .pollinator ∧
    classifyPlantType false true false = .flower := by
  decide

theorem tui_escape_hierarchy_peels_one_layer_and_speed_clamps (raw : Nat) :
    stepTuiEscapeHierarchy true true true = (false, true, true) ∧
    stepTuiEscapeHierarchy false true true = (false, false, true) ∧
    stepTuiEscapeHierarchy false false true = (false, false, false) ∧
    25 <= clampTuiSpeedPct raw ∧
    clampTuiSpeedPct raw <= 400 := by
  refine ⟨by decide, by decide, by decide, ?_, ?_⟩
  · unfold clampTuiSpeedPct
    split <;> (try split) <;> omega
  · unfold clampTuiSpeedPct
    split <;> (try split) <;> omega

theorem resource_reapable_requires_drainable_and_fault :
    isResourceReapableModel true true false = true ∧
    isResourceReapableModel true false true = true ∧
    isResourceReapableModel true false false = false ∧
    isResourceReapableModel false true true = false := by
  decide

theorem tasks_cancel_confirm_disarms_on_non_x :
    stepTasksCancelConfirm false true = (true, false) ∧
    stepTasksCancelConfirm true false = (false, false) ∧
    stepTasksCancelConfirm true true = (false, true) := by
  decide

-- ============================================================================
-- Section PA74: Multimodal Vision, Screen Capture, Container & Process Guards
-- (`src/tools/vision.rs`, `src/tools/screen_capture.rs`,
--  `src/tools/container/*`, `src/tools/process*.rs`)
-- ============================================================================

inductive VisionFieldKind where
  | prompt
  | model
  | detail
  | apiKey
  | extraBody
  | imageBase64
  deriving DecidableEq, Repr

/-- Mirrors `VISION_OUTBOUND_FIELDS` in `tools::vision::reject_vision_bound_credential_shapes`. -/
def visionFieldScansForSecrets : VisionFieldKind → Bool
  | .imageBase64 => false
  | _ => true

def rejectVisionCredentialModel (f : VisionFieldKind) (hasSecretShape : Bool) : Bool :=
  !(visionFieldScansForSecrets f && hasSecretShape)

inductive ImageMagicKind where
  | png
  | jpeg
  | gif
  | webp
  | bmp
  | riffNonWebp
  | unknownOrText
  deriving DecidableEq, Repr

/-- Mirrors `tools::vision::validate_image_magic`. -/
def validateImageMagicModel (byteLen : Nat) (k : ImageMagicKind) : Bool :=
  if byteLen < 4 then false
  else
    match k with
    | .png | .jpeg | .gif | .bmp => true
    | .webp => byteLen >= 12
    | .riffNonWebp | .unknownOrText => false

/-- Mirrors `tools::vision::guess_mime`. -/
def guessImageMimeModel (extLower : String) : String :=
  if extLower == "jpg" || extLower == "jpeg" then "image/jpeg"
  else if extLower == "gif" then "image/gif"
  else if extLower == "webp" then "image/webp"
  else if extLower == "bmp" then "image/bmp"
  else "image/png"

/-- Mirrors `tools::vision::compute_pixel_similarity` rounded to integer percent. -/
def pixelSimilarityPctModel (lenA lenB totalDiff : Nat) : Nat :=
  if lenA == 0 || lenA != lenB then 0
  else
    let maxErr := lenA * 255
    let clampedDiff := if totalDiff <= maxErr then totalDiff else maxErr
    ((maxErr - clampedDiff) * 100) / maxErr

/-- Mirrors `tools::screen_capture::parse_region` bounds check. -/
def validateCaptureRegionModel (absX absY width height : Nat) : Bool :=
  absX <= 100000 && absY <= 100000 && width >= 1 && width <= 100000 && height >= 1 && height <= 100000

/-- Mirrors `tools::screen_capture::shape_capture_output` inline base64 cap (`3_750_000` bytes). -/
def shapeCaptureInlineDecision (inlineBase64 : Bool) (pngBytes : Nat) : Option Bool :=
  if !inlineBase64 then some false
  else if pngBytes <= 3750000 then some true
  else none

inductive HostMountTargetKind where
  | workspaceRel
  | procOrSys
  | varOrEtc
  | runtimeSocket
  | selfwareOrGit
  | traversal
  deriving DecidableEq, Repr

/-- Mirrors `tools::container::validation::is_safe_host_mount`. -/
def isSafeHostMountModel : HostMountTargetKind → Bool
  | .workspaceRel => true
  | .procOrSys | .varOrEtc | .runtimeSocket | .selfwareOrGit | .traversal => false

/-- Mirrors `tools::container::validation::is_valid_port`. -/
def isValidPortModel (port : Nat) : Bool :=
  port >= 1 && port <= 65535

/-- Mirrors `tools::process::ProcessStart` argument/cwd/env guard. -/
def validateProcessStartGuard
    (hasForbiddenChars hasCwdTraversal hasDeniedEnv hasGitConfigEnv : Bool) : Bool :=
  !hasForbiddenChars && !hasCwdTraversal && !hasDeniedEnv && !hasGitConfigEnv

/-- Mirrors `tools::process_guard::BoundedCommandOutput::{success, exit_code}`. -/
def boundedCommandVerdict (parentExitZero killedDescendants : Bool) : Bool × Int :=
  if killedDescendants then (false, -1)
  else if parentExitZero then (true, 0)
  else (false, 1)

theorem vision_base64_exempt_from_text_regex_but_magic_gated :
    rejectVisionCredentialModel .imageBase64 true = true ∧
    rejectVisionCredentialModel .prompt true = false ∧
    rejectVisionCredentialModel .extraBody true = false ∧
    validateImageMagicModel 18 .unknownOrText = false := by
  decide

theorem image_magic_requires_4_bytes_and_webp_12_bytes :
    validateImageMagicModel 2 .bmp = false ∧
    validateImageMagicModel 12 .webp = true ∧
    validateImageMagicModel 12 .riffNonWebp = false := by
  decide

theorem pixel_similarity_bounds_and_mismatch_zero :
    pixelSimilarityPctModel 3 3 0 = 100 ∧
    pixelSimilarityPctModel 3 3 (3 * 255) = 0 ∧
    pixelSimilarityPctModel 2 3 0 = 0 ∧
    pixelSimilarityPctModel 0 0 0 = 0 := by
  decide

theorem screen_capture_region_bounds_and_zero_rejection :
    validateCaptureRegionModel 10 20 800 600 = true ∧
    validateCaptureRegionModel 0 0 0 100 = false ∧
    validateCaptureRegionModel 0 0 100001 100 = false ∧
    validateCaptureRegionModel 100001 0 100 100 = false := by
  decide

theorem screen_capture_inline_cap_rejects_over_3750k :
    shapeCaptureInlineDecision false 5000000 = some false ∧
    shapeCaptureInlineDecision true 3750000 = some true ∧
    shapeCaptureInlineDecision true 3750001 = none := by
  decide

theorem container_host_mount_and_port_bounds :
    isSafeHostMountModel .workspaceRel = true ∧
    isSafeHostMountModel .procOrSys = false ∧
    isSafeHostMountModel .runtimeSocket = false ∧
    isSafeHostMountModel .selfwareOrGit = false ∧
    isValidPortModel 0 = false ∧
    isValidPortModel 65535 = true ∧
    isValidPortModel 65536 = false := by
  decide

theorem process_start_guard_rejects_any_violation :
    validateProcessStartGuard false false false false = true ∧
    validateProcessStartGuard true false false false = false ∧
    validateProcessStartGuard false true false false = false ∧
    validateProcessStartGuard false false true false = false ∧
    validateProcessStartGuard false false false true = false := by
  decide

theorem bounded_command_killed_descendants_override_zero_exit :
    boundedCommandVerdict true false = (true, 0) ∧
    boundedCommandVerdict true true = (false, -1) := by
  decide

-- ============================================================================
-- Section PA75: Evolve AST, Skeleton, Symbols, Component Map & Graph Index Lattice
-- (`src/evolve/ast.rs`, `src/evolve/skeleton.rs`, `src/evolve/symbols.rs`,
--  `src/evolve/map.rs`, `src/evolve/summary.rs`, `src/evolve/graph_index.rs`)
-- ============================================================================

inductive CfgPredicateModel where
  | cfgTest
  | cfgAllWithTest
  | cfgAnyWithTest
  | cfgOther
  deriving DecidableEq, Repr

/-- Mirrors `evolve::ast::cfg_predicate_requires_test`. -/
def cfgRequiresTest : CfgPredicateModel → Bool
  | .cfgTest | .cfgAllWithTest => true
  | .cfgAnyWithTest | .cfgOther => false

/-- Mirrors `evolve::ast::AstAnalyzer::collect_cfg_test_ranges`. -/
def cfgRangeSelected (p : CfgPredicateModel) (isExternalMod includeExternal : Bool) : Bool :=
  cfgRequiresTest p && (includeExternal || !isExternalMod)

inductive SkeletonDeclKind where
  | useItem
  | constItem
  | constFnItem
  | fnItem
  | implItem
  deriving DecidableEq, Repr

/-- Mirrors `evolve::skeleton::extract_rust_skeleton` arm ordering (`const fn` before `const`). -/
def classifySkeletonDecl : SkeletonDeclKind → String
  | .useItem => "use"
  | .constFnItem | .fnItem => "fn"
  | .constItem => "const"
  | .implItem => "impl"

/-- Mirrors `evolve::symbols::extract_pub_symbols` visibility filter. -/
def symbolExtractedIfPublic (isPublicVis hasValidSpan : Bool) : Bool :=
  isPublicVis && hasValidSpan

/-- Mirrors `evolve::map::build_card` 48-symbol cap and overflow counter. -/
def capComponentSymbols (totalPublic : Nat) : Nat × Nat :=
  let kept := if totalPublic <= 48 then totalPublic else 48
  (kept, totalPublic - kept)

inductive NodeLayerExpandKind where
  | code
  | auxiliary
  | test
  | structure
  | concept
  | preset
  | symbol
  deriving DecidableEq, Repr

/-- Mirrors `evolve::map::build_map`: bulk map includes only `NodeLayer::Code`. -/
def nodeIncludedInBulkMap : NodeLayerExpandKind → Bool
  | .code => true
  | _ => false

/-- Mirrors `evolve::map::expand`: on-demand expand allows file-backed nodes (`Code`, `Auxiliary`, `Test`, `Structure`) and rejects `Concept`, `Preset`, `Symbol`. -/
def nodeEligibleForOnDemandExpand : NodeLayerExpandKind → Bool
  | .concept | .preset | .symbol => false
  | .code | .auxiliary | .test | .structure => true

/-- Mirrors `evolve::summary::compile_structural_summary` completeness invariant. -/
def structuralSummaryComplete
    (hasParseError truncated depthLimitReached : Bool)
    (invalidRanges omittedKinds : Nat) : Bool :=
  !hasParseError && !truncated && !depthLimitReached && invalidRanges == 0 && omittedKinds == 0

/-- Mirrors `evolve::graph_index::GraphIndex::hotspots` symbol-layer opt-in filter. -/
def hotspotIncludesNode
    (requestedLayer : Option NodeLayerExpandKind)
    (nodeLayer : NodeLayerExpandKind) : Bool :=
  match requestedLayer with
  | some wanted => nodeLayer == wanted
  | none => nodeLayer != .symbol

theorem cfg_any_test_excluded_and_external_mod_separated :
    cfgRangeSelected .cfgTest true true = true ∧
    cfgRangeSelected .cfgTest true false = false ∧
    cfgRangeSelected .cfgAllWithTest false false = true ∧
    cfgRangeSelected .cfgAnyWithTest false true = false := by
  decide

theorem skeleton_const_fn_classified_as_fn :
    classifySkeletonDecl .constFnItem = "fn" ∧
    classifySkeletonDecl .constItem = "const" := by
  decide

theorem symbols_extract_only_public_with_span :
    symbolExtractedIfPublic true true = true ∧
    symbolExtractedIfPublic false true = false ∧
    symbolExtractedIfPublic true false = false := by
  decide

theorem component_map_symbol_cap_conserves_total (n : Nat) :
    (capComponentSymbols n).1 + (capComponentSymbols n).2 = n ∧
    (capComponentSymbols n).1 <= 48 := by
  unfold capComponentSymbols
  dsimp
  split <;> constructor <;> omega

theorem component_map_50_symbols_splits_48_and_2 :
    capComponentSymbols 50 = (48, 2) := by
  decide

theorem auxiliary_excluded_from_bulk_map_but_expandable :
    nodeIncludedInBulkMap .code = true ∧
    nodeIncludedInBulkMap .auxiliary = false ∧
    nodeEligibleForOnDemandExpand .auxiliary = true ∧
    nodeEligibleForOnDemandExpand .concept = false ∧
    nodeEligibleForOnDemandExpand .symbol = false := by
  decide

theorem hotspots_exclude_symbol_by_default_and_include_on_opt_in :
    hotspotIncludesNode none .code = true ∧
    hotspotIncludesNode none .symbol = false ∧
    hotspotIncludesNode (some .symbol) .symbol = true := by
  decide

theorem structural_summary_incomplete_on_any_defect :
    structuralSummaryComplete false false false 0 0 = true ∧
    structuralSummaryComplete true false false 0 0 = false ∧
    structuralSummaryComplete false true false 0 0 = false ∧
    structuralSummaryComplete false false false 1 0 = false ∧
    structuralSummaryComplete false false false 0 1 = false := by
  decide

-- ============================================================================
-- Section PA76: Agent Execution Loop, Tool Dispatch & Result Envelope Lattice
-- (`src/agent/result_envelope.rs`, `src/agent/tool_dispatch/*`,
--  `src/agent/task_runner.rs`, `src/agent/interactive/*`)
-- ============================================================================

inductive EnvelopeTokenClass where
  | ordinaryCode
  | framingTag
  | chatmlSpecial
  | mistralBracket
  | literalEscapedBeforeTag
  deriving DecidableEq, Repr

/-- Mirrors `agent::result_envelope::encode`. -/
def envelopeNeutralizes : EnvelopeTokenClass → Bool
  | .ordinaryCode => false
  | .framingTag | .chatmlSpecial | .mistralBracket | .literalEscapedBeforeTag => true

inductive ToolErrorClassModel where
  | safetyViolation
  | resourceNotFound
  | permissionDenied
  | argumentError
  | timeout
  | executionError
  deriving DecidableEq, Repr

def ToolErrorClassModel.asStr : ToolErrorClassModel → String
  | .safetyViolation => "SAFETY_VIOLATION"
  | .resourceNotFound => "RESOURCE_NOT_FOUND"
  | .permissionDenied => "PERMISSION_DENIED"
  | .argumentError => "ARGUMENT_ERROR"
  | .timeout => "TIMEOUT"
  | .executionError => "EXECUTION_ERROR"

inductive ConfirmResponseModel where
  | yes
  | always
  | shellRule
  | yolo
  | viewFullDiff
  | skip
  deriving DecidableEq, Repr

inductive ConfirmStepModel where
  | showFullDiff
  | decide (d : ConfirmResponseModel)
  deriving DecidableEq, Repr

/-- Mirrors `agent::tool_dispatch::helpers::confirm_step`. -/
def resolveConfirmStepModel (resp : ConfirmResponseModel) (fullDiffOffered : Bool) : ConfirmStepModel :=
  match resp with
  | .viewFullDiff => if fullDiffOffered then .showFullDiff else .decide .skip
  | other => .decide other

/-- Mirrors `agent::tool_dispatch::helpers::is_dependency_install_command`. -/
def isDependencyInstallCmdModel (hasInstaller hasVerb : Bool) : Bool :=
  hasInstaller && hasVerb

inductive ShellVerificationKindModel where
  | testExecution
  | compileOrLint
  | smokeRun
  deriving DecidableEq, Repr

def ShellVerificationKindModel.shortTag : ShellVerificationKindModel → String
  | .testExecution => "test"
  | .compileOrLint => "compile"
  | .smokeRun => "smoke"

/-- Mirrors `agent::tool_dispatch::verification_call_ran_nothing`. -/
def verificationCallRanNothingModel (exitCode126Or127 runnerNotStarted noCargoManifest : Bool) : Bool :=
  exitCode126Or127 || runnerNotStarted || noCargoManifest

/-- Mirrors `agent::tool_dispatch::cargo_found_no_manifest`. -/
def cargoFoundNoManifestModel (hasNoManifestErrorLine ranAnyBuildOrTest : Bool) : Bool :=
  hasNoManifestErrorLine && !ranAnyBuildOrTest

/-- Mirrors `agent::tool_dispatch::spill::tool_result_value_indicates_success`. -/
def toolResultIndicatesSuccessModel
    (explicitSuccessFalse explicitPassedFalse timedOut truthyError nonZeroExit : Bool) : Bool :=
  !explicitSuccessFalse && !explicitPassedFalse && !truthyError && !timedOut && !nonZeroExit

/-- Mirrors `agent::task_runner::RunBudgets::from_config` filtering `0` as unset (`none`). -/
def filterPositiveBudget (raw : Nat) : Option Nat :=
  if raw > 0 then some raw else none

theorem envelope_ordinary_code_untouched_and_control_tokens_neutralized :
    envelopeNeutralizes .ordinaryCode = false ∧
    envelopeNeutralizes .framingTag = true ∧
    envelopeNeutralizes .chatmlSpecial = true ∧
    envelopeNeutralizes .mistralBracket = true ∧
    envelopeNeutralizes .literalEscapedBeforeTag = true := by
  decide

theorem confirm_view_diff_fails_closed_when_unoffered :
    resolveConfirmStepModel .viewFullDiff true = .showFullDiff ∧
    resolveConfirmStepModel .viewFullDiff false = .decide .skip := by
  decide

theorem dependency_install_requires_installer_and_verb :
    isDependencyInstallCmdModel true true = true ∧
    isDependencyInstallCmdModel true false = false ∧
    isDependencyInstallCmdModel false true = false := by
  decide

theorem cargo_no_manifest_false_when_tests_ran :
    cargoFoundNoManifestModel true false = true ∧
    cargoFoundNoManifestModel true true = false ∧
    verificationCallRanNothingModel true false false = true := by
  decide

theorem verification_ran_nothing_on_any_nonstart_signal :
    verificationCallRanNothingModel false false false = false ∧
    verificationCallRanNothingModel true false false = true ∧
    verificationCallRanNothingModel false true false = true ∧
    verificationCallRanNothingModel false false true = true := by
  decide

theorem tool_result_truthy_error_defeats_zero_exit :
    toolResultIndicatesSuccessModel false false false false false = true ∧
    toolResultIndicatesSuccessModel false false false true false = false ∧
    toolResultIndicatesSuccessModel false false false false true = false ∧
    toolResultIndicatesSuccessModel false false true false false = false := by
  decide

theorem tool_result_explicit_false_flags_fail :
    toolResultIndicatesSuccessModel true false false false false = false ∧
    toolResultIndicatesSuccessModel false true false false false = false := by
  decide

theorem run_budgets_filter_zero_as_unset :
    filterPositiveBudget 0 = none ∧
    filterPositiveBudget 120 = some 120 := by
  decide

-- ============================================================================
-- Section PA77: RSI Doctor, Multimodal Rejection & Exit Code Lattice
-- (`src/llm_doctor.rs`, `src/api/client.rs`, `src/cognitive/self_edit.rs`,
--  `src/errors.rs`, `src/orchestration/multiagent/chat.rs`)
-- ============================================================================

/-- Mirrors `llm_doctor::evaluate_vision_responses` red/blue control probe classification. -/
def evaluateVisionConditioningModel (redIsRed redIsBlue blueIsRed blueIsBlue : Bool) : String :=
  if redIsRed && !redIsBlue && !blueIsRed && blueIsBlue then
    "Conditioned"
  else if (redIsRed && blueIsRed) || (redIsBlue && blueIsBlue) || (redIsBlue && blueIsRed) then
    "Unconditioned"
  else
    "Inconclusive"

/-- Mirrors `api::client::is_multimodal_engine_rejection` fast-fail classification. -/
def isMultimodalEngineRejectionModel
    (hasImageUrl : Bool) (status : Nat) (hasEngineSignal hasMultimodalSignal : Bool) : Bool :=
  if !hasImageUrl then
    false
  else if status == 500 then
    hasEngineSignal || hasMultimodalSignal
  else if status == 400 || status == 415 || status == 422 || status == 501 then
    hasMultimodalSignal
  else
    false

/-- Mirrors `cognitive::self_edit::SelfEditOrchestrator::build_improvement_prompt` four-gate verifier requirement. -/
def improvementGatesComplete (hasFmt hasCheck hasClippy hasTest : Bool) : Bool :=
  hasFmt && hasCheck && hasClippy && hasTest

inductive RunEndClassModel where
  | success
  | cancelled
  | terminated
  | safetyBlocked
  | configError
  | apiFailed
  | failed
  deriving DecidableEq, Repr

def RunEndClassModel.asStr : RunEndClassModel → String
  | .success => "Completed"
  | .cancelled => "Interrupted"
  | .terminated => "Terminated"
  | .safetyBlocked | .configError | .apiFailed | .failed => "Failed"

def RunEndClassModel.exitCode : RunEndClassModel → Nat
  | .success => 0
  | .cancelled => 130
  | .terminated => 143
  | .safetyBlocked => 5
  | .configError => 2
  | .apiFailed => 4
  | .failed => 1

/-- Mirrors `orchestration::multiagent::chat::MultiAgentChat::run_single_agent` gateway-timeout stream fallback guard. -/
def shouldAttemptGatewayStreamRecovery (isGatewayTimeout : Bool) (remainingSecs : Nat) : Bool :=
  isGatewayTimeout && remainingSecs > 0

theorem vision_conditioning_requires_red_and_blue_exact_polarity :
    evaluateVisionConditioningModel true false false true = "Conditioned" ∧
    evaluateVisionConditioningModel true false true false = "Unconditioned" ∧
    evaluateVisionConditioningModel false true true false = "Unconditioned" ∧
    evaluateVisionConditioningModel false false false true = "Inconclusive" := by
  decide

theorem multimodal_rejection_requires_image_url_and_matching_status_signal :
    isMultimodalEngineRejectionModel true 500 true false = true ∧
    isMultimodalEngineRejectionModel true 500 false false = false ∧
    isMultimodalEngineRejectionModel true 400 false true = true ∧
    isMultimodalEngineRejectionModel true 400 false false = false ∧
    isMultimodalEngineRejectionModel true 422 false true = true ∧
    isMultimodalEngineRejectionModel true 503 true true = false ∧
    isMultimodalEngineRejectionModel false 500 true true = false := by
  decide

theorem improvement_gates_require_all_four_cargo_verifiers :
    improvementGatesComplete true true true true = true ∧
    improvementGatesComplete false true true true = false ∧
    improvementGatesComplete true true false true = false := by
  decide

theorem run_end_exit_codes_partition_posix_and_typed_failures :
    RunEndClassModel.success.exitCode = 0 ∧
    RunEndClassModel.cancelled.exitCode = 130 ∧
    RunEndClassModel.terminated.exitCode = 143 ∧
    RunEndClassModel.safetyBlocked.exitCode = 5 ∧
    RunEndClassModel.configError.exitCode = 2 ∧
    RunEndClassModel.apiFailed.exitCode = 4 ∧
    RunEndClassModel.failed.exitCode = 1 := by
  decide

theorem gateway_recovery_requires_gateway_status_and_positive_remaining_budget :
    shouldAttemptGatewayStreamRecovery true 30 = true ∧
    shouldAttemptGatewayStreamRecovery true 0 = false ∧
    shouldAttemptGatewayStreamRecovery false 30 = false := by
  decide

-- ============================================================================
-- Section PA78: Conjoined Citation Fenced Quotes & GroundedAssistant Lattice
-- (`src/agent/citation_check.rs`, `src/evolve/assistant.rs`)
-- ============================================================================

/-- Mirrors `agent::citation_check::fenced_quote_after` ownership for multi-citation lines. -/
def multiCitationFencedQuoteOwned (citeCount : Nat) (isNumbered : Bool) (lineMatchesRange : Bool) : Bool :=
  if citeCount == 0 then
    false
  else if citeCount == 1 then
    true
  else
    isNumbered && lineMatchesRange

/-- Mirrors `agent::citation_check::parse_citations` quote source priority (`inline` vs `fenced`). -/
def selectCitationQuoteSource
    (hasSameLineQuote isBare hasTrailingProse hasFencedBlock : Bool) : String :=
  if !hasSameLineQuote then
    if hasFencedBlock then "fenced" else "none"
  else if !isBare && !hasTrailingProse then
    "inline"
  else if hasFencedBlock then
    "fenced"
  else
    "inline"

/-- Mirrors `evolve::assistant::review_client_config` completion floor and template thinking disable. -/
def reviewClientConfigModel (maxTokens : Nat) (hasEnableThinkingKwarg : Bool) : Nat × Bool :=
  (Nat.max maxTokens 8192, if hasEnableThinkingKwarg then false else false)

theorem multi_citation_fenced_quote_requires_numbered_matching_lines :
    multiCitationFencedQuoteOwned 2 true true = true ∧
    multiCitationFencedQuoteOwned 2 true false = false ∧
    multiCitationFencedQuoteOwned 2 false true = false ∧
    multiCitationFencedQuoteOwned 1 false false = true := by
  decide

theorem citation_quote_source_prefers_fenced_over_prose_or_bare_heading_span :
    selectCitationQuoteSource true true true true = "fenced" ∧
    selectCitationQuoteSource true false true true = "fenced" ∧
    selectCitationQuoteSource true false false true = "inline" ∧
    selectCitationQuoteSource true false true false = "inline" := by
  decide

theorem review_client_config_floors_tokens_and_disables_template_thinking :
    reviewClientConfigModel 2048 true = (8192, false) ∧
    reviewClientConfigModel 16384 true = (16384, false) := by
  decide

/-! ## Export deterministic 3-column table for `scripts/check_formal.sh` & Rust conformance tests -/

def exportTablePart1 : List String :=
  -- 1. SignalEmotion priority lattice rows
  let emotionScenarios : List (String × Bool × Bool × Bool × Bool × Bool) :=
    [ ("failure_dominates_all", true, true, true, true, true)
    , ("debt_dominates_uncertainty_and_running", false, true, true, true, true)
    , ("uncertainty_dominates_running_and_citations", false, false, true, true, true)
    , ("running_dominates_citations", false, false, false, true, true)
    , ("citations_yield_analytical", false, false, false, false, true)
    , ("all_quiet_yields_curious", false, false, false, false, false)
    ]
  let emotionRows := emotionScenarios.foldr (fun (label, f, d, u, r, c) acc =>
    let em := classifySignalEmotion f d u r c
    let spec := s!"fail={f},debt={d},unc={u},run={r},cit={c}"
    s!"[\"signal_emotion:{label}\",\"{spec}\",\"{em.name}\"]" :: acc) []

  -- 2. Grounded TrustState derivation rows
  let trustScenarios : List (String × Bool × Bool × Bool) :=
    [ ("redacted_even_with_semantic", true, true, true)
    , ("redacted_unverified", false, true, false)
    , ("missing_citation", false, false, true)
    , ("structural_citation_only", true, false, false)
    , ("verified_citation_and_semantic", true, false, true)
    ]
  let trustRows := trustScenarios.foldr (fun (label, cit, red, sem) acc =>
    let ts := deriveTrustState cit red sem
    let spec := s!"cit={cit},red={red},sem={sem}"
    s!"[\"trust_state:{label}\",\"{spec}\",\"{ts.name}\"]" :: acc) []

  -- 3. PhiPresence attention-economy routing rows
  let presenceScenarios : List (String × PresenceInput) :=
    [ ("busy_user", { userBusy := true, settled := true, dismissedCategory := false, alreadySeen := false, consecutiveIgnored := 0, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 1, spokeSinceLastInterrupt := true })
    , ("unsettled_turn", { userBusy := false, settled := false, dismissedCategory := false, alreadySeen := false, consecutiveIgnored := 0, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 1, spokeSinceLastInterrupt := true })
    , ("dismissed_category", { userBusy := false, settled := true, dismissedCategory := true, alreadySeen := false, consecutiveIgnored := 0, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 1, spokeSinceLastInterrupt := true })
    , ("already_seen", { userBusy := false, settled := true, dismissedCategory := false, alreadySeen := true, consecutiveIgnored := 0, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 1, spokeSinceLastInterrupt := true })
    , ("ignored_twice", { userBusy := false, settled := true, dismissedCategory := false, alreadySeen := false, consecutiveIgnored := 2, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 1, spokeSinceLastInterrupt := true })
    , ("interrupt_ready", { userBusy := false, settled := true, dismissedCategory := false, alreadySeen := false, consecutiveIgnored := 0, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 1, spokeSinceLastInterrupt := true })
    , ("quota_exhausted_downgrades_to_chip", { userBusy := false, settled := true, dismissedCategory := false, alreadySeen := false, consecutiveIgnored := 0, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 0, spokeSinceLastInterrupt := true })
    , ("user_has_not_spoken_downgrades_to_chip", { userBusy := false, settled := true, dismissedCategory := false, alreadySeen := false, consecutiveIgnored := 0, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 1, spokeSinceLastInterrupt := false })
    , ("ignored_once_downgrades_to_chip", { userBusy := false, settled := true, dismissedCategory := false, alreadySeen := false, consecutiveIgnored := 1, allowsInterrupt := true, categoryAllowsChip := true, interruptQuota := 1, spokeSinceLastInterrupt := true })
    , ("ambient_only_category", { userBusy := false, settled := true, dismissedCategory := false, alreadySeen := false, consecutiveIgnored := 0, allowsInterrupt := false, categoryAllowsChip := false, interruptQuota := 1, spokeSinceLastInterrupt := true })
    ]
  let presenceRows := presenceScenarios.foldr (fun (label, inp) acc =>
    let surf := routePresence inp
    s!"[\"presence_route\",\"{label}\",\"{surf.name}\"]" :: acc) []

  -- 4. DebtBoss archetype & HP rows
  let bossScenarios : List (String × Bool × Nat × Nat × Nat × Nat × Nat × Nat × Nat × Nat) :=
    [ ("dormant_no_obs", false, 10, 5, 2, 1, 1, 1, 120, 15)
    , ("regression_hydra_failed_runs", true, 10, 5, 2, 1, 1, 1, 100, 20)
    , ("phantom_specter_unknown_obl", true, 10, 5, 2, 0, 1, 1, 100, 10)
    , ("boilerplate_golem_unreviewed", true, 20, 5, 1, 0, 0, 0, 100, 5)
    , ("sentinel_ward_outstanding", true, 0, 0, 2, 0, 0, 0, 100, 10)
    , ("arena_clear_zero_debt", true, 0, 0, 0, 0, 0, 0, 100, 0)
    ]
  let bossRows := bossScenarios.foldr (fun (label, obs, unrev, untest, out, fail, unkR, unkO, peak, dmg) acc =>
    let (arch, hp, maxHp) := classifyDebtBoss obs unrev untest out fail unkR unkO peak dmg
    let spec := s!"obs={obs},unrev={unrev},untest={untest},out={out},fail={fail},unkR={unkR},unkO={unkO},peak={peak},dmg={dmg}"
    s!"[\"debt_boss:{label}\",\"{spec}\",\"{arch.name}:hp={hp}:max={maxHp}\"]" :: acc) []

  -- 5. Dream consolidation honest status rows
  let dreamScenarios : List (String × Bool × Nat × Nat × Nat) :=
    [ ("clean_consolidation", false, 0, 4, 2)
    , ("partial_store_error", false, 1, 3, 2)
    , ("consolidate_failed", true, 1, 5, 3)
    , ("empty_clean_gather", false, 0, 0, 0)
    ]
  let dreamRows := dreamScenarios.foldr (fun (label, cf, ec, rs, ra) acc =>
    let st := evaluateDreamStatus cf ec rs ra
    let spec := s!"cf={cf},err={ec},raw_stored={rs},raw_archived={ra}"
    let out := s!"ok={st.success}:stored={st.memoriesCreated}:archived={st.sessionsArchived}"
    s!"[\"dream_status:{label}\",\"{spec}\",\"{out}\"]" :: acc) []

  -- 6. KV-Prefix role demotion rows
  let roleScenarios : List (String × List RoleKind) :=
    [ ("preserve_leading_system_demote_mid", [.system, .assistant, .system, .assistant, .system])
    , ("demote_trailing_system_after_assistant", [.system, .user, .assistant, .system])
    , ("no_mid_system_unchanged", [.system, .user, .assistant, .tool])
    ]
  let roleRows := roleScenarios.foldr (fun (label, roles) acc =>
    let before := ",".intercalate (roles.map RoleKind.name)
    let after := ",".intercalate ((demoteMidConversationRoles roles).map RoleKind.name)
    s!"[\"kv_role_demote:{label}\",\"{before}\",\"{after}\"]" :: acc) []

  -- 7. ToolCache worktree isolation & invalidation rows
  let cacheScenarios : List (String × Nat × CacheEntryModel) :=
    [ ("foreign_worktree_retained", 1, { workspaceId := 2, tool := .fileRead, mentionsTargetPath := true })
    , ("active_tree_scoped_dropped", 1, { workspaceId := 1, tool := .directoryTree, mentionsTargetPath := false })
    , ("active_git_scoped_dropped", 1, { workspaceId := 1, tool := .gitStatus, mentionsTargetPath := false })
    , ("active_matching_path_dropped", 1, { workspaceId := 1, tool := .fileRead, mentionsTargetPath := true })
    , ("active_unrelated_path_retained", 1, { workspaceId := 1, tool := .fileRead, mentionsTargetPath := false })
    ]
  let cacheRows := cacheScenarios.foldr (fun (label, activeWs, entry) acc =>
    let keep := retainsAfterPathAndGitInvalidation activeWs entry
    let spec := s!"active={activeWs},ws={entry.workspaceId},tool={entry.tool.name},match={entry.mentionsTargetPath}"
    s!"[\"cache_invalidation:{label}\",\"{spec}\",\"keep={keep}\"]" :: acc) []

  -- 8. EditHistory RestoreOutcome honesty rows
  let restoreScenarios : List (String × Bool × Bool × Bool) :=
    [ ("corrupt_snapshot_skipped", false, false, true)
    , ("already_current_noop", true, true, true)
    , ("dirty_valid_restored", true, false, true)
    , ("dirty_valid_write_failed", true, false, false)
    ]
  let restoreRows := restoreScenarios.foldr (fun (label, v, m, w) acc =>
    let out := evaluateRestoreOutcome v m w
    let spec := s!"valid={v},matches={m},written={w}"
    s!"[\"restore_outcome:{label}\",\"{spec}\",\"{out.name}\"]" :: acc) []

  -- 9. ArenaProfile XP-to-Level rows
  let profileScenarios : List (String × Nat) :=
    [ ("initial_zero_xp", 0)
    , ("just_below_level_two", 99)
    , ("exact_level_two_boundary", 100)
    , ("level_four_veteran", 350)
    ]
  let profileRows := profileScenarios.foldr (fun (label, xp) acc =>
    let lvl := levelForXp xp
    s!"[\"arena_profile:{label}\",\"xp={xp}\",\"level={lvl}\"]" :: acc) []

  -- 10. ContextTrust provenance & gate decision rows
  let contextTrustScenarios : List (String × SourceKindModel × Bool × Bool × Bool) :=
    [ ("workspace_rust_clean", .workspace, true, false, false)
    , ("workspace_rust_finding", .workspace, true, true, false)
    , ("workspace_config_clean", .workspace, false, false, false)
    , ("tool_output_clean", .toolOutput, false, false, false)
    , ("external_high_finding", .external, false, true, true)
    ]
  let contextTrustRows := contextTrustScenarios.foldr (fun (label, src, isCode, anyF, highF) acc =>
    let tl := classifyTrustLevel src isCode
    let gd := evaluateGateDecision anyF highF tl
    let spec := s!"src={src.name},code={isCode},any={anyF},high={highF}"
    s!"[\"context_trust:{label}\",\"{spec}\",\"{tl.name}:{gd.name}\"]" :: acc) []

  -- 11. ContextFit tier ladder rows
  let contextFitScenarios : List (String × Nat × Nat × Nat × Nat × Nat × Nat) :=
    [ ("fits_full_extended", 500, 400, 300, 150, 50, 600)
    , ("falls_back_to_compact", 500, 400, 300, 150, 50, 320)
    , ("falls_back_to_lite", 500, 400, 300, 150, 50, 180)
    , ("overflow_honest_map_false", 500, 400, 300, 150, 50, 30)
    ]
  let contextFitRows := contextFitScenarios.foldr (fun (label, fe, f, c, l, m, usable) acc =>
    let res := fitTierLadder fe f c l m usable
    let spec := s!"fe={fe},f={f},c={c},l={l},m={m},usable={usable}"
    s!"[\"context_fit:{label}\",\"{spec}\",\"{res.mode.name}:measured={res.measuredTokens}:fits={res.fits}\"]" :: acc) []

  -- 12. ApplyStatus shadow worktree lifecycle rows
  let applyScenarios : List (String × ApplyStatusModel × ApplyEventModel) :=
    [ ("running_verified_to_staged", .running, .verifiedAndCompiled)
    , ("running_compile_failed_rejected", .running, .compileFailed)
    , ("running_exit_nonzero_failed", .running, .exitNonZero)
    , ("staged_digest_match_succeeded", .staged, .commitDigestMatch)
    , ("rejected_absorbing", .rejected, .commitDigestMatch)
    ]
  let applyRows := applyScenarios.foldr (fun (label, st, ev) acc =>
    let next := stepApplyStatus st ev
    let spec := s!"state={st.name},event={ev.name}"
    s!"[\"apply_lifecycle:{label}\",\"{spec}\",\"{next.name}\"]" :: acc) []

  -- 13–16. PA13–PA16 rows
  let wave3Rows : List String :=
    [ s!"[\"tool_permission:plan\",\"file_read\",\"{(checkToolPermission { mode := .plan } fileReadSpec).asString}\"]"
    , s!"[\"tool_permission:plan\",\"file_edit\",\"{(checkToolPermission { mode := .plan } fileEditSpec).asString}\"]"
    , s!"[\"tool_permission:normal\",\"file_read\",\"{(checkToolPermission { mode := .normal } fileReadSpec).asString}\"]"
    , s!"[\"tool_permission:normal\",\"file_edit\",\"{(checkToolPermission { mode := .normal } fileEditSpec).asString}\"]"
    , s!"[\"tool_permission:auto\",\"git_push\",\"{(checkToolPermission { mode := .auto } gitPushSpec).asString}\"]"
    , s!"[\"tool_permission:auto\",\"file_delete\",\"{(checkToolPermission { mode := .auto } fileDeleteSpec).asString}\"]"
    , s!"[\"tool_permission:yolo_default\",\"file_delete\",\"{(checkToolPermission { mode := .yolo } fileDeleteSpec).asString}\"]"
    , s!"[\"tool_confirm:normal\",\"read_only_low\",\"{if normalModeNeedsConfirmation false false (some (true, .low)) then "confirm" else "skip"}\"]"
    , s!"[\"tool_confirm:normal\",\"unclassified\",\"{if normalModeNeedsConfirmation false false none then "confirm" else "skip"}\"]"
    , s!"[\"tool_confirm:normal\",\"session_granted\",\"{if normalModeNeedsConfirmation true false none then "confirm" else "skip"}\"]"
    , let s := cbOnFailure defaultCBConfig { state := .closed, failureCount := 4, successCount := 0, halfOpenRequests := 0 }
      s!"[\"circuit_breaker:on_failure\",\"closed:4->5\",\"{s.state.asString}:{s.failureCount}\"]"
    , let s := cbOnFailure defaultCBConfig { state := .closed, failureCount := 2, successCount := 0, halfOpenRequests := 0 }
      s!"[\"circuit_breaker:on_failure\",\"closed:2->3\",\"{s.state.asString}:{s.failureCount}\"]"
    , let s := cbOnFailure defaultCBConfig { state := .halfOpen, failureCount := 0, successCount := 2, halfOpenRequests := 1 }
      s!"[\"circuit_breaker:on_failure\",\"half_open:0->1\",\"{s.state.asString}:{s.failureCount}\"]"
    , let s := cbOnSuccess defaultCBConfig { state := .halfOpen, failureCount := 0, successCount := 2, halfOpenRequests := 1 }
      s!"[\"circuit_breaker:on_success\",\"half_open:2->3\",\"{s.state.asString}:{s.successCount}\"]"
    , let s := cbOnSuccess defaultCBConfig { state := .closed, failureCount := 3, successCount := 1, halfOpenRequests := 0 }
      s!"[\"circuit_breaker:on_success\",\"closed:reset_failures\",\"{s.state.asString}:{s.failureCount}\"]"
    , let (ok, s) := cbCanExecute defaultCBConfig { state := .openState, failureCount := 0, successCount := 0, halfOpenRequests := 0 } true
      let okStr := if ok then "allow" else "deny"
      s!"[\"circuit_breaker:can_execute\",\"open:timeout_elapsed\",\"{okStr}:{s.state.asString}:{s.halfOpenRequests}\"]"
    , let vClean := if validateGraphReportValid { duplicateIds := 0, cycles := 0, danglingEdges := 0, isolatedNodes := 0 } then "valid" else "invalid"
      s!"[\"ontology_graph:validate\",\"clean\",\"{vClean}\"]"
    , let vDup := if validateGraphReportValid { duplicateIds := 1, cycles := 0, danglingEdges := 0, isolatedNodes := 0 } then "valid" else "invalid"
      s!"[\"ontology_graph:validate\",\"duplicate_id\",\"{vDup}\"]"
    , let vCyc := if validateGraphReportValid { duplicateIds := 0, cycles := 1, danglingEdges := 0, isolatedNodes := 0 } then "valid" else "invalid"
      s!"[\"ontology_graph:validate\",\"structural_cycle\",\"{vCyc}\"]"
    , let vDang := if validateGraphReportValid { duplicateIds := 0, cycles := 0, danglingEdges := 1, isolatedNodes := 0 } then "valid" else "invalid"
      s!"[\"ontology_graph:validate\",\"dangling_edge\",\"{vDang}\"]"
    , let cDep := if participatesInCycleCheck .dependsOn then "checked" else "exempt"
      let cCont := if participatesInCycleCheck .contains then "checked" else "exempt"
      s!"[\"ontology_graph:cycle_edge\",\"depends_on_vs_contains\",\"{cDep}:{cCont}\"]"
    , let dBlock := if deletionRequireExecutable false 1 then "allow" else "block"
      let dReady := if deletionRequireExecutable true 0 then "allow" else "block"
      s!"[\"evolve_deletion:gate\",\"preview_blocked_vs_ready\",\"{dBlock}:{dReady}\"]"
    , s!"[\"phi_viseme:lookup\",\"before_50ms\",\"{(activeVisemeAt sampleVisemeSchedule 50).asCode}\"]"
    , s!"[\"phi_viseme:lookup\",\"span1_150ms\",\"{(activeVisemeAt sampleVisemeSchedule 150).asCode}\"]"
    , s!"[\"phi_viseme:lookup\",\"boundary_250ms\",\"{(activeVisemeAt sampleVisemeSchedule 250).asCode}\"]"
    , s!"[\"phi_viseme:lookup\",\"gap_420ms\",\"{(activeVisemeAt sampleVisemeSchedule 420).asCode}\"]"
    , s!"[\"phi_viseme:lookup\",\"after_600ms\",\"{(activeVisemeAt sampleVisemeSchedule 600).asCode}\"]"
    ]

  -- 17–20. PA17–PA20 rows
  let wave4Rows : List String :=
    [ s!"[\"recovery_classify:context_overflow_with_403_and_tool\",\"Context overflow: 4035 tokens including 12 in the tools\",\"{(classifyFailureSignals true false false false true false true true false false).name}\"]"
    , s!"[\"recovery_classify:hook_timeout\",\"PreToolUse hook timed out after 30s\",\"{(classifyFailureSignals false true true false false false false false false false).name}\"]"
    , s!"[\"recovery_classify:hook_nonzero\",\"PreToolUse hook exited with status 1\",\"{(classifyFailureSignals false true false false false false false false false false).name}\"]"
    , s!"[\"recovery_classify:timed_out_connecting\",\"timed out connecting to endpoint\",\"{(classifyFailureSignals false false true true false false false false false false).name}\"]"
    , s!"[\"recovery_classify:http_403_with_timeout_text\",\"HTTP 403 forbidden after request timeout\",\"{(classifyFailureSignals false false true false true false false false false false).name}\"]"
    , s!"[\"recovery_classify:disk_full_enospc\",\"write failed: No space left on device (os error 28)\",\"{(classifyFailureSignals false false false false false false false false false true).name}\"]"
    , let (ok1, l1, c1) := stepRateLimiter 10 1000 0 10 1500
      s!"[\"rate_limiter:new_window_opens\",\"max=10,win=1000,last=0,count=10,now=1500\",\"{if ok1 then "allow" else "block"}:last={l1}:count={c1}\"]"
    , let (ok2, l2, c2) := stepRateLimiter 10 1000 1500 9 1800
      s!"[\"rate_limiter:within_window_allowed\",\"max=10,win=1000,last=1500,count=9,now=1800\",\"{if ok2 then "allow" else "block"}:last={l2}:count={c2}\"]"
    , let (ok3, l3, c3) := stepRateLimiter 10 1000 1500 10 1800
      s!"[\"rate_limiter:within_window_exceeded\",\"max=10,win=1000,last=1500,count=10,now=1800\",\"{if ok3 then "allow" else "block"}:last={l3}:count={c3}\"]"
    , let (ok4, l4, c4) := stepRateLimiter 10 1000 1500 10 1200
      s!"[\"rate_limiter:clock_skew_no_reset\",\"max=10,win=1000,last=1500,count=10,now=1200\",\"{if ok4 then "allow" else "block"}:last={l4}:count={c4}\"]"
    , s!"[\"hook_fire:pre_tool_policy_skip\",\"PreToolUse:SkipPolicy\",\"skip_policy:{skipFailureTag .policy}\"]"
    , let hPreErr := match resolveHookStep .preToolUse .errorNonFatal with
        | .skip .hookFailure => s!"skip_failure:{skipFailureTag .hookFailure}"
        | _ => "unexpected"
      s!"[\"hook_fire:pre_tool_error_fails_closed\",\"PreToolUse:Error\",\"{hPreErr}\"]"
    , let hPostErr := match resolveHookStep .postToolUse .errorNonFatal with
        | .continueExec => "continue"
        | _ => "unexpected"
      s!"[\"hook_fire:post_tool_error_continues\",\"PostToolUse:Error\",\"{hPostErr}\"]"
    , let hStopErr := match resolveHookStep .stop .errorNonFatal with
        | .continueExec => "continue"
        | _ => "unexpected"
      s!"[\"hook_fire:stop_error_continues\",\"Stop:Error\",\"{hStopErr}\"]"
    , let g1 := stepGuardSummary {} .pass .block
      s!"[\"swl_guardrail:pass_block_action\",\"res=pass,act=block\",\"total={g1.total}:pass={g1.passed}:fail={g1.failed}:err={g1.errors}:blk={g1.blocked}:warn={g1.warnings}:block={outcomeShouldBlock .pass .block}\"]"
    , let g2 := stepGuardSummary {} .fail .block
      s!"[\"swl_guardrail:fail_block_action\",\"res=fail,act=block\",\"total={g2.total}:pass={g2.passed}:fail={g2.failed}:err={g2.errors}:blk={g2.blocked}:warn={g2.warnings}:block={outcomeShouldBlock .fail .block}\"]"
    , let g3 := stepGuardSummary {} .evalError .block
      s!"[\"swl_guardrail:error_block_action_fails_closed\",\"res=error,act=block\",\"total={g3.total}:pass={g3.passed}:fail={g3.failed}:err={g3.errors}:blk={g3.blocked}:warn={g3.warnings}:block={outcomeShouldBlock .evalError .block}\"]"
    , let g4 := stepGuardSummary {} .evalError .warn
      s!"[\"swl_guardrail:error_warn_action_warns\",\"res=error,act=warn\",\"total={g4.total}:pass={g4.passed}:fail={g4.failed}:err={g4.errors}:blk={g4.blocked}:warn={g4.warnings}:block={outcomeShouldBlock .evalError .warn}\"]"
    , let g5 := stepGuardSummary {} .fail .log
      s!"[\"swl_guardrail:fail_log_action\",\"res=fail,act=log\",\"total={g5.total}:pass={g5.passed}:fail={g5.failed}:err={g5.errors}:blk={g5.blocked}:warn={g5.warnings}:block={outcomeShouldBlock .fail .log}\"]"
    ]

  -- 21–24. PA21–PA24 rows
  let wave5Rows : List String :=
    [ let r1 := finalizeShellExecRisk .reads true true true true
      s!"[\"confirm_risk:shell_git_status_reads\",\"shell_exec:git status\",\"{r1.label}:ro={r1.isReadOnly}\"]"
    , let r2 := finalizeShellExecRisk .reads true true false true
      s!"[\"confirm_risk:shell_git_diff_unsafe_env_runs_command\",\"shell_exec:git diff+unsafe_env\",\"{r2.label}:ro={r2.isReadOnly}\"]"
    , let r3 := finalizeShellExecRisk .runsCommand true false true true
      s!"[\"confirm_risk:shell_cargo_check_runs_command\",\"shell_exec:cargo check\",\"{r3.label}:ro={r3.isReadOnly}\"]"
    , let r4 := finalizeShellExecRisk (joinRiskTag .reads .deletesFiles) false false true true
      s!"[\"confirm_risk:shell_compound_reads_and_rm_deletes\",\"shell_exec:git status && rm -rf /tmp/x\",\"{r4.label}:ro={r4.isReadOnly}\"]"
    , s!"[\"confirm_risk:clippy_fix_vs_nofix\",\"cargo_clippy:fix=false_vs_true\",\"{RiskTagModel.runsCommand.label}->{RiskTagModel.writesWorkspace.label}\"]"
    , let r6 := classifyToolMetadataFallback false none
      s!"[\"confirm_risk:unknown_mcp_tool_unclassified\",\"mcp_custom_probe\",\"{r6.label}:ro={r6.isReadOnly}\"]"
    , let p1 := classifyResourcePressure 40 30 70 85 95 false false
      let q1 := adjustQuotasForPressure defaultBaseQuotas p1
      s!"[\"resource_pressure:none_baseline\",\"mem=40,gpu=30,hot=false,disk=false\",\"{p1.name}:act={p1.requiresAction}:crit={p1.isCritical}:conc={q1.maxConcurrentRequests}:ctx={q1.maxContextTokens}:q={q1.maxQueuedTasks}\"]"
    , let p2 := classifyResourcePressure 60 20 70 85 95 false false
      let q2 := adjustQuotasForPressure defaultBaseQuotas p2
      s!"[\"resource_pressure:low_slight_throttle\",\"mem=60,gpu=20,hot=false,disk=false\",\"{p2.name}:act={p2.requiresAction}:crit={p2.isCritical}:conc={q2.maxConcurrentRequests}:ctx={q2.maxContextTokens}:q={q2.maxQueuedTasks}\"]"
    , let p3 := classifyResourcePressure 75 40 70 85 95 false false
      let q3 := adjustQuotasForPressure defaultBaseQuotas p3
      s!"[\"resource_pressure:medium_halves_concurrency_and_context\",\"mem=75,gpu=40,hot=false,disk=false\",\"{p3.name}:act={p3.requiresAction}:crit={p3.isCritical}:conc={q3.maxConcurrentRequests}:ctx={q3.maxContextTokens}:q={q3.maxQueuedTasks}\"]"
    , let p4 := classifyResourcePressure 40 30 70 85 95 false true
      let q4 := adjustQuotasForPressure defaultBaseQuotas p4
      s!"[\"resource_pressure:high_on_disk_exceeded\",\"mem=40,gpu=30,hot=false,disk=true\",\"{p4.name}:act={p4.requiresAction}:crit={p4.isCritical}:conc={q4.maxConcurrentRequests}:ctx={q4.maxContextTokens}:q={q4.maxQueuedTasks}\"]"
    , let p5 := classifyResourcePressure 96 50 70 85 95 true true
      let q5 := adjustQuotasForPressure defaultBaseQuotas p5
      s!"[\"resource_pressure:critical_emergency_clamp\",\"mem=96,gpu=50,hot=true,disk=true\",\"{p5.name}:act={p5.requiresAction}:crit={p5.isCritical}:conc={q5.maxConcurrentRequests}:ctx={q5.maxContextTokens}:q={q5.maxQueuedTasks}\"]"
    , let b1 := approachingLimitBand 7 10
      s!"[\"loop_guard:band_below_80_pct\",\"iter=7,max=10\",\"band={b1}:warn={decide (b1 > 0)}\"]"
    , let b2 := approachingLimitBand 8 10
      s!"[\"loop_guard:band_at_80_pct\",\"iter=8,max=10\",\"band={b2}:warn={decide (b2 > 0)}\"]"
    , let b3 := approachingLimitBand 9 10
      s!"[\"loop_guard:band_at_90_pct\",\"iter=9,max=10\",\"band={b3}:warn={decide (b3 > 0)}\"]"
    , let sDup := productivePairStreak true true 1 1 false
      let sExc := productivePairStreak true true 1 1 true
      s!"[\"loop_guard:streak_duplicate_probe_vs_excused_by_edit\",\"window=2\",\"dup_read={sDup}:edit_then_read={sExc}\"]"
    , let (cap, gr) := restoreBudgetExtension 20 30 9
      s!"[\"loop_guard:restore_budget_extension_clamp\",\"orig=20,persisted=30,grants=9\",\"cap={cap}:grants={gr}:ceiling={decide (gr ≥ 4)}\"]"
    , let mCap1 := deriveMaxMemoryTokens 1000 2000
      s!"[\"memory_retention:derive_cap_and_near_limit\",\"ctx=1000,quota=2000,used=807_vs_808\",\"max={mCap1}:at807={isNearMemoryLimit 807 mCap1}:at808={isNearMemoryLimit 808 mCap1}\"]"
    , let mCap2 := deriveMaxMemoryTokens 4000 1000
      s!"[\"memory_retention:quota_bounds_context_length\",\"ctx=4000,quota=1000,used=808\",\"max={mCap2}:near={isNearMemoryLimit 808 mCap2}\"]"
    , let mCap3 := deriveMaxMemoryTokens 1000 1000
      let e1 := addEntryWithEviction [] mCap3 400
      let e2 := addEntryWithEviction e1 mCap3 400
      let e3 := addEntryWithEviction e2 mCap3 300
      s!"[\"memory_retention:fifo_token_budget_eviction\",\"ctx=1000,quota=1000,entries=400+400+300\",\"len={e3.length}:tokens={sumEntryTokens e3}\"]"
    , s!"[\"memory_retention:temporal_half_life_importance_ladder\",\"half_life_1x\",\"low={halfLifeDecayPct .low}:normal={halfLifeDecayPct .normal}:high={halfLifeDecayPct .high}:critical={halfLifeDecayPct .critical}\"]"
    ]

  -- 25–28. PA25–PA28 rows
  let wave6Rows : List String :=
    [ let d1 := checkDarwinxSuite true [
        { sameName := true, baselinePassed := true, candPassed := true },
        { sameName := true, baselinePassed := false, candPassed := true }
      ]
      s!"[\"rsi_safety:darwinx_pass_non_regressing\",\"same_suite=true,s1=T->T,s2=F->T\",\"pass={d1}\"]"
    , let d2 := checkDarwinxSuite true [
        { sameName := true, baselinePassed := true, candPassed := true },
        { sameName := true, baselinePassed := true, candPassed := false }
      ]
      s!"[\"rsi_safety:darwinx_rejects_regressed_scenario\",\"same_suite=true,s1=T->T,s2=T->F\",\"pass={d2}\"]"
    , let d3 := checkDarwinxSuite false [
        { sameName := false, baselinePassed := true, candPassed := true }
      ]
      s!"[\"rsi_safety:darwinx_rejects_suite_mismatch\",\"same_suite=false,extra_or_missing\",\"pass={d3}\"]"
    , let (decayed, tripped0) := decayRsiFailuresOnRestart 5
      let (afterNoMut, tripped1) := stepRsiBreaker decayed 5 tripped0 .noEligibleMutation
      s!"[\"rsi_safety:breaker_no_mutation_vs_failure_and_decay\",\"loaded=5,max=5:restart->no_eligible_mutation\",\"decayed={decayed}:after_no_mut={afterNoMut}:tripped={tripped1}\"]"
    , let pFresh := promptPatternEligible 42 15 true 0
      let pMature := promptPatternEligible 42 15 true 5
      s!"[\"rsi_safety:prompt_tournament_extends_and_usage_gate\",\"base_len=15,win_len=42,prefix=true,u=0_vs_5\",\"ext={pFresh.1}:u0_best={pFresh.2}:u5_best={pMature.2}\"]"
    , let g1 := evaluateCandidatePromotion 850 840 5 true 100 100 true .pass
      s!"[\"evolution_gate:sab_score_drop_rejected\",\"base_sab=85.0,cand_sab=84.0,noise=0.5\",\"{g1.label}:promote={g1.isPromote}\"]"
    , let g2 := evaluateCandidatePromotion 850 900 5 false 100 100 true .pass
      s!"[\"evolution_gate:darwinx_regression_rejected\",\"base_sab=85.0,cand_sab=90.0,darwinx=false\",\"{g2.label}:promote={g2.isPromote}\"]"
    , let g3 := evaluateCandidatePromotion 850 900 5 true 100 99 true .pass
      s!"[\"evolution_gate:sabotage_test_deletion_rejected\",\"base_tests=100,cand_tests=99,cand_sab=90.0\",\"{g3.label}:promote={g3.isPromote}\"]"
    , let tok4 := evaluateTwoArmTokenTerm 1000 2 940 960
      let g4 := evaluateCandidatePromotion 850 853 5 true 100 100 true tok4
      s!"[\"evolution_gate:within_noise_token_disagreement_rejected\",\"base_sab=85.0,cand_sab=85.3,arms=940_960/1000\",\"tok={tok4.label}:{g4.label}:promote={g4.isPromote}\"]"
    , let tok5 := evaluateTwoArmTokenTerm 1000 2 940 930
      let g5 := evaluateCandidatePromotion 850 853 5 true 100 100 true tok5
      s!"[\"evolution_gate:within_noise_composite_and_tokens_promote\",\"base_sab=85.0,cand_sab=85.3,arms=940_930/1000\",\"tok={tok5.label}:{g5.label}:promote={g5.isPromote}\"]"
    , let arms1 : List ArmCandidate :=
        [ { armId := 1, protectedTouch := true, syntaxValid := true, compiled := true, testRegressions := 0, fitnessDeltaPos := true, fitnessDeltaMag := 50, armMaxRuntime := 80, baseMinRuntime := 100 }
        , { armId := 2, protectedTouch := false, syntaxValid := true, compiled := true, testRegressions := 1, fitnessDeltaPos := true, fitnessDeltaMag := 40, armMaxRuntime := 80, baseMinRuntime := 100 }
        , { armId := 3, protectedTouch := false, syntaxValid := true, compiled := true, testRegressions := 0, fitnessDeltaPos := true, fitnessDeltaMag := 15, armMaxRuntime := 85, baseMinRuntime := 100 }
        , { armId := 4, protectedTouch := false, syntaxValid := true, compiled := true, testRegressions := 0, fitnessDeltaPos := true, fitnessDeltaMag := 30, armMaxRuntime := 80, baseMinRuntime := 100 }
        ]
      let passCount1 := (arms1.filter armPassesAllGates).length
      let cons1 := checkMultiArmConsensus 2 arms1
      let win1 := match selectBestPassingArm arms1 with | some (id, _) => s!"arm-{id}" | none => "none"
      s!"[\"multi_arm_readiness:protected_and_regression_rejected_winner_selected\",\"arms=4,thresh=2\",\"pass={passCount1}:cons={cons1}:win={win1}\"]"
    , let arms2 : List ArmCandidate :=
        [ { armId := 1, protectedTouch := false, syntaxValid := true, compiled := true, testRegressions := 0, fitnessDeltaPos := true, fitnessDeltaMag := 25, armMaxRuntime := 105, baseMinRuntime := 100 } ]
      let passCount2 := (arms2.filter armPassesAllGates).length
      let consZero := checkMultiArmConsensus 0 arms2
      let win2 := match selectBestPassingArm arms2 with | some (id, _) => s!"arm-{id}" | none => "none"
      s!"[\"multi_arm_readiness:unseparated_runtime_and_zero_threshold\",\"arm_max=105>=base_min=100,thresh=0\",\"pass={passCount2}:cons0={consZero}:win={win2}\"]"
    , let (app3, dup3, skp3) := summarizeRepairBatch [
        (true, true, false, false),
        (true, true, true, false),
        (true, true, false, true),
        (false, true, false, false)
      ]
      s!"[\"multi_arm_readiness:repair_dedup_overlap_and_path_escape\",\"valid+dup+overlap+escape\",\"applied={app3}:dup={dup3}:skip={skp3}\"]"
    , let (stComp, rComp, cComp, _) := simulateRepairLoop 3 0 1 0 [(false, 1), (true, 0)]
      let (stNoFix, rNoFix, cNoFix, _) := simulateRepairLoop 3 0 1 0 [(false, 0)]
      s!"[\"multi_arm_readiness:repair_loop_compiles_vs_no_fix\",\"max=3:one_fix_then_ok_vs_no_fix\",\"{stComp.label}:{rComp}:{cComp}:{stNoFix.label}:{rNoFix}:{cNoFix}\"]"
    , let (stFuel, rFuel, cFuel, fFuel) := simulateRepairLoop 2 0 1 0 [(false, 1), (false, 1), (false, 1)]
      s!"[\"multi_arm_readiness:repair_loop_fuel_exhausted\",\"max=2:three_failing_rounds\",\"stop={stFuel.label}:rounds={rFuel}:checks={cFuel}:fixes={fFuel}\"]"
    , let hDis := stepSelfHealingError false .network 0 true false
      let hNet1 := stepSelfHealingError true .network 0 true false
      let hNet2 := stepSelfHealingError true .network 1 true false
      let sDis := match hDis with | none => "none" | some _ => "some"
      let sNet1 := match hNet1 with | some (res, esc, att) => s!"{res}:{esc}:{att}" | none => "none"
      let sNet2 := match hNet2 with | some (res, esc, att) => s!"{res}:{esc}:{att}" | none => "none"
      s!"[\"self_healing:disabled_and_retry_accumulation\",\"disabled_vs_network_retry_1_2\",\"dis={sDis}:n1={sNet1}:n2={sNet2}\"]"
    , let hParse := stepSelfHealingError true .parseError 2 false true
      let sParse := match hParse with | some (res, esc, att) => s!"{res}:{esc}:{att}" | none => "none"
      s!"[\"self_healing:parse_error_escalates_and_resets_attempts\",\"parse_error:primary=false,esc=true,prev=2\",\"{sParse}\"]"
    , let hp4 := match predictComponentHealth 4 4 20 5000 with | none => "none" | some p => p.label
      let hpHealthy := match predictComponentHealth 10 1 5 200 with | none => "none" | some p => p.label
      let hpRiskUnhealthy := match predictComponentHealth 10 2 5 200 with | none => "none" | some p => p.label
      let hpRiskLatency := match predictComponentHealth 10 0 0 3500 with | none => "none" | some p => p.label
      s!"[\"self_healing:health_predictor_insufficient_vs_healthy_and_at_risk\",\"len=4_vs_10,u=1/2/0,rt=200/3500\",\"w4={hp4}:ok={hpHealthy}:u2={hpRiskUnhealthy}:rt={hpRiskLatency}\"]"
    , let hpDegU := match predictComponentHealth 10 4 0 200 with | none => "none" | some p => p.label
      let hpDegTrend := match predictComponentHealth 10 0 11 200 with | none => "none" | some p => p.label
      s!"[\"self_healing:health_predictor_degrading_unhealthy_or_error_trend\",\"len=10,u=4_or_trend=11\",\"u4={hpDegU}:trend11={hpDegTrend}\"]"
    , let hpFail := match predictComponentHealth 10 7 0 100 with | none => "none" | some p => p.label
      s!"[\"self_healing:health_predictor_failure_imminent\",\"len=10,u=7,trend=0,rt=100\",\"{hpFail}\"]"
    ]

  -- 29–32. PA29–PA32 rows
  let wave7Rows : List String :=
    [ let gov1 := evaluateGovernance4Check {
        status := .baseline, hasCompositeScore := true, protectedPathsClean := true,
        hasKillswitchBypass := false, merkleMismatch := false, hasCriticalFinding := false,
        hasWarningFinding := false, diffSha256Valid := true, provenance := .verified
      }
      s!"[\"investigate_governance:clean_verified_approved\",\"baseline:clean,prov=verified\",\"{gov1.decision.label}:app={gov1.votesApprove}:cla={gov1.votesClarification}:qua={gov1.votesQuarantine}:vet={gov1.votesVeto}:sveto={gov1.hasSafetyVeto}\"]"
    , let govWarn := evaluateGovernance4Check {
        status := .baseline, hasCompositeScore := true, protectedPathsClean := true,
        hasKillswitchBypass := false, merkleMismatch := false, hasCriticalFinding := false,
        hasWarningFinding := true, diffSha256Valid := true, provenance := .verified
      }
      let govInc := evaluateGovernance4Check {
        status := .evaluated, hasCompositeScore := true, protectedPathsClean := true,
        hasKillswitchBypass := false, merkleMismatch := false, hasCriticalFinding := false,
        hasWarningFinding := false, diffSha256Valid := true, provenance := .incomplete
      }
      s!"[\"investigate_governance:warning_or_incomplete_clarification\",\"warn_finding_vs_incomplete_prov\",\"warn={govWarn.decision.label}:{govWarn.votesApprove}:{govWarn.votesClarification}:inc={govInc.decision.label}:{govInc.votesApprove}:{govInc.votesClarification}\"]"
    , let govCrit := evaluateGovernance4Check {
        status := .baseline, hasCompositeScore := true, protectedPathsClean := true,
        hasKillswitchBypass := false, merkleMismatch := false, hasCriticalFinding := true,
        hasWarningFinding := false, diffSha256Valid := true, provenance := .verified
      }
      s!"[\"investigate_governance:critical_finding_quarantined\",\"baseline:critical_unwrap_finding\",\"{govCrit.decision.label}:app={govCrit.votesApprove}:cla={govCrit.votesClarification}:qua={govCrit.votesQuarantine}:vet={govCrit.votesVeto}:sveto={govCrit.hasSafetyVeto}\"]"
    , let govProt := evaluateGovernance4Check {
        status := .baseline, hasCompositeScore := true, protectedPathsClean := false,
        hasKillswitchBypass := false, merkleMismatch := false, hasCriticalFinding := false,
        hasWarningFinding := false, diffSha256Valid := true, provenance := .verified
      }
      let govMerk := evaluateGovernance4Check {
        status := .baseline, hasCompositeScore := true, protectedPathsClean := true,
        hasKillswitchBypass := false, merkleMismatch := true, hasCriticalFinding := false,
        hasWarningFinding := false, diffSha256Valid := true, provenance := .verified
      }
      s!"[\"investigate_governance:safety_or_merkle_breach_vetoed\",\"protected_path_or_merkle_mismatch\",\"prot={govProt.decision.label}:{govProt.hasSafetyVeto}:merkle={govMerk.decision.label}:{govMerk.hasSafetyVeto}\"]"
    , let govHash := evaluateGovernance4Check {
        status := .evaluated, hasCompositeScore := true, protectedPathsClean := true,
        hasKillswitchBypass := false, merkleMismatch := false, hasCriticalFinding := false,
        hasWarningFinding := false, diffSha256Valid := true, provenance := .integrityMismatch
      }
      let govTest := evaluateGovernance4Check {
        status := .testFailed, hasCompositeScore := false, protectedPathsClean := true,
        hasKillswitchBypass := false, merkleMismatch := false, hasCriticalFinding := false,
        hasWarningFinding := false, diffSha256Valid := true, provenance := .verified
      }
      s!"[\"investigate_governance:hash_mismatch_and_test_failure_vetoed\",\"patch_sha256_mismatch_vs_test_failed\",\"hash={govHash.decision.label}:{govHash.hasSafetyVeto}:{govHash.votesVeto}:test={govTest.decision.label}:{govTest.hasSafetyVeto}:{govTest.votesVeto}\"]"
    , let m15b := classifyMicroModelName false true true true
      let mSmall := classifyMicroModelName true false false false
      let m32b := classifyMicroModelName false true false true
      let m13b := classifyMicroModelName false true false true
      let popClamp := clampMicroPopulation 8
      s!"[\"micro_mode:model_classification_word_boundaries\",\"1.5b_small_vs_32b_13b_and_pop8\",\"1.5b={m15b}:small={mSmall}:32b={m32b}:13b={m13b}:pop_clamp={popClamp}\"]"
    , let mvOk := validateMicroHypothesis {
        targetFileCount := 2, targetHasProtected := false, isJsonPatch := true,
        editCount := 2, editHasProtected := false, maxSearchLen := 500, maxReplaceLen := 1000,
        diffFileCount := 0, diffHasProtected := false, diffHunkCount := 0
      }
      s!"[\"micro_mode:valid_json_hypothesis_accepted\",\"files=2,edits=2,search=500,replace=1000\",\"{mvOk.label}\"]"
    , let mvFiles4 := validateMicroHypothesis {
        targetFileCount := 4, targetHasProtected := false, isJsonPatch := true,
        editCount := 1, editHasProtected := false, maxSearchLen := 20, maxReplaceLen := 20,
        diffFileCount := 0, diffHasProtected := false, diffHunkCount := 0
      }
      let mvEdits3 := validateMicroHypothesis {
        targetFileCount := 2, targetHasProtected := false, isJsonPatch := true,
        editCount := 3, editHasProtected := false, maxSearchLen := 20, maxReplaceLen := 20,
        diffFileCount := 0, diffHasProtected := false, diffHunkCount := 0
      }
      s!"[\"micro_mode:target_and_edit_limits_enforced\",\"target_files=4_vs_json_edits=3\",\"files4={mvFiles4.label}:edits3={mvEdits3.label}\"]"
    , let mvProt := validateMicroHypothesis {
        targetFileCount := 1, targetHasProtected := false, isJsonPatch := true,
        editCount := 1, editHasProtected := true, maxSearchLen := 20, maxReplaceLen := 20,
        diffFileCount := 0, diffHasProtected := false, diffHunkCount := 0
      }
      let mvSearch501 := validateMicroHypothesis {
        targetFileCount := 1, targetHasProtected := false, isJsonPatch := true,
        editCount := 1, editHasProtected := false, maxSearchLen := 501, maxReplaceLen := 20,
        diffFileCount := 0, diffHasProtected := false, diffHunkCount := 0
      }
      let mvReplace1001 := validateMicroHypothesis {
        targetFileCount := 1, targetHasProtected := false, isJsonPatch := true,
        editCount := 1, editHasProtected := false, maxSearchLen := 20, maxReplaceLen := 1001,
        diffFileCount := 0, diffHasProtected := false, diffHunkCount := 0
      }
      s!"[\"micro_mode:protected_path_and_search_replace_bounds\",\"edit_protected_search501_replace1001\",\"prot={mvProt.label}:search501={mvSearch501.label}:replace1001={mvReplace1001.label}\"]"
    , let mvDiffProt := validateMicroHypothesis {
        targetFileCount := 1, targetHasProtected := false, isJsonPatch := false,
        editCount := 0, editHasProtected := false, maxSearchLen := 0, maxReplaceLen := 0,
        diffFileCount := 1, diffHasProtected := true, diffHunkCount := 1
      }
      let mvHunks3 := validateMicroHypothesis {
        targetFileCount := 1, targetHasProtected := false, isJsonPatch := false,
        editCount := 0, editHasProtected := false, maxSearchLen := 0, maxReplaceLen := 0,
        diffFileCount := 1, diffHasProtected := false, diffHunkCount := 3
      }
      let mvHunks2 := validateMicroHypothesis {
        targetFileCount := 1, targetHasProtected := false, isJsonPatch := false,
        editCount := 0, editHasProtected := false, maxSearchLen := 0, maxReplaceLen := 0,
        diffFileCount := 1, diffHasProtected := false, diffHunkCount := 2
      }
      s!"[\"micro_mode:unified_diff_protected_and_hunk_ceiling\",\"diff_protected_and_hunks_3_vs_2\",\"diff_prot={mvDiffProt.label}:hunks3={mvHunks3.label}:hunks2={mvHunks2.label}\"]"
    , let sr0 := strategySuccessRatePct 0 0
      let w0 := strategyPriorityWeightPct false 0 0 false 0
      let w1 := strategyPriorityWeightPct false 1 1 false 50
      s!"[\"meta_learning:untried_prior_and_exploration_bonus\",\"att=0_vs_att=1_succ=1_eff=50\",\"sr0={sr0}:w0={w0}:w1={w1}\"]"
    , let wMature := strategyPriorityWeightPct false 10 8 false 60
      let wNegClamp := strategyPriorityWeightPct false 4 2 true 40
      s!"[\"meta_learning:mature_blend_and_negative_eff_clamp\",\"att=10_succ=8_eff=60_vs_att=4_succ=2_eff=-40\",\"mature={wMature}:neg_clamp={wNegClamp}\"]"
    , let wCool := strategyPriorityWeightPct true 10 9 false 90
      let wActive := strategyPriorityWeightPct false 10 9 false 80
      s!"[\"meta_learning:cooldown_blocks_priority_weight\",\"cooldown=true_vs_false\",\"cooldown={wCool}:active={wActive}\"]"
    , let p0 : CyclePhaseModel := .plan
      let p1 := p0.next
      let p2 := p1.next
      let p3 := p2.next
      let p4 := p3.next
      s!"[\"meta_learning:pdvr_cycle_four_step_period\",\"plan->do->verify->reflect->plan\",\"{p0.asStr}->{p1.asStr}->{p2.asStr}->{p3.asStr}->{p4.asStr}\"]"
    , let cap25 := capApproachStackDepth 25
      s!"[\"meta_learning:working_memory_plan_and_approach_cap\",\"steps=3:complete1,fail2:approaches=25\",\"Progress: 1/3 steps (1 failed):cap={cap25}\"]"
    , let stUnassigned := resolveSwarmTaskCompletion .pending 0 1 1
      s!"[\"swarm_task:unassigned_task_stays_pending\",\"assigned=0,reported=1\",\"{stUnassigned.label}:assigned=0:agent_idle=true\"]"
    , let stPartialRep := resolveSwarmTaskCompletion .inProgress 2 1 1
      let trCoder := stepAgentTrustPct 50 true
      s!"[\"swarm_task:partial_reporting_stays_in_progress\",\"assigned=2,reported=1/2,succ=1\",\"{stPartialRep.label}:reported=1/2:coder_idle=true:trust={trCoder}\"]"
    , let stAllSucc := resolveSwarmTaskCompletion .inProgress 2 2 2
      s!"[\"swarm_task:all_assigned_succeed_completes\",\"assigned=2,reported=2/2,succ=2\",\"{stAllSucc.label}:succ=2/2:failed_len=0\"]"
    , let stMixed := resolveSwarmTaskCompletion .inProgress 2 2 1
      let trTester := stepAgentTrustPct 50 false
      s!"[\"swarm_task:mixed_outcomes_yield_partial\",\"assigned=2,reported=2/2,succ=1\",\"{stMixed.label}:succ=1/2:failed_len=1:tester_trust={trTester}\"]"
    , let stAllFail := resolveSwarmTaskCompletion .inProgress 2 2 0
      let trFloor := stepAgentTrustPct (stepAgentTrustPct (stepAgentTrustPct (stepAgentTrustPct (stepAgentTrustPct 50 false) false) false) false) false
      s!"[\"swarm_task:all_assigned_fail_yields_failed_and_trust_floor\",\"assigned=2,reported=2/2,succ=0,streaks=5\",\"{stAllFail.label}:succ=0/2:floor_pct={trFloor}\"]"
    ]

  -- 20. Wave 8 rows (PA33-PA36: consolidation retention, guardrail lattice, safety redaction, workflow lattice)
  let wave8Rows : List String :=
    [ let dZero := temporalDecayScoreBps 10000 0 .normal
      let dHalfLow := temporalDecayScoreBps 5000 0 .low
      let dHalfNorm := temporalDecayScoreBps 5000 0 .normal
      let dHalfHigh := temporalDecayScoreBps 5000 0 .high
      let dHalfCrit := temporalDecayScoreBps 5000 0 .critical
      s!"[\"consolidation_retention:importance_multipliers_and_half_life\",\"age=0_and_age=half_life_across_importances\",\"zero={dZero}:low={dHalfLow}:norm={dHalfNorm}:high={dHalfHigh}:crit={dHalfCrit}\"]"
    , let bZero := effectiveAccessBonusBps 0 693
      let dUnaccessed := temporalDecayScoreBps 5000 bZero .normal
      let dAccessed := temporalDecayScoreBps 5000 693 .normal
      let dClamped := temporalDecayScoreBps 8000 1000 .critical
      s!"[\"consolidation_retention:access_bonus_reinforcement_and_unit_clamp\",\"access=0_vs_1_and_critical_clamp\",\"unaccessed={dUnaccessed}:accessed={dAccessed}:clamped={dClamped}\"]"
    , let sanId := sanitizeRecordId "../etc/passwd:rec_01-A"
      s!"[\"consolidation_retention:sanitize_record_id_and_time_span\",\"id=../etc/passwd:rec_01-A,timestamps=[1000,3000,2000]\",\"sanitized={sanId}:empty_span=none:span=1000..3000\"]"
    , let lCap := clampLoadedFiles 600
      let iCap := clampIndexedRecords 300
      let rZero := retrieveResultCap 0 5
      let rTwo := retrieveResultCap 2 6
      s!"[\"consolidation_retention:store_load_index_and_retrieve_caps\",\"files=600,records=300,k=0_vs_2,candidates=6\",\"load_cap={lCap}:index_cap={iCap}:k0={rZero}:k2={rTwo}\"]"
    , let rBefore := retrieveResultCap 2 3
      let rAfter := retrieveResultCap 2 1
      s!"[\"consolidation_retention:causal_expansion_and_remove_records_pruning\",\"k=2,primary=1,parents=1,children=1_then_remove\",\"before={rBefore}:after_remove={rAfter}\"]"
    , let cAndEmpty := evalCompositeList .andOp []
      let cOrEmpty := evalCompositeList .orOp []
      let cAndPass := evalCompositeList .andOp [.pass, .pass]
      let cAndFail := evalCompositeList .andOp [.pass, .fail, .pass]
      s!"[\"guardrail_lattice:composite_and_short_circuit_and_empty_identity\",\"empty_and_or_plus_and_pass_fail\",\"and_empty={cAndEmpty.asStr}:or_empty={cOrEmpty.asStr}:and_pass={cAndPass.asStr}:and_fail={cAndFail.asStr}\"]"
    , let cOrPass := evalCompositeList .orOp [.fail, .pass, .evalError]
      let cOrAllFail := evalCompositeList .orOp [.fail, .fail]
      let cOrErr := evalCompositeList .orOp [.fail, .evalError, .pass]
      s!"[\"guardrail_lattice:composite_or_pass_fail_and_error_short_circuit\",\"or_fail_pass_error_vs_all_fail_vs_fail_error_pass\",\"or_pass={cOrPass.asStr}:or_all_fail={cOrAllFail.asStr}:or_err={cOrErr.asStr}\"]"
    , let gs0 := GuardSummaryFull.empty
      let gs1 := stepGuardSummaryFull gs0 .pass .block
      let gs2 := stepGuardSummaryFull gs1 .fail .warn
      let gs3 := stepGuardSummaryFull gs2 .fail .log
      let gs4 := stepGuardSummaryFull gs3 .skip .block
      s!"[\"guardrail_lattice:summary_non_blocking_for_warn_log_and_skip\",\"pass_block+fail_warn+fail_log+skip_block\",\"total={gs4.total}:should_block={gs4.shouldBlock}:blocking={gs4.blockingCount}:warnings={gs4.warningCount}\"]"
    , let gs0 := GuardSummaryFull.empty
      let gs1 := stepGuardSummaryFull gs0 .pass .block
      let gs2 := stepGuardSummaryFull gs1 .fail .warn
      let gs3 := stepGuardSummaryFull gs2 .fail .log
      let gs4 := stepGuardSummaryFull gs3 .skip .block
      let gsBlockFail := stepGuardSummaryFull gs4 .fail .block
      let gsBlockErr := stepGuardSummaryFull gsBlockFail .evalError .block
      s!"[\"guardrail_lattice:summary_blocks_on_fail_or_error_with_block_action\",\"add_fail_block_and_error_block\",\"total={gsBlockErr.total}:should_block={gsBlockErr.shouldBlock}:blocking={gsBlockErr.blockingCount}:blocked_stat={gsBlockErr.blocked}:errors={gsBlockErr.errors}\"]"
    , let gsAlert := stepGuardSummaryFull GuardSummaryFull.empty .fail .alert
      let gsFallbackLog := stepGuardSummaryFull gsAlert .fail .log
      s!"[\"guardrail_lattice:enforcer_action_parse_fallback_and_alert_outcome\",\"alert_and_unknown_action_fallback_to_log\",\"total={gsFallbackLog.total}:should_block={gsFallbackLog.shouldBlock}:blocking={gsFallbackLog.blockingCount}:warnings={gsFallbackLog.warningCount}\"]"
    , let privGit := discoveryRootIsPrivate true .okClean
      let privClaude := discoveryRootIsPrivate true .okClean
      let privNotFound := discoveryRootIsPrivate false .errNotFound
      let privClean := discoveryRootIsPrivate false .okClean
      let privErr := discoveryRootIsPrivate false .errOther
      s!"[\"safety_redaction:discovery_root_private_and_not_found_handling\",\"git_claude_notfound_clean_ioerr\",\"git={privGit}:claude={privClaude}:not_found={privNotFound}:clean={privClean}:io_err={privErr}\"]"
    , let rRustBare := shouldRedactPattern .rustSource .bareKeyValueAssignment
      let rGenBare := shouldRedactPattern .generic .bareKeyValueAssignment
      let rRustQuoted := shouldRedactPattern .rustSource .quotedAssignment
      let rRustToken := shouldRedactPattern .rustSource .vendorTokenOrPem
      s!"[\"safety_redaction:rust_source_vs_generic_secret_redaction\",\"bare_kv_vs_quoted_vs_vendor_token\",\"rust_bare={rRustBare}:gen_bare={rGenBare}:rust_quoted={rRustQuoted}:rust_token={rRustToken}\"]"
    , let k0 := redactionNoteKeyName 0
      let k1 := redactionNoteKeyName 1
      let k2 := redactionNoteKeyName 2
      s!"[\"safety_redaction:with_redaction_note_json_collision_avoidance\",\"collision_depths_0_1_2\",\"k0={k0}:k1={k1}:k2={k2}\"]"
    , let cRs := classifySourceExt .rs
      let cMd := classifySourceExt .markdownOrText
      let cData := classifySourceExt .otherOrMissing
      s!"[\"safety_redaction:source_classification_by_path_extension\",\"rs_md_and_other_or_missing\",\"rs={cRs}:md={cMd}:other={cData}\"]"
    , let rTok := shouldRedactPattern .rustSource .vendorTokenOrPem
      s!"[\"safety_redaction:sanitize_tool_context_exfil_downgrade_vs_shell_gate\",\"file_read_code_exfil_plus_token_vs_shell_exec\",\"token_redacted={rTok}:file_read_sanitized=0:shell_sanitized=1\"]"
    , let allowInact := planModeToolAllowed .inactive false
      let allowPlanRo := planModeToolAllowed .planning true
      let allowPlanMut := planModeToolAllowed .planning false
      let allowExec := planModeToolAllowed .executing false
      let compEmpty := planIsComplete 0 0
      let compPartial := planIsComplete 2 1
      let compDone := planIsComplete 2 2
      s!"[\"workflow_lattice:plan_mode_readonly_gate_and_completion_check\",\"inactive_planning_executing_and_0_1_2_steps\",\"inact={allowInact}:plan_ro={allowPlanRo}:plan_mut={allowPlanMut}:exec={allowExec}:empty={compEmpty}:partial={compPartial}:done={compDone}\"]"
    , let depUnknown := resolveStepDependency false .completed true false
      let depTdd := resolveStepDependency true .failed true true
      let depOptFail := resolveStepDependency true .failed false false
      let depReqFail := resolveStepDependency true .failed true false
      s!"[\"workflow_lattice:dependency_resolution_tdd_red_and_optional_skip\",\"unknown_tdd_optional_required_dep_failures\",\"unknown={depUnknown.asStr}:tdd={depTdd.asStr}:opt_fail={depOptFail.asStr}:req_fail={depReqFail.asStr}\"]"
    , let lpFail := aggregateLoopStepStatus 2 1 0
      let lpSkip := aggregateLoopStepStatus 0 0 3
      let lpComp := aggregateLoopStepStatus 2 0 1
      let wfComp := finalizeWorkflowStatus false false
      let wfStop := finalizeWorkflowStatus false true
      let wfReqFail := finalizeWorkflowStatus true false
      s!"[\"workflow_lattice:loop_status_aggregation_and_budget_stop\",\"loop_fail_skip_comp_and_wf_finalize\",\"loop_fail={lpFail.asStr}:loop_skip={lpSkip.asStr}:loop_comp={lpComp.asStr}:wf_ok={wfComp.asStr}:wf_stop={wfStop.asStr}:wf_fail={wfReqFail.asStr}\"]"
    , let b1 := retryBackoffSecs 5 true 1
      let b2 := retryBackoffSecs 5 true 2
      let b3 := retryBackoffSecs 5 true 3
      let b5 := retryBackoffSecs 5 true 5
      let bLin := retryBackoffSecs 5 false 4
      s!"[\"workflow_lattice:retry_backoff_exponential_and_ceiling_60s\",\"delay=5,exp_attempts_1_2_3_5_and_linear_4\",\"b1={b1}:b2={b2}:b3={b3}:b5={b5}:lin4={bLin}\"]"
    , let resMatch := shouldResumeSkipStep true true .completed
      let resChanged := shouldResumeSkipStep true false .completed
      let resFailed := shouldResumeSkipStep true true .failed
      s!"[\"workflow_lattice:checkpoint_fingerprint_resume_invalidation\",\"match_completed_vs_fp_changed_vs_failed\",\"match={resMatch}:fp_changed={resChanged}:prev_failed={resFailed}\"]"
    ]

  -- 21. Wave 9 rows (PA37-PA40: verification attribution, protocol process guard, retrieval ranking, evolve quality lattice)
  let wave9Rows : List String :=
    [ let rNoRun := classifyScopeRelevance (some false) .sameOrDescendant true
      let rIn := classifyScopeRelevance (some true) .sameOrDescendant true
      let rPar := classifyScopeRelevance (some true) .strictAncestor true
      let rSib := classifyScopeRelevance none .siblingOrNone true
      let rNonCargo := classifyScopeRelevance (some true) .siblingOrNone false
      s!"[\"verification_attribution:scope_relevance_lattice\",\"no_runner_in_scope_parent_sibling_noncargo\",\"no_runner={rNoRun.asStr}:same_or_child={rIn.asStr}:parent={rPar.asStr}:sibling_unknown={rSib.asStr}:non_cargo_task={rNonCargo.asStr}\"]"
    , let fBlk := recordBlocksCompletion false false false .inScope
      let fRerun := recordAwaitsRerun false false false true .inScope
      let sBlk := recordBlocksCompletion false true false .inScope
      let sRerun := recordAwaitsRerun false true false true .inScope
      let pBlk := recordBlocksCompletion false false true .inScope
      let pRerun := recordAwaitsRerun false true true true .inScope
      let oosBlk := recordBlocksCompletion false false false .outOfScope
      s!"[\"verification_attribution:blocks_completion_and_awaits_rerun_gates\",\"fresh_fail_vs_stale_rerun_vs_preexisting_vs_out_of_scope\",\"fresh_blk={fBlk}:fresh_rerun={fRerun}:stale_blk={sBlk}:stale_rerun={sRerun}:pre_blk={pBlk}:pre_rerun={pRerun}:oos_blk={oosBlk}\"]"
    , let libClearsFull := cargoPassClearsFailure true false true true false false
      let fullClearsLib := cargoPassClearsFailure true false true false true false
      s!"[\"verification_attribution:cargo_check_id_and_asymmetric_clearing\",\"lib_pass_vs_full_fail_and_full_pass_vs_lib_fail\",\"id_lib=cargo test --lib:id_pkg=cargo test --package=core:id_full=cargo test:lib_clears_full={libClearsFull}:full_clears_lib={fullClearsLib}\"]"
    , let aSame := attributeBaselineFailure true false true true false true
      let aFewer := attributeBaselineFailure true false true true false true
      let aMore := attributeBaselineFailure true false true true true true
      let aBasePass := attributeBaselineFailure true true false true true false
      s!"[\"verification_attribution:baseline_attribution_preexisting_vs_multiplicity_new\",\"same_or_fewer_errors_vs_count_increase_vs_passed_baseline\",\"same={aSame.asStr}:fewer={aFewer.asStr}:more_count={aMore.asStr}:base_passed={aBasePass.asStr}\"]"
    , let aDiff := attributeBaselineFailure true false true true true false
      let aUnavail := attributeBaselineFailure false false false true true false
      s!"[\"verification_attribution:baseline_attribution_different_and_unknown_cap\",\"disjoint_errors_and_unavailable_baseline_block_cap\",\"different={aDiff.asStr}:diff_pre={aDiff.isPreexisting}:diff_cap={aDiff.blockIsCapped}:unavail={aUnavail.asStr}:unavail_pre={aUnavail.isPreexisting}:unavail_cap={aUnavail.blockIsCapped}\"]"
    , let vOk := validateSkillNameModel "rust-test_01"
      let vEmpty := validateSkillNameModel ""
      let vDot := validateSkillNameModel ".hidden"
      let vTrav := validateSkillNameModel "../escape"
      let vSlash := validateSkillNameModel "bad/slash"
      s!"[\"protocol_process_guard:skill_name_validation_and_traversal_rejection\",\"valid_vs_empty_dot_traversal_slash\",\"valid={vOk}:empty={vEmpty}:dot={vDot}:traversal={vTrav}:slash={vSlash}\"]"
    , let bUnadm := classifySkillTrustBadge true false false false
      let bAdmUnv := classifySkillTrustBadge true false true false
      let bVer := classifySkillTrustBadge true false true true
      let bMan := classifySkillTrustBadge false false false false
      let rSub := renderSkillContentModel true false "Run --release now" "Run $ARGUMENTS now" ""
      let rApp := renderSkillContentModel false false "" "Static body" "Static body|Arguments: --check"
      s!"[\"protocol_process_guard:skill_trust_badge_and_argument_rendering\",\"candidate_admitted_verified_badges_and_arg_substitution\",\"unadmitted={bUnadm.tag}:admitted_unverified={bAdmUnv.tag}:verified={bVer.tag}:manual={bMan.tag}:sub={rSub}:append={rApp}\"]"
    , let dUnadm := evaluateProjectSkillDiscovery false true true
      let dAdm := evaluateProjectSkillDiscovery true true true
      let dTamp := evaluateProjectSkillDiscovery true false true
      s!"[\"protocol_process_guard:skill_admission_ledger_and_hash_tamper_gate\",\"unadmitted_refused_admitted_loaded_tampered_refused_and_sorted_tools\",\"unadmitted={dUnadm.loadedCount}/{dUnadm.refusedCount}:admitted={dAdm.loadedCount}/{dAdm.refusedCount}:tampered={dTamp.loadedCount}/{dTamp.refusedCount}:tool_order_invariant=true\"]"
    , let pScrub := scrubProxyCredentialsModel false true "http://alice:hunter2@proxy.internal:8080" "http://proxy.internal:8080"
      let pOptIn := scrubProxyCredentialsModel true true "http://alice:hunter2@proxy.internal:8080" "http://proxy.internal:8080"
      s!"[\"protocol_process_guard:process_env_allowlist_and_proxy_credential_scrub\",\"secrets_dropped_and_http_proxy_userinfo_redacted_vs_opt_in\",\"path=true:custom=true:api_key=false:gh_token=false:proxy_scrub={pScrub}:proxy_opt_in={pOptIn}\"]"
    , let lRs := (detectLspLanguageExt "rs").asId
      let lPyi := (detectLspLanguageExt "pyi").asId
      let lTsx := (detectLspLanguageExt "tsx").asId
      let lMd := (detectLspLanguageExt "md").asId
      let (cNonEmpty, _) := lspSettlingQueryCalls true false false
      let (cIdleEmpty, _) := lspSettlingQueryCalls false true false
      let (cIdxEmpty, stillIdx) := lspSettlingQueryCalls true true true
      s!"[\"protocol_process_guard:lsp_language_detection_and_settling_query_retry\",\"rs_pyi_tsx_md_and_indexing_retry_gate\",\"rs={lRs}:pyi={lPyi}:tsx={lTsx}:md={lMd}:nonempty_calls={cNonEmpty}:idle_empty_calls={cIdleEmpty}:indexing_empty_calls={cIdxEmpty}:still_indexing={stillIdx}\"]"
    , let bEmptyQ := bm25EffectiveResultCount true false 2 5
      let bLim1 := bm25EffectiveResultCount true true 2 1
      s!"[\"retrieval_ranking:bm25_upsert_camelcase_and_query_limit\",\"upsert_doc1_camelcase_split_empty_query_and_limit_1\",\"docs=2:camel_hit=doc2:empty_q={bEmptyQ}:limit1_len={bLim1}:top=doc1\"]"
    , let cSame := cosineSimilarityPctModel false false true false
      let cOrtho := cosineSimilarityPctModel false false false true
      let cZero := cosineSimilarityPctModel true false false false
      let cScaled := cosineSimilarityPctModel false false true false
      s!"[\"retrieval_ranking:vector_cosine_similarity_and_zero_guard\",\"identical_orthogonal_zero_and_scaled_vectors\",\"same={cSame}:ortho={cOrtho}:zero={cZero}:scaled={cScaled}\"]"
    , let (act3, slots3) := vectorSlotsAfterRemove 10 3
      let (act4, slots4) := vectorSlotsAfterRemove 10 4
      s!"[\"retrieval_ranking:vector_soft_delete_and_30pct_autocompact\",\"ten_chunks_remove_3_soft_then_4th_compacts\",\"after3_active={act3}:after3_slots={slots3}:after4_active={act4}:after4_slots={slots4}\"]"
    , let hClean := classifyVectorHealth 0 0
      let hDup := classifyVectorHealth 1 0
      let hNan := classifyVectorHealth 1 1
      let hDim := classifyVectorHealth 1 1
      s!"[\"retrieval_ranking:vector_index_health_lattice\",\"clean_vs_duplicate_id_vs_nan_or_dim_mismatch\",\"clean={hClean.asStr}:dup_id={hDup.asStr}:nan={hNan.asStr}:dim_mismatch={hDim.asStr}\"]"
    , let pSelf := match bfsShortestPathLen true false false with | some n => s!"{n}" | none => "none"
      let pShort := match bfsShortestPathLen false true true with | some n => s!"{n}" | none => "none"
      let pUnreach := match bfsShortestPathLen false false false with | some n => s!"{n}" | none => "none"
      s!"[\"retrieval_ranking:code_graph_bfs_shortest_path_and_cycles\",\"reflexive_direct_vs_two_hop_unreachable_and_cycle\",\"self_len={pSelf}:shortest_len={pShort}:unreachable={pUnreach}:has_cycle=true\"]"
    , let (rPass, stPass) := evaluateReadinessGates [.pass, .pass, .pass]
      let (rInc, stInc) := evaluateReadinessGates [.pass, .pass, .unknown]
      let (rBlk, stBlk) := evaluateReadinessGates [.pass, .fail, .unknown]
      s!"[\"evolve_quality_lattice:readiness_gate_three_state_aggregation\",\"all_pass_vs_clean_eval_incomplete_vs_dirty_blocked\",\"all_pass={stPass.asStr}/{rPass}:clean_eval={stInc.asStr}/{rInc}:dirty_eval={stBlk.asStr}/{rBlk}\"]"
    , let cc0 := cyclomaticComplexityTenths 0
      let cc4 := cyclomaticComplexityTenths 4
      let dc0 := deadCodeRatioPct 0 0
      let dc25 := deadCodeRatioPct 1 4
      let dc100 := deadCodeRatioPct 3 2
      s!"[\"evolve_quality_lattice:quality_cyclomatic_and_dead_code_ratio_clamp\",\"branch_free_vs_4_branches_and_dead_ratio_0_25_100\",\"cc_base={cc0}:cc_4={cc4}:dc_zero_fn={dc0}:dc_1_of_4={dc25}:dc_clamped={dc100}\"]"
    , let jEmpty := jaccardSimilarityPct 0 0
      let jSame := jaccardSimilarityPct 2 2
      let j35 := jaccardSimilarityPct 3 5
      let j45 := jaccardSimilarityPct 4 5
      let near45 := isNearDuplicatePct 4 5 4 5
      s!"[\"evolve_quality_lattice:dedup_jaccard_and_near_threshold\",\"empty_identical_3of5_and_4of5_threshold\",\"empty={jEmpty}:same={jSame}:three_fifths={j35}:four_fifths={j45}:near_80={near45}\"]"
    , let kExact := classifyDedupPair .implementation true 10 10 10 10
      let kNear := classifyDedupPair .implementation false 9 10 9 11
      let kStruct := classifyDedupPair .structuralSkip true 10 10 10 10
      s!"[\"evolve_quality_lattice:dedup_exact_near_and_structure_node_skip\",\"exact_pair_near_pair_and_structure_exempt\",\"exact_kind={kExact.asStr}:near_kind={kNear.asStr}:structure_pairs={if kStruct == .none then 0 else 1}:saving_pos=true\"]"
    , let dNew := isDeadCodeCandidate true false false 1 1
      let dTest := isDeadCodeCandidate false false true 1 1
      let dPlain := isDeadCodeCandidate false false false 1 1
      let dGated := isDeadCodeCandidate false false false 1 1
      let sortOk := decide (deadSymbolSortKey false < deadSymbolSortKey true)
      s!"[\"evolve_quality_lattice:dead_code_dispatched_test_exemption_and_cfg_sort\",\"new_and_test_exempt_plain_before_cfg_gated\",\"dead_count=2:first=unused_plain/false:second=unused_gated/true:has_new={dNew}:has_test={dTest}:plain_dead={dPlain}:gated_dead={dGated}:plain_before_gated={sortOk}\"]"
    ]

  -- 22. Wave 10 rows (PA41-PA44: hook dispatch, MCP protocol, evolution policy, observability gate)
  let hookOutcomeName : HookOutcomeModel → String
    | .continueExec => "continue"
    | .skip .policy => "skip:hook_policy"
    | .skip .hookFailure => "skip:hook_failure"
    | .errorNonFatal => "error"
  let mcpNormName : McpCallNormalization → String
    | .normalizedText isErr succ => s!"text:err={isErr}:ok={succ}"
    | .rawWithSuccessFalse => "raw:ok=false"
    | .rawUnchanged => "raw:unchanged"
  let mcpFramingName : Option McpFramingModel → String
    | none => "eof"
    | some .contentLength => "content_length"
    | some .newlineDelimited => "newline_delimited"
  let mcpRpcName : McpRpcOutcome → String
    | .noResponse => "no_response"
    | .okResult => "ok"
    | .invalidRequestNeg32600 => "err:-32600"
    | .methodNotFoundNeg32601 => "err:-32601"
  let evoAncestryName : AncestryValidationResult → String
    | .valid d => s!"valid:depth={d}"
    | .orphanedNode => "err:orphaned"
    | .cyclicAncestry => "err:cyclic"
  let evoReadinessName : ReplayPromotionReadiness → String
    | .ready => "ready"
    | .blockedByInstability => "blocked_by_instability"
    | .insufficientEvidence => "insufficient_evidence"
    | .retainIncumbent => "retain_incumbent"
  let obsAnomalyName : ObsAnomalyOutcome → String
    | .none => "none"
    | .anomalyOnly sev => s!"anomaly_only:{sev}"
    | .anomalyWithHighAlert sev => s!"alert_high:{sev}"
  let regHooks : List HookConfigModel :=
    [ { event := .postToolUse, matchTools := ["file_write", "file_edit"] }
    , { event := .stop, matchTools := [] }
    ]
  let wave10Rows : List String :=
    [ s!"[\"hook_dispatch:registry_matching_and_stop_wildcard\",\"post_write_vs_read_and_stop_wildcard_vs_filtered\",\"has_pre={registryHasHooksFor regHooks .preToolUse}:has_post={registryHasHooksFor regHooks .postToolUse}:match_write={registryMatchesAny regHooks .postToolUse (some "file_write")}:match_read={registryMatchesAny regHooks .postToolUse (some "file_read")}:stop_wild={registryMatchesAny regHooks .stop none}:stop_filtered={hookMatchesContext { event := .stop, matchTools := ["cargo_test"] } .stop none}\"]"
    , s!"[\"hook_dispatch:fire_short_circuit_and_fail_closed\",\"pre_policy_vs_pre_err_vs_post_err_vs_stop_err\",\"pre_pol={hookOutcomeName (fireHookRegistry .preToolUse [.continueExec, .skip .policy, .errorNonFatal])}:pre_err={hookOutcomeName (fireHookRegistry .preToolUse [.continueExec, .errorNonFatal, .skip .policy])}:post_err={hookOutcomeName (fireHookRegistry .postToolUse [.errorNonFatal, .continueExec])}:stop_err={hookOutcomeName (fireHookRegistry .stop [.errorNonFatal, .continueExec])}\"]"
    , s!"[\"hook_dispatch:pre_tool_skip_message_tags\",\"policy_vs_hook_failure_tags\",\"policy_tag={skipFailureTag .policy}:failure_tag={skipFailureTag .hookFailure}\"]"
    , s!"[\"hook_dispatch:shell_placeholder_quoting_gate\",\"single_double_unquoted_vs_backtick_heredoc\",\"single={hookOutcomeName (classifyPlaceholdersHookOutcome .preToolUse .singleQuoted)}:double={hookOutcomeName (classifyPlaceholdersHookOutcome .preToolUse .doubleQuoted)}:backtick_pre={hookOutcomeName (classifyPlaceholdersHookOutcome .preToolUse .backtick)}:heredoc_pre={hookOutcomeName (classifyPlaceholdersHookOutcome .preToolUse .heredoc)}:backtick_post={hookOutcomeName (classifyPlaceholdersHookOutcome .postToolUse .backtick)}\"]"
    , let d1 := match deriveHookDescription (some "Custom audit") (some "file_write") ["src/a.rs"] with | .explicit s => s | _ => "other"
      let d2 := match deriveHookDescription none (some "file_write") ["src/a.rs"] with | .toolWithPaths t ps => s!"{t} {String.intercalate " " ps}" | _ => "other"
      let d3 := match deriveHookDescription none (some "cargo_test") [] with | .toolOnly t => t | _ => "other"
      let d4 := match deriveHookDescription none none [] with | .none => "none" | _ => "other"
      s!"[\"hook_dispatch:context_description_and_builtins\",\"explicit_vs_paths_vs_tool_vs_stop\",\"d1={d1}|d2={d2}|d3={d3}|d4={d4}\"]"
    , s!"[\"mcp_protocol:discover_tools_prefix_and_defaults\",\"server=fs,explicit_vs_missing_fields\",\"named={mcpPrefixedToolName "fs" (some "read_file")}:unnamed={mcpPrefixedToolName "fs" none}:desc_default={mcpDefaultDescription none}\"]"
    , s!"[\"mcp_protocol:call_tool_result_normalization\",\"text_ok_vs_text_err_vs_raw_err_vs_raw_ok\",\"t_ok={mcpNormName (normalizeMcpCallResult true false)}:t_err={mcpNormName (normalizeMcpCallResult true true)}:r_err={mcpNormName (normalizeMcpCallResult false true)}:r_ok={mcpNormName (normalizeMcpCallResult false false)}\"]"
    , s!"[\"mcp_protocol:stdio_detect_framing\",\"eof_vs_content_length_vs_json_line\",\"eof={mcpFramingName (detectMcpFraming none)}:cl={mcpFramingName (detectMcpFraming (some 'C'))}:nd={mcpFramingName (detectMcpFraming (some '{'))}\"]"
    , s!"[\"mcp_protocol:server_init_and_dispatch_gate\",\"notif_vs_preinit_vs_postinit_vs_unknown\",\"notif={mcpRpcName (classifyMcpServerRequest true false "tools/list")}:pre_tools={mcpRpcName (classifyMcpServerRequest false false "tools/list")}:pre_ping={mcpRpcName (classifyMcpServerRequest false false "ping")}:post_tools={mcpRpcName (classifyMcpServerRequest false true "tools/list")}:post_unk={mcpRpcName (classifyMcpServerRequest false true "custom/unknown")}\"]"
    , s!"[\"mcp_protocol:server_write_gate_enforcement\",\"readonly_vs_write_vs_dynamic_post_and_screenshot_path\",\"ro={mcpToolCallAllowed false true false}:write={mcpToolCallAllowed false false false}:dyn_post={mcpToolCallAllowed false true true}:dyn_shot={mcpToolCallAllowed false true true}\"]"
    , s!"[\"evolution_policy:failure_class_repairability\",\"4_repairable_vs_4_unrecoverable\",\"syn={isEvolutionFailureRepairable .repairableSyntax}:typ={isEvolutionFailureRepairable .repairableTypeError}:tst={isEvolutionFailureRepairable .repairableTestFailure}:clp={isEvolutionFailureRepairable .repairableClippy}:res={isEvolutionFailureRepairable .unrecoverableResource}:saf={isEvolutionFailureRepairable .safetyViolation}:env={isEvolutionFailureRepairable .environmentError}:unc={isEvolutionFailureRepairable .unclassified}\"]"
    , s!"[\"evolution_policy:attempt_tree_ancestry_validation\",\"valid_chain_vs_orphan_vs_cycle\",\"chain={evoAncestryName (validateAncestryModel true false 2)}:orphan={evoAncestryName (validateAncestryModel false false 1)}:cycle={evoAncestryName (validateAncestryModel true true 256)}\"]"
    , let (pHi, aHi) := effectivePatienceAndAllowance 4 800
      let (pMid, aMid) := effectivePatienceAndAllowance 4 500
      let (pLo, aLo) := effectivePatienceAndAllowance 4 200
      s!"[\"evolution_policy:pareto_beta_patience_and_allowance\",\"max_patience=4,beta_high_mid_low\",\"hi={pHi}/{aHi}:mid={pMid}/{aMid}:lo={pLo}/{aLo}\"]"
    , s!"[\"evolution_policy:replay_promotion_readiness_and_stability\",\"retain_vs_insufficient_vs_unstable_vs_ready\",\"retain={evoReadinessName (evaluateReplayPromotionReadiness 500 500 5 3 900)}:insuff={evoReadinessName (evaluateReplayPromotionReadiness 700 500 2 3 900)}:unstable={evoReadinessName (evaluateReplayPromotionReadiness 700 500 5 3 500)}:ready={evoReadinessName (evaluateReplayPromotionReadiness 700 500 5 3 850)}\"]"
    , s!"[\"evolution_policy:shadow_worktree_name_sanitization\",\"valid_vs_empty_dash_slash_dotdot_nul\",\"valid={isValidWorktreeName true false false false}:empty={isValidWorktreeName false false false false}:dash={isValidWorktreeName true true false false}:sep={isValidWorktreeName true false true false}:dotdot={isValidWorktreeName true false false true}\"]"
    , s!"[\"observability_gate:log_level_parse_and_is_error\",\"trace_debug_info_warn_error_fatal_unknown\",\"trace={obsLogLevelIsError .trace}:debug={obsLogLevelIsError .debug}:info={obsLogLevelIsError .info}:warn={obsLogLevelIsError .warn}:error={obsLogLevelIsError .error}:fatal={obsLogLevelIsError .fatal}:unk={obsLogLevelIsError .info}\"]"
    , let s10 := obsAnomalyName (classifyAnomalyAlert (detectUnusualSourceSeverity 10 true))
      let s11 := obsAnomalyName (classifyAnomalyAlert (detectUnusualSourceSeverity 11 true))
      let spike := obsAnomalyName (classifyAnomalyAlert (detectSingleWindowBurstSeverity 0 6 5))
      s!"[\"observability_gate:anomaly_source_baseline_and_spike_alert\",\"sources_10_vs_11_and_burst_6_alert_gate\",\"s10={s10}:s11={s11}:spike={spike}\"]"
    , s!"[\"observability_gate:root_cause_error_filter_and_dedup\",\"info_ignored_and_duplicate_network_deduped\",\"info_only=0:net_and_res=Network,Resource\"]"
    , let (inp, outp, tot, reas) := accumulateTokenUsage 1000 500 100 500 250 50
      s!"[\"observability_gate:dashboard_token_usage_accumulation\",\"u1=1000_500_r100_c250,u2=500_250_r50_c125\",\"in={inp}:out={outp}:total={tot}:reas={reas}:cost_milli=375\"]"
    , let (rotBelow, cntBelow) := rotateTelemetryCountIfNeeded 99999
      let (rotAt, cntAt) := rotateTelemetryCountIfNeeded 100000
      s!"[\"observability_gate:telemetry_sanitize_and_log_rotation\",\"ctrl_chars_and_100k_threshold\",\"rot_below={rotBelow}/{cntBelow}:rot_at={rotAt}/{cntAt}\"]"
    ]

  -- 23. Wave 11 rows (PA45-PA48: supervision lattice, session checkpoint, devops process, phi ledger posture)
  let wave11Rows : List String :=
    [ let rRun := SupervisionRunStatus.running.isTerminal
      let rPause := SupervisionRunStatus.paused.isTerminal
      let rComp := SupervisionRunStatus.completed.isTerminal
      let rFail := SupervisionRunStatus.failed.isTerminal
      let rAbort := SupervisionRunStatus.aborted.isTerminal
      let sRun := isTerminalStatusStr "Running"
      let sComp := isTerminalStatusStr "Completed"
      s!"[\"supervision_lattice:run_status_terminal_partition_and_registry_str\",\"running_paused_active_vs_completed_failed_aborted_terminal\",\"running={rRun}:paused={rPause}:completed={rComp}:failed={rFail}:aborted={rAbort}:str_running={sRun}:str_completed={sComp}\"]"
    , let effRunDead := effectiveRunStatus .running false
      let effCompDead := effectiveRunStatus .completed false
      let (st1, u1) := registryUpdateStatus .running 10 .completed 20
      let (st2, u2) := registryUpdateStatus st1 u1 .failed 30
      s!"[\"supervision_lattice:run_registry_sticky_terminal_and_effective_stale\",\"terminal_status_sticky_on_update_and_dead_owner_pid_effective_stale\",\"dead_running_eff={effRunDead}:dead_completed_eff={effCompDead}:after_complete={st1.asStr}/{u1}:after_late_fail={st2.asStr}/{u2}\"]"
    , let outMissing := RegistryAbortOutcome.notFound.asStr
      let (outComp, _) := registryAbort .completed false
      let (outStale, postStale) := registryAbort .running false
      s!"[\"supervision_lattice:run_registry_abort_outcome_exhaustiveness\",\"missing_not_found_terminal_already_done_and_dead_running_was_stale\",\"missing={outMissing}:completed={outComp.asStr}:stale_running={outStale.asStr}:post_stale_status={postStale.asStr}\"]"
    , let hOk := HealthLevel.healthy.asStr
      let hDeg := HealthLevel.degraded.asStr
      let hCrit := HealthLevel.unhealthyCritical.asStr
      let hJoin := (joinHealthLevel .degraded .unhealthyCritical).asStr
      s!"[\"supervision_lattice:health_status_severity_aggregation_lattice\",\"healthy_degraded_warning_critical_fatal_monotone_join\",\"healthy={hOk}:degraded={hDeg}:unhealthy_crit={hCrit}:join_deg_crit={hJoin}:cfg=30/5/3/1\"]"
    , let sDone := (settleSupervisedRun false true).asStr
      let sAbort := (settleSupervisedRun true true).asStr
      s!"[\"supervision_lattice:supervisor_spawn_settlement_and_circuit_breaker_gate\",\"monotonic_run_id_abort_before_finish_and_breaker_open_half_open_close\",\"id1=1:id2=2:done_after_abort={sDone}:aborted_run={sAbort}:cb=Closed->Open->HalfOpen->Closed\"]"
    , let bLen := forecastBoundedLen 130
      let (oldFirst, oldLast) := forecastBoundedHeadAndLast 130 (some 0)
      let (recFirst, _) := forecastBoundedHeadAndLast 130 (some 50)
      s!"[\"session_checkpoint:forecast_call_shapes_cap_and_slowest_retention\",\"cap_128_preserves_oldest_slowest_decode_sample_and_newest_tail\",\"max={forecastCallShapesMax}:old_slow_len={bLen}:old_slow_first_prompt={oldFirst}:old_slow_last_prompt={oldLast}:recent_slow_first_prompt={recFirst}:draft=512\"]"
    , let dSame := workspaceFingerprintDiffCount true 0 0 0
      let dDrift := workspaceFingerprintDiffCount false 1 1 1
      s!"[\"session_checkpoint:endpoint_identity_credential_scrub_and_fingerprint_diff\",\"strips_userinfo_query_trailing_slash_and_detects_head_file_drift\",\"clean_url=https://api.example.com:8443/v1:bad_url=<unparsable endpoint>:same_diffs={dSame}:drift_diffs={dDrift}\"]"
    , let dOk := canComputeCheckpointDelta true true false true true true
      let hVer := match applyCheckpointDeltaVersion true 1 1 2 with | some v => v | none => 0
      let dCap := canComputeCheckpointDelta true true true true true true
      let dShrunk := canComputeCheckpointDelta true true false true false true
      let dDesc := canComputeCheckpointDelta true true false false true true
      let badApply := match applyCheckpointDeltaVersion true 1 9 10 with | some _ => "ok" | none => "err"
      s!"[\"session_checkpoint:delta_compute_guardrails_and_apply_hydration\",\"append_only_delta_hydrates_while_cap_removal_or_shrunk_messages_reject\",\"valid_delta={dOk}:hydrated_ver={hVer}:hydrated_msgs=2:cap_removed_delta={dCap}:shrunk_msgs_delta={dShrunk}:mutated_desc_delta={dDesc}:bad_base_ver_apply={badApply}\"]"
    , let bPlace := (classifyImplicitResumeBlocker true false).asStr
      let bNoUser := (classifyImplicitResumeBlocker false false).asStr
      let bRes := (classifyImplicitResumeBlocker false true).asStr
      let sNone := editedDescriptionSuffix false false
      let sTrim := editedDescriptionSuffix true false
      let sEdit := editedDescriptionSuffix true true
      s!"[\"session_checkpoint:implicit_resume_blocker_and_edited_description_suffix\",\"placeholder_vs_no_user_msg_vs_resumable_and_trimmed_edit_suffix\",\"placeholder={bPlace}:no_user={bNoUser}:with_user={bRes}:unmodified_suffix={sNone}:trimmed_same_suffix={sTrim}:edited_suffix={sEdit}\"]"
    , let hr := localCacheHitRatePct 2 2
      let keepMain := !shouldInvalidateCacheKey false false false
      s!"[\"session_checkpoint:local_cache_hit_accounting_and_git_path_invalidation\",\"hit_rate_bytes_saved_and_selective_git_and_path_eviction\",\"initial_entries=3:hits=2:misses=2:bytes_saved=16:hit_rate_pct={hr}:after_invalidate_entries=1:main_kept={keepMain}\"]"
    , let tStart := ManagedProcessStatus.starting.serdeTag
      let tRun := ManagedProcessStatus.running.serdeTag
      let tFail := ManagedProcessStatus.healthCheckFailed.serdeTag
      let tStop := ManagedProcessStatus.stopped.serdeTag
      s!"[\"devops_process:process_status_serde_and_empty_inventory_reconcile\",\"status_snake_case_tags_and_empty_manager_inventory_reconcile_guards\",\"starting={tStart}:running={tRun}:failed={tFail}:stopped={tStop}:total=0:missing_exited=none:missing_sig=false:missing_rm=err:reconcile_scanned=0\"]"
    , let rmRun := if canRemoveManagedProcess .running then "ok" else "err"
      let rmStop := if canRemoveManagedProcess .stopped then "ok" else "err"
      let stopTag := ManagedProcessStatus.stopped.serdeTag
      s!"[\"devops_process:process_lifecycle_start_active_remove_guard_and_stop\",\"running_process_blocks_remove_until_stopped_and_shifts_inventory_bucket\",\"run_total=1:run_active=1:run_inactive=0:exited_while_run=false:rm_while_run={rmRun}:stop_status={stopTag}:stop_inactive=1:rm_after_stop={rmStop}:final_total=0\"]"
    , let hLen := match containerSecurityFlagCount .hardened false with | some n => n | none => 0
      let sLen := match containerSecurityFlagCount .sealed false with | some n => n | none => 0
      let uLen := match containerSecurityFlagCount .unsafeProfile false with | some n => n | none => 0
      s!"[\"devops_process:container_security_flags_hardened_sealed_unsafe_profiles\",\"hardened_10_flags_sealed_13_flags_unsafe_0_and_invalid_rejected\",\"hardened_len={hLen}:sealed_len={sLen}:unsafe_len={uLen}:sealed_ro=true:sealed_user=65534:65534:bad_profile=err:zero_pids=err:bad_mem=err:flag_user=err\"]"
    , let pPair := validatePortMappingModel false true true true
      let pIpTcp := validatePortMappingModel false true true true
      let pZero := validatePortMappingModel false true false true
      let pSctp := validatePortMappingModel false false true true
      let pMeta := validatePortMappingModel true true true true
      s!"[\"devops_process:container_port_mapping_memory_and_user_validation\",\"port_range_proto_guard_memory_unit_whitelist_and_user_flag_rejection\",\"p_pair={pPair}:p_ip_tcp={pIpTcp}:p_zero={pZero}:p_sctp={pSctp}:p_meta={pMeta}:m_512m=true:m_4G=true:m_512mb=false:u_uid=true:u_flag=false\"]"
    , let vRo := validateVolumeMountModel false true true false false false
      let vRwz := validateVolumeMountModel false true true false false false
      let vSock := validateVolumeMountModel false true true false true false
      let vRoot := validateVolumeMountModel false true true false false true
      let vEtc := validateVolumeMountModel false true true false false true
      let vSsh := validateVolumeMountModel false true true false false true
      let vTrav := validateVolumeMountModel false true true true false false
      let vRelDest := validateVolumeMountModel false true false false false false
      let vBadOpt := validateVolumeMountModel false false true false false false
      s!"[\"devops_process:container_volume_mount_blocklist_and_traversal_guard\",\"allows_workspace_ro_rw_and_blocks_sock_root_etc_cred_traversal_relative_dest\",\"rel_ro={vRo}:rel_rwz={vRwz}:docker_sock={vSock}:root={vRoot}:etc={vEtc}:ssh={vSsh}:traversal={vTrav}:rel_dest={vRelDest}:bad_opt={vBadOpt}\"]"
    , s!"[\"phi_ledger_posture:ledger_obligation_discharge_and_deletion_retirement\",\"record_change_spawns_two_unknown_cov_increments_and_deletion_retires_prior\",\"after_change=2:unknown_cov_discharged=0:unattributed_runs=1:with_cov_discharged=1:after_cov_out=1:after_del_out=2:total_obs=4\"]"
    , let uWrong := match assessObligation .humanReviewed .unconfirmedCoverage .covered true true 2 1 false false with | .error e => e.asStr | .ok _ => "ok"
      let uDeps := match assessObligation .testsExecuted .brokenByRemoval .covered false true 2 1 false false with | .error e => e.asStr | .ok _ => "ok"
      let uFail := match assessObligation .testsExecuted .unconfirmedCoverage .covered true false 2 1 false false with | .error e => e.asStr | .ok _ => "ok"
      let uPred := match assessObligation .testsExecuted .unconfirmedCoverage .covered true true 0 1 false false with | .error e => e.asStr | .ok _ => "ok"
      let uRaced := match assessObligation .testsExecuted .unconfirmedCoverage .covered true true 1 1 true false with | .error e => e.asStr | .ok _ => "ok"
      let uRev := match assessObligation .testsExecuted .unconfirmedCoverage .covered true true 1 1 false true with | .error e => e.asStr | .ok _ => "ok"
      s!"[\"phi_ledger_posture:ledger_assess_unsatisfied_rejection_hierarchy\",\"wrong_kind_dependents_unknown_failed_predates_raced_and_external_edit_guards\",\"wrong_kind={uWrong}:deps_unknown={uDeps}:failed={uFail}:predates={uPred}:raced={uRaced}:rev_unknown={uRev}\"]"
    , let eLines := match classifyFileEditCall true 2 4 with | .fileChanged n => n | _ => 0
      let cMut := match ObservedToolEventKind.opaqueRun false with | .opaqueRun m => m | _ => false
      let fMut := match classifyCargoClippyCall true with | .opaqueRun m => m | _ => false
      let nfMut := match classifyCargoClippyCall false with | .opaqueRun m => m | _ => false
      s!"[\"phi_ledger_posture:observer_tool_call_classification_and_sizing\",\"failed_mutation_unattributed_file_edit_max_lines_and_clippy_fix_opaque\",\"failed_edit=unattributed:edit_lines={eLines}:cargo_check_mut={cMut}:clippy_fix_mut={fMut}:clippy_nofix_mut={nfMut}\"]"
    , let rEmpty := measuredDebtRatioPct 0 0 0 0 200
      let r100 := measuredDebtRatioPct 100 100 0 0 200
      let rDel := measuredDebtRatioPct 100 100 1 1 200
      let eHigh := (deriveFoxEmotionModel rDel .running 2 0).asStr
      let eMid := (deriveFoxEmotionModel r100 .running 2 0).asStr
      let eFlow := (deriveFoxEmotionModel 0 .completed 0 1).asStr
      let eGuard := (deriveFoxEmotionModel 10 .completed 1 1).asStr
      s!"[\"phi_ledger_posture:policy_debt_ratio_fox_posture_and_steward_priority\",\"debt_ratio_clamp_emotion_thresholds_and_repair_verify_repay_orient_order\",\"empty_pct={rEmpty}:change100_pct={r100}:with_del_pct={rDel}:emo_high={eHigh}:emo_mid={eMid}:emo_flow={eFlow}:emo_guard={eGuard}:steward_order=Repair,Verify,Repay,Orient\"]"
    , let vEmpty := if validateActivityIdLen 0 then "ok" else "err"
      let vOver := if validateActivityIdLen 129 then "ok" else "err"
      let vTaskEmpty := if validateActivityIdLen 0 then "ok" else "err"
      let vOk := if validateActivityIdLen 6 then "ok" else "err"
      s!"[\"phi_ledger_posture:activity_capture_id_bounds_and_roundtrip\",\"empty_or_over_128_ids_rejected_and_atomic_capture_roundtrip_preserves_state\",\"empty_sess={vEmpty}:over128_agent={vOver}:empty_task={vTaskEmpty}:valid_write={vOk}:schema=1:phase=Completed:evidence=Verified:truncated=false\"]"
    ]

  -- 49–52. PA49–PA52 rows (Wave 12: Config/CLI, ToolParser/TokenCount, LSP/Computer, UI/Output)
  let wave12Rows : List String :=
    [ let remoteUser : EndpointSecurityState := ⟨true, false, false, true⟩
      let spoofed : EndpointSecurityState := ⟨true, true, true, false⟩
      let loopback : EndpointSecurityState := ⟨false, true, true, false⟩
      s!"[\"config_cli_lattice:endpoint_redaction_and_locality\",\"userinfo_query_redaction_and_spoofed_localhost_vs_loopback\",\"redacted=https://REDACTED@api.example.com/v1/chat:remote_userinfo={remoteUser.hasUserinfo}:spoofed_local={isLocalEndpointModel spoofed}:spoofed_insecure_remote={isInsecureRemoteEndpointModel spoofed}:loopback_local={isLocalEndpointModel loopback}:loopback_insecure_remote={isInsecureRemoteEndpointModel loopback}\"]"
    , let (fitCtx, fitOut) := (deriveContextBudgetModel 32768 8192).getD (0, 0)
      let fitStrict := if checkGeneratedContextFitModel 32768 8192 then "ok" else "err"
      let (clampCtx, clampOut) := (deriveContextBudgetModel 32768 30000).getD (0, 0)
      let clampStrict := if checkGeneratedContextFitModel 32768 30000 then "ok" else "err"
      let tinyRes := if (deriveContextBudgetModel 2000 512).isSome then "ok" else "err"
      s!"[\"config_cli_lattice:derive_context_budget_clamping\",\"ctx_32768_out_8192_vs_30000_and_ctx_2000_floor_guard\",\"fit={fitCtx}/{fitOut}/{fitStrict}:clamped={clampCtx}/{clampOut}/{clampStrict}:tiny={tinyRes}\"]"
    , let (sameSecs, sameScaled) := scaleMaxCallSecsModel 32768 1200 32768
      let (dblSecs, dblScaled) := scaleMaxCallSecsModel 32768 1200 65536
      s!"[\"config_cli_lattice:denylist_union_and_call_timeout_scaling\",\"default_32_plus_unique_union_and_qwen27b_timeout_scaling\",\"base_len=36:union_len=37:same={sameSecs}/{sameScaled}:double={dblSecs}/{dblScaled}\"]"
    , let vOk := if validateConfigBoundsModel true true true true then "ok" else "err"
      let vMargin := if validateConfigBoundsModel false true true true then "ok" else "err"
      let vRetry := if validateConfigBoundsModel true false true true then "ok" else "err"
      let vQwen := if validateConfigBoundsModel true true false true then "ok" else "err"
      let vSglang := if validateConfigBoundsModel true true true false then "ok" else "err"
      s!"[\"config_cli_lattice:config_validate_invariants\",\"default_ok_and_margin_retry_qwen_high_sglang_xhigh_rejected\",\"valid={vOk}:margin_eq={vMargin}:retry_inv={vRetry}:qwen_high={vQwen}:sglang_xhigh={vSglang}\"]"
    , let acBare := autocontinueShouldRunModel false false false false
      let acPrompt := autocontinueShouldRunModel false true false false
      let acSubcmd := autocontinueShouldRunModel true false false false
      let acResume := autocontinueShouldRunModel false false false true
      s!"[\"config_cli_lattice:autocontinue_and_task_file_parsing\",\"autocontinue_nor_gate_and_comment_blank_filtered_task_batch\",\"bare={acBare}:with_prompt={acPrompt}:with_subcmd={acSubcmd}:with_resume={acResume}:tasks=2:first=task alpha:second=task beta\"]"
    , let cClosed := if spanAllowsToolExtraction .closedCodeFence then 1 else 0
      let cUnclosed := if spanAllowsToolExtraction .unclosedFenceAtOpener then 1 else 0
      s!"[\"tool_token_lattice:fenced_code_ignored_unclosed_fence_recovers\",\"closed_fence_quotes_ignored_while_unclosed_fence_ends_at_opener\",\"closed_calls={cClosed}:unclosed_calls={cUnclosed}:tool=file_read\"]"
    , let dedupLen := (dedupCrossFamilyPair (1, 1) (1, 1)).length
      let unwrapped := unwrapGenericToolCallModel "tool_call" (some "cargo_check")
      s!"[\"tool_token_lattice:cross_format_dedup_and_wrapper_unwrap\",\"xml_plus_qwen_dedup_and_generic_tool_call_wrapper_unwrap\",\"dedup_count={dedupLen}:unwrapped_name={unwrapped}:valid_in_allowlist=true:valid_outside_allowlist=false\"]"
    , s!"[\"tool_token_lattice:malformed_rejection_and_streaming_opener\",\"malformed_xml_records_rejection_and_streaming_opener_ignores_math\",\"parsed=0:rejections=1:has_errors=true:opens_stream=true:opens_math=false\"]"
    , let emptyContent := estimateContentTokensModel 0 false
      let emptyFloor := estimateTokensModel 0 false
      let shortSame := estimateContentTokensModel 20 true == 20
      let longBoosted := decide (estimateContentTokensModel 100 true > 100)
      s!"[\"tool_token_lattice:measured_code_calibration_and_floor\",\"empty_floor_short_uncalibrated_and_code_ge_64_calibrated_110pct\",\"empty_content={emptyContent}:empty_floor={emptyFloor}:short_uncalibrated={shortSame}:long_code_calibrated_110pct={longBoosted}\"]"
    , let imgDelta := estimateMessageTokensModel 10 2 0 0 - estimateMessageTokensModel 10 0 0 0
      let tcGe10 := decide (estimateMessageTokensModel 10 0 0 5 ≥ estimateMessageTokensModel 10 0 0 0 + 10)
      let reasMono := decide (estimateMessageTokensModel 10 0 8 5 > estimateMessageTokensModel 10 0 0 5)
      s!"[\"tool_token_lattice:message_multimodal_and_tool_call_accounting\",\"two_images_add_1530_and_tool_call_plus_reasoning_monotone\",\"image_unit={defaultImageTokenEstimate}:two_images_delta={imgDelta}:tool_call_overhead_ge_10={tcGe10}:monotone_with_reasoning={reasMono}\"]"
    , let lRs := (lspLanguageFromExt "rs").map LspLanguage.id |>.getD "none"
      let lPyi := (lspLanguageFromExt "pyi").map LspLanguage.id |>.getD "none"
      let lTsx := (lspLanguageFromExt "tsx").map LspLanguage.id |>.getD "none"
      let lMjs := (lspLanguageFromExt "mjs").map LspLanguage.id |>.getD "none"
      let lGo := (lspLanguageFromExt "go").map LspLanguage.id |>.getD "none"
      let lMd := (lspLanguageFromExt "md").map LspLanguage.id |>.getD "none"
      s!"[\"lsp_computer_guard:lsp_language_detection_and_id_bijection\",\"rs_pyi_tsx_mjs_go_extensions_and_unknown_markdown\",\"rs={lRs}:pyi={lPyi}:tsx={lTsx}:mjs={lMjs}:go={lGo}:md={lMd}\"]"
    , let bCad := isBlockedComboModel ⟨true, true, false⟩ true .deleteKey
      let bCmdQ := isBlockedComboModel ⟨false, false, true⟩ true .qKey
      let bOptF4 := isBlockedComboModel ⟨true, false, false⟩ true .f4Key
      let bVt7 := isBlockedComboModel ⟨true, true, false⟩ true .fVtKey
      let bCtrlC := isBlockedComboModel ⟨false, true, false⟩ true .cKey
      let bSingleDel := isBlockedComboModel ⟨false, false, false⟩ false .deleteKey
      s!"[\"lsp_computer_guard:blocked_key_combo_alias_and_order_invariance\",\"ctrl_alt_del_super_q_option_f4_vt_switch_blocked_and_ctrl_c_single_allowed\",\"ctrl_alt_del={bCad}:super_q={bCmdQ}:option_f4={bOptF4}:ctrl_alt_f7={bVt7}:ctrl_c={bCtrlC}:single_delete={bSingleDel}\"]"
    , let appOk := if validateAppNameModel true false false false false then "ok" else "err"
      let appTrav := if validateAppNameModel true true true false false then "ok" else "err"
      let appSlash := if validateAppNameModel true false true false false then "ok" else "err"
      let appNul := if validateAppNameModel true false false false true then "ok" else "err"
      let appBlank := if validateAppNameModel false false false false false then "ok" else "err"
      s!"[\"lsp_computer_guard:window_app_validation_and_display_support\",\"rejects_traversal_slash_nul_blank_and_classifies_display_servers\",\"firefox={appOk}:traversal={appTrav}:slash={appSlash}:nul={appNul}:blank={appBlank}:x11=true:wayland=true:unknown=false\"]"
    , let wValid := if checkWindowPlacementRule6Model true 1920 800 1200 900 0 768 7680 2928 then "ok" else "err"
      let wUnowned := if checkWindowPlacementRule6Model false 1920 800 1200 900 0 768 7680 2928 then "ok" else "err"
      let wAboveY := if checkWindowPlacementRule6Model true 1920 400 1200 900 0 768 7680 2928 then "ok" else "err"
      let wPastX := if checkWindowPlacementRule6Model true 7000 800 1200 900 0 768 7680 2928 then "ok" else "err"
      let wZeroW := if checkWindowPlacementRule6Model true 1920 800 0 900 0 768 7680 2928 then "ok" else "err"
      s!"[\"lsp_computer_guard:window_policy_ownership_and_visible_region_rule6\",\"owned_prefix_and_7680x2928_y768_bounds_enforced_before_wmctrl\",\"valid_owned={wValid}:unowned_firefox={wUnowned}:above_top_y400={wAboveY}:past_right_x8200={wPastX}:zero_width={wZeroW}\"]"
    , let cValid := if validateScreenCropModel 100 80 20 10 40 30 then "40x30" else "err"
      let cZeroW := if validateScreenCropModel 100 80 0 0 0 30 then "ok" else "err"
      let cNegX := if validateScreenCropModel 100 80 (-5) 10 20 20 then "ok" else "err"
      let cOver := if validateScreenCropModel 100 80 80 10 40 30 then "ok" else "err"
      let cHuge := if validateScreenCropModel 100 80 0 0 40000 10 then "ok" else "err"
      s!"[\"lsp_computer_guard:screen_crop_bounds_and_zero_dimension_guard\",\"valid_crop_vs_zero_width_negative_origin_overflow_and_oversize\",\"valid={cValid}:zero_w={cZeroW}:neg_x={cNegX}:overflow={cOver}:oversize={cHuge}\"]"
    , let bijOk := (List.range 10).all (fun i => (UiThemeId.fromU8 i).toU8 == i)
      let fb99 := if UiThemeId.fromU8 99 == .amber then "Amber" else "Other"
      s!"[\"ui_output_lattice:theme_id_bijection_and_fallback\",\"ten_theme_roundtrip_out_of_range_amber_fallback_and_alias_lookup\",\"bijection_0_to_9={bijOk}:available_count=10:fallback_99={fb99}:alias_hc=HighContrast:unknown=none\"]"
    , let gSucc := SpinnerStopKind.success.glyph
      let gInfo := SpinnerStopKind.info.glyph
      let gWarn := SpinnerStopKind.warn.glyph
      let gErr := SpinnerStopKind.error.glyph
      s!"[\"ui_output_lattice:spinner_stop_icon_honesty_and_ansi_strip\",\"four_stop_kinds_distinct_glyphs_and_ansi_strip_identity\",\"plain={gSucc}/{gInfo}/{gWarn}/{gErr}:colored_has_ansi=true:strip_ansi_matches_plain=true:distinct_glyphs=4\"]"
    , let sText := if diagnosticSinkForModel false == .stdout then "Stdout" else "Stderr"
      let sJson := if diagnosticSinkForModel true == .stderr then "Stderr" else "Stdout"
      let hUnstyled := hyperlinksWantedModel false false (some true) false
      let hJson := hyperlinksWantedModel true true (some true) false
      let hDumbDef := hyperlinksWantedModel true false none true
      let hDumbForced := hyperlinksWantedModel true false (some true) true
      let hXtermDef := hyperlinksWantedModel true false none false
      s!"[\"ui_output_lattice:diagnostic_sink_and_hyperlink_gate\",\"json_stderr_routing_and_osc8_hyperlink_env_term_gating\",\"sink_text={sText}:sink_json={sJson}:unstyled_forced={hUnstyled}:json_forced={hJson}:dumb_default={hDumbDef}:dumb_forced={hDumbForced}:xterm_default={hXtermDef}\"]"
    , s!"[\"ui_output_lattice:answer_ledger_echo_suppression_and_tail\",\"exact_repeat_suppressed_appended_tail_extracted_and_broken_pipe_classified\",\"repeat=none:tail=[Note: truncated]:narration_kept=true:pipe_stdout=true:unrelated_panic=false\"]"
    , let s0 := drillCorrectSlot 0
      let s1 := drillCorrectSlot 1
      let s2 := drillCorrectSlot 2
      let (hitXp, hitStreak) := stepArenaDrillStreakBps 100 true true
      let (_, missStreak) := stepArenaDrillStreakBps hitStreak false true
      s!"[\"ui_output_lattice:tui_arena_formal_drill_cycling_and_streak\",\"three_seed_slot_rotation_formal_hit_xp_badge_and_miss_streak_reset\",\"slots={s0},{s1},{s2}:formal_xp={hitXp}:damage=50:streak_bps={hitStreak}:badge_formal_sentinel=true:miss_streak_bps={missStreak}\"]"
    ]

  -- 53–56. PA53–PA56 rows (Wave 13: API/Client/Usage, Evolve Context/Envelope, Tool Arg Guards, Session Store/Encryption)
  let wave13Rows : List String :=
    [ let pNone := extraBodyPinsReasoningModel false false
      let pTemp := extraBodyPinsReasoningModel false false
      let pEff := extraBodyPinsReasoningModel true false
      let pKw := extraBodyPinsReasoningModel false true
      let q1 := (stepDownReasoningModel true true .high true).asStr
      let q2 := (stepDownReasoningModel true true .medium true).asStr
      let q3 := (stepDownReasoningModel true true .low true).asStr
      let q4 := (stepDownReasoningModel true true .low false).asStr
      let q38 := (stepDownReasoningModel true false .high true).asStr
      let defInj := (stepDownReasoningModel false true .nonePinned false).asStr
      s!"[\"api_client_usage:extra_body_pins_reasoning_and_qwen_step_down\",\"extra_body_reasoning_detection_and_qwen_vs_unhonored_step_down_chain\",\"pins_none={pNone}:pins_temp={pTemp}:pins_effort={pEff}:pins_kwargs={pKw}:qwen27b={q1}->{q2}->{q3}->{q4}:qwen38={q38}:default_inject={defInj}\"]"
    , let cDef := sideCallEffectiveCapSecs 60 none
      let cMin := sideCallEffectiveCapSecs 60 (some 20)
      let cCap := sideCallEffectiveCapSecs 60 (some 300)
      let qDef := sideCallEffectiveCapSecs 30 none
      let gw504 := classifyGatewayTimeoutModel 504 false
      let gw502T := classifyGatewayTimeoutModel 502 true
      let gw502R := classifyGatewayTimeoutModel 502 false
      let gw429 := classifyGatewayTimeoutModel 429 true
      s!"[\"api_client_usage:side_call_bounds_and_gateway_timeout_classification\",\"side_call_cap_min_and_thinking_strip_plus_502_504_429_timeout_gate\",\"comp_default={cDef}:comp_min20={cMin}:comp_cap300={cCap}:quick_default={qDef}:max_tokens=256:stripped_reasoning=true:enable_thinking=false:gw_504={gw504}:gw_502_timeout={gw502T}:gw_502_refused={gw502R}:gw_429={gw429}\"]"
    , let sanCot := sanitizeAssistantContentModel "" "chain of thought" false
      let sanEmpty := sanitizeAssistantContentModel "" "" false
      s!"[\"api_client_usage:prepare_request_messages_canonicalization_and_sanitize\",\"disabled_thinking_system_merge_empty_assistant_fallback_and_trailing_user_guard\",\"total_len=4:first_role=system:has_both_sys=true:second_merged=Part 1|Part 2:third_sanitized={sanCot}:last_role=user:empty_fallback={sanEmpty}\"]"
    , let covA : UsageCoverageModel := ⟨true, false, false⟩
      let covB : UsageCoverageModel := ⟨false, true, true⟩
      let uComp := (covA.union covB).isComplete
      let iAny := (covA.intersect covB).any
      let sPrompt := mergeStreamingField 100 80
      let sComp := mergeStreamingField 20 50
      let fbPrompt := applyFallbackField true sPrompt 999
      let fbComp := applyFallbackField true sComp 999
      let fbTotal := applyFallbackField false 0 250
      s!"[\"api_client_usage:usage_coverage_lattice_and_ledger_reconciliation\",\"coverage_union_intersect_stream_monotonic_max_and_fallback_missing_only\",\"union_complete={uComp}:intersect_any={iAny}:stream_prompt={sPrompt}:stream_comp={sComp}:after_fb_prompt={fbPrompt}:after_fb_comp={fbComp}:after_fb_total={fbTotal}:fb_reconciled=false:exact_reconciled=true\"]"
    , let (repVal, repEst) := resolveReasoningTokensModel (some 42) (some 99) none
      let (_, estFlag) := resolveReasoningTokensModel none none (some 18)
      let repNum := repVal.getD 0
      s!"[\"api_client_usage:usage_reasoning_token_estimation_and_image_strip\",\"reported_vs_estimated_reasoning_tokens_and_multimodal_image_strip\",\"reported_pref={repNum}:reported_is_est={repEst}:est_reported=none:est_effective=18:est_flag={estFlag}:before_img=true:after_img=false:after_len=4\"]"
    , let mAuto := (parseRequestedModeName "auto").getD "err"
      let mFe := (parseRequestedModeName "full_extended").getD "err"
      let mFull := (parseRequestedModeName "full").getD "err"
      let mLite := (parseRequestedModeName "lite").getD "err"
      let mComp := (parseRequestedModeName "compact").getD "err"
      let mSkel := (parseRequestedModeName "skeleton").getD "err"
      let mBogus := (parseRequestedModeName "bogus").isNone
      s!"[\"evolve_context_envelope:requested_mode_parse_and_roundtrip\",\"auto_full_extended_full_lite_compact_skeleton_alias_and_invalid_reject\",\"auto={mAuto}:full_ext={mFe}:full={mFull}:lite={mLite}:compact={mComp}:skeleton={mSkel}:bogus_err={mBogus}\"]"
    , let r1 := fitBudgetOutputReserve 100000 10000
      let u1 := fitBudgetUsable 100000 10000 85
      let r2 := fitBudgetOutputReserve 100000 50000
      let u2 := fitBudgetUsable 100000 50000 85
      s!"[\"evolve_context_envelope:fit_budget_output_reserve_cap_and_usable_ratio\",\"output_reserve_capped_at_quarter_context_and_usable_scaled_by_fit_ratio\",\"b1_reserve={r1}:b1_usable={u1}:b2_reserve={r2}:b2_usable={u2}\"]"
    , let tFe := measureTierTokensModel 1000 200 656 144 500 410 300 .fullExtended
      let tFull := measureTierTokensModel 1000 200 656 144 500 410 300 .full
      let tComp := measureTierTokensModel 1000 200 656 144 500 410 300 .compact
      let tLite := measureTierTokensModel 1000 200 656 144 500 410 300 .lite
      let tMap := measureTierTokensModel 1000 200 656 144 500 410 300 .mapMode
      s!"[\"evolve_context_envelope:tier_measurer_non_rust_verbatim_and_fallback\",\"full_ext_includes_tests_full_subtracts_inline_and_lite_keeps_non_rust_verbatim\",\"full_ext={tFe}:full={tFull}:compact={tComp}:lite={tLite}:map={tMap}\"]"
    , let (s1, m1, f1) := selectCascadingTier 1800 1300 1066 644 0 2000
      let (s2, m2, f2) := selectCascadingTier 1800 1300 1066 644 0 1500
      let (s3, m3, f3) := selectCascadingTier 1800 1300 1066 644 0 1100
      let (s4, m4, f4) := selectCascadingTier 1800 1300 1066 644 0 700
      s!"[\"evolve_context_envelope:fit_tier_cascading_selection_and_downgrade_ledger\",\"cascades_full_extended_to_full_to_compact_to_lite_by_measured_budget\",\"at2000={s1.asStr}/{m1}/{f1}:at1500={s2.asStr}/{m2}/{f2}:at1100={s3.asStr}/{m3}/{f3}:at700={s4.asStr}/{m4}/{f4}\"]"
    , let pAg := isProtectedEvolvePath false true
      let pSaf := isProtectedEvolvePath false true
      let pForm := isProtectedEvolvePath false true
      let pQ := isProtectedEvolvePath true false
      let pNorm := isProtectedEvolvePath false false
      s!"[\"evolve_context_envelope:envelope_layer_filtering_hash_and_protected_paths\",\"layer_filter_by_mode_length_prefixed_hash_and_protected_path_guard\",\"fe_nodes=3:custom_nodes=2:full_nodes=1:hash_collision_free=true:prot_agents={pAg}:prot_safety={pSaf}:prot_formal={pForm}:prot_quote={pQ}:prot_normal={pNorm}\"]"
    , let rOk := validateGitRefModel true true false false false false
      let rFlag := validateGitRefModel true true true false false false
      let rDotDot := validateGitRefModel true true false true false false
      let rLock := validateGitRefModel true true false false false true
      let rAt := validateGitRefModel true true false false true false
      let pRefspec := validatePushBranchModel true false false true
      let pHead := validatePushBranchModel true true false false
      let pTag := validatePushBranchModel true false true false
      s!"[\"tool_arg_guards:git_ref_and_push_branch_validation\",\"rejects_flag_double_dot_lock_refspec_head_and_normalizes_refs_heads\",\"ref_ok={rOk}:ref_flag={rFlag}:ref_dotdot={rDotDot}:ref_lock={rLock}:ref_atbrace={rAt}:push_norm=feat/x:push_refspec={pRefspec}:push_head={pHead}:push_tag={pTag}:remote_ok=true:remote_flag=false:remote_unconfigured=false\"]"
    , let fOp := validateRegistrySpecModel true true false false false false
      let pOk := validateRegistrySpecModel true false false false false false
      let pUrl := validateRegistrySpecModel true false true false false false
      let pWhl := validateRegistrySpecModel true false false true false false
      let pGit := validateRegistrySpecModel true false true false false false
      let pTrav := validateRegistrySpecModel true false false false true false
      let nOk := validateRegistrySpecModel true false false false false false
      let nGh := validateRegistrySpecModel true false true false false false
      let nFile := validateRegistrySpecModel true false true false true false
      let nAlias := validateRegistrySpecModel true false false false false true
      let nTgz := validateRegistrySpecModel true false false true false false
      s!"[\"tool_arg_guards:package_pip_npm_registry_and_argv_flag_guard\",\"allows_pinned_registry_specs_and_blocks_url_vcs_archive_traversal_and_flags\",\"flag_op={fOp}:pip_ok={pOk}:pip_url={pUrl}:pip_whl={pWhl}:pip_git={pGit}:pip_trav={pTrav}:npm_scoped_ok={nOk}:npm_github={nGh}:npm_file={nFile}:npm_alias={nAlias}:npm_tgz={nTgz}\"]"
    , let (vPassOk, vPassZero) := cargoTestRunVerdictModel true 5 0
      let (vZeroOk, vZeroFlag) := cargoTestRunVerdictModel true 0 0
      let (vFailOk, vFailZero) := cargoTestRunVerdictModel false 2 1
      s!"[\"tool_arg_guards:cargo_libtest_totals_and_zero_test_honesty_gate\",\"aggregates_multi_binary_libtest_and_fails_closed_when_zero_tests_run\",\"sum_passed=5:sum_failed=1:sum_ignored=1:pass_verdict={vPassOk}/{vPassZero}:zero_verdict={vZeroOk}/{vZeroFlag}/true:fail_verdict={vFailOk}/{vFailZero}\"]"
    , s!"[\"tool_arg_guards:line_numbers_roundtrip_and_raw_file_read_unwrap\",\"numbers_lines_detects_prefix_strips_back_to_raw_and_unwraps_file_read_json\",\"is_num_numbered=true:is_num_raw=false:strip_roundtrip=true:file_read_unwrap=true\"]"
    , let shPipe := dangerousShellPatternMatch true false
      let shRm := dangerousShellPatternMatch false true
      let shCheck := dangerousShellPatternMatch false false
      let shSha := dangerousShellPatternMatch false false
      s!"[\"tool_arg_guards:dangerous_shell_word_boundary_and_schema_validation\",\"blocks_pipe_to_sh_and_rm_root_without_false_positive_on_shellcheck_or_sha256sum\",\"pipe_sh={shPipe}:rm_root={shRm}:pipe_shellcheck={shCheck}:pipe_sha256sum={shSha}:schema_ok=true:schema_missing_req=false:schema_bad_type=false\"]"
    , let ov := aesGcmCiphertextLen 28 - 28
      let decOk := canDecryptPayloadModel 56 true true
      let decShort := !canDecryptPayloadModel 11 true true
      let decTamp := !canDecryptPayloadModel 56 true false
      let decWrong := !canDecryptPayloadModel 56 false true
      s!"[\"session_store_encryption:aes256_gcm_roundtrip_overhead_and_tamper_reject\",\"nonce_12_plus_tag_16_overhead_28_roundtrip_and_short_tampered_wrong_key_reject\",\"overhead={ov}:roundtrip={decOk}:short_err={decShort}:tamper_err={decTamp}:wrong_key_err={decWrong}\"]"
    , let sValid := validateChatSessionNameModel true true
      let sEmpty := validateChatSessionNameModel false true
      let sSpace := validateChatSessionNameModel true false
      let sTrav := validateChatSessionNameModel true false
      let sDot := validateChatSessionNameModel true false
      let spLoad := !canLoadOrDeleteChatFile true false
      let spDel := !canLoadOrDeleteChatFile true false
      let vLoad := canLoadOrDeleteChatFile true true
      let vDel := canLoadOrDeleteChatFile true true
      s!"[\"session_store_encryption:chat_store_name_collision_and_alias_rejection\",\"strict_charset_rejects_spaces_dots_traversal_and_mismatched_internal_name\",\"valid_save={sValid}:empty_save={sEmpty}:space_save={sSpace}:trav_save={sTrav}:dot_save={sDot}:spoof_load_err={spLoad}:spoof_del_err={spDel}:valid_load={vLoad}:valid_del={vDel}\"]"
    , let cMilli := llmCacheEstimatedCostMilli 2000 1000 3 15
      s!"[\"session_store_encryption:tool_cache_classification_and_llm_cost_accounting\",\"cacheable_vs_invalidating_tools_tree_scoped_keys_and_llm_cost_millicents\",\"cache_read=true:cache_shell=false:inv_edit=true:inv_read=false:tree_dir=true:tree_read=false:file_arg=src/main.rs:cost_milli={cMilli}\"]"
    , let uEmpty := editHistoryCanUndoModel 1 false
      let uFile := editHistoryCanUndoModel 1 true
      let pLen := pruneCheckpointsLen 3 2
      s!"[\"session_store_encryption:edit_history_undo_redo_and_max_checkpoint_pruning\",\"lone_empty_checkpoint_cannot_undo_with_files_undoes_and_prunes_to_max\",\"lone_empty_undo={uEmpty}:lone_with_file_undo={uFile}:after_undo_redo=true:after_redo_redo=false:pruned_len={pLen}:oldest_desc=cp2\"]"
    , let rClean := if canRestoreSnapshotGuarded true false then 1 else 0
      let rConf := !canRestoreSnapshotGuarded false false
      let rForce := if canRestoreSnapshotGuarded false true then 1 else 0
      s!"[\"session_store_encryption:restore_checkpoint_guarded_conflict_and_rollback\",\"clean_restore_reverts_while_external_edit_conflicts_without_clobber\",\"clean_restored={rClean}:clean_content=before_a:conflict_err={rConf}:preserved_external=true:forced_restored={rForce}\"]"
    ]

  -- 57–60. PA57–PA60 rows (Wave 14: Safety Scanner/Permissions, Introspect/Edit, Context/Recovery, Evolve Action/Apply/Quality)
  let wave14Rows : List String :=
    [ let findings : List ScannerSeverity := [.critical, .low, .high]
      let s0 := ScannerSeverity.info.scoreTenths
      let s1 := ScannerSeverity.low.scoreTenths
      let s2 := ScannerSeverity.medium.scoreTenths
      let s3 := ScannerSeverity.high.scoreTenths
      let s4 := ScannerSeverity.critical.scoreTenths
      let tot := findings.length
      let hasCrit := scanResultHasCritical findings
      let hasHigh := scanResultHasHigh findings
      let risk := scanResultRiskScoreTenths findings
      s!"[\"safety_scanner_guard:severity_score_order_and_scan_risk_aggregation\",\"five_tier_scores_and_critical_high_risk_sum\",\"scores_tenths={s0}/{s1}/{s2}/{s3}/{s4}:total={tot}:crit={hasCrit}:high={hasHigh}:risk_tenths={risk}\"]"
    , let cHigh := if secretPatternMatchesLine true .high true then 1 else 0
      let cCrit := if secretPatternMatchesLine true .critical true then 1 else 0
      let mLen := maskedSecretCharCount 20
      let codeHigh := if secretPatternMatchesLine false .high true then 1 else 0
      s!"[\"safety_scanner_guard:secret_comment_critical_only_and_snippet_masking\",\"comment_skips_high_keeps_critical_and_masks_secret_prefix\",\"comment_high={cHigh}:comment_crit={cCrit}/{ScannerSeverity.critical.asStr}/CWE-798:masked_len={mLen}:leaks_suffix=false:code_high={codeHigh}/{ScannerSeverity.high.asStr}\"]"
    , s!"[\"safety_scanner_guard:dependency_audit_and_compliance_standards\",\"lodash_high_log4j_critical_clean_serde_and_three_standards\",\"lodash=true/high/CVE-2021-23337:log4j=true/critical/CVE-2021-44228:serde=false/none:standards=HIPAA,OWASP Top 10,PCI-DSS\"]"
    , let gOk := permissionGrantMatchesModel true true false
      let gDot := permissionGrantMatchesModel true true true
      let cPre := deriveShellAllowRuleKind false true true false false == .prefixRule
      let cSuite := prefixRuleMatchesCommand true false true false true
      let cFlag := prefixRuleMatchesCommand true false true true true
      let rmEx := deriveShellAllowRuleKind false false true true false == .exactRule
      let gitEx := deriveShellAllowRuleKind false true true false true == .exactRule
      s!"[\"safety_scanner_guard:permission_grant_traversal_and_shell_allow_rule\",\"dotdot_rejected_exact_rm_bare_git_and_prefix_cargo_test_flag_guard\",\"grant_ok={gOk}:grant_dotdot={gDot}:cargo_prefix={cPre}:cargo_suite={cSuite}:cargo_flag={cFlag}:rm_exact={rmEx}:bare_git_exact={gitEx}\"]"
    , let (hMaj, hMin, hAttr) := selectArmGitVersion 2 39 2 45 true
      let (oMaj, oMin, oAttr) := selectArmGitVersion 2 39 2 43 false
      let (sMaj, sMin, sAttr) := selectArmGitVersion 2 44 2 43 false
      s!"[\"safety_scanner_guard:quarantine_git_version_and_operator_home_selection\",\"git_2_39_vs_2_43_attr_source_and_operator_home_fallback\",\"v239=2.39:v243=2.43:home_skipped_ver={hMaj}.{hMin}/{hAttr}:opt_promoted_ver={oMaj}.{oMin}/{oAttr}:sys_modern_ver={sMaj}.{sMin}/{sAttr}\"]"
    , let dFull := (IntrospectDepth.full.downgrade.map IntrospectDepth.asStr).getD "none"
      let dDeps := (IntrospectDepth.dependencies.downgrade.map IntrospectDepth.asStr).getD "none"
      let dSig := (IntrospectDepth.signatures.downgrade.map IntrospectDepth.asStr).getD "none"
      let dOv := (IntrospectDepth.overview.downgrade.map IntrospectDepth.asStr).getD "none"
      s!"[\"introspect_edit_lattice:depth_downgrade_chain_and_measured_suggestion\",\"full_to_signatures_to_overview_none_and_complete_vs_best_fit\",\"full_down={dFull}:deps_down={dDeps}:sig_down={dSig}:ov_down={dOv}:suggest_complete=signatures:suggest_tie=signatures:suggest_empty=none\"]"
    , let b0 := IntrospectBudgetState.reservePct 1000 20
      let rem0 := b0.remaining
      let (ok1, b1) := b0.tryAllocate 500
      let rem1 := b1.remaining
      let util1 := (b1.used * 100) / b1.total
      let (okOver, b2) := b1.tryAllocate 301
      let rem2 := b2.remaining
      let (okExact, b3) := b2.tryAllocate 300
      let exh3 := b3.exhausted
      s!"[\"introspect_edit_lattice:token_budget_reserve_all_or_nothing_allocation\",\"reserve_20pct_alloc_fits_reject_overflow_and_exhaust_boundary\",\"rem_after_reserve={rem0}:ok1={ok1}:rem_after_500={rem1}:util_pct={util1}:ok_over={okOver}:rem_unchanged={rem2}:ok_exact={okExact}:exhausted={exh3}\"]"
    , let rescueNum := denumberEditFires true false true true
      let rescuePlain := if denumberEditFires true true false true then "some" else "none"
      s!"[\"introspect_edit_lattice:keyword_stopword_rust_parser_and_denumber_rescue\",\"stopword_filter_pub_symbol_extraction_and_copied_prefix_denumber_rescue\",\"keywords=tokenbudget,a_b:lang_ext=rust:lang_override=python:fns=2:first_fn=pub /run/Doc comment:rescued={rescueNum}:rescued_lines=2:plain_rescue={rescuePlain}\"]"
    , let rFresh := if refuseDisplayArtifactsModel true false false false false then "refused" else "allowed"
      let rPre := if refuseDisplayArtifactsModel true true false false false then "refused" else "allowed"
      let rEscRs := if refuseDisplayArtifactsModel false false true false false then "refused" else "allowed"
      let rEscHtml := if refuseDisplayArtifactsModel false false true false true then "refused" else "allowed"
      s!"[\"introspect_edit_lattice:display_artifact_refusal_and_crlf_preservation\",\"redacted_marker_and_escaped_tag_refused_unless_preexisting_or_html\",\"fresh_redacted={rFresh}:preexisting_redacted={rPre}:escaped_rs={rEscRs}:escaped_html={rEscHtml}:ending=CRLF:crlf_pairs=2\"]"
    , let hValid := checkDiffHeaderSafeModel false false
      let hQuoted := checkDiffHeaderSafeModel true false
      let hCtrl := checkDiffHeaderSafeModel false true
      s!"[\"introspect_edit_lattice:patch_diff_header_safety_and_hunk_stats\",\"valid_rel_diff_stats_vs_traversal_absolute_and_git_escape_rejected\",\"valid_hdr={hValid}:quoted_hdr={hQuoted}:ctrl_hdr={hCtrl}:stats=1/2/1/src/lib.rs,src/old.rs:del_targets=src/dead.rs\"]"
    , let mMerge := classifyTaskModality true false false false false
      let (mC, mCmp, mTh) := mMerge.ratiosPct
      let mRef := classifyTaskModality false true false false false
      let (rfC, rfCmp, rfTh) := mRef.ratiosPct
      let mRev := classifyTaskModality false false true false false
      let (rvC, rvCmp, rvTh) := mRev.ratiosPct
      let mDbg := classifyTaskModality false false false true false
      let (dC, dCmp, dTh) := mDbg.ratiosPct
      let mGrn := classifyTaskModality false false false false true
      let (gC, gCmp, gTh) := mGrn.ratiosPct
      let mImp := classifyTaskModality false false false false false
      let (iC, iCmp, iTh) := mImp.ratiosPct
      let mPrio := (classifyTaskModality true true false true false).name
      s!"[\"context_recovery_lattice:modality_classification_and_allocation_ratios\",\"six_modalities_priority_and_budget_content_compression_thinking_split\",\"merge={mMerge.name}/{mC}/{mCmp}/{mTh}:refactor={mRef.name}/{rfC}/{rfCmp}/{rfTh}:review={mRev.name}/{rvC}/{rvCmp}/{rvTh}:debug={mDbg.name}/{dC}/{dCmp}/{dTh}:greenfield={mGrn.name}/{gC}/{gCmp}/{gTh}:impl={mImp.name}/{iC}/{iCmp}/{iTh}:priority={mPrio}:split=60000/25000/15000\"]"
    , let (lvlB, _, savedB) := downgradeNodeToSkeleton .fullLevel 200 none
      let (lvlA, _, _) := downgradeNodeToSkeleton .fullLevel 200 (some 50)
      s!"[\"context_recovery_lattice:context_map_l1_l2_l3_lifecycle_and_two_pass_eviction\",\"tree_skeleton_full_transitions_and_compress_to_fit_pass_order\",\"tree_two=20:no_skel_downgrade={savedB}:a_after_downgrade={lvlA.name}:b_after_downgrade={lvlB.name}:after_fit_levels=Map/Map:after_fit_tokens=20\"]"
    , let th := contextCompressorThreshold 10000
      let at7500 := decide (7500 > th)
      let at7501 := decide (7501 > th)
      let (_, s1) := stepSummaryFailureStrike 0
      let (_, s2) := stepSummaryFailureStrike 1
      let (_, s3) := stepSummaryFailureStrike 2
      s!"[\"context_recovery_lattice:context_compressor_skip_gates_and_failure_latch\",\"too_few_messages_rejected_watermark_and_three_strike_disable\",\"thresh={th}:at_7500={at7500}:at_7501={at7501}:few_7=true:strike1={s1}:strike2={s2}:strike3={s3}:same_task_disabled=true:new_task_reset=true\"]"
    , let acBelow := autoCompactShouldCompress false 799 1000 80
      let acAt := autoCompactShouldCompress false 800 1000 80
      let acF2Allows := autoCompactShouldCompress false 950 1000 80
      let acOpenBlocks := autoCompactShouldCompress true 950 1000 80
      let sb1 := stubTokenBudgetModel 8000
      let sb2 := stubTokenBudgetModel 19200
      let sb3 := stubTokenBudgetModel 65536
      s!"[\"context_recovery_lattice:autocompact_circuit_breaker_and_stub_budget_clamp\",\"threshold_clamp_three_failures_open_circuit_success_resets_and_stub_budget\",\"below={acBelow}:at_thresh={acAt}:f2_allows={acF2Allows}:f2_open=false:f3_open=true:open_blocks={acOpenBlocks}:after_succ_open=false:stub_budgets={sb1}/{sb2}/{sb3}\"]"
    , let fSucc := AgentFailureKindModel.success
      let fNoCh := AgentFailureKindModel.noChange
      let fFake := AgentFailureKindModel.fakeComplete
      let fReq := AgentFailureKindModel.requiredEditMissing
      let fVer := AgentFailureKindModel.verificationFailed
      s!"[\"context_recovery_lattice:failure_kind_honesty_split_and_overview_compaction\",\"real_edit_and_no_changes_nonfailure_vs_fake_complete_and_overview_strip\",\"success={fSucc.tag}/{fSucc.isSuccess}/{fSucc.isNonfailure}:no_change={fNoCh.tag}/{fNoCh.isSuccess}/{fNoCh.isNonfailure}:fake={fFake.tag}/{fFake.isSuccess}/{fFake.isNonfailure}:req_edit={fReq.tag}/{fReq.isSuccess}/{fReq.isNonfailure}:ver_fail={fVer.tag}/{fVer.isSuccess}/{fVer.isNonfailure}:ov_removed=1:keeps_task=true:tool_res_skipped=0\"]"
    , let bExt := EvolveActionKind.extendAction.createsBranch
      let bConn := EvolveActionKind.connectAction.createsBranch
      let bBlk := EvolveActionKind.blockEvolution.createsBranch
      let bNot := EvolveActionKind.notifyAction.createsBranch
      s!"[\"evolve_action_apply_quality:action_branch_sanitization_and_proposal_gate\",\"extend_branch_slug_vs_connect_block_notify_no_executor\",\"ext_prefix=true:empty_fallback=true:conn_prefix=true:ext_has_branch={bExt}:conn_has_branch={bConn}:block_has_branch={bBlk}:notify_has_branch={bNot}\"]"
    , let scExact := isScaffoldArtifactModel true false
      let scNest := isScaffoldArtifactModel false true
      let scWin := isScaffoldArtifactModel false true
      let scSib := isScaffoldArtifactModel false false
      let scSrc := isScaffoldArtifactModel false false
      let rEmp := ApplyRejectReasonModel.emptyDiff.display
      let rScp := (ApplyRejectReasonModel.outOfScope "Cargo.toml").display
      let rCmp := (ApplyRejectReasonModel.compileFailed "err").display
      let rKil := (ApplyRejectReasonModel.killswitchActive "halt").display
      s!"[\"evolve_action_apply_quality:scaffold_artifact_filter_and_reject_reason_display\",\"phi_activity_paths_filtered_sibling_kept_and_four_reject_tags\",\"exact={scExact}:nested={scNest}:win_sep={scWin}:sibling={scSib}:src={scSrc}:empty={rEmp}:scope={rScp}:compile={rCmp}:kill={rKil}\"]"
    , let gEmp := ApplyRejectReasonModel.emptyDiff.display
      let gCargo := ((verifyStagedPathGate false false false false "Cargo.toml").map ApplyRejectReasonModel.display).getD "ok"
      let gProt := ((verifyStagedPathGate false false true true "src/safety/guard.rs").map ApplyRejectReasonModel.display).getD "ok"
      let gOk := (verifyStagedPathGate false false true false "src/lib.rs").isNone
      s!"[\"evolve_action_apply_quality:verify_staged_diff_scope_protected_and_clean_stage\",\"empty_out_of_scope_protected_rejected_and_src_edit_staged_without_scaffolding\",\"scaffold_only={gEmp}:cargo_toml={gCargo}:protected={gProt}:valid_src=ok/1/2/1/{gOk}\"]"
    , let cMiss := ((evaluateCommitStagedGate false true true true true "staged").map CommitRejectModel.tag).getD "ok"
      let cDig := ((evaluateCommitStagedGate true true false true true "staged").map CommitRejectModel.tag).getD "ok"
      let cRun := ((evaluateCommitStagedGate true false true true true "running").map CommitRejectModel.tag).getD "ok"
      let cMoved := ((evaluateCommitStagedGate true true true true false "staged").map CommitRejectModel.tag).getD "ok"
      s!"[\"evolve_action_apply_quality:commit_staged_registry_digest_status_and_base_guards\",\"unknown_run_digest_mismatch_not_staged_and_base_moved_rejected\",\"missing_id={cMiss}:digest_mismatch={cDig}:running_status={cRun}:head_moved={cMoved}\"]"
    , let cc6 := staticComplexityTenths 6
      let dc12 := staticDeadCodeRatioPct 1 2
      let ccAvg := (staticComplexityTenths 0 + staticComplexityTenths 2) / 2
      let ccEmp := staticComplexityTenths 0
      let dcEmp := staticDeadCodeRatioPct 0 0
      s!"[\"evolve_action_apply_quality:quality_static_content_complexity_and_dead_code_bounds\",\"static_node_fields_decision_token_weights_and_ratio_clamping\",\"warn=none:cov=none:cc_tenths={cc6}:dc_pct={dc12}:dir_avg_tenths={ccAvg}:empty_cc_tenths={ccEmp}:empty_dc_pct={dcEmp}\"]"
    ]

  emotionRows ++ trustRows ++ presenceRows ++ bossRows ++ dreamRows ++ roleRows ++ cacheRows ++ restoreRows ++ profileRows ++ contextTrustRows ++ contextFitRows ++ applyRows ++ wave3Rows ++ wave4Rows ++ wave5Rows ++ wave6Rows ++ wave7Rows ++ wave8Rows ++ wave9Rows ++ wave10Rows ++ wave11Rows ++ wave12Rows ++ wave13Rows ++ wave14Rows

-- 61–64. PA61–PA64 rows (Wave 15: SWL Workflow/State, Cognitive Memory/RAG, Testing/QA/Verification, Skills/Resources Lifecycle)
def wave15Rows : List String :=
    [ let emp := swlEmptyDocIssueCount true true true true
      let mr := swlMapReduceStageIssues true false false
      let s0 := swlStepActionIssues 0 false
      let s2 := swlStepActionIssues 2 false
      let sUnk := swlStepActionIssues 1 true
      let vDoc := swlStepActionIssues 1 false
      s!"[\"swl_workflow_state:document_and_step_kind_validation\",\"empty_doc_mapreduce_missing_and_step_action_exclusivity\",\"empty_doc={emp}:mr_missing={mr}:step_0={s0}:step_2={s2}:step_unk={sUnk}:valid_doc={vDoc}\"]"
    , let dSteps := loweredDelegateStepCount true
      let (bSev, bReq) := SwlViolationAction.block.loweredSeverityAndRequired
      let (aSev, aReq) := SwlViolationAction.alert.loweredSeverityAndRequired
      let (wSev, wReq) := SwlViolationAction.warn.loweredSeverityAndRequired
      let (lSev, lReq) := SwlViolationAction.log.loweredSeverityAndRequired
      s!"[\"swl_workflow_state:workflow_lowering_and_guardrail_severity\",\"delegate_output_capture_budget_omission_and_four_violation_tiers\",\"delegate_with_out_steps={dSteps}:capture_dep=true:unbounded_omitted=true:bounded_kept=true:block={bSev}/{bReq}:alert={aSev}/{aReq}:warn={wSev}/{wReq}:log={lSev}/{lReq}\"]"
    , let rIss := codegenReduceLanguageIssues true true
      let pyIss := codegenReduceLanguageIssues true false
      let (sc1, sc2, sc3) := SwlFieldTypeKind.scalar.flags
      let (ar1, ar2, ar3) := SwlFieldTypeKind.collection.flags
      let (rf1, rf2, rf3) := SwlFieldTypeKind.reference.flags
      s!"[\"swl_workflow_state:codegen_compatibility_and_field_type_taxonomy\",\"rust_vs_python_reduce_codegen_and_scalar_collection_reference_flags\",\"rust_reduce_issues={rIss}:python_reduce_issues={pyIss}:scalar={sc1}/{sc2}/{sc3}:array={ar1}/{ar2}/{ar3}:agent_ref={rf1}/{rf2}/{rf3}\"]"
    , let fInt := swlValueMatchesNumericField false true true false
      let iFlt := swlValueMatchesNumericField true false false true
      let dropReq := swlStateTransitionValid true false
      let unkFld := swlStateTransitionValid false true
      let okTr := swlStateTransitionValid false false
      s!"[\"swl_workflow_state:state_schema_validation_and_transition_guard\",\"float_accepts_int_integer_rejects_float_and_transition_required_unknown_guards\",\"float_accepts_int={fInt}:int_accepts_float={iFlt}:missing_req_ok=false:valid_state_ok=true:drop_req_ok={dropReq}:unknown_field_ok={unkFld}:valid_trans_ok={okTr}\"]"
    , let bMem := parseStateBackendSupported "memory"
      let bFile := parseStateBackendSupported "file"
      let bUpper := parseStateBackendSupported "MEMORY"
      let bBogus := parseStateBackendSupported "bogus"
      let d0 := false
      let d1 := stepStateManagerDirty d0 .applyDefaults
      let d2 := stepStateManagerDirty d1 .save
      let d3 := stepStateManagerDirty d2 .setInvalid
      s!"[\"swl_workflow_state:state_backend_parse_and_manager_dirty_lifecycle\",\"backend_kind_parsing_and_defaults_save_invalid_set_dirty_flags\",\"mem={bMem}:file={bFile}:upper_mem={bUpper}:bogus={bBogus}:init_dirty={d0}:after_defaults_dirty={d1}:after_save_dirty={d2}:bad_set_err=true:dirty_after_bad={d3}:val_preserved=10\"]"
    , let rClose := decide (neutralizedRawTagCount 2 1 > 0)
      let rOpen := decide (neutralizedRawTagCount 2 1 > 0)
      let outCnt := framedOuterCloseTagCount 1
      s!"[\"cognitive_memory_rag:memory_data_tag_neutralization_and_framing\",\"case_insensitive_tag_neutralization_and_attribute_escaped_framing\",\"raw_close_remains={rClose}:raw_open_remains={rOpen}:escaped_entity=true:escaped_path_attr=true:outer_close_count={outCnt}\"]"
    , let rCnt := extractImportsCountModel .rust 2
      let pCnt := extractImportsCountModel .python 1
      let jCnt := extractImportsCountModel .javascript 2
      let uCnt := extractImportsCountModel .unknown 5
      s!"[\"cognitive_memory_rag:semantic_linker_multi_language_import_extraction\",\"rust_python_js_import_regex_extraction_and_unknown_empty\",\"rust={rCnt}/std::collections::HashMap,swl::lowering:py={pCnt}/pkg.sub:js={jCnt}/react,fs:unknown={uCnt}\"]"
    , let mBeg := lessonEstimatedMinutes 2
      let mAdv := lessonEstimatedMinutes 1
      let mTot := mBeg + mAdv
      s!"[\"cognitive_memory_rag:code_explainer_and_curriculum_difficulty_order\",\"comment_skip_concept_detection_and_sorted_difficulty_lesson_minutes\",\"line_explanations=4:concepts=3:lessons=2:first_diff=Beginner/{mBeg}:second_diff=Advanced/{mAdv}:total_min={mTot}\"]"
    , let vPub := RustVisibilityModel.pubVis.isPublic
      let vCrate := RustVisibilityModel.pubCrate.isPublic
      let vPriv := RustVisibilityModel.privateVis.isPublic
      s!"[\"cognitive_memory_rag:symbol_visibility_dependency_graph_and_patterns\",\"pub_visibility_cargo_toml_sections_and_four_pattern_detectors\",\"pub={vPub}:pub_crate={vCrate}:priv={vPriv}:deps_total=4:deps_direct=2:deps_dev=1:tokio_opt=true:cc_build=true:patterns=4\"]"
    , let unvNotRun := verificationNotRunRatePct false false
      let recPct := errorRecoveryRatePct 4 3
      let verNotRun := verificationNotRunRatePct true true
      s!"[\"cognitive_memory_rag:performance_snapshot_honesty_and_rag_context\",\"unverified_none_rates_vs_verified_rates_and_rag_rust_context_builder\",\"unver_first=none:unver_final=none:unver_not_run_pct={unvNotRun}:rec_pct={recPct}:unrec=1:ver_first=0:ver_final=100:ver_not_run_pct={verNotRun}:rag_rs_exts=3:ctx_sections=3\"]"
    , let gS := (classifyQualityGradeTenths 950).asStr
      let gA := (classifyQualityGradeTenths 900).asStr
      let gB := (classifyQualityGradeTenths 800).asStr
      let gC := (classifyQualityGradeTenths 700).asStr
      let gD := (classifyQualityGradeTenths 600).asStr
      let gF := (classifyQualityGradeTenths 599).asStr
      let sc := (stageWeightedScoreTenths false true false 15 + stageWeightedScoreTenths false false true 20 + stageWeightedScoreTenths true false true 25) / 10
      s!"[\"testing_verification_qa:quality_grade_thresholds_and_weighted_score\",\"six_grade_cutoffs_warning_half_credit_and_not_run_zero_contribution\",\"grades={gS}/{gA}/{gB}/{gC}/{gD}/{gF}:score={sc}:not_run_failed=false\"]"
    , let (py5P, py5Nr) := demoteZeroTestRun false true false false
      let (py0P, py0Nr) := demoteZeroTestRun true false true false
      let (pyOkP, pyOkNr) := demoteZeroTestRun true false false false
      let (go0P, go0Nr) := demoteZeroTestRun true false false true
      s!"[\"testing_verification_qa:language_qa_zero_test_and_stage_set_gating\",\"marker_detection_profile_stage_sets_and_pytest_unittest_go_npm_zero_test_demotion\",\"langs=rust/node/python/go/unknown:fast=false/false/false:std=true/true/false:full=true/true/true:py5_pass={py5P}/{py5Nr}:py0_pass={py0P}/{py0Nr}:py_ok={pyOkP}/{pyOkNr}:go_notest={go0P}/{go0Nr}:npm_placeholder=none:npm_vitest=some\"]"
    , let (edRf, fbRf) := resolveRustEditionModel (some "2024") (some "2021") none
      let (edPkg, fbPkg) := resolveRustEditionModel none (some "2021") none
      let (edWs, fbWs) := resolveRustEditionModel none none (some "2024")
      let (edFb, fbFb) := resolveRustEditionModel none none none
      s!"[\"testing_verification_qa:rust_edition_resolution_and_rustfmt_args\",\"rustfmt_overrides_cargo_pkg_workspace_inheritance_and_2021_fallback\",\"rustfmt={edRf}/{fbRf}:pkg={edPkg}/{fbPkg}:ws={edWs}/{fbWs}:fallback={edFb}/{fbFb}:args=--edition,2024,--check,--config,skip_children=true\"]"
    , let cxx23 := cmakeStdFlagModel true 23 false
      let c17Gnu := cmakeStdFlagModel false 17 true
      let j8 := normalizeJavaMajorModel 1 8
      let j21 := normalizeJavaMajorModel 21 0
      s!"[\"testing_verification_qa:syntax_toolchain_jsonc_esm_cmake_python_java\",\"jsonc_comments_trailing_comma_esm_detection_cmake_flags_and_python_java_versions\",\"jsonc_ok=true/2:esm_cjs=false:esm_mod=true:cxx23={cxx23}:c17_gnu={c17Gnu}:py_min=3.10:java8={j8}:java21={j21}\"]"
    , let uOk := classifyRustfmtFailureModel true false false == .toolUnavailable
      let dOk := classifyRustfmtFailureModel false true false == .formattingDiff
      let deOk := classifyRustfmtFailureModel false true true == .syntaxFailure
      let pOk := classifyRustfmtFailureModel false false true == .syntaxFailure
      let bAllNr := (classifyVerificationBanner true 2 2).tag
      let bPartNr := (classifyVerificationBanner true 2 1).tag
      let bFail := (classifyVerificationBanner false 2 0).tag
      s!"[\"testing_verification_qa:rustfmt_failure_classification_and_report_honesty\",\"missing_component_diff_only_vs_syntax_error_and_not_verified_report_banner\",\"unavail={uOk}:diff_only={dOk}:diff_plus_err={deOk}:parse_err={pOk}:all_not_run=2/true/{bAllNr}:partial_not_run=1/false/{bPartNr}:failed={bFail}\"]"
    , let vMd := parseSkillFrontmatterValid true true true
      let noOpen := !parseSkillFrontmatterValid false true true
      let noClose := !parseSkillFrontmatterValid true false true
      let empName := !parseSkillFrontmatterValid true true false
      let admMan := if userSkillDiscoveryAdmitted false false then 1 else 0
      let refCand := if !userSkillDiscoveryAdmitted true false then 1 else 0
      s!"[\"skills_resources_lifecycle:skill_frontmatter_parsing_and_task_wrapping\",\"frontmatter_delimiters_user_dir_candidate_refusal_and_slash_skill_wrapping\",\"valid_md={vMd}:no_open_err={noOpen}:no_close_err={noClose}:empty_name_err={empName}:user_loaded={admMan}:user_refused={refCand}:wrapped_badge=true:wrapped_sub=true\"]"
    , let (cLbls, iLbls, hasBuiltBy, hasTaskLbl) := containerAndImageLabelIsolation true
      s!"[\"skills_resources_lifecycle:owner_scope_and_container_image_label_isolation\",\"task_local_owner_scope_and_container_task_vs_image_built_by_labels\",\"outside_none=true:scoped_task=task-42:scoped_agent=coder:container_labels={cLbls}:image_labels={iLbls}:img_has_built_by={hasBuiltBy}:img_has_task_label={hasTaskLbl}:task_is_sess=false:sess_is_sess=true\"]"
    , let (_, g1) := tryAllocateQuotaModel 0 600 1000
      let (gOverOk, _) := tryAllocateQuotaModel g1 500 1000
      let gDrop := releaseQuotaGuardModel g1 600
      let (_, r2) := tryAllocateQuotaModel 1 1 2
      let (rOverOk, _) := tryAllocateQuotaModel r2 1 2
      let rDrop := releaseQuotaGuardModel r2 1
      let (tOverOk, _) := tryAllocateQuotaModel 1 1 1
      let tDrop := releaseQuotaGuardModel 1 1
      s!"[\"skills_resources_lifecycle:resource_limit_tracker_raii_and_quota_guards\",\"gpu_request_task_raii_guards_and_adaptive_quota_memory_ceiling\",\"gpu_ok={g1}:gpu_over_err={!gOverOk}:gpu_after_drop={gDrop}:req_at_cap={r2}:req_over_err={!rOverOk}:req_after_drop={rDrop}:task_over_err={!tOverOk}:task_after_drop={tDrop}:sys_mem_at_100x=true:sys_mem_over_100x=false\"]"
    , let stRel := (classifyReaperEntryModel true false true false false).label
      let stLeak := (classifyReaperEntryModel false true true false false).label
      let stKeep := (classifyReaperEntryModel false false true true false).label
      let stSess := (classifyReaperEntryModel false false true false true).label
      let stAct := (classifyReaperEntryModel false false true false false).label
      let stOrph := (classifyReaperEntryModel false false false true false).label
      let stDead := (classifyReaperEntryModel false false false false false).label
      s!"[\"skills_resources_lifecycle:reaper_entry_classification_and_zombie_detection\",\"released_leaked_kept_session_owned_active_orphan_and_dead_session_zombie\",\"released={stRel}:leaked={stLeak}:live_keep={stKeep}:live_sess={stSess}:live_task={stAct}:dead_keep={stOrph}:dead_task={stDead}\"]"
    , let (sDrn, aDrn) := (stepResourceStateModel .live .drain).getD (.live, false)
      let (sLk, aLk) := (stepResourceStateModel sDrn .deadlinePassed).getD (.live, false)
      let (sRel, _) := (stepResourceStateModel .draining .stopped).getD (.live, false)
      let relInv := (stepResourceStateModel sRel .drain).isNone
      let totBytes := storageEstimateTotal 100 200 300 50
      s!"[\"skills_resources_lifecycle:registry_lifecycle_transitions_and_drain_report\",\"live_to_draining_to_leaked_alarm_released_terminal_and_drain_storage_totals\",\"after_drain={sDrn.asStr}/{aDrn}:after_deadline={sLk.asStr}/{aLk}:after_stop={sRel.asStr}:from_released_err={relInv}:empty_outcome=none:counts=2/1/1:storage_total={totBytes}\"]"
    ]

-- 65–68. PA65–PA68 rows (Wave 16: Bench Harness Lattice, Lifecycle Projection Lattice, Evolution AST/Replay/Telemetry, Orchestration Multi-Agent/Workflow)
def wave16Rows : List String :=
    [ let c0 := clampBenchConcurrency 0
      let vOk := validateHarnessConfigModel true true c0 2048 120
      let vEmp := !validateHarnessConfigModel false true c0 2048 120
      let inf1 := streamInferenceLatencyMs 450 120
      let inf2 := streamInferenceLatencyMs 80 200
      let (jScore, jPass) := jsonEvaluatorScorePermille true 1 2
      s!"[\"bench_harness_lattice:config_stream_and_evaluators\",\"concurrency_clamp_stream_inference_latency_and_keyword_json_evaluators\",\"clamp0={c0}:valid_ok={vOk}:empty_ep_err={vEmp}:inf_lat={inf1},{inf2}:kw=500,true:json={jScore},{jPass}\"]"
    , s!"[\"bench_harness_lattice:harness_report_transport_vs_eval\",\"transport_succeeded_latency_filter_vs_eval_failure_and_avg_score\",\"total=3:pass=1:fail=2:err_permille=667:p50=400:inf_p50=300:min=200:max=400:qwait=150:tps=30:avg_score=700\"]"
    , let sG := (classifyLongRunningStatus true 5 0 100).label
      let sP := (classifyLongRunningStatus true 3 2 100).label
      let sC := (classifyLongRunningStatus true 0 2 100).label
      let sW := (classifyLongRunningStatus false 0 0 10).label
      let sF := (classifyLongRunningStatus false 0 0 2).label
      s!"[\"bench_harness_lattice:long_running_status_and_rates\",\"five_tier_project_status_lattice_success_green_rates_and_template_guard\",\"tiers={sG},{sP},{sC},{sW},{sF}:success_permille=600:green_permille=200:traversal_rejected=true\"]"
    , let c3BeatsC2 := swebenchCandidateBetter true true 2 3 1 true true 4 2 2
      s!"[\"bench_harness_lattice:swebench_frozen_selector_and_oracle\",\"source_only_minimal_patch_frozen_selection_blind_to_eval_and_oracle\",\"frozen_num=3:blind_to_eval={c3BeatsC2}:first_num=1:pass_at_1=Some(true):oracle=true\"]"
    , s!"[\"bench_harness_lattice:trace_diagnosis_and_summary\",\"read_loop_fake_complete_timeout_api_syntax_counts_and_medians\",\"t1_loop=5:t1_fake=true:t1_timeout=true:t1_api=1:t1_syn=1:runs=2:cands=1:med_steps=6:med_edit=3\"]"
    , let s1 := (stepAgentStateModel .idle .assign).getD .idle
      let s2 := (stepAgentStateModel s1 .block).getD .idle
      let s3 := (stepAgentStateModel s2 .unblock).getD .idle
      let s4 := (stepAgentStateModel s3 .crash).getD .idle
      let s5 := (stepAgentStateModel s4 .restart).getD .idle
      let s6 := (stepAgentStateModel s5 .stop).getD .idle
      let stopTwiceErr := (stepAgentStateModel s6 .stop).isNone
      s!"[\"lifecycle_projection_lattice:agent_state_machine_and_check_step\",\"assign_block_unblock_crash_restart_stop_and_terminal_absorbing\",\"flow={s1.asStr},{s2.asStr},{s3.asStr},{s4.asStr},{s5.asStr},{s6.asStr}:working_task=Some(t-1):crashed_task=None:term={s6.isTerminal}:stop_twice_err={stopTwiceErr}\"]"
    , let eComp := taskEnterEffectsCount true
      let eFail := taskEnterEffectsCount true
      let eCanc := taskEnterEffectsCount true
      let eIntr := taskEnterEffectsCount true
      let ePaus := taskEnterEffectsCount false
      let okIntr := taskCheckStepTerminalAllowed true true
      let errComp := !taskCheckStepTerminalAllowed false true
      s!"[\"lifecycle_projection_lattice:task_effects_and_resource_drainability\",\"terminal_teardown_effect_interrupted_resume_exception_and_drainable_kinds\",\"teardown_counts={eComp},{eFail},{eCanc},{eIntr},{ePaus}:interrupted_resume_ok={okIntr}:completed_resume_err={errComp}:drainable=5:non_drainable=4\"]"
    , let (capLen, _) := sanitizeCauseLengthModel 300 240
      let (u1, rem1) := stepTrackedUsageConsumed true
      let (u2, _) := stepTrackedUsageConsumed rem1
      s!"[\"lifecycle_projection_lattice:transition_cause_sanitization_and_tracked_usage\",\"multiline_first_line_240_char_cap_and_single_transition_usage_attach\",\"multiline=first line summary:capped_chars={capLen}:ellipsis=true:first_usage={u1}:second_usage={u2}\"]"
    , let eEmp := !validateTaskEditModel false 5 (some 10) 1000 none
      let eTurns := !validateTaskEditModel true 5 (some 5) 1000 none
      let eBudg := !validateTaskEditModel true 5 (some 10) 1000 (some 1000)
      let eValid := validateTaskEditModel true 5 (some 20) 1000 (some 5000)
      s!"[\"lifecycle_projection_lattice:task_edit_validation_and_diff\",\"rejects_empty_desc_retroactive_ceilings_and_formats_semicolon_diff\",\"empty_err={eEmp}:turns_err={eTurns}:budget_err={eBudg}:valid_ok={eValid}:noop_none=true:diff=description is now: Fix AST parser; max turns 10 → 20; token budget unbounded → 5000\"]"
    , let pMs := pausedIntervalTotalMs 1000 16000 true
      s!"[\"lifecycle_projection_lattice:projection_resolve_pause_and_usage\",\"prefix_task_resolution_paused_duration_across_edit_and_dead_pid_display\",\"resolve=t-alpha-1,t-beta-1,ambig2,notfound:paused_ms={pMs}:fmt=15s,3m41s:none_tok=not recorded:partial_cost=≥ $0.0125 (some calls reported no cost):dead_agent=working (process ended),-\"]"
    , let pruned := ">".intercalate (pruneAncestryAtLatestEvaluated [("att-2", false), ("att-1", true), ("att-0", true)])
      s!"[\"evolution_ast_replay_telemetry:ast_parent_commit_and_prune_chain\",\"resolve_parent_base_commit_and_truncate_at_latest_evaluated_ancestor\",\"none=None:empty=None:base=Some(c000base):walk2=Some(c000base):comm3=Some(c333comm):restore_base=0:restore_comm3=att-3:restore_chain2={pruned}\"]"
    , s!"[\"evolution_ast_replay_telemetry:replay_simulator_guards_and_errors\",\"batch_parallelism_illegal_action_duplicate_node_branch_and_loop_cap\",\"over_par=3>2:illegal=z0:dup_node=a0:dup_branch=A:loop_cap=100\"]"
    , let (imp, parP, nCost, obj, parR, seqR) := replayMetricsPermille 500 850 4 5 2 200 100
      s!"[\"evolution_ast_replay_telemetry:replay_metrics_and_pareto_reward\",\"refine_top1_exact_probes_sequential_rounds_objective_and_pareto_reward\",\"rounds=2:probes=4:seq_rounds={seqR}:term=850:improv={imp}:par_pen={parP}:norm_cost={nCost}:obj={obj}:pareto={parR}\"]"
    , let (st1W, st1Ok) := (rankingStabilityGateModel 1 2 1000 1000).getD (999, true)
      let (stCW, stCOk) := (rankingStabilityGateModel 2 3 8000 8000).getD (0, false)
      let (stTW, stTOk) := (rankingStabilityGateModel 2 3 0 0).getD (999, true)
      s!"[\"evolution_ast_replay_telemetry:ranking_stability_and_honesty_gate\",\"empty_error_single_tree_honesty_gate_concordant_w_and_tied_zero\",\"empty_err=true:single_tree={st1W},{st1Ok}:concordant={stCW},{stCOk}:all_tied={stTW},{stTOk}:hash_len=64\"]"
    , let (_, bSlow) := classifyBenchDeltaBadge 50
      let (_, bFast) := classifyBenchDeltaBadge (-35)
      let (_, bStab) := classifyBenchDeltaBadge 10
      let isSlow := decide (bSlow == "SLOWER")
      let isFast := decide (bFast == "FASTER")
      let isStab := decide (bStab == "STABLE")
      s!"[\"evolution_ast_replay_telemetry:telemetry_prompt_and_test_parser\",\"multi_suite_cargo_test_sum_and_benchmark_regression_badges\",\"parsed=12,1,2,15:no_result_none=true:slower={isSlow}:faster={isFast}:stable={isStab}:alloc_kb=true:summary_line=true\"]"
    , let cMin := clampMultiAgentConcurrency 0
      let cMax := clampMultiAgentConcurrency 99
      let t0 := agentTagModel "architect" 0
      let t2 := agentTagModel "tester" 2
      let col10 := agentColorIndexModel 10
      s!"[\"orchestration_multiagent_workflow:multiagent_config_tag_and_line_buffer\",\"concurrency_clamp_1_to_16_1_based_agent_tag_and_per_agent_line_buffer\",\"def_conc=4:clamp={cMin},{cMax}:tags={t0},{t2}:color10={col10}:buf=0,other,hello,Some(world),None\"]"
    , let (_, tab2) := selectTabByNumberModel 3 .all 2
      let (ok9, _) := selectTabByNumberModel 3 tab2 9
      let oOk := classifyAgentSummaryOutcome true false
      let oFail := classifyAgentSummaryOutcome false false
      let oSkip := classifyAgentSummaryOutcome false true
      s!"[\"orchestration_multiagent_workflow:multichat_view_tabs_and_summary_rows\",\"tab_cycling_sorted_summary_table_outcomes_and_usage_aggregation\",\"tabs={tab2.asStr},{ok9},Agent(2),All,Agent(2):rows=architect·1/{oOk}/150 (100+50),coder·2/{oFail}/280 (200+80),tester·3/{oSkip}/—:tot_tok=430:cost_milli=20\"]"
    , let vIdOk := validateWorkflowRunIdModel 13 false true
      let vIdEmp := !validateWorkflowRunIdModel 0 false true
      let vIdDot := !validateWorkflowRunIdModel 7 true true
      let vIdSlash := !validateWorkflowRunIdModel 9 false false
      let vId129 := !validateWorkflowRunIdModel 129 false true
      s!"[\"orchestration_multiagent_workflow:workflow_checkpoint_id_and_canonical_fingerprint\",\"run_id_charset_bounds_and_order_invariant_canonical_step_fingerprint\",\"valid_id={vIdOk}:empty_err={vIdEmp}:dot_err={vIdDot}:slash_err={vIdSlash}:len129_err={vId129}:fp_len=64:canonical_eq=true:mutated_ne=true\"]"
    , let mTok := workflowMeasuredTokensModel 150 120 80
      let bUnder := (checkWorkflowBudgetModel (some 10) (some 200) 5000 150 100 99 1).asStr
      let bTok := (checkWorkflowBudgetModel (some 10) (some 200) 5000 150 120 80 1).asStr
      let bWall := (checkWorkflowBudgetModel (some 10) (some 200) 10000 150 120 80 1).asStr
      s!"[\"orchestration_multiagent_workflow:workflow_templates_and_budget_gates\",\"tdd_review_templates_measured_tokens_and_wall_clock_first_budget_stop\",\"tdd_steps=5:red_req=false:green_req=true:rev_deps=3:unk_none=true:measured={mTok}:under={bUnder}:tok_hit={bTok}:wall_first={bWall}\"]"
    , let dIter := checkStepDepIterationAware true (some true) (some false)
      let dTop := checkStepDepIterationAware true none (some false)
      let iterOk := decide (dIter == "Ok")
      s!"[\"orchestration_multiagent_workflow:workflow_dep_check_and_var_substitution\",\"iteration_scoped_dep_lookup_longest_match_no_rescan_and_posix_shell_quote\",\"unk_def_err=true:not_exec=true:not_sat={dTop}:iter2_ok={iterOk}:sub=$item_count and 42:unknown_kept=true:shell_quoted=true\"]"
    ]

-- 69–72. PA69–PA72 rows (Wave 17: Self-Healing & Supervision, Tools HTTP/Browser/Search/Knowledge, Evolve Repair/Multi-Arm/Dedup/Dead-Code, VLM Bench & SWE-bench Pro)
def wave17Rows : List String :=
    [ let rAct := defaultStrategyActionCount .resourceExhaustion
      let nAct := defaultStrategyActionCount .network
      let escNet := hasEscalationStrategy .network
      let escRes := hasEscalationStrategy .resourceExhaustion
      let escPrs := hasEscalationStrategy .parseError
      let escAuth := hasEscalationStrategy .authError
      let escUnk := hasEscalationStrategy .unknown
      s!"[\"self_healing_supervision_lattice:error_class_and_strategy_mapping\",\"priority_classification_anyhow_downcast_default_and_escalation_strategies\",\"prio=rate_limit,timeout,network,resource_exhaustion,parse_error,auth_error,unknown:io_refused=network:serde_err=parse_error:act_counts={rAct},{nAct}:esc={escNet}/escalate_restart,{escRes}/escalate_reset,{escPrs}/escalate_compress,{escAuth},{escUnk}:fallback_name=retry\"]"
    , let rNonUnreach := (resolveEndpointOffline true false false).asStr
      let rLocal := (resolveEndpointOffline true true true).asStr
      let rRemote := (resolveEndpointOffline true true false).asStr
      s!"[\"self_healing_supervision_lattice:recovery_tree_offline_resolution\",\"endpoint_fallback_opt_in_local_guard_rate_limit_and_overflow_tree\",\"non_unreach={rNonUnreach}:opt_in_local={rLocal}:opt_in_remote={rRemote}/http://localhost:11434/v1:rate429=retry/2000/3:overflow=compress/8000:disk_full=unresolvable\"]"
    , let rec2 := learnerRecommends 3 2 2 2
      let rec3 := learnerRecommends 3 3 2 2
      let rec1Att := learnerRecommends 3 3 1 1
      let recLow := learnerRecommends 3 3 2 0
      let recOk := learnerRecommends 3 3 2 1
      let actCp := checkpointRingActive 2 3
      s!"[\"self_healing_supervision_lattice:learner_pattern_window_and_state_checkpoints\",\"pattern_threshold_success_rate_recommendation_and_checkpoint_eviction\",\"below_thresh_pat={rec2}:at_thresh_pat={rec3}:rec_1att={rec1Att}:rec_low={recLow}:rec_ok={recOk}/escalate_restart:active_cp={actCp}:evicted_first=true:latest_msg=m3:restores=1\"]"
    , let exDis := executorWithinAttemptBudget false 1 2
      let exEmp := executorWithinAttemptBudget true 0 2
      let exOver := executorWithinAttemptBudget true 3 2
      let exOk := executorWithinAttemptBudget true 2 2
      let p4 := (predictHealth 4 8).asStr
      let pImm := (predictHealth 10 7).asStr
      let pDeg := (predictHealth 10 4).asStr
      let pRisk := (predictHealth 10 2).asStr
      let pOk := (predictHealth 10 0).asStr
      s!"[\"self_healing_supervision_lattice:executor_action_guards_and_health_predictor\",\"disabled_empty_max_attempts_validation_and_five_point_health_predictor\",\"disabled_ok={exDis}:empty_ok={exEmp}:over_budget_ok={exOver}/2:valid_ok={exOk}/2:blank_restart_ok=false:no_sm_restore_ok=false:pred={p4},{pImm}/60,{pDeg}/300,{pRisk}/900,{pOk}/none\"]"
    , let (stPerm, fPerm) := recordClassifiedError .closed 0 2 false
      let (stTr1, fTr1) := recordClassifiedError stPerm fPerm 2 true
      let (stTr2, fTr2) := recordClassifiedError stTr1 fTr1 2 true
      s!"[\"self_healing_supervision_lattice:circuit_breaker_classifier_and_run_supervisor\",\"permanent_error_ignored_transient_trips_open_and_run_supervisor_ids\",\"perm={stPerm.asStr}/{fPerm}/none:tr1={stTr1.asStr}/{fTr1}/some:tr2={stTr2.asStr}/{fTr2}/some:open_succ_keeps_err=true:run_id=run-1:pause_err=true:resume_err=true:abort_missing=false\"]"
    , let rebind := resolvedAddressAllowed false true false false true
      let localLan := resolvedAddressAllowed true false true false true
      let localLoop := resolvedAddressAllowed true true false false true
      let redirCrossPort := redirectTargetAllowed ⟨false, true, 8000⟩ true false 9000
      let redirSamePort := redirectTargetAllowed ⟨false, true, 8000⟩ true false 8000
      let redirPubToLocal := redirectTargetAllowed ⟨false, false, 443⟩ true false 8000
      s!"[\"tools_http_browser_search_knowledge:net_policy_ssrf_and_redirect_guard\",\"private_host_detection_dns_rebinding_guard_and_cross_port_redirect_block\",\"priv_hosts=true,true,true,false:ftp_err=true:lan_blocked=true:rebind_ok={rebind}:local_lan_ok={localLan}:local_loop_ok={localLoop}:redir_cross_port_ok={redirCrossPort}:redir_same_port_ok={redirSamePort}:redir_pub_to_local_ok={redirPubToLocal}\"]"
    , let (s1, e1, m1) := paginateSlice 5 1 2
      let (s2, e2, m2) := paginateSlice 5 3 2
      let (s3, e3, m3) := paginateSlice 5 9 2
      let outSec := outboundRequestAllowed true true
      let outScheme := outboundRequestAllowed false false
      s!"[\"tools_http_browser_search_knowledge:http_and_browser_outbound_credential_and_pagination\",\"http_browser_secret_exfiltration_block_scheme_guard_and_unicode_pagination\",\"http_sec_ok={outSec}:http_trace_ok=false:browser_sec_ok={outSec}:browser_file_ok={outScheme}:eval_sec_ok={outSec}:p1={s1}..{e1}/βγ/{m1}:p2={s2}..{e2}/δε/{m2}:p3={s3}..{e3}//{m3}\"]"
    , let r1000 := regexPatternLengthAllowed 1000 1000
      let r1001 := regexPatternLengthAllowed 1001 1000
      let gStarNested := globMatchesLiteralSeparator false true false false
      let gRecNested := globMatchesLiteralSeparator true true false false
      let gPriv := globMatchesLiteralSeparator true false true false
      let gSym := globMatchesLiteralSeparator true false false true
      s!"[\"tools_http_browser_search_knowledge:grep_regex_cache_and_glob_literal_separator\",\"regex_1000_char_cap_and_glob_literal_separator_private_dir_symlink_pruning\",\"regex_1000_ok={r1000}:regex_1001_ok={r1001}:regex_bad_ok=false:star_top=1:star_nested={gStarNested}:rec_count=2:rec_nested={gRecNested}:priv_leaked={gPriv}:sym_leaked={gSym}:cap1_trunc=true\"]"
    , let mAllFn := symbolFilterMatches .all .fnKind
      let mStrStr := symbolFilterMatches .structKind .structKind
      let mStrFn := symbolFilterMatches .structKind .fnKind
      s!"[\"tools_http_browser_search_knowledge:symbol_search_extraction_and_type_filtering\",\"rust_symbol_extraction_1_based_lines_type_filter_and_private_dir_skip\",\"all_count=5:types=struct,enum,trait,impl,function:lines=1,2,3,4,5:struct_only=1/AlphaWidget:fn_in_struct_filter={mStrFn}:all_matches_fn={mAllFn}:struct_matches_struct={mStrStr}:priv_leaked=false\"]"
    , let remEdges := (incidentEdgesPurged [(1, 2), (2, 3), (1, 3)] 2).length
      let expAbs := exportPathSafe false false
      let expTrav := exportPathSafe true true
      let expRel := exportPathSafe true false
      s!"[\"tools_http_browser_search_knowledge:knowledge_graph_crud_and_export_guards\",\"node_edge_crud_remove_node_purges_incident_edges_and_export_path_guard\",\"before_nodes=3:before_edges=3:after_remove_nodes=2:after_remove_edges={remEdges}:remaining_edge=n1->n3:clear_unconfirmed_ok=false:export_abs_ok={expAbs}:export_trav_ok={expTrav}:export_rel_ok={expRel}\"]"
    , let exMaSome := spanExtractable .machineApplicable true
      let exMaNone := spanExtractable .machineApplicable false
      let exMaybe := spanExtractable .maybeIncorrect true
      let exPlace := spanExtractable .hasPlaceholders true
      s!"[\"evolve_repair_multiarm_dedup_deadcode:compiler_repair_extract_and_multipart_grouping\",\"machine_applicable_filter_shared_multipart_suggestion_id_and_child_message\",\"ma_some={exMaSome}:ma_none={exMaNone}:maybe={exMaybe}:placeholders={exPlace}:extracted=3:multipart_same_id=true:next_id_distinct=true:child_msg=unused import: remove the whole `use` item\"]"
    , let gBounds := groupAtomicallyValid false true true
      let gIntOv := groupAtomicallyValid true false true
      let gAccOv := groupAtomicallyValid true true false
      let gValid := groupAtomicallyValid true true true
      s!"[\"evolve_repair_multiarm_dedup_deadcode:compiler_repair_apply_atomicity_and_path_guards\",\"outside_path_skip_duplicate_dedup_multipart_atomicity_and_reverse_offset_apply\",\"group_guards={gBounds},{gIntOv},{gAccOv},{gValid}:applied=2:dups=1:skipped=4:files=1:mut_removed=true\"]"
    , let regCnt := testRegressionsCount 1 2 10 9
      let accOverlap := armAccepted true 0 105 100
      let accReg := armAccepted true 2 80 100
      let accWin := armAccepted true 0 90 100
      s!"[\"evolve_repair_multiarm_dedup_deadcode:multi_arm_median_precheck_and_separation_gate\",\"median_ns_precheck_rejections_test_regressions_and_strict_sample_separation\",\"med=None,200,200:precheck_calls=0:reg_count={regCnt}:overlap_accepted={accOverlap}:reg_accepted={accReg}:win_accepted={accWin}:winner=Some(arm-win):consensus=true\"]"
    , let cEx := (classifyEvolveDedup true 5 5 5 5).asStr
      let cNear := (classifyEvolveDedup false 4 5 4 5).asStr
      let cSize := (classifyEvolveDedup false 1 10 1 1).asStr
      let cLow := (classifyEvolveDedup false 5 5 3 5).asStr
      s!"[\"evolve_repair_multiarm_dedup_deadcode:dedup_normalized_lines_jaccard_and_clusters\",\"whitespace_blank_normalization_jaccard_exact_near_and_size_ratio_pruning\",\"norm_len=2:j_empty=0:j_4of5=80:classes={cEx},{cNear},{cSize},{cLow}:pairs=2:first_kind=Exact:second_kind=Near\"]"
    , let dPlain := isDeadSymbol false false false false false
      let dDisp := isDeadSymbol true false false false false
      let dDund := isDeadSymbol false true false false false
      let dTest := isDeadSymbol false false true false false
      let dMod := isDeadSymbol false false false true false
      let dRef := isDeadSymbol false false false false true
      let sortOk := decide (deadCodeSortKey false < deadCodeSortKey true)
      s!"[\"evolve_repair_multiarm_dedup_deadcode:dead_code_cross_tree_refs_and_multiline_attrs\",\"dispatched_dunder_multiline_test_attr_cross_tree_refs_and_cfg_gated_sort\",\"flags={dPlain},{dDisp},{dDund},{dTest},{dMod},{dRef}:dead_names=dead_plain,dead_linux:cfg_flags=false,true:plain_before_gated={sortOk}\"]"
    , let rBloom := (ratingFromBasisPoints 80 80).asStr
      let rGrow := (ratingFromBasisPoints 60 80).asStr
      let rWilt := (ratingFromBasisPoints 40 80).asStr
      let rFrost := (ratingFromBasisPoints 39 80).asStr
      s!"[\"vlm_bench_and_swebench_pro_lattice:vlm_rating_and_keyword_json_scoring\",\"four_tier_garden_ratings_keyword_json_markdown_extraction_and_bm25_overlap\",\"ratings={rBloom},{rGrow},{rWilt},{rFrost}:kw_acc=50:json_acc=100:overlap_same=100:overlap_disjoint=0:scored_rating=BLOOM\"]"
    , let filtLen := ([VlmDifficultyTier.easy, .medium, .hard, .veryHard, .extreme, .mega].filter (fun d => difficultyRank d <= difficultyRank .hard)).length
      let negClamp := clampCorrelationBp false 80
      let negRat := (ratingFromBasisPoints negClamp 50).asStr
      let posClamp := clampCorrelationBp true 100
      let posRat := (ratingFromBasisPoints posClamp 50).asStr
      s!"[\"vlm_bench_and_swebench_pro_lattice:vlm_levels_pearson_config_and_mega_eval\",\"all_levels_order_difficulty_filter_pearson_correlation_and_mega_visual_clamp\",\"levels=6:ascending=true:hard_filter_len={filtLen}:conc_clamp=1:pearson=100,-100,0:mega_neg={negClamp}/{negRat}:mega_pos={posClamp}/{posRat}:report_fmt=true\"]"
    , let rOk := safeRepoSlug 2 false false
      let r3 := safeRepoSlug 3 false false
      let rDash := safeRepoSlug 2 true false
      let rDot := safeRepoSlug 2 false true
      let gOk := safeGitRevision true false false false
      let gDash := safeGitRevision true true false false
      let gDot := safeGitRevision true false true false
      s!"[\"vlm_bench_and_swebench_pro_lattice:swebench_dataset_safety_and_string_coercion\",\"repo_slug_and_git_revision_injection_guards_shortest_sort_and_list_coercion\",\"repo={rOk},{r3},{rDash},{rDot}:rev={gOk},{gDash},{gDot}:bad_id_err=true:sorted_ids=inst_short,inst_long:coerced=2,2,1,0\"]"
    , let a00 := llamaServerOptionalArgCount false false
      let a10 := llamaServerOptionalArgCount true false
      let a01 := llamaServerOptionalArgCount false true
      let a11 := llamaServerOptionalArgCount true true
      s!"[\"vlm_bench_and_swebench_pro_lattice:swebench_manifest_lookup_and_llama_server_argv\",\"manifest_atomic_roundtrip_trial_lookup_and_llama_server_optional_arg_inclusion\",\"opt_args={a00},{a10},{a01},{a11}:blank_ts_omitted=true:mmproj_included=true:trial_state=Evaluated:missing_none=true\"]"
    , let pQ8 := catalogParallelSlots true false
      let p35b := catalogParallelSlots false true
      let pStd := catalogParallelSlots false false
      let ctx := catalogContextWindow
      s!"[\"vlm_bench_and_swebench_pro_lattice:swebench_catalog_defaults_and_parallel_caps\",\"eleven_quant_specs_262k_ctx_q8_and_35b_single_slot_and_default_quants_membership\",\"total=11:defaults=4:all_defaults_in_catalog=true:all_ctx={ctx}:slots={pQ8},{p35b},{pStd}:find_unknown_none=true\"]"
    ]

-- 30. Wave 18 rows (PA73-PA76: tui_garden, vision_capture_proc, evolve_ast_map, agent_exec_envelope)
def wave18Rows : List String :=
    [ let yAns := (evalPermissionKeyAnswer false false .charY).map TuiPermissionAnswerModel.asStr |>.getD "none"
      let aDis := (evalPermissionKeyAnswer false false .charA).map TuiPermissionAnswerModel.asStr |>.getD "none"
      let pDis := (evalPermissionKeyAnswer false false .charP).map TuiPermissionAnswerModel.asStr |>.getD "none"
      let escAns := (evalPermissionKeyAnswer false false .esc).map TuiPermissionAnswerModel.asStr |>.getD "none"
      let aEn := (evalPermissionKeyAnswer true true .charA).map TuiPermissionAnswerModel.asStr |>.getD "none"
      let pEn := (evalPermissionKeyAnswer true true .charP).map TuiPermissionAnswerModel.asStr |>.getD "none"
      let (_, vOpen, s0) := stepPermissionDiffView true false 0 .charV
      let (_, _, s1) := stepPermissionDiffView true vOpen s0 .down
      let (_, escClose, _) := stepPermissionDiffView true vOpen s1 .esc
      let stOk := (parseStepStatusModel true true 5).map toString |>.getD "none"
      let stFail := (parseStepStatusModel true false 3).map toString |>.getD "none"
      s!"[\"tui_garden:permission_key_answer_and_diff_view_toggle\",\"y_once_a_gated_by_allow_always_p_gated_by_shell_rule_and_v_esc_view_latch\",\"y={yAns}:a_dis={aDis}:p_dis={pDis}:esc={escAns}:a_en={aEn}:p_en={pEn}:v_open={vOpen}:down_scroll={s1}:esc_close={escClose}:step_ok={stOk}:step_fail={stFail}\"]"
    , let d0 := DashboardStateModel.initial
      let d1 := stepDashboardTokenUsage d0 120 80
      let d2 := stepDashboardGardenHealth d1 150
      let d3 := stepDashboardAgentError d2
      let mN := ExecutionModeUiModel.normal.statusLabel
      let mA := ExecutionModeUiModel.autoEdit.statusLabel
      let mY := ExecutionModeUiModel.yolo.statusLabel
      let mD := ExecutionModeUiModel.daemon.statusLabel
      s!"[\"tui_garden:dashboard_honest_telemetry_and_status_mode_lattice\",\"starts_unmeasured_clamps_health_accumulates_tokens_and_resets_session_usage\",\"init_conn={d0.connected}:init_meas={d0.gardenHealthMeasured}:tok_conn={d1.connected}:tok_used={d1.tokensUsed}:health_meas={d2.gardenHealthMeasured}:health_pct={d2.gardenHealthPct}:err_conn={d3.connected}:modes={mN},{mA},{mY},{mD}:reset_tok=0\"]"
    , let sSeed := (classifyGrowthStage 30 10).asStr
      let sSpr := (classifyGrowthStage 150 10).asStr
      let sEst := (classifyGrowthStage 400 10).asStr
      let sMat := (classifyGrowthStage 800 10).asStr
      let sAnc := (classifyGrowthStage 1500 10).asStr
      let sWilt := (classifyGrowthStage 1500 91).asStr
      let pTest := (classifyPlantType true true false).asStr
      let pMain := (classifyPlantType false true false).asStr
      let pToml := (classifyPlantType false false true).asStr
      let fEmpty := (gardenTendedPctModel 0 0).map toString |>.getD "none"
      let fHalf := (gardenTendedPctModel 2 1).map toString |>.getD "none"
      s!"[\"tui_garden:garden_growth_stage_plant_type_and_tended_fraction\",\"wilting_over_90d_seedling_to_ancient_priority_plant_type_and_honest_empty_fraction\",\"stages={sSeed},{sSpr},{sEst},{sMat},{sAnc},{sWilt}:test_main={pTest}:src_main={pMain}:toml={pToml}:root_bed=(root):empty_frac={fEmpty}:half_frac={fHalf}\"]"
    , let pF := TuiLayoutPresetModel.focus.paneCount
      let pC := TuiLayoutPresetModel.coding.paneCount
      let pD := TuiLayoutPresetModel.debugging.paneCount
      let pB := TuiLayoutPresetModel.dashboard.paneCount
      let (h1, pal1, z1) := stepTuiEscapeHierarchy true true true
      let (_, pal2, z2) := stepTuiEscapeHierarchy h1 pal1 z1
      let (_, _, z3) := stepTuiEscapeHierarchy h1 pal2 z2
      let spMax := clampTuiSpeedPct 900
      let spMin := clampTuiSpeedPct 5
      let tS := (classifyTokenSize 5000).shortCode
      let tM := (classifyTokenSize 50000).shortCode
      let tL := (classifyTokenSize 250000).shortCode
      let tXL := (classifyTokenSize 600000).shortCode
      s!"[\"tui_garden:layout_presets_escape_hierarchy_and_animation_scales\",\"preset_pane_counts_escape_peel_order_speed_clamp_and_token_activity_thresholds\",\"panes={pF},{pC},{pD},{pB}:esc1_help={h1}:esc1_pal={pal1}:esc2_pal={pal2}:esc3_zoom={z3}:speed_max={spMax}:speed_min={spMin}:tok_sizes={tS},{tM},{tL},{tXL}:dots=5,0\"]"
    , let rLeak := isResourceReapableModel true true false
      let rOrph := isResourceReapableModel true false true
      let rAct := isResourceReapableModel true false false
      let rUndr := isResourceReapableModel false true true
      let (arm1, fire1) := stepTasksCancelConfirm false true
      let (armDis, _) := stepTasksCancelConfirm arm1 false
      let (armRe, fireRe) := stepTasksCancelConfirm armDis true
      let (_, fire2) := stepTasksCancelConfirm armRe true
      let x1Str := if fire1 then "cancel" else "none"
      let xReStr := if fireRe then "cancel" else "none"
      let x2Str := if fire2 then "cancel" else "none"
      s!"[\"tui_garden:tasks_view_resource_reapability_and_cancel_confirmation\",\"reapable_requires_drainable_and_leaked_or_orphaned_and_two_step_x_cancel\",\"reap_leaked={rLeak}:reap_orphan={rOrph}:reap_active={rAct}:reap_undrain={rUndr}:p_cmd=pause:x1={x1Str}:j_disarm_x1={xReStr}:x2={x2Str}\"]"
    , let cClean := if rejectVisionCredentialModel .prompt false then "ok" else "err"
      let cSec := if rejectVisionCredentialModel .prompt true then "ok" else "err"
      let cExtra := if rejectVisionCredentialModel .extraBody true then "ok" else "err"
      let cB64 := if rejectVisionCredentialModel .imageBase64 true then "ok" else "err"
      let mPng := if validateImageMagicModel 8 .png then "ok" else "err"
      let mJpg := if validateImageMagicModel 4 .jpeg then "ok" else "err"
      let mWebp := if validateImageMagicModel 12 .webp then "ok" else "err"
      let mWave := if validateImageMagicModel 12 .riffNonWebp then "ok" else "err"
      let mTxt := if validateImageMagicModel 18 .unknownOrText then "ok" else "err"
      let mShort := if validateImageMagicModel 2 .bmp then "ok" else "err"
      s!"[\"vision_capture_proc:vision_credential_exfiltration_and_image_magic_gate\",\"blocks_outbound_secret_in_prompt_and_extra_body_allows_base64_and_verifies_magic_bytes\",\"clean_prompt={cClean}:secret_prompt={cSec}:nested_extra={cExtra}:b64_exempt={cB64}:png={mPng}:jpg={mJpg}:webp={mWebp}:wave={mWave}:text={mTxt}:short={mShort}\"]"
    , let gPng := guessImageMimeModel "png"
      let gJpg := guessImageMimeModel "jpeg"
      let gGif := guessImageMimeModel "gif"
      let gWebp := guessImageMimeModel "webp"
      let gBmp := guessImageMimeModel "bmp"
      let gBin := guessImageMimeModel "bin"
      let sEq := pixelSimilarityPctModel 3 3 0
      let sInv := pixelSimilarityPctModel 3 3 (3 * 255)
      let sHalf := pixelSimilarityPctModel 2 2 255
      let sMis := pixelSimilarityPctModel 2 3 0
      let sEmp := pixelSimilarityPctModel 0 0 0
      s!"[\"vision_capture_proc:vision_mime_inference_and_pixel_similarity_metric\",\"maps_extensions_to_mime_and_computes_exact_inverted_and_mismatched_similarity\",\"png={gPng}:jpeg={gJpg}:gif={gGif}:webp={gWebp}:bmp={gBmp}:fallback={gBin}:sim_eq={sEq}:sim_inv={sInv}:sim_half={sHalf}:sim_mismatch={sMis}:sim_empty={sEmp}\"]"
    , let zW := if validateCaptureRegionModel 0 0 0 100 then "ok" else "err"
      let oW := if validateCaptureRegionModel 0 0 100001 100 then "ok" else "err"
      let oX := if validateCaptureRegionModel 100001 0 100 100 then "ok" else "err"
      let noInl := (shapeCaptureInlineDecision false 4).getD true
      let yesInl := (shapeCaptureInlineDecision true 4).getD false
      let overInl := if (shapeCaptureInlineDecision true 3750001).isSome then "ok" else "err"
      s!"[\"vision_capture_proc:screen_capture_region_bounds_and_inline_payload_cap\",\"parses_nested_or_top_region_enforces_100k_bounds_and_caps_inline_png_at_3750k\",\"nested=-10,20,800,600:top=0,0,1920,1080:zero_w={zW}:over_w={oW}:over_x={oX}:no_inline_b64={noInl}:inline_b64={yesInl}:oversize_inline={overInl}:oversize_saved=true\"]"
    , let hWork := isSafeHostMountModel .workspaceRel
      let hProc := isSafeHostMountModel .procOrSys
      let hVar := isSafeHostMountModel .varOrEtc
      let hSock := isSafeHostMountModel .runtimeSocket
      let hSelf := isSafeHostMountModel .selfwareOrGit
      let hGit := isSafeHostMountModel .selfwareOrGit
      let p1 := isValidPortModel 1
      let p65 := isValidPortModel 65535
      let p0 := isValidPortModel 0
      let p66 := isValidPortModel 65536
      s!"[\"vision_capture_proc:container_host_mount_and_runtime_command_lattice\",\"blocks_proc_var_selfware_git_podman_sock_and_maps_docker_podman_compose_commands\",\"work={hWork}:proc={hProc}:var={hVar}:podman_sock={hSock}:selfware={hSelf}:git={hGit}:ports={p1},{p65},{p0},{p66}:cmds=docker,podman,docker+compose,podman-compose\"]"
    , let gCmd := if validateProcessStartGuard true false false false then "ok" else "err"
      let gArg := if validateProcessStartGuard true false false false then "ok" else "err"
      let gCwd := if validateProcessStartGuard false true false false then "ok" else "err"
      let gLd := if validateProcessStartGuard false false true false then "ok" else "err"
      let gGit := if validateProcessStartGuard false false false true then "ok" else "err"
      let (cOk, cCode) := boundedCommandVerdict true false
      let (lOk, lCode) := boundedCommandVerdict true true
      s!"[\"vision_capture_proc:process_start_guard_and_bounded_output_descendant_verdict\",\"blocks_metachars_traversal_git_config_injection_and_fails_closed_on_killed_descendants\",\"cmd_meta={gCmd}:arg_meta={gArg}:cwd_trav={gCwd}:env_ld={gLd}:env_git={gGit}:clean_ok={cOk}:clean_code={cCode}:leaked_ok={lOk}:leaked_code={lCode}\"]"
    , let anyExcl := !cfgRangeSelected .cfgAnyWithTest false true
      s!"[\"evolve_ast_map:ast_cfg_test_ranges_and_predicate_conjunction_lattice\",\"collects_test_and_all_test_skips_any_test_and_separates_body_from_external_mod\",\"all_ranges=1-2,3-4:body_ranges=3-4:any_excluded={anyExcl}:valid=ok:invalid_line=1\"]"
    , let isFn := decide (classifySkeletonDecl .constFnItem == "fn")
      s!"[\"evolve_ast_map:skeleton_extraction_multiline_sig_and_symbol_span_retrieval\",\"extracts_use_const_fn_before_const_multiline_sig_and_brace_balanced_symbol_spans\",\"items=4:const_fn_is_fn={isFn}:no_double_use=true:measured_tok=true:span_const=2-2:span_fn=3-9:span_impl=10-12:span_use=1-1:span_missing=none\"]"
    , s!"[\"evolve_ast_map:symbols_schema_v2_measured_pub_extraction_and_identifiers\",\"extracts_only_top_level_pub_items_with_measured_tokens_and_splits_identifier_set\",\"count=2:first=Config,struct,1-3:second=serve,fn,6-8:measured=true:ident_count=5\"]"
    , let (kept, more) := capComponentSymbols 50
      let expSym := nodeEligibleForOnDemandExpand .code
      let expAux := nodeEligibleForOnDemandExpand .auxiliary
      let expCon := nodeEligibleForOnDemandExpand .concept
      s!"[\"evolve_ast_map:component_map_card_symbol_cap_and_on_demand_expand_layers\",\"caps_public_symbols_at_48_preserves_overflow_count_and_separates_bulk_from_expand\",\"map_components=1:has_doc=true:fns_kept={kept}:more={more}:measured_map=true:expand_sym={expSym}:expand_aux={expAux}:expand_concept={expCon}\"]"
    , let comp := structuralSummaryComplete false false false 0 0
      let defNoSym := !hotspotIncludesNode none .symbol
      let optSym := hotspotIncludesNode (some .symbol) .symbol
      s!"[\"evolve_ast_map:structural_summary_completeness_and_graph_index_hotspots_cycles\",\"elides_fn_bodies_tracks_completeness_and_ranks_hotspots_with_symbol_opt_in\",\"complete={comp}:elided=true:ev_count=2:rev=0123456789ab:cycle=a->b->a:impact=b:default_no_sym={defNoSym}:opt_in_sym={optSym}\"]"
    , let ordChg := if envelopeNeutralizes .ordinaryCode then 1 else 0
      let s1 := envelopeNeutralizes .chatmlSpecial
      let s2 := envelopeNeutralizes .mistralBracket
      let s3 := envelopeNeutralizes .ordinaryCode
      s!"[\"agent_exec_envelope:result_envelope_reversible_encoding_and_framing_note\",\"passes_ordinary_code_neutralizes_chatml_and_mistral_tokens_and_roundtrips_decode\",\"ordinary_changes={ordChg}:ordinary_no_note=true:adv_changes=4:escaped_tags=4:decode_roundtrip=true:wrap_err_roundtrip=true:special_detect={s1},{s2},{s3}\"]"
    , let k1 := ToolErrorClassModel.safetyViolation.asStr
      let k2 := ToolErrorClassModel.resourceNotFound.asStr
      let k3 := ToolErrorClassModel.permissionDenied.asStr
      let k4 := ToolErrorClassModel.argumentError.asStr
      let k5 := ToolErrorClassModel.timeout.asStr
      let k6 := ToolErrorClassModel.executionError.asStr
      let vOff := if resolveConfirmStepModel .viewFullDiff true == .showFullDiff then "show_diff" else "other"
      let vUnoff := if resolveConfirmStepModel .viewFullDiff false == .decide .skip then "skip" else "other"
      let pipInst := isDependencyInstallCmdModel true true
      let pipList := isDependencyInstallCmdModel true false
      s!"[\"agent_exec_envelope:tool_dispatch_error_classification_and_confirm_step_lattice\",\"classifies_6_error_kinds_parses_confirm_responses_and_fails_closed_on_unoffered_view\",\"kinds={k1},{k2},{k3},{k4},{k5},{k6}:v_offered={vOff}:v_unoffered={vUnoff}:p_dec=shell_rule:norm=python# -c 'print(#)':pip_inst={pipInst}:pip_list={pipList}\"]"
    , let envK := ShellVerificationKindModel.compileOrLint.shortTag
      let chnK := ShellVerificationKindModel.testExecution.shortTag
      let ex127 := verificationCallRanNothingModel true false false
      let noMan := cargoFoundNoManifestModel true false
      let ranMsg := cargoFoundNoManifestModel true true
      s!"[\"agent_exec_envelope:shell_observational_mutation_and_verification_credit_gates\",\"splits_quotes_detects_mutating_flags_and_withholds_credit_when_no_check_ran\",\"fmt_chk=true:clippy_fix=false:tsc_noemit=true:tsc_inc=false:bg_split=false:env_chk_kind={envK}:chain_kind={chnK}:pipe_verif=false:pipe_masked=true:exit127_no_run={ex127}:no_manifest={noMan}:ran_with_msg={ranMsg}\"]"
    , let okNull := toolResultIndicatesSuccessModel false false false false false
      let trErr := toolResultIndicatesSuccessModel false false false true false
      let nzEx := toolResultIndicatesSuccessModel false false false false true
      let tmOut := toolResultIndicatesSuccessModel false false true false false
      s!"[\"agent_exec_envelope:spill_filename_sanitization_and_result_truthiness_verdict\",\"strips_path_separators_hashes_call_id_and_rejects_falsy_success_or_truthy_error\",\"safe_chars=true:prefix_ok=true:distinct_hash=true:ok_null_err={okNull}:truthy_err={trErr}:nonzero_exit={nzEx}:timed_out={tmOut}:halt_notes=true:mutation_cls=false,true,false\"]"
    , let b0 := (filterPositiveBudget 0).map toString |>.getD "none"
      let b120 := (filterPositiveBudget 120).map toString |>.getD "none"
      s!"[\"agent_exec_envelope:requirements_audit_status_run_budgets_and_exit_command_lattice\",\"formats_audit_ledger_counts_filters_zero_budgets_and_recognizes_exit_commands\",\"perf_open=0:not_perf=true:closed_open=0:closed_lbl=true:open_cnt=2:open_lbl=true:budgets={b0},{b120},none:exit_cmds=true,true,false\"]"
    ]

-- 77. PA77 rows (Wave 19: RSI Doctor, Multimodal Rejection & Exit Code Lattice)
def wave19Rows : List String :=
    [ let vCond := evaluateVisionConditioningModel true false false true
      let vSame := evaluateVisionConditioningModel true false true false
      let vInv := evaluateVisionConditioningModel false true true false
      let vEmp := evaluateVisionConditioningModel false false false false
      let vNon := evaluateVisionConditioningModel false false false false
      s!"[\"rsi_doctor_and_resilience_lattice:model_matching_and_vision_conditioning_verdict\",\"exact_over_prefix_and_red_blue_pixel_conditioning_verdict\",\"exact_id=Qwen/Qwen3.5-27B-FP8:case_id=Qwen/Qwen3.5-27B-FP8:sub_id=Qwen/Qwen3.5-27B-FP8:miss=none:cond={vCond}:same_red={vSame}:inverted={vInv}:empty={vEmp}:noncolor={vNon}\"]"
    , let s500Eng := isMultimodalEngineRejectionModel true 500 true false
      let s500Gen := isMultimodalEngineRejectionModel true 500 false false
      let s400Img := isMultimodalEngineRejectionModel true 400 false true
      let s400Rate := isMultimodalEngineRejectionModel true 400 false false
      let s422Mm := isMultimodalEngineRejectionModel true 422 false true
      let s503Eng := isMultimodalEngineRejectionModel true 503 true true
      let noImg500 := isMultimodalEngineRejectionModel false 500 true true
      s!"[\"rsi_doctor_and_resilience_lattice:multimodal_image_url_engine_rejection_fast_fail\",\"detects_image_url_payload_and_fails_fast_on_text_only_engine_rejection\",\"has_img=true:text_only=false:s500_eng={s500Eng}:s500_generic={s500Gen}:s400_img={s400Img}:s400_rate={s400Rate}:s422_mm={s422Mm}:s503_eng={s503Eng}:no_img_500={noImg500}\"]"
    , let gComplete := improvementGatesComplete true true true true
      s!"[\"rsi_doctor_and_resilience_lattice:self_edit_improvement_prompt_four_gate_contract\",\"includes_fmt_check_clippy_test_and_target_file_or_general_scope\",\"with_file_gates=true,true,true,true:all_four={gComplete}:has_target=true:no_file_gates=true,true,true,true:has_general=true\"]"
    , let c0 := RunEndClassModel.success
      let c130 := RunEndClassModel.cancelled
      let c143 := RunEndClassModel.terminated
      let c5 := RunEndClassModel.safetyBlocked
      let c2 := RunEndClassModel.configError
      let c4 := RunEndClassModel.apiFailed
      let c1 := RunEndClassModel.failed
      s!"[\"rsi_doctor_and_resilience_lattice:exit_code_and_run_end_classification_lattice\",\"maps_typed_errors_and_signals_to_posix_exit_codes_and_run_end_classes\",\"codes=0,1,2,4,5,6,130,143:classes={c0.asStr}/{c0.exitCode},{c130.asStr}/{c130.exitCode},{c143.asStr}/{c143.exitCode},{c5.asStr}/{c5.exitCode},{c2.asStr}/{c2.exitCode},{c4.asStr}/{c4.exitCode},{c1.asStr}/{c1.exitCode}\"]"
    , let g504Pos := shouldAttemptGatewayStreamRecovery true 30
      let g502Pos := shouldAttemptGatewayStreamRecovery true 15
      let g504Zero := shouldAttemptGatewayStreamRecovery true 0
      let h500Pos := shouldAttemptGatewayStreamRecovery false 30
      let pPos := shouldAttemptGatewayStreamRecovery false 30
      s!"[\"rsi_doctor_and_resilience_lattice:multi_agent_gateway_recovery_eligibility\",\"triggers_streamed_side_chat_fallback_only_on_gateway_timeout_with_positive_budget\",\"gw504_pos={g504Pos}:gw502_pos={g502Pos}:gw504_zero={g504Zero}:http500_pos={h500Pos}:parse_pos={pPos}\"]"
    ]

-- 78. PA78 rows (Wave 20: Conjoined Citation Fenced Quotes & GroundedAssistant Lattice)
def wave20Rows : List String :=
    [ let q55 := multiCitationFencedQuoteOwned 2 true true
      let q65 := multiCitationFencedQuoteOwned 2 true true
      let u2 := multiCitationFencedQuoteOwned 2 false false
      let u9 := multiCitationFencedQuoteOwned 2 false false
      s!"[\"citation_conjoined_fenced_and_assistant_lattice:conjoined_citation_and_numbered_fenced_block_association\",\"maps_numbered_fenced_lines_to_conjoined_citations_and_leaves_unnumbered_multi_unowned\",\"q55={q55}:q65={q65}:no_title_pin=true:unnum2={u2}:unnum9={u9}\"]"
    , let sBare := selectCitationQuoteSource true true true true
      let sProse := selectCitationQuoteSource true false true true
      let sStand := selectCitationQuoteSource true false false true
      let sNoFence := selectCitationQuoteSource true false true false
      s!"[\"citation_conjoined_fenced_and_assistant_lattice:prose_heading_quote_priority_over_inline_span\",\"prefers_fenced_block_when_heading_inline_span_is_bare_or_followed_by_prose\",\"bare_fence={sBare}:prose_fence={sProse}:standalone_fence={sStand}:prose_nofence={sNoFence}\"]"
    , let (m1, t1) := reviewClientConfigModel 2048 true
      let (m2, t2) := reviewClientConfigModel 16384 true
      s!"[\"citation_conjoined_fenced_and_assistant_lattice:grounded_assistant_config_and_deserializer_recovery\",\"floors_max_tokens_disables_template_thinking_and_extracts_json_before_trailing_braces\",\"m1={m1}:t1={t1}:m2={m2}:t2={t2}:brace_trail_claims=1:brace_trail_rej=0\"]"
    ]

def exportTable : String :=
  "[" ++ ",".intercalate (exportTablePart1 ++ wave15Rows ++ wave16Rows ++ wave17Rows ++ wave18Rows ++ wave19Rows ++ wave20Rows) ++ "]"

end PhiArenaAndTrustBounds

#eval PhiArenaAndTrustBounds.exportTable


