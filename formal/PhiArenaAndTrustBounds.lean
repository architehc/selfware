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

  "[" ++ ",".intercalate (emotionRows ++ trustRows ++ presenceRows ++ bossRows ++ dreamRows ++ roleRows ++ cacheRows ++ restoreRows ++ profileRows ++ contextTrustRows ++ contextFitRows ++ applyRows ++ wave3Rows) ++ "]"

end PhiArenaAndTrustBounds

#eval PhiArenaAndTrustBounds.exportTable

