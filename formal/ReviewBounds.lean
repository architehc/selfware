/-
  selfware code-review pipeline — formal model, core Lean 4 (no Mathlib).
  Check with: `lean ReviewBounds.lean` (scripts/check_formal.sh does, and
  compares the exported tables with formal/review_*_table.json).

  Code modelled: src/agent/review_coverage.rs (the coverage ledger, the
  completion gate `ReviewSession::decide`, the report), src/agent/
  review_shards.rs (`schedule_shards`, `wall_dispatch_stop` + the reserve
  cut, `verify_quote`), src/analysis/repo_inventory.rs (the plan: relevant
  files with their line counts, plus the in-scope files it could not read).

  What is proved:
  * RV1 coverage: recording a delivered range only grows a file's covered
    set (`merge_grows`, `record_monotone`); covered ≤ relevant
    (`covered_le_relevant`); the percentage is 100 only for a complete
    review (`percent_100_only_complete`) and is 100 when it is
    (`percent_complete`).
  * RV2 honest completeness: `complete` ⇔ every planned file fully covered
    ∧ no unreadable in-scope file (`complete_iff`, `readAll_iff`); a
    PARTIAL report stays PARTIAL through anything that adds no coverage
    (`partial_stays_partial`).
  * RV3 gate termination: a fresh gate refuses for coverage at most 2·L+2
    times (L = relevant lines) whatever the model answers
    (`gate_terminates`), for citations at most once
    (`citation_refusals_at_most_once`), and — with the iteration reserve —
    at most max_iterations − 3 times in all
    (`gate_refusals_within_iterations`); the accepted answer's verdict is
    green only when coverage is complete (`never_silently_green`).
  * RV4 shard scheduling: at most `cap` shards in flight (`cap_invariant`);
    each shard is dispatched at most twice (`at_most_two_attempts`); after
    the end sweep every shard is succeeded, failed (after its retry) or not
    run (`none_lost`, `all_terminal_after_sweep`); a shard is credited
    (`ok`) iff it ends succeeded (`ok_means_succeeded`,
    `succeeded_means_ok`).
  * RV5 answer reserve: the dispatch rule alone keeps the reserve only up to
    the amount calls outrun their estimate (`reserve_by_dispatch_rule`), and
    that is not enough (`dispatch_rule_alone_can_eat_reserve`); the reserve
    cut keeps it for any call durations (`reserve_by_cut`).
  * RV7 shard circuit breaker: the breaker trips only on evidence — at
    least min(N, K) ≥ 4 counted failed attempts (`trip_needs_evidence`,
    `sched_trip_needs_evidence`); reserve cuts never count
    (`cuts_never_count`, `trip_guard_ignores_cuts`); the exported first
    trip is where the scheduler's trip guard holds (`firstTrip_sound`); an endpoint that fails every attempt trips it at
    exactly the N-th (`always_failing_trips_at_n`); the recorded live
    core-review runs and a failed first wave that recovers never trip it
    (`live_runs_never_trip`, `transient_burst_never_trips`). A tripped
    scheduler stays tripped (`tripped_sticky`), starts nothing — a queued
    shard stays queued or ends not run (`tripped_starts_nothing`) — and its
    in-flight count only falls (`tripped_in_flight_never_grows`); the cap,
    none-lost and credit-iff-success results of RV4 hold with the breaker
    in the scheduler (per-shard machine unchanged).
  * RV6 findings: a recorded finding cites a delivered line inside the span
    where its quote matched; no match, no finding (`recorded_is_grounded`,
    `no_match_unverified`); the pre-fix rule could cite an unread line
    (`old_rule_cited_an_unread_line`).

  Modelling assumptions (each is a place the proof does not reach):
  * A1 Lines are whole numbers ≥ 1; a file's coverage is a set of lines
    (`Nat → Bool`). The Rust ledger keeps sorted merged ranges; the
    property tests check `merge_range`/`covered_within` against this set
    semantics (review_coverage_formal_test.rs).
  * A2 Coverage only enters through `record` (delivered `file_read`
    ranges, shard deliveries, checkpoint restore). The what-if turn note
    (`review_turn_note_for`) commits pending reads and restores the saved
    map; it is single-threaded, so the committed ledger is unchanged by it.
  * A3 The gate is evaluated once per (step, answer) — `ReviewSession::gate`
    memoizes repeats — and each evaluation happens at a later iteration than
    the previous one (the refused answer costs a model turn). `limit` is
    `review_limit_reached`: the completion-gate step-aside (deadline /
    budget) or `max − used ≤ 3`.
  * A4 `schedule_shards` is modelled per shard plus a global `stopped`
    flag, a global `tripped` flag, the observed attempt outcomes and the
    in-flight count; the Rust test drives the real scheduler and folds the
    observed events through the exported table. The model lets the trip
    happen at any later step while the evidence holds; the runtime trips at
    the completion that first satisfies it (one of those behaviours). The
    breaker's decision is exported per history (`review_breaker_table.json`)
    and the Rust `ShardBreaker` is checked against it.
  * A5 Time is whole seconds. The reserve cut drops every shard call future
    at K (tokio `timeout_at`), K = phase start + remaining − reserve − 1 s
    (the extra second covers the whole-second floor of the elapsed clock).
    Not modelled: timer lateness and the bookkeeping after the last call.
    The runtime MEASURES the longest completed shard call (rule 4) and uses
    it in the dispatch rule; it cannot guarantee that the next call stays
    under that maximum — which is why the cut exists.
  * A6 The quote matcher (normalized substring match of each quote line,
    ≤ 3 elided lines between) is abstracted to the list of delivered lines
    where it matches; the Rust property test compares `verify_quote` with
    this decision on generated slices.
-/

namespace Review

/-! ## RV1 — the coverage ledger -/

/-- Covered lines of `1..n`. -/
def count (cov : Nat → Bool) : Nat → Nat
  | 0 => 0
  | n + 1 => count cov n + (if cov (n + 1) then 1 else 0)

theorem count_le (cov : Nat → Bool) : ∀ n, count cov n ≤ n := by
  intro n; induction n with
  | zero => simp [count]
  | succ n ih => simp only [count]; split <;> omega

theorem count_mono (cov cov' : Nat → Bool) (h : ∀ l, cov l = true → cov' l = true) :
    ∀ n, count cov n ≤ count cov' n := by
  intro n; induction n with
  | zero => simp [count]
  | succ n ih =>
    simp only [count]
    by_cases hc : cov (n + 1) = true
    · have := h _ hc; simp [hc, this]; omega
    · simp [hc]; split <;> omega

/-- `merge_range`: reversed ranges are swapped, line 0 is clamped to 1. -/
def merge (cov : Nat → Bool) (a b : Nat) : Nat → Bool :=
  let lo := max 1 (min a b)
  let hi := max 1 (max a b)
  fun l => cov l || (decide (lo ≤ l) && decide (l ≤ hi))

theorem merge_grows (cov : Nat → Bool) (a b l : Nat) (h : cov l = true) :
    merge cov a b l = true := by simp [merge, h]

structure File where
  lines : Nat
  cov : Nat → Bool

def fileCovered (f : File) : Nat := count f.cov f.lines

def covered : List File → Nat
  | [] => 0
  | f :: fs => fileCovered f + covered fs

def relevant : List File → Nat
  | [] => 0
  | f :: fs => f.lines + relevant fs

/-- `ReviewSession::complete` (every planned file read in full). -/
def readAll : List File → Bool
  | [] => true
  | f :: fs => decide (fileCovered f = f.lines) && readAll fs

theorem covered_le_relevant : ∀ fs, covered fs ≤ relevant fs := by
  intro fs; induction fs with
  | nil => simp [covered, relevant]
  | cons f fs ih =>
    simp only [covered, relevant, fileCovered]
    have := count_le f.cov f.lines; omega

theorem readAll_iff : ∀ fs, readAll fs = true ↔ covered fs = relevant fs := by
  intro fs; induction fs with
  | nil => simp [readAll, covered, relevant]
  | cons f fs ih =>
    simp only [readAll, covered, relevant, Bool.and_eq_true, decide_eq_true_eq]
    have h1 := count_le f.cov f.lines
    have h2 := covered_le_relevant fs
    simp only [fileCovered] at *
    constructor
    · rintro ⟨ha, hb⟩; rw [ih] at hb; omega
    · intro h; refine ⟨by omega, ?_⟩; rw [ih]; omega

def recordAt : List File → Nat → Nat → Nat → List File
  | [], _, _, _ => []
  | f :: fs, 0, a, b => { f with cov := merge f.cov a b } :: fs
  | f :: fs, i + 1, a, b => f :: recordAt fs i a b

theorem record_relevant : ∀ fs i a b, relevant (recordAt fs i a b) = relevant fs := by
  intro fs; induction fs with
  | nil => intro i a b; simp [recordAt]
  | cons f fs ih =>
    intro i a b; cases i with
    | zero => simp [recordAt, relevant]
    | succ i => simp [recordAt, relevant, ih]

theorem record_monotone : ∀ fs i a b, covered fs ≤ covered (recordAt fs i a b) := by
  intro fs; induction fs with
  | nil => intro i a b; simp [recordAt]
  | cons f fs ih =>
    intro i a b; cases i with
    | zero =>
      simp only [recordAt, covered, fileCovered]
      have := count_mono f.cov (merge f.cov a b) (merge_grows f.cov a b) f.lines
      omega
    | succ i => simp only [recordAt, covered]; have := ih i a b; omega

/-- `percent_lines`: floor, capped at 99 unless complete; an empty plan
    (only unreadable files) is 0 % unless complete. -/
def percent (complete : Bool) (cov rel : Nat) : Nat :=
  let p := if rel = 0 then (if complete then 100 else 0) else cov * 100 / rel
  if !complete && decide (100 ≤ p) then 99 else p

theorem percent_100_only_complete (c : Bool) (cov rel : Nat) (h : percent c cov rel = 100) :
    c = true := by
  cases c <;> simp [percent] at h ⊢
  split at h <;> (try split at h) <;> omega

theorem percent_complete (cov rel : Nat) (h : cov = rel) : percent true cov rel = 100 := by
  subst h; unfold percent
  by_cases hr : cov = 0
  · simp [hr]
  · simp [hr]; rw [Nat.mul_div_cancel_left 100 (by omega)]


/-! ## RV2 — honest completeness -/

/-- `ReviewCoverageReport::complete`. -/
def reportComplete (fs : List File) (unreadable : Nat) : Bool :=
  readAll fs && decide (unreadable = 0)

theorem readAll_iff_each : ∀ fs, readAll fs = true ↔ ∀ f ∈ fs, fileCovered f = f.lines := by
  intro fs; induction fs with
  | nil => simp [readAll]
  | cons f fs ih => simp [readAll, ih]

theorem complete_iff (fs : List File) (u : Nat) :
    reportComplete fs u = true ↔ (∀ f ∈ fs, fileCovered f = f.lines) ∧ u = 0 := by
  simp [reportComplete, readAll_iff_each]

/-- `f'` is `f` after steps that added no coverage (same file, no line newly covered). -/
def noNewCoverage (f f' : File) : Prop :=
  f'.lines = f.lines ∧ ∀ l, f'.cov l = true → f.cov l = true

/-- The same plan, file by file, with no line newly covered. -/
def NoNewCoverage : List File → List File → Prop
  | [], [] => True
  | f :: fs, f' :: fs' => noNewCoverage f f' ∧ NoNewCoverage fs fs'
  | _, _ => False

theorem partial_stays_partial :
    ∀ (fs fs' : List File), NoNewCoverage fs fs' → ∀ u,
      reportComplete fs u = false → reportComplete fs' u = false := by
  intro fs
  induction fs with
  | nil =>
    intro fs' h u; cases fs' with
    | nil => simp
    | cons _ _ => simp [NoNewCoverage] at h
  | cons f rest ih0 =>
    intro fs' h u
    cases fs' with
    | nil => simp [NoNewCoverage] at h
    | cons f' rest' =>
    obtain ⟨⟨hl, hc⟩, hrest⟩ := h
    have ih := ih0 rest' hrest u
    intro hfalse
    simp only [reportComplete, readAll, Bool.and_eq_false_iff, decide_eq_false_iff_not] at hfalse ih ⊢
    have hmono := count_mono f'.cov f.cov hc f.lines
    have hle := count_le f.cov f.lines
    simp only [fileCovered] at *
    rw [hl]
    rcases hfalse with (h1 | h1) | h1
    · left; left; omega
    · rcases ih (Or.inl h1) with h2 | h2
      · left; right; exact h2
      · right; exact h2
    · right; exact h1


/-! ## RV3 — the completion gate terminates -/

inductive Decision where
  | accept | refuseCoverage | refuseCitation
  deriving DecidableEq, Repr

structure Gate where
  stopped : Bool
  np : Nat
  last : Option Nat
  nudged : Bool
  deriving DecidableEq, Repr

def fresh : Gate := { stopped := false, np := 0, last := none, nudged := false }

/-- The no-progress counter after this evaluation (`covered <= before`). -/
def nextNp (g : Gate) (cov : Nat) : Nat :=
  match g.last with
  | some b => if cov ≤ b then g.np + 1 else 0
  | none => 0

/-- `ReviewSession::decide`, over what it reads and writes. -/
def gateStep (g : Gate) (complete : Bool) (cov : Nat) (limit cites : Bool) : Decision × Gate :=
  if limit then
    (.accept, { g with stopped := g.stopped || !complete })
  else
    let g1 :=
      if !complete && !g.stopped then
        { g with np := nextNp g cov, stopped := decide (2 ≤ nextNp g cov) }
      else g
    if complete || g1.stopped then
      if !g1.nudged && !cites then (.refuseCitation, { g1 with nudged := true })
      else (.accept, g1)
    else (.refuseCoverage, { g1 with last := some cov })

/-- Potential: refusals for coverage still possible. -/
def phi (L : Nat) (g : Gate) : Nat :=
  if g.stopped then 0 else
  match g.last with
  | none => 2 * L + 2
  | some b => 2 * (L - b) + (1 - g.np)

def lastLe (g : Gate) (c : Nat) : Prop :=
  match g.last with
  | none => True
  | some b => b ≤ c

theorem gate_step_phi (L : Nat) (g : Gate) (c : Nat) (limit cites : Bool)
    (hl : lastLe g c) :
    phi L (gateStep g (decide (L ≤ c)) c limit cites).2
      + (if (gateStep g (decide (L ≤ c)) c limit cites).1 = .refuseCoverage then 1 else 0)
      ≤ phi L g := by
  obtain ⟨st, np, last, nd⟩ := g
  unfold gateStep phi nextNp
  cases limit <;> cases st <;> cases cites <;> cases nd <;>
    cases last <;> simp [lastLe] at hl ⊢ <;>
    (repeat' split) <;> simp_all <;> omega

theorem gate_last (g : Gate) (complete : Bool) (c : Nat) (limit cites : Bool) :
    (gateStep g complete c limit cites).2.last = g.last ∨
    (gateStep g complete c limit cites).2.last = some c := by
  obtain ⟨st, np, last, nd⟩ := g
  unfold gateStep
  by_cases h3 : 2 ≤ nextNp ⟨st, np, last, nd⟩ c <;>
    cases limit <;> cases complete <;> cases st <;> cases nd <;> cases cites <;> simp [h3]

theorem gate_step_lastLe (g : Gate) (complete : Bool) (c c' : Nat) (limit cites : Bool)
    (hl : lastLe g c) (hcc : c ≤ c') :
    lastLe (gateStep g complete c limit cites).2 c' := by
  unfold lastLe at hl ⊢
  rcases gate_last g complete c limit cites with h | h <;> rw [h]
  · split at hl <;> simp_all <;> omega
  · simpa using hcc

/-- One gate evaluation: the covered lines at that moment, whether the
    budget/iteration/deadline limit is reached, whether the answer cites. -/
structure Eval where
  cov : Nat
  limit : Bool
  cites : Bool
  /-- Iterations used when the answer was judged. -/
  used : Nat := 0

/-- Refusals for coverage over a run of evaluations. -/
def coverageRefusals (L : Nat) : Gate → List Eval → Nat
  | _, [] => 0
  | g, e :: es =>
    let r := gateStep g (decide (L ≤ e.cov)) e.cov e.limit e.cites
    (if r.1 = .refuseCoverage then 1 else 0) + coverageRefusals L r.2 es

/-- Coverage is monotone between evaluations (RV1) and starts at or above `lo`. -/
def monotoneFrom : Nat → List Eval → Prop
  | _, [] => True
  | lo, e :: es => lo ≤ e.cov ∧ monotoneFrom e.cov es

theorem coverage_refusals_bounded (L : Nat) :
    ∀ (es : List Eval) (g : Gate) (lo : Nat), lastLe g lo → monotoneFrom lo es →
      coverageRefusals L g es ≤ phi L g := by
  intro es
  induction es with
  | nil => intro g lo _ _; simp [coverageRefusals]
  | cons e es ih =>
    intro g lo hl hm
    obtain ⟨hle, hrest⟩ := hm
    have hl' : lastLe g e.cov := by
      unfold lastLe at hl ⊢; split <;> simp_all <;> omega
    simp only [coverageRefusals]
    have hstep := gate_step_phi L g e.cov e.limit e.cites hl'
    have hnext := ih (gateStep g (decide (L ≤ e.cov)) e.cov e.limit e.cites).2 e.cov
      (gate_step_lastLe g _ e.cov e.cov e.limit e.cites hl' (Nat.le_refl _)) hrest
    omega

/-- RV3: a fresh review gate refuses for coverage at most `2·L + 2` times,
    whatever the model does (L = relevant lines). -/
theorem gate_terminates (L : Nat) (es : List Eval) (hm : monotoneFrom 0 es) :
    coverageRefusals L fresh es ≤ 2 * L + 2 := by
  have := coverage_refusals_bounded L es fresh 0 (by simp [lastLe, fresh]) hm
  simpa [phi, fresh] using this

def citationRefusals (L : Nat) : Gate → List Eval → Nat
  | _, [] => 0
  | g, e :: es =>
    let r := gateStep g (decide (L ≤ e.cov)) e.cov e.limit e.cites
    (if r.1 = .refuseCitation then 1 else 0) + citationRefusals L r.2 es

theorem citation_refusals_at_most_once (L : Nat) :
    ∀ (es : List Eval) (g : Gate),
      citationRefusals L g es ≤ (if g.nudged then 0 else 1) := by
  intro es
  induction es with
  | nil => intro g; simp [citationRefusals]
  | cons e es ih =>
    intro g
    simp only [citationRefusals]
    have h := ih (gateStep g (decide (L ≤ e.cov)) e.cov e.limit e.cites).2
    obtain ⟨st, np, last, nd⟩ := g
    revert h
    unfold gateStep
    cases e.limit <;> cases nd <;> simp <;> (repeat' split) <;> simp_all <;> omega

/-- Every refusal, for coverage or for citations. -/
def refusals (L : Nat) : Gate → List Eval → Nat
  | _, [] => 0
  | g, e :: es =>
    let r := gateStep g (decide (L ≤ e.cov)) e.cov e.limit e.cites
    (if r.1 = .accept then 0 else 1) + refusals L r.2 es

theorem limit_accepts (g : Gate) (complete : Bool) (c : Nat) (cites : Bool) :
    (gateStep g complete c true cites).1 = .accept := by simp [gateStep]

/-- Evaluations that were not under the limit. -/
def unlimited : List Eval → Nat
  | [] => 0
  | e :: es => (if e.limit then 0 else 1) + unlimited es

theorem refusals_le_unlimited (L : Nat) :
    ∀ (es : List Eval) (g : Gate), refusals L g es ≤ unlimited es := by
  intro es
  induction es with
  | nil => intro g; simp [refusals, unlimited]
  | cons e es ih =>
    intro g
    simp only [refusals, unlimited]
    cases hl : e.limit
    · have := ih (gateStep g (decide (L ≤ e.cov)) e.cov false e.cites).2
      simp; split <;> omega
    · have := ih (gateStep g (decide (L ≤ e.cov)) e.cov true e.cites).2
      simp [limit_accepts]; omega

/-- Each evaluation is a later iteration than the one before. -/
def strictFrom : Nat → List Eval → Prop
  | _, [] => True
  | lo, e :: es => lo ≤ e.used ∧ strictFrom (e.used + 1) es

/-- The iteration reserve: the limit holds once `max - used ≤ reserve`. -/
def reserveRespected (max reserve : Nat) : List Eval → Prop
  | [] => True
  | e :: es => (max - e.used ≤ reserve → e.limit = true) ∧ reserveRespected max reserve es

theorem unlimited_le (max reserve : Nat) :
    ∀ (es : List Eval) (lo : Nat), strictFrom lo es → reserveRespected max reserve es →
      unlimited es ≤ (max - reserve) - lo := by
  intro es
  induction es with
  | nil => intro lo _ _; simp [unlimited]
  | cons e es ih =>
    intro lo hs hr
    obtain ⟨h1, h2⟩ := hs
    obtain ⟨h3, h4⟩ := hr
    have := ih (e.used + 1) h2 h4
    simp only [unlimited]
    cases hl : e.limit
    · have : ¬ (max - e.used ≤ reserve) := fun h => by simp [h3 h] at hl
      simp; omega
    · simp; omega

/-- RV3 (iterations): with `REVIEW_ITERATION_RESERVE = 3`, a task with
    `max` iterations sees at most `max - 3` refusals of any kind from the
    review gate. -/
theorem gate_refusals_within_iterations (L max : Nat) (es : List Eval) (g : Gate)
    (hs : strictFrom 0 es) (hr : reserveRespected max 3 es) :
    refusals L g es ≤ max - 3 := by
  have h1 := refusals_le_unlimited L es g
  have h2 := unlimited_le max 3 es 0 hs hr
  omega

/-- The run's verdict after an accepted review answer: `with_review_coverage`
    folds PARTIAL into a non-failure verdict and the banner withholds ✅. -/
def verdictGreen (baseGreen : Bool) (complete : Bool) : Bool := baseGreen && complete

theorem never_silently_green (b c : Bool) (h : verdictGreen b c = true) : c = true := by
  cases c <;> simp_all [verdictGreen]

/-! ## RV4 — shard scheduling -/

inductive Shard where
  | queued0 | flying0 | queued1 | flying1 | succeeded | failed | notRun
  deriving DecidableEq, Repr

inductive SEvent where
  | dispatch | ok | err | abort | sweep
  deriving DecidableEq, Repr

def Shard.terminal : Shard → Bool
  | .succeeded | .failed | .notRun => true
  | _ => false

def Shard.flying : Shard → Bool
  | .flying0 | .flying1 => true
  | _ => false

def Shard.queued : Shard → Bool
  | .queued0 | .queued1 => true
  | _ => false

/-- One shard's life in `schedule_shards`. -/
def sstep : Shard → SEvent → Option Shard
  | .queued0, .dispatch => some .flying0
  | .queued1, .dispatch => some .flying1
  | .flying0, .ok       => some .succeeded
  | .flying1, .ok       => some .succeeded
  | .flying0, .err      => some .queued1   -- re-queued once, thinking off
  | .flying1, .err      => some .failed    -- unread after the retry
  | .flying0, .abort    => some .notRun    -- dropped in flight
  | .flying1, .abort    => some .notRun
  | .queued0, .sweep    => some .notRun    -- never dispatched (stop)
  | .queued1, .sweep    => some .notRun
  | _,        _         => none

def allShard : List Shard := [.queued0, .flying0, .queued1, .flying1, .succeeded, .failed, .notRun]
def allSEvent : List SEvent := [.dispatch, .ok, .err, .abort, .sweep]

theorem allShard_complete (s : Shard) : s ∈ allShard := by cases s <;> decide
theorem allSEvent_complete (e : SEvent) : e ∈ allSEvent := by cases e <;> decide

theorem terminal_absorbing (s : Shard) (e : SEvent) (h : s.terminal = true) :
    sstep s e = none := by
  cases s <;> cases e <;> simp_all [Shard.terminal, sstep]

def srun : Shard → List SEvent → Option Shard
  | s, [] => some s
  | s, e :: es => match sstep s e with
    | some t => srun t es
    | none => none

def dispatches : List SEvent → Nat
  | [] => 0
  | e :: es => (if e = .dispatch then 1 else 0) + dispatches es

/-- Dispatches still possible from a state. -/
def budget : Shard → Nat
  | .queued0 => 2 | .flying0 => 1 | .queued1 => 1
  | _ => 0

theorem attempts_bounded :
    ∀ (es : List SEvent) (s t : Shard), srun s es = some t → dispatches es ≤ budget s := by
  intro es
  induction es with
  | nil => intro s t _; simp [dispatches]
  | cons e es ih =>
    intro s t h
    simp only [srun] at h
    split at h
    · rename_i u hu
      have := ih u t h
      simp only [dispatches]
      cases s <;> cases e <;> simp [sstep] at hu <;> subst hu <;> simp [budget] at this ⊢ <;> omega
    · simp at h

/-- RV4: each shard is attempted at most twice. -/
theorem at_most_two_attempts (es : List SEvent) (t : Shard)
    (h : srun .queued0 es = some t) : dispatches es ≤ 2 := by
  simpa [budget] using attempts_bounded es .queued0 t h

theorem srun_terminal (s : Shard) (hs : s.terminal = true) :
    ∀ es t, srun s es = some t → t = s := by
  intro es
  induction es with
  | nil => intro t h; simp [srun] at h; exact h.symm
  | cons e es _ =>
    intro t h
    simp [srun, terminal_absorbing s e hs] at h

/-- RV4: coverage is credited only on `ok`, and an `ok` ends the shard
    `succeeded`: a shard that ends failed or not run was never credited. -/
theorem ok_means_succeeded :
    ∀ (es : List SEvent) (s t : Shard), srun s es = some t → SEvent.ok ∈ es →
      t = .succeeded := by
  intro es
  induction es with
  | nil => intro s t _ h; simp at h
  | cons e es ih =>
    intro s t h hmem
    simp only [srun] at h
    split at h
    · rename_i u hu
      by_cases he : e = .ok
      · subst he
        have hu' : u = .succeeded := by cases s <;> simp_all [sstep]
        subst hu'
        exact srun_terminal .succeeded rfl es t h
      · have : SEvent.ok ∈ es := by
          rcases List.mem_cons.mp hmem with h' | h'
          · exact absurd h'.symm he
          · exact h'
        exact ih u t h this
    · simp at h

theorem succeeded_means_ok :
    ∀ (es : List SEvent) (s : Shard), s ≠ .succeeded →
      srun s es = some .succeeded → SEvent.ok ∈ es := by
  intro es
  induction es with
  | nil => intro s hs h; simp [srun] at h; exact absurd h hs
  | cons e es ih =>
    intro s hs h
    simp only [srun] at h
    split at h
    · rename_i u hu
      by_cases he : e = .ok
      · simp [he]
      · have hu' : u ≠ .succeeded := by
          intro hc; subst hc; cases s <;> cases e <;> simp_all [sstep]
        exact List.mem_cons_of_mem _ (ih u hu' h)
    · simp at h

/-- The end-of-phase sweep: what is still queued was never run. -/
def sweepAll (s : Shard) : Shard := if s.queued then .notRun else s

/-- RV4 (none lost): once nothing is in flight, the sweep leaves every
    shard succeeded, failed after its retry, or not run. -/
theorem none_lost (s : Shard) (hf : s.flying = false) : (sweepAll s).terminal = true := by
  cases s <;> simp_all [sweepAll, Shard.flying, Shard.queued, Shard.terminal]

theorem sweep_is_step (s : Shard) (hq : s.queued = true) : sstep s .sweep = some (sweepAll s) := by
  cases s <;> simp_all [sweepAll, Shard.queued, sstep]

/-! ## RV7 — the shard circuit breaker

`ShardBreaker` (src/agent/review_shards.rs) watches completed shard
attempts and stops dispatching when shards fail systematically — an
endpoint that cannot produce the JSON shard answer, or one that only
returns errors. Reserve cuts are the harness's own doing and are not
evidence about the endpoint: they are left out. Two rules, over the
counted attempts in completion order:

* never worked: the first N all failed, N = min 16 (max 4 (2 · cap)) —
  two waves of the parallelism, so one failed wave (its calls hit the
  endpoint at the same moment) is not enough on its own;
* collapsed: at least K = 12 of the last W = 16 failed.

Measured on the recorded core reviews (125 shards, 6 in parallel): at most
1 failure at the start of a run and at most 5 in any 16 consecutive
attempts (`live_runs_never_trip`). -/

inductive Out where
  | ok | fail | cut
  deriving DecidableEq, Repr

/-- The attempts the breaker counts, in order: `true` = failed. -/
def counted : List Out → List Bool
  | [] => []
  | .ok :: os => false :: counted os
  | .fail :: os => true :: counted os
  | .cut :: os => counted os

def fails : List Bool → Nat
  | [] => 0
  | b :: bs => (if b then 1 else 0) + fails bs

/-- N: failed attempts at the start of the phase that trip the breaker. -/
def streakN (cap : Nat) : Nat := min 16 (max 4 (2 * cap))
def breakerWindow : Nat := 16
def breakerFailures : Nat := 12

/-- The breaker's rule on the counted attempts `c` (in completion order). -/
def tripNow (n : Nat) (c : List Bool) : Bool :=
  (decide (n ≤ c.length) && (c.take n).all id) ||
  (decide (breakerWindow ≤ c.length) &&
    decide (breakerFailures ≤ fails (c.drop (c.length - breakerWindow))))

/-- The 1-based index of the outcome at which the breaker first trips. -/
def firstTripAux (n : Nat) : Nat → List Bool → List Out → Option Nat
  | _, _, [] => none
  | i, c, .ok :: os =>
    if tripNow n (c ++ [false]) then some (i + 1) else firstTripAux n (i + 1) (c ++ [false]) os
  | i, c, .fail :: os =>
    if tripNow n (c ++ [true]) then some (i + 1) else firstTripAux n (i + 1) (c ++ [true]) os
  | i, c, .cut :: os => firstTripAux n (i + 1) c os

def firstTrip (cap : Nat) (h : List Out) : Option Nat := firstTripAux (streakN cap) 0 [] h

theorem streakN_ge_four (cap : Nat) : 4 ≤ streakN cap := by
  simp only [streakN]; omega

theorem streakN_le_window (cap : Nat) : streakN cap ≤ breakerWindow := by
  simp only [streakN, breakerWindow]; omega

theorem fails_drop_le : ∀ (c : List Bool) (k : Nat), fails (c.drop k) ≤ fails c := by
  intro c
  induction c with
  | nil => intro k; simp [fails]
  | cons b bs ih =>
    intro k
    cases k with
    | zero => simp
    | succ k => simp only [List.drop, fails]; have := ih k; omega

theorem take_all_fails : ∀ (c : List Bool) (n : Nat), n ≤ c.length →
    (c.take n).all id = true → n ≤ fails c := by
  intro c
  induction c with
  | nil => intro n h _; simp at h; omega
  | cons b bs ih =>
    intro n h hall
    cases n with
    | zero => omega
    | succ m =>
      simp only [List.take, List.all_cons, Bool.and_eq_true, id] at hall
      obtain ⟨hb, hrest⟩ := hall
      have hlen : m ≤ bs.length := by simp only [List.length_cons] at h; omega
      have := ih m hlen hrest
      simp [fails, hb]; omega

/-- RV7: a trip needs at least min(N, K) counted failures — at least 4. -/
theorem trip_needs_evidence (n : Nat) (c : List Bool) (h : tripNow n c = true) :
    min n breakerFailures ≤ fails c := by
  simp only [tripNow, Bool.or_eq_true, Bool.and_eq_true, decide_eq_true_eq] at h
  rcases h with ⟨hlen, hall⟩ | ⟨_, hk⟩
  · have := take_all_fails c n hlen hall; omega
  · have := fails_drop_le c (c.length - breakerWindow); omega

theorem four_le_evidence (cap : Nat) : 4 ≤ min (streakN cap) breakerFailures := by
  have := streakN_ge_four cap; simp only [breakerFailures]; omega

/-- RV7: reserve cuts never count — dropping every cut from the history
    changes neither whether nor after how many counted attempts it trips. -/
theorem cuts_never_count (n : Nat) : ∀ (h : List Out) (i j : Nat) (c : List Bool),
    (firstTripAux n i c h).isSome = (firstTripAux n j c (h.filter (· != .cut))).isSome := by
  intro h
  induction h with
  | nil => intro i j c; simp [firstTripAux]
  | cons o os ih =>
    intro i j c
    cases o with
    | cut => simp only [firstTripAux, List.filter_cons]; simpa using ih (i + 1) j c
    | ok =>
      simp only [firstTripAux, List.filter_cons]
      simp only [bne_iff_ne, ne_eq, reduceCtorEq, not_false_eq_true, ↓reduceIte, firstTripAux]
      split
      · simp
      · exact ih (i + 1) (j + 1) _
    | fail =>
      simp only [firstTripAux, List.filter_cons]
      simp only [bne_iff_ne, ne_eq, reduceCtorEq, not_false_eq_true, ↓reduceIte, firstTripAux]
      split
      · simp
      · exact ih (i + 1) (j + 1) _

/-- RV7: a reserve cut adds nothing to what the scheduler's trip guard
    (`Move.trip`) looks at. -/
theorem counted_append : ∀ (a b : List Out), counted (a ++ b) = counted a ++ counted b := by
  intro a
  induction a with
  | nil => intro b; simp [counted]
  | cons o os ih => intro b; cases o <;> simp [counted, ih]

theorem trip_guard_ignores_cuts (n : Nat) (obs : List Out) :
    tripNow n (counted (obs ++ [.cut])) = tripNow n (counted obs) := by
  simp [counted_append, counted]

theorem firstTripAux_gt (n : Nat) : ∀ (h : List Out) (i : Nat) (c : List Bool) (k : Nat),
    firstTripAux n i c h = some k → i < k := by
  intro h
  induction h with
  | nil => intro i c k hk; simp [firstTripAux] at hk
  | cons o os ih =>
    intro i c k hk
    cases o with
    | cut => have := ih (i + 1) c k (by simpa [firstTripAux] using hk); omega
    | ok =>
      simp only [firstTripAux] at hk
      split at hk
      · simp at hk; omega
      · have := ih (i + 1) _ k hk; omega
    | fail =>
      simp only [firstTripAux] at hk
      split at hk
      · simp at hk; omega
      · have := ih (i + 1) _ k hk; omega

/-- RV7: the exported first trip is where the scheduler's guard first
    holds: after the first `k` outcomes, the counted attempts satisfy
    `tripNow` (the `Move.trip` premise). -/
theorem firstTrip_sound (n : Nat) : ∀ (h : List Out) (i : Nat) (c : List Bool) (k : Nat),
    firstTripAux n i c h = some k → tripNow n (c ++ counted (h.take (k - i))) = true := by
  intro h
  induction h with
  | nil => intro i c k hk; simp [firstTripAux] at hk
  | cons o os ih =>
    intro i c k hk
    have hgt := firstTripAux_gt n (o :: os) i c k hk
    obtain ⟨m, hm⟩ : ∃ m, k - i = m + 1 := ⟨k - i - 1, by omega⟩
    rw [hm, List.take_succ_cons]
    cases o with
    | cut =>
      have hk' : firstTripAux n (i + 1) c os = some k := by simpa [firstTripAux] using hk
      have := ih (i + 1) c k hk'
      have hm' : k - (i + 1) = m := by omega
      simpa [counted, hm'] using this
    | ok =>
      simp only [firstTripAux] at hk
      split at hk
      · rename_i ht
        simp at hk
        have : m = 0 := by omega
        subst this
        simpa [counted] using ht
      · have := ih (i + 1) _ k hk
        have hm' : k - (i + 1) = m := by omega
        simpa [counted, hm'] using this
    | fail =>
      simp only [firstTripAux] at hk
      split at hk
      · rename_i ht
        simp at hk
        have : m = 0 := by omega
        subst this
        simpa [counted] using ht
      · have := ih (i + 1) _ k hk
        have hm' : k - (i + 1) = m := by omega
        simpa [counted, hm'] using this

/-- Run-length encoding for the recorded histories below. -/
def rle : List (Nat × Out) → List Out
  | [] => []
  | (k, o) :: rs => List.replicate k o ++ rle rs

/-- Completed attempts of the recorded 125-shard core reviews on
    llm.selfware.design (6 in parallel), from each run's `review_shard`
    progress events: 8–12 first-attempt failures, 0–2 double failures,
    retries clustered at the end (they are queued at the back). -/
def liveRuns : List (String × List Out) := [
  ("core_long_1f96", rle [(5, .ok), (1, .fail), (9, .ok), (1, .fail), (16, .ok), (1, .fail),
    (8, .ok), (1, .fail), (26, .ok), (1, .fail), (4, .ok), (1, .fail), (42, .ok), (1, .fail),
    (2, .ok), (1, .fail), (10, .ok), (1, .fail), (1, .ok), (1, .fail)]),
  ("core_long_dcbc", rle [(3, .ok), (1, .fail), (2, .ok), (1, .fail), (7, .ok), (1, .fail),
    (12, .ok), (1, .fail), (5, .ok), (1, .fail), (3, .ok), (1, .fail), (20, .ok), (1, .fail),
    (5, .ok), (1, .fail), (57, .ok), (1, .fail), (2, .ok), (1, .fail), (3, .ok), (1, .fail),
    (5, .ok), (1, .fail), (3, .ok)]),
  ("core_long_3c7d", rle [(14, .ok), (1, .fail), (21, .ok), (1, .fail), (8, .ok), (1, .fail),
    (28, .ok), (1, .fail), (16, .ok), (1, .fail), (31, .ok), (1, .fail), (7, .ok), (1, .fail),
    (1, .ok), (1, .fail), (1, .ok), (2, .fail), (1, .ok)]),
  ("core_long_2744", rle [(8, .ok), (1, .fail), (1, .ok), (1, .fail), (6, .ok), (1, .fail),
    (23, .ok), (1, .fail), (5, .ok), (1, .fail), (3, .ok), (2, .fail), (14, .ok), (1, .fail),
    (8, .ok), (1, .fail), (18, .ok), (1, .fail), (23, .ok), (1, .fail), (16, .ok)]),
  ("core_long_90a7", rle [(3, .ok), (1, .fail), (2, .ok), (1, .fail), (8, .ok), (2, .fail),
    (14, .ok), (1, .fail), (23, .ok), (1, .fail), (41, .ok), (1, .fail), (21, .ok),
    (1, .fail), (13, .ok)]),
  -- The small reviews' only failure pattern: one shard, retried once.
  ("one_shard_retried", rle [(1, .fail), (1, .ok)])]

/-- RV7: no recorded healthy run trips the breaker, at any parallelism. -/
theorem live_runs_never_trip :
    ∀ cap, cap < 9 → ∀ r ∈ liveRuns, firstTrip cap r.2 = none := by decide +kernel

/-- RV7: an endpoint that fails every attempt trips the breaker at exactly
    the N-th attempt, for every N the parallelism can give. -/
theorem always_failing_trips_at_n :
    ∀ n, n < 17 → 4 ≤ n → firstTripAux n 0 [] (List.replicate (n + 20) .fail) = some n := by
  decide +kernel

/-- RV7: a whole failed first wave (6 in parallel, a transient endpoint
    fault) whose retries and followers then succeed never trips; neither
    does one failure in two, sustained. -/
theorem transient_burst_never_trips :
    firstTrip 6 (rle [(6, .fail), (60, .ok), (6, .ok)]) = none ∧
    firstTrip 6 (rle [(3, .ok), (8, .fail), (60, .ok)]) = none ∧
    firstTrip 6 ((List.range 60).map (fun i => if i % 2 = 0 then .fail else .ok)) = none := by
  decide +kernel

/-- …while an endpoint that collapses mid-run trips once 12 of the last 16
    attempts failed. -/
theorem collapse_trips :
    firstTrip 6 (rle [(40, .ok), (30, .fail)]) = some 52 := by decide +kernel

/-! ### The concurrency cap over all shards -/

def fl (s : Shard) : Nat := if s.flying then 1 else 0

def nFlying : List Shard → Nat
  | [] => 0
  | s :: ss => fl s + nFlying ss

def setAt : List Shard → Nat → Shard → List Shard
  | [], _, _ => []
  | _ :: ss, 0, t => t :: ss
  | s :: ss, i + 1, t => s :: setAt ss i t

def getAt : List Shard → Nat → Option Shard
  | [], _ => none
  | s :: _, 0 => some s
  | _ :: ss, i + 1 => getAt ss i

theorem nFlying_setAt : ∀ (ss : List Shard) (i : Nat) (s t : Shard), getAt ss i = some s →
    nFlying (setAt ss i t) + fl s = nFlying ss + fl t := by
  intro ss
  induction ss with
  | nil => intro i s t h; simp [getAt] at h
  | cons x ss ih =>
    intro i s t h
    cases i with
    | zero => simp [getAt] at h; subst h; simp [setAt, nFlying]; omega
    | succ i => simp [getAt] at h; simp [setAt, nFlying]; have := ih i s t h; omega

/-- The scheduler: every shard's state, whether dispatching stopped
    (budget, deadline, reserve, cancel), whether the breaker tripped, and
    the outcomes of the attempts completed so far. -/
structure Sched where
  shards : List Shard
  stopped : Bool
  tripped : Bool
  obs : List Out

/-- What a completed attempt tells the breaker: `ok`; an error, or a
    reserve cut (`cut`, not counted). -/
def outcomeOk (e : SEvent) (o : Out) : Prop :=
  (e = .ok ∧ o = .ok) ∨ (e = .err ∧ (o = .fail ∨ o = .cut))

/-- `schedule_shards`' moves. A dispatch needs a free slot (`in_flight.len()
    < parallelism`), no stop and no trip; a stop (budget, deadline, reserve)
    only ends dispatching; the breaker trips only while its rule holds on
    the outcomes seen, and also only ends dispatching; an abort drops
    everything in flight; the end sweep runs once nothing is in flight. -/
inductive Move (cap : Nat) : Sched → Sched → Prop
  | dispatch (S : Sched) (i : Nat) (s t : Shard) :
      S.stopped = false → S.tripped = false → nFlying S.shards < cap →
      getAt S.shards i = some s → sstep s .dispatch = some t →
      Move cap S ⟨setAt S.shards i t, S.stopped, S.tripped, S.obs⟩
  | finish (S : Sched) (i : Nat) (s t : Shard) (e : SEvent) (o : Out) :
      outcomeOk e o → getAt S.shards i = some s → sstep s e = some t →
      Move cap S ⟨setAt S.shards i t, S.stopped, S.tripped, S.obs ++ [o]⟩
  | stop (S : Sched) : Move cap S ⟨S.shards, true, S.tripped, S.obs⟩
  | trip (S : Sched) : tripNow (streakN cap) (counted S.obs) = true →
      Move cap S ⟨S.shards, S.stopped, true, S.obs⟩
  | abort (S : Sched) :
      Move cap S ⟨S.shards.map (fun s => if s.flying then .notRun else s), true, S.tripped, S.obs⟩
  | sweep (S : Sched) : nFlying S.shards = 0 →
      Move cap S ⟨S.shards.map sweepAll, S.stopped, S.tripped, S.obs⟩

theorem nFlying_map_abort : ∀ ss : List Shard,
    nFlying (ss.map (fun s => if s.flying then .notRun else s)) = 0 := by
  intro ss; induction ss with
  | nil => simp [nFlying]
  | cons s ss ih => cases s <;> simp_all [nFlying, fl, Shard.flying]

theorem nFlying_map_sweep : ∀ ss : List Shard, nFlying (ss.map sweepAll) = nFlying ss := by
  intro ss; induction ss with
  | nil => simp [nFlying]
  | cons s ss ih => cases s <;> simp_all [nFlying, fl, Shard.flying, sweepAll, Shard.queued]

/-- RV4: at most `cap` shards are ever in flight. -/
theorem cap_invariant (cap : Nat) (S S' : Sched) (hm : Move cap S S')
    (h : nFlying S.shards ≤ cap) : nFlying S'.shards ≤ cap := by
  cases hm with
  | dispatch i s t _ _ hlt hg hst =>
    have := nFlying_setAt S.shards i s t hg
    simp only
    cases s <;> cases t <;> simp [sstep] at hst <;> simp [fl, Shard.flying] at this hlt ⊢ <;> omega
  | finish i s t e o he hg hst =>
    have := nFlying_setAt S.shards i s t hg
    simp only
    rcases he with ⟨he, _⟩ | ⟨he, _⟩ <;> subst he <;>
      cases s <;> cases t <;> simp [sstep] at hst <;> simp [fl, Shard.flying] at this ⊢ <;> omega
  | stop => exact h
  | trip _ => exact h
  | abort => simp [nFlying_map_abort]
  | sweep _ => simp [nFlying_map_sweep]; exact h

/-- RV7: the breaker trips only on evidence — at least min(N, K) ≥ 4
    counted failed attempts (`four_le_evidence`); cuts are not counted. -/
theorem sched_trip_needs_evidence (cap : Nat) (S S' : Sched) (hm : Move cap S S')
    (h0 : S.tripped = false) (h1 : S'.tripped = true) :
    min (streakN cap) breakerFailures ≤ fails (counted S.obs) := by
  cases hm with
  | trip ht => exact trip_needs_evidence _ _ ht
  | dispatch => simp_all
  | finish => simp_all
  | stop => simp_all
  | abort => simp_all
  | sweep => simp_all

/-- RV7: once tripped, always tripped. -/
theorem tripped_sticky (cap : Nat) (S S' : Sched) (hm : Move cap S S')
    (h : S.tripped = true) : S'.tripped = true := by
  cases hm <;> simp_all

theorem getAt_setAt_other : ∀ (ss : List Shard) (i j : Nat) (t : Shard), j ≠ i →
    getAt (setAt ss j t) i = getAt ss i := by
  intro ss
  induction ss with
  | nil => intro i j t _; simp [setAt, getAt]
  | cons x ss ih =>
    intro i j t hne
    cases j with
    | zero => cases i with
      | zero => exact absurd rfl hne
      | succ i => simp [setAt, getAt]
    | succ j => cases i with
      | zero => simp [setAt, getAt]
      | succ i => simp only [setAt, getAt]; exact ih i j t (by omega)

theorem getAt_map (f : Shard → Shard) : ∀ (ss : List Shard) (i : Nat),
    getAt (ss.map f) i = (getAt ss i).map f := by
  intro ss
  induction ss with
  | nil => intro i; simp [getAt]
  | cons x ss ih => intro i; cases i <;> simp [getAt, ih]

/-- RV7: a tripped scheduler starts nothing: a queued shard (a first
    attempt or a queued retry) stays queued, or the end sweep marks it not
    run. Calls already in flight may still finish. -/
theorem tripped_starts_nothing (cap : Nat) (S S' : Sched) (hm : Move cap S S')
    (h : S.tripped = true) (i : Nat) (s : Shard) (hg : getAt S.shards i = some s)
    (hq : s.queued = true) :
    getAt S'.shards i = some s ∨ getAt S'.shards i = some .notRun := by
  cases hm with
  | dispatch => simp_all
  | finish j s0 t0 e o he hg0 hst =>
    left
    simp only
    by_cases hij : j = i
    · subst hij
      rw [hg] at hg0
      cases hg0
      rcases he with ⟨he, _⟩ | ⟨he, _⟩ <;> subst he <;> cases s <;> simp_all [sstep, Shard.queued]
    · rw [getAt_setAt_other _ _ _ _ hij]; exact hg
  | stop => left; exact hg
  | trip _ => left; exact hg
  | abort =>
    left
    simp only [getAt_map, hg, Option.map_some]
    cases s <;> simp_all [Shard.queued, Shard.flying]
  | sweep _ =>
    right
    simp only [getAt_map, hg, Option.map_some]
    cases s <;> simp_all [Shard.queued, sweepAll]

/-- RV7: once tripped, the number of calls in flight never grows. -/
theorem tripped_in_flight_never_grows (cap : Nat) (S S' : Sched) (hm : Move cap S S')
    (h : S.tripped = true) : nFlying S'.shards ≤ nFlying S.shards := by
  cases hm with
  | dispatch => simp_all
  | finish i s t e o he hg hst =>
    have := nFlying_setAt S.shards i s t hg
    simp only
    rcases he with ⟨he, _⟩ | ⟨he, _⟩ <;> subst he <;>
      cases s <;> cases t <;> simp [sstep] at hst <;> simp [fl, Shard.flying] at this ⊢ <;> omega
  | stop => exact Nat.le_refl _
  | trip _ => exact Nat.le_refl _
  | abort => simp [nFlying_map_abort]
  | sweep _ => simp [nFlying_map_sweep]

/-- RV4 (none lost, whole phase): after the end sweep with nothing in
    flight, every shard is terminal. -/
theorem all_terminal_after_sweep : ∀ ss : List Shard, nFlying ss = 0 →
    ∀ s ∈ ss.map sweepAll, s.terminal = true := by
  intro ss
  induction ss with
  | nil => simp
  | cons x ss ih =>
    intro h s hs
    simp only [nFlying] at h
    simp only [List.map, List.mem_cons] at hs
    rcases hs with hs | hs
    · subst hs; apply none_lost; cases x <;> simp_all [fl, Shard.flying]
    · exact ih (by omega) s hs

/-! ## RV5 — the synthesis reserve -/

/-- One shard call: dispatch time, how long it would run uncut, and the
    per-call estimate the dispatch rule used (`longest_shard_secs`). -/
structure Call where
  start : Nat
  dur : Nat
  est : Nat

/-- `wall_dispatch_stop` said Go: `remaining ≥ reserve + longest`. -/
def dispatchOk (B R : Nat) (c : Call) : Prop := c.start + R + c.est ≤ B

def maxEnd : Nat → List Nat → Nat
  | m, [] => m
  | m, x :: xs => maxEnd (max m x) xs

theorem maxEnd_le : ∀ (xs : List Nat) (m M : Nat), m ≤ M → (∀ x ∈ xs, x ≤ M) →
    maxEnd m xs ≤ M := by
  intro xs; induction xs with
  | nil => intro m M h _; simpa [maxEnd] using h
  | cons x xs ih =>
    intro m M h hx
    simp only [maxEnd]
    apply ih
    · have := hx x (by simp); omega
    · intro y hy; exact hx y (by simp [hy])

theorem maxEnd_eq_or : ∀ (xs : List Nat) (m : Nat), maxEnd m xs = m ∨ ∃ x ∈ xs, maxEnd m xs = x := by
  intro xs; induction xs with
  | nil => intro m; simp [maxEnd]
  | cons x xs ih =>
    intro m
    simp only [maxEnd]
    rcases ih (max m x) with h | ⟨y, hy, h⟩
    · rw [h]
      by_cases hmx : m ≤ x
      · right; exact ⟨x, by simp, by omega⟩
      · left; omega
    · right; exact ⟨y, by simp [hy], h⟩

/-- RV5a — the dispatch rule alone. If every call was dispatched with
    `remaining ≥ R + est` and ran at most `slack` seconds past its estimate,
    then when the phase ends (all calls finished) the time left is at least
    `R - slack` — or the phase used no time at all. `slack = 0` is the
    assumption that no call outlasts the longest call measured before it was
    dispatched; the runtime measures that maximum (rule 4) but cannot
    guarantee it for the next call (endpoint latency spikes), and before any
    call has finished the measured maximum is 0. -/
theorem reserve_by_dispatch_rule (B R slack t0 : Nat) (calls : List Call)
    (hok : ∀ c ∈ calls, dispatchOk B R c)
    (hdur : ∀ c ∈ calls, c.dur ≤ c.est + slack) :
    let E := maxEnd t0 (calls.map (fun c => c.start + c.dur))
    E = t0 ∨ E + R ≤ B + slack := by
  intro E
  rcases maxEnd_eq_or (calls.map (fun c => c.start + c.dur)) t0 with h | ⟨x, hx, h⟩
  · left; exact h
  · right
    simp only [List.mem_map] at hx
    obtain ⟨c, hc, rfl⟩ := hx
    have h1 := hok c hc
    have h2 := hdur c hc
    unfold dispatchOk at h1
    show maxEnd t0 _ + R ≤ B + slack
    rw [h]; omega

/-- The rule without the cut is not enough: before any shard finished the
    measured longest call is 0, so a first-wave call that runs to its 1,200 s
    side-call cap can eat the reserve (1,800 s budget, 600 s reserve). -/
theorem dispatch_rule_alone_can_eat_reserve :
    ∃ (c : Call), dispatchOk 1800 600 c ∧ c.dur ≤ 1200 ∧ c.start + c.dur + 600 > 1800 :=
  ⟨⟨5, 1200, 0⟩, by simp [dispatchOk], by decide, by decide⟩

/-- A call cut at `K` (`tokio::time::timeout_at` at the reserve line) ends
    by `K`, whatever the endpoint does. -/
def cutEnd (K : Nat) (c : Call) : Nat := min (c.start + c.dur) K

/-- RV5b — the reserve cut. Every shard call is dropped at `K`, the moment
    the time left reaches the synthesis reserve (`K + R ≤ B`). Then the phase
    ends with at least `R` seconds left (or used no time), for ANY call
    durations — latency spikes included. Assumed, not proved: the timer fires
    on time (whole-second clock; the Rust cut keeps one extra second for the
    floor of the elapsed clock) and the bookkeeping after the last call is
    negligible. -/
theorem reserve_by_cut (B R K t0 : Nat) (calls : List Call) (hK : K + R ≤ B) :
    let E := maxEnd t0 (calls.map (cutEnd K))
    E = t0 ∨ E + R ≤ B := by
  intro E
  rcases maxEnd_eq_or (calls.map (cutEnd K)) t0 with h | ⟨x, hx, h⟩
  · left; exact h
  · right
    simp only [List.mem_map] at hx
    obtain ⟨c, _, rfl⟩ := hx
    show maxEnd t0 _ + R ≤ B
    rw [h]; unfold cutEnd; omega

/-- With the cut, the dispatch rule keeps its job: a call is dispatched only
    while `remaining ≥ R + est`, so it starts before the cut. -/
theorem dispatched_before_cut (B R : Nat) (c : Call) (h : dispatchOk B R c) :
    c.start ≤ B - R := by unfold dispatchOk at h; omega

/-! ## RV6 — findings are verified against the delivered lines -/

def dist (a b : Nat) : Nat := if a ≤ b then b - a else a - b

/-- The matching start nearest to the cited line (first on a tie):
    `candidates.min_by_key(|i| line_i.abs_diff(cited))`. -/
def nearest (cited : Nat) : List Nat → Option Nat
  | [] => none
  | a :: rest =>
    match nearest cited rest with
    | none => some a
    | some b => if dist a cited ≤ dist b cited then some a else some b

theorem nearest_mem (cited : Nat) : ∀ (cs : List Nat) (a : Nat), nearest cited cs = some a → a ∈ cs := by
  intro cs; induction cs with
  | nil => intro a h; simp [nearest] at h
  | cons x xs ih =>
    intro a h
    simp only [nearest] at h
    split at h
    · simp at h; subst h; simp
    · rename_i b hb
      split at h
      · simp at h; subst h; simp
      · simp at h; subst h; exact List.mem_cons_of_mem _ (ih _ hb)

theorem nearest_none (cited : Nat) : ∀ cs : List Nat, nearest cited cs = none → cs = [] := by
  intro cs; cases cs with
  | nil => intro _; rfl
  | cons x xs => intro h; simp only [nearest] at h; split at h <;> (try split at h) <;> simp at h

inductive Verdict where
  | verified (line : Nat)
  | relocated (line : Nat)
  | unverified
  deriving DecidableEq, Repr

/-- `verify_quote`. `delivered`: the line numbers of the slice(s) the shard
    read; `cands`: the delivered lines where the quote's lines match (in
    order, small gaps allowed — the text matcher itself is not modelled);
    `qlen`: the quote's line count; `cited`: the line the finding claims. -/
def verify (delivered cands : List Nat) (qlen cited : Nat) : Verdict :=
  match nearest cited cands with
  | none => .unverified
  | some at_ =>
    if at_ = cited ∨ (at_ ≤ cited ∧ cited < at_ + qlen ∧ cited ∈ delivered) then .verified cited
    else if dist at_ cited ≤ 30 ∨ cands.length = 1 then .relocated at_
    else .unverified

/-- The line a finding is recorded at, if it is recorded. -/
def recorded : Verdict → Option Nat
  | .verified l => some l
  | .relocated l => some l
  | .unverified => none

/-- RV6: a recorded finding cites a delivered line inside a span where its
    quote matched; a quote that matches nowhere is never recorded. -/
theorem recorded_is_grounded (delivered cands : List Nat) (qlen cited l : Nat)
    (hsub : ∀ c ∈ cands, c ∈ delivered)
    (h : recorded (verify delivered cands qlen cited) = some l) :
    l ∈ delivered ∧ ∃ a ∈ cands, a ≤ l ∧ l < a + max qlen 1 := by
  unfold verify at h
  split at h
  · simp [recorded] at h
  · rename_i a ha
    have hm := nearest_mem cited cands a ha
    split at h
    · simp [recorded] at h; subst h
      rename_i hcond
      rcases hcond with hc | ⟨h1, h2, h3⟩
      · subst hc; exact ⟨hsub _ hm, a, hm, Nat.le_refl _, by omega⟩
      · exact ⟨h3, a, hm, h1, by omega⟩
    · split at h
      · simp [recorded] at h; subst h; exact ⟨hsub _ hm, a, hm, Nat.le_refl _, by omega⟩
      · simp [recorded] at h

theorem no_match_unverified (delivered : List Nat) (qlen cited : Nat) :
    verify delivered [] qlen cited = .unverified := by simp [verify, nearest]

/-- The rule before the fix: `spans_cited` without "the cited line was
    delivered". A two-line quote matched across two slices of one file
    (lines 1–10 and 50–60 in one shard) verified line 11, which the shard
    never read. -/
def verifyOld (cands : List Nat) (qlen cited : Nat) : Verdict :=
  match nearest cited cands with
  | none => .unverified
  | some at_ =>
    if at_ = cited ∨ (at_ ≤ cited ∧ cited < at_ + qlen) then .verified cited
    else if dist at_ cited ≤ 30 ∨ cands.length = 1 then .relocated at_
    else .unverified

def twoSlices : List Nat := (List.range 10).map (· + 1) ++ (List.range 11).map (· + 50)

theorem old_rule_cited_an_unread_line :
    recorded (verifyOld [10] 2 11) = some 11 ∧ 11 ∉ twoSlices := by decide

theorem fixed_rule_relocates_instead :
    verify twoSlices [10] 2 11 = .relocated 10 := by decide

/-! ## Export: the tables the Rust conformance tests check the code against -/

def Shard.name : Shard → String
  | .queued0 => "queued0" | .flying0 => "flying0" | .queued1 => "queued1"
  | .flying1 => "flying1" | .succeeded => "succeeded" | .failed => "failed"
  | .notRun => "not_run"

def SEvent.name : SEvent → String
  | .dispatch => "dispatch" | .ok => "ok" | .err => "err" | .abort => "abort"
  | .sweep => "sweep"

def Decision.name : Decision → String
  | .accept => "accept" | .refuseCoverage => "refuse_coverage"
  | .refuseCitation => "refuse_citation"

def q (s : String) : String := "\"" ++ s ++ "\""

def row (cells : List String) : String := "[" ++ ",".intercalate (cells.map q) ++ "]"

def shardRows : List String :=
  allShard.foldr (fun s acc =>
    allSEvent.foldr (fun e acc2 =>
      match sstep s e with
      | some t => row [s.name, e.name, t.name] :: acc2
      | none => acc2) acc) []

def bools : List Bool := [false, true]

/-- `last`: no refusal yet, or the covered lines at the last refusal —
    below the current 5 (progress since) or equal (no progress). -/
def lastCases : List (String × Option Nat) :=
  [("none", none), ("progress", some 4), ("no_progress", some 5)]

def gateRows : List String := Id.run do
  let mut rows : List String := []
  for complete in bools do
    for stopped in bools do
      for limit in bools do
        for (lname, last) in lastCases do
          for np in [0, 1, 2] do
            for cites in bools do
              for nudged in bools do
                let g : Gate := { stopped := stopped, np := np, last := last, nudged := nudged }
                let (d, g') := gateStep g complete 5 limit cites
                rows := rows ++ [row [toString complete, toString stopped, toString limit,
                  lname, toString np, toString cites, toString nudged,
                  d.name, toString g'.stopped, toString g'.np, toString g'.nudged,
                  if g'.last = g.last then "unchanged" else "covered"]]
  return rows

def Out.code : Out → String
  | .ok => "o" | .fail => "x" | .cut => "c"

def histCode (h : List Out) : String := String.join (h.map Out.code)

/-- Histories the Rust `ShardBreaker` is checked against, each at several
    parallelisms: the recorded runs, and the shapes that must (not) trip. -/
def breakerHistories : List (String × List Out) :=
  liveRuns ++ [
    ("always_fails", List.replicate 40 .fail),
    ("always_fails_with_cuts", rle [(1, .cut), (3, .fail), (2, .cut), (40, .fail)]),
    ("only_cuts", List.replicate 40 .cut),
    ("first_wave_fails_then_recovers", rle [(6, .fail), (60, .ok)]),
    ("first_ok_then_fails", rle [(1, .ok), (40, .fail)]),
    ("collapse_mid_run", rle [(40, .ok), (30, .fail)]),
    ("collapse_with_cuts", rle [(40, .ok), (5, .fail), (10, .cut), (25, .fail)]),
    ("half_fail", (List.range 60).map (fun i => if i % 2 = 0 then .fail else .ok)),
    ("three_in_four_fail", (List.range 60).map (fun i => if i % 4 = 3 then .ok else .fail)),
    ("eleven_of_sixteen", rle [(20, .ok), (11, .fail), (5, .ok), (11, .fail), (5, .ok)])]

def breakerRows : List String :=
  breakerHistories.foldr (fun (name, h) acc =>
    [1, 2, 3, 6, 8, 32].foldr (fun cap acc2 =>
      row [toString cap, name, histCode h,
        match firstTrip cap h with | some k => toString k | none => "none"] :: acc2) acc) []

def exportTables : String :=
  "{\"shard\":[" ++ ",".intercalate shardRows ++ "],\"gate\":[" ++ ",".intercalate gateRows ++
  "],\"breaker\":[" ++ ",".intercalate breakerRows ++ "]}"

end Review

#eval Review.exportTables
