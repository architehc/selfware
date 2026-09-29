/-
  selfware path containment, deny-list and model-facing redaction — formal
  model. Core Lean 4 only (no Mathlib). Check with: `lean SafetyBounds.lean`.

  Modelled code:
  * `normalize` is the lexical normalization `PathValidator::validate`
    applies before its protected-namespace check (`normalize_lexical`) and
    `lexical_normalize_path` (citation / checkpoint path resolution): `.`
    dropped, `..` pops one component, `..` at the root stays at the root,
    `..` with nothing before it on a relative path is dropped.
  * `isAllowed` is the decision order of `PathValidator::validate` once the
    path is resolved: inside the workspace AND not matched by the deny list.
    The deny list is a parameter (`denied`), because the real one is the
    configured glob list (`default_denied_paths()` unioned with the config's);
    the Rust test `every_default_denied_path_is_refused_inside_the_workspace`
    ties every default entry to a refusal of the real validator.
  * `redactTokens` is `redact_for_model` over a tokenized text: a secret
    value becomes a marker, everything else is kept. The model's one
    assumption — a marker is never classified as a secret again — is what
    makes S4 hold; the Rust test
    `redact_for_model_is_idempotent_on_the_secret_and_code_corpora` checks it
    on the real redactor (second pass: same text, zero redactions).

  Not modelled: filesystem resolution (symlinks, `O_NOFOLLOW`, canonical
  paths — the real containment check compares CANONICAL paths), the
  shell-read policy (`safety/shell_read.rs`) and git hardening
  (`safety/git_exec.rs`); those have their own fail-closed tests.

  What is proved here:
  * S1 normalization removes every `..`: no `parent` segment survives, so a
    prefix check on a normalized path cannot be escaped by a later `..`.
  * S1b the workspace contains itself.
  * S2 traversal: `..` never climbs above the root, and a path that climbs
    out of the workspace is not contained (a concrete escape is refused).
  * S3 deny-list immunity: a path the deny list matches is refused, for
    every deny list and every workspace — containment cannot re-allow it.
  * S4 redaction is idempotent.
  * S5 no secret token survives redaction.
-/

namespace Safety

inductive PathSegment where
  | root
  | dir (name : String)
  | parent
  deriving DecidableEq, Repr

def normalizeHelper : List PathSegment → List PathSegment → List PathSegment
  | [],        acc => acc.reverse
  | .root :: rest, _ => normalizeHelper rest [.root]
  | .dir name :: rest, acc => normalizeHelper rest (.dir name :: acc)
  | .parent :: rest, acc =>
    match acc with
    | [] => normalizeHelper rest []
    | [.root] => normalizeHelper rest [.root]
    | _ :: tail => normalizeHelper rest tail

def normalize (p : List PathSegment) : List PathSegment :=
  normalizeHelper p []

def isPrefixOf (pre : List PathSegment) (full : List PathSegment) : Bool :=
  match pre, full with
  | [], _ => true
  | _, [] => false
  | p :: prest, f :: frest =>
    if p == f then isPrefixOf prest frest else false

def isContainedIn (target : List PathSegment) (workspace : List PathSegment) : Bool :=
  isPrefixOf (normalize workspace) (normalize target)

def isAllowed (denied : List PathSegment → Bool)
    (target : List PathSegment) (workspace : List PathSegment) : Bool :=
  isContainedIn target workspace && !denied (normalize target)

/-! ## S1 — Normalization removes every `..` -/

/-- The accumulator (the reversed output so far) holds no `parent`. -/
def WellFormedAcc (acc : List PathSegment) : Prop :=
  PathSegment.parent ∉ acc

theorem helper_no_parent (p acc : List PathSegment) (h : WellFormedAcc acc) :
    PathSegment.parent ∉ normalizeHelper p acc := by
  induction p generalizing acc with
  | nil =>
    simp only [normalizeHelper, List.mem_reverse]
    exact h
  | cons seg rest ih =>
    cases seg with
    | root =>
      simp only [normalizeHelper]
      apply ih
      simp [WellFormedAcc]
    | dir name =>
      simp only [normalizeHelper]
      apply ih
      simp only [WellFormedAcc, List.mem_cons, not_or]
      exact ⟨by simp, h⟩
    | parent =>
      simp only [normalizeHelper]
      split
      · exact ih [] (by simp [WellFormedAcc])
      · exact ih [.root] (by simp [WellFormedAcc])
      · rename_i x tail _ _
        apply ih
        intro hm
        exact h (List.mem_cons_of_mem _ hm)

theorem normalize_has_no_parent (p : List PathSegment) :
    PathSegment.parent ∉ normalize p :=
  helper_no_parent p [] (by simp [WellFormedAcc])

/-! ## S1b — Reflexive containment -/

theorem prefix_self (p : List PathSegment) : isPrefixOf p p = true := by
  induction p with
  | nil => rfl
  | cons h t ih =>
    simp [isPrefixOf]
    exact ih

theorem workspace_contains_self (w : List PathSegment) :
    isContainedIn w w = true := by
  simp [isContainedIn]
  exact prefix_self (normalize w)

/-! ## S2 — Traversal -/

theorem parent_cannot_escape_root (rest : List PathSegment) :
    normalizeHelper (.parent :: rest) [.root] = normalizeHelper rest [.root] := rfl

theorem dotdot_above_root_is_root : normalize [.root, .parent, .parent] = [.root] := rfl

/-- `/ws/../../etc/passwd` resolves to `/etc/passwd`, outside `/ws`. -/
theorem traversal_escape_refused :
    isContainedIn [.root, .dir "ws", .parent, .parent, .dir "etc", .dir "passwd"]
      [.root, .dir "ws"] = false := by decide

/-- `/ws/sub/../ok.txt` stays inside `/ws`. -/
theorem inner_dotdot_stays_contained :
    isContainedIn [.root, .dir "ws", .dir "sub", .parent, .dir "ok.txt"]
      [.root, .dir "ws"] = true := by decide

/-! ## S3 — Deny-list immunity -/

theorem denied_path_never_allowed (denied : List PathSegment → Bool)
    (target workspace : List PathSegment)
    (h : denied (normalize target) = true) :
    isAllowed denied target workspace = false := by
  simp [isAllowed, h]

theorem allowed_implies_contained (denied : List PathSegment → Bool)
    (target workspace : List PathSegment)
    (h : isAllowed denied target workspace = true) :
    isContainedIn target workspace = true := by
  simp [isAllowed] at h
  exact h.1

/-! ## S4 & S5 — Redaction -/

inductive Token where
  | plain (s : String)
  | secret (kind : String) (val : String)
  | marker (kind : String)
  deriving DecidableEq, Repr

def redactToken : Token → Token
  | .secret kind _ => .marker kind
  | t => t

def redactTokens (tokens : List Token) : List Token :=
  tokens.map redactToken

theorem redact_token_idempotent (t : Token) :
    redactToken (redactToken t) = redactToken t := by
  cases t <;> rfl

theorem redact_tokens_idempotent (tokens : List Token) :
    redactTokens (redactTokens tokens) = redactTokens tokens := by
  induction tokens with
  | nil => rfl
  | cons h t ih =>
    change redactToken (redactToken h) :: redactTokens (redactTokens t) =
      redactToken h :: redactTokens t
    rw [redact_token_idempotent h, ih]

def isSecret : Token → Bool
  | .secret _ _ => true
  | _ => false

theorem redact_eliminates_secrets (tokens : List Token) :
    (redactTokens tokens).any isSecret = false := by
  induction tokens with
  | nil => rfl
  | cons h t ih =>
    cases h <;> simp_all [redactTokens, redactToken, isSecret]

/-- Redaction keeps the token count: nothing is dropped or merged (the
Rust side: every line of the input is still there). -/
theorem redact_preserves_length (tokens : List Token) :
    (redactTokens tokens).length = tokens.length := by
  simp [redactTokens]

end Safety
