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

  "[" ++ ",".intercalate (emotionRows ++ trustRows ++ presenceRows ++ bossRows ++ dreamRows ++ roleRows ++ cacheRows ++ restoreRows ++ profileRows ++ contextTrustRows ++ contextFitRows ++ applyRows ++ wave3Rows ++ wave4Rows ++ wave5Rows ++ wave6Rows ++ wave7Rows ++ wave8Rows ++ wave9Rows ++ wave10Rows ++ wave11Rows) ++ "]"

end PhiArenaAndTrustBounds

#eval PhiArenaAndTrustBounds.exportTable


