/-
  selfware resource lifecycle — transition table (formal model).
  Core Lean 4 only (no Mathlib). Check with: `lean ResourceFsm.lean`.

  `TaskFsm.lean` proves the *timed* part of teardown (P3–P5: finishing a
  task starts draining everything it owns, a draining resource settles by
  its deadline whatever the environment does, the reaper drains orphans) on a
  four-state abstraction (live / draining / released / leaked). This file is
  the full seven-state table the registry (`src/resources`) actually runs
  through `lifecycle::ResourceMachine`, exported as
  `formal/resource_table.json` for the Rust conformance test.

  What is proved here:
  * R1 `released` is sticky: no event leaves it.
  * R2 `leaked` is left only by `reap` (the reaper retries) or `stopped`
    (reconciliation found it gone).
  * R3 every exit of `draining` is settled (released or leaked), and its
    deadline exit exists — the table half of P4.
  * R4 release only on confirmation: the only ways into `released` are
    `stopped` (the resource was observed gone) and `drain` of a resource
    that was never started. A drain that gives up (`abandon`) or runs out
    of time (`deadline_passed`) can never be reported as released.
  * R5 nothing is stuck: every unsettled state has an event into
    `draining` or into a settled state (so teardown / the reaper can always
    act, and P4 then bounds it).
  * R6 the reaper path of P5: live ─owner_gone→ orphaned ─reap→ draining,
    and a leaked resource can be retried (leaked ─reap→ draining).
-/

namespace Selfware.Res

inductive ResourceState where
  | requested | starting | live | orphaned | draining | released | leaked
  deriving DecidableEq, Repr

inductive ResourceEvent where
  | start | ready | fail | drain | stopped | deadlinePassed | ownerGone | reap
  | abandon
  deriving DecidableEq, Repr

def ResourceState.settled : ResourceState → Bool
  | .released | .leaked => true
  | _ => false

/-- The transition table. `none` = refused (in Rust: a typed
    `InvalidTransition`, logged, never a panic). -/
def step : ResourceState → ResourceEvent → Option ResourceState
  | .requested, .start          => some .starting
  | .requested, .drain          => some .released
  | .starting,  .ready          => some .live
  | .starting,  .fail           => some .leaked
  | .starting,  .drain          => some .draining
  | .live,      .drain          => some .draining
  | .live,      .ownerGone      => some .orphaned
  | .live,      .stopped        => some .released
  | .orphaned,  .reap           => some .draining
  | .orphaned,  .stopped        => some .released
  | .draining,  .stopped        => some .released
  | .draining,  .deadlinePassed => some .leaked
  -- the drain gave up before its deadline: the handle is foreign or its
  -- state unknown, the kind is not stopped automatically, or the stopped
  -- resource could not be removed. Never a release.
  | .draining,  .abandon        => some .leaked
  | .leaked,    .reap           => some .draining
  | .leaked,    .stopped        => some .released
  | _,          _               => none

def allStates : List ResourceState :=
  [.requested, .starting, .live, .orphaned, .draining, .released, .leaked]

def allEvents : List ResourceEvent :=
  [.start, .ready, .fail, .drain, .stopped, .deadlinePassed, .ownerGone, .reap,
   .abandon]

theorem allStates_complete (s : ResourceState) : s ∈ allStates := by
  cases s <;> simp [allStates]

theorem allEvents_complete (e : ResourceEvent) : e ∈ allEvents := by
  cases e <;> simp [allEvents]

/-! ### R1 — `released` is sticky -/

theorem released_sticky_table : ∀ e ∈ allEvents, step .released e = none := by
  decide

theorem released_sticky (e : ResourceEvent) : step .released e = none :=
  released_sticky_table e (allEvents_complete e)

/-! ### R2 — `leaked` is left only by `reap` or `stopped` -/

theorem leaked_exits_table :
    ∀ e ∈ allEvents, step .leaked e ≠ none → e = .reap ∨ e = .stopped := by
  decide

theorem leaked_exits (e : ResourceEvent) (h : step .leaked e ≠ none) :
    e = .reap ∨ e = .stopped :=
  leaked_exits_table e (allEvents_complete e) h

/-! ### R3 — every exit of `draining` is settled; the deadline exit exists -/

theorem draining_exits_settle_table :
    ∀ e ∈ allEvents, ∀ t ∈ allStates, step .draining e = some t → t.settled = true := by
  decide

theorem draining_exits_settle (e : ResourceEvent) (t : ResourceState)
    (h : step .draining e = some t) : t.settled = true :=
  draining_exits_settle_table e (allEvents_complete e) t (allStates_complete t) h

theorem draining_has_deadline_exit : step .draining .deadlinePassed = some .leaked := rfl

/-! ### R4 — `released` only on confirmation -/

theorem release_only_confirmed_table :
    ∀ s ∈ allStates, ∀ e ∈ allEvents, step s e = some .released →
      e = .stopped ∨ (s = .requested ∧ e = .drain) := by
  decide

theorem release_only_confirmed (s : ResourceState) (e : ResourceEvent)
    (h : step s e = some .released) : e = .stopped ∨ (s = .requested ∧ e = .drain) :=
  release_only_confirmed_table s (allStates_complete s) e (allEvents_complete e) h

/-! ### R5 — nothing is stuck: every unsettled state can be taken into
    teardown (draining) or settled directly -/

def canAct (s : ResourceState) : Bool :=
  allEvents.any fun e =>
    match step s e with
    | some t => t == .draining || t.settled
    | none => false

theorem unsettled_can_act_table :
    ∀ s ∈ allStates, s.settled = false → canAct s = true := by
  decide

theorem unsettled_can_act (s : ResourceState) (h : s.settled = false) : canAct s = true :=
  unsettled_can_act_table s (allStates_complete s) h

/-! ### R6 — the reaper path (P5 in the seven-state table) -/

theorem orphan_is_reaped :
    (step .live .ownerGone).bind (fun t => step t .reap) = some .draining := rfl

theorem leak_is_retryable : step .leaked .reap = some .draining := rfl

/-! ## Export: the transition table the Rust implementation must match -/

def ResourceState.name : ResourceState → String
  | .requested => "requested" | .starting => "starting" | .live => "live"
  | .orphaned => "orphaned" | .draining => "draining" | .released => "released"
  | .leaked => "leaked"

def ResourceEvent.name : ResourceEvent → String
  | .start => "start" | .ready => "ready" | .fail => "fail" | .drain => "drain"
  | .stopped => "stopped" | .deadlinePassed => "deadline_passed"
  | .ownerGone => "owner_gone" | .reap => "reap" | .abandon => "abandon"

def exportTable : String :=
  let rows := allStates.foldr (fun s acc =>
    allEvents.foldr (fun e acc2 =>
      match step s e with
      | some t => s!"[\"{s.name}\",\"{e.name}\",\"{t.name}\"]" :: acc2
      | none => acc2) acc) []
  "[" ++ ",".intercalate rows ++ "]"

end Selfware.Res

#eval Selfware.Res.exportTable
