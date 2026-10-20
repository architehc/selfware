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

/-! ## Export deterministic 3-column table for `scripts/check_formal.sh` & Rust conformance tests -/

def exportTable : String :=
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

  "[" ++ ",".intercalate (emotionRows ++ trustRows ++ presenceRows ++ bossRows ++ dreamRows ++ roleRows ++ cacheRows ++ restoreRows ++ profileRows ++ contextTrustRows ++ contextFitRows ++ applyRows ++ wave3Rows ++ wave4Rows ++ wave5Rows ++ wave6Rows) ++ "]"

end PhiArenaAndTrustBounds

#eval PhiArenaAndTrustBounds.exportTable


