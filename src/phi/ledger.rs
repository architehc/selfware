//! Revision-keyed evidence ledger.
//!
//! Phi's scalar `debt` could not say *which* changes a verification covered, so
//! anything that looked like checking cleared everything: seventeen unrun tests
//! took debt from 1.0 to zero, and a green suite in module A absolved unread
//! code in module B. This replaces the accumulator with discrete obligations
//! and the evidence that does or does not discharge them.
//!
//! # Invariants
//!
//! 1. **A change creates obligations; nothing else does.** Reading a file is
//!    exposure, not comprehension. Writing a test is a promise, not a result.
//! 2. **Review and testing are distinct.** One edit creates one
//!    [`ObligationKind::UnreviewedChange`] and one
//!    [`ObligationKind::UntestedLogic`]. Passing tests never discharge the
//!    review obligation; a human reading the diff never discharges the test
//!    obligation.
//! 3. **Evidence is scoped.** It discharges obligations on the paths it covers
//!    and no others.
//! 4. **Evidence carries an execution snapshot.** A run is anchored to the
//!    ledger sequence at which it *started*. A test that began before an edit
//!    cannot have tested that edit, however green it is.
//! 5. **A run racing an edit proves nothing about that path.** If a path
//!    changes between a run's start and its result, the result is stale for
//!    that path — it read some indeterminate mixture.
//! 6. **A human confirmation must name its scope.** [`Ledger::record_human_review`]
//!    takes paths, not an optional scope, so "looks good" cannot discharge
//!    anything.
//! 7. **Only a passing outcome discharges.** A failing run establishes that the
//!    work is not done, not that it was checked.
//!
//! # What this deliberately does not model
//!
//! **Dependency effects.** Changing `a.rs` can invalidate a passing test about
//! `b.rs` when `b` depends on `a`. The ledger has no dependency graph and makes
//! no attempt to guess one, so evidence about `b` survives a change to `a`.
//! This is a known gap, pinned by a test so it cannot be mistaken for a solved
//! problem. Closing it needs real dependency data, not a heuristic.
//!
//! **What a test actually exercised.** Without coverage data, a passing suite
//! says the suite is green and nothing more — see [`Scope`].
//!
//! # Status: observe-only
//!
//! This module records and reports. It is deliberately not wired to the
//! steward, the mediator or any agent decision. Recorded sessions are to be
//! evaluated before anything acts on this.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Monotonic position in the ledger. Every recorded fact gets one, and they are
/// the only ordering this module trusts — wall-clock timestamps are carried for
/// display but never used to decide whether evidence is current.
pub type Seq = u64;

/// Identity of an obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObligationId(pub u64);

/// Identity of a recorded piece of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EvidenceId(pub u64);

/// What a change still owes. One edit owes both, independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObligationKind {
    /// Nobody has read this change.
    UnreviewedChange,
    /// No executed test has covered this change.
    UntestedLogic,
}

/// The two kinds of evidence, kept apart on purpose. A green suite is not a
/// review, and a review is not a test run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceKind {
    /// A test run that actually executed.
    TestsExecuted,
    /// A human read the change and said so, naming what they read.
    HumanReviewed,
}

impl EvidenceKind {
    /// Which obligation this kind of evidence can discharge. Total and
    /// deliberately one-to-one: widening it here is how "tests passed" starts
    /// clearing unread code again.
    pub fn discharges(self) -> ObligationKind {
        match self {
            EvidenceKind::TestsExecuted => ObligationKind::UntestedLogic,
            EvidenceKind::HumanReviewed => ObligationKind::UnreviewedChange,
        }
    }
}

/// What an evidence record covers.
///
/// There is no variant meaning "everything". A suite that runs across the whole
/// workspace proves the suite is green; it does not prove that any particular
/// file was exercised, because a file with no test touching it passes the suite
/// by being ignored. Treating a whole-workspace run as universal coverage is
/// the same mistake as treating a green run as a review — it clears an
/// obligation nothing actually discharged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    /// Exactly these paths, and no others.
    Paths(Vec<PathBuf>),
    /// A workspace-wide run that reported which paths it exercised.
    WorkspaceWithCoverage(Vec<PathBuf>),
    /// A workspace-wide run with no coverage data. It discharges nothing; what
    /// it exercised is unknown, and unknown is not covered.
    WorkspaceCoverageUnknown,
}

/// Whether this evidence covers a path, and if not, whether that is a definite
/// "no" or an admission of ignorance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Covered,
    NotCovered,
    /// The run may or may not have exercised this path. Nobody can say.
    Unknown,
}

impl Scope {
    pub fn coverage_of(&self, path: &Path) -> Coverage {
        match self {
            Scope::Paths(paths) | Scope::WorkspaceWithCoverage(paths) => {
                if paths.iter().any(|p| p == path) {
                    Coverage::Covered
                } else {
                    Coverage::NotCovered
                }
            }
            Scope::WorkspaceCoverageUnknown => Coverage::Unknown,
        }
    }

    /// Only a definite yes counts.
    pub fn covers(&self, path: &Path) -> bool {
        self.coverage_of(path) == Coverage::Covered
    }
}

/// Whether the run or review concluded successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Passed,
    Failed,
}

/// An outstanding verification debt, keyed to a specific change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obligation {
    pub id: ObligationId,
    pub kind: ObligationKind,
    pub path: PathBuf,
    /// Lines touched, used for weighting and for citing the hunk to a human.
    pub line_count: usize,
    /// Which agent turn produced the change.
    pub turn_index: usize,
    /// Sequence anchor: the ledger position at which this change was recorded.
    pub seq: Seq,
    /// Wall clock, for display only.
    pub recorded_at_ms: u64,
    /// Content identity after the change, where the caller knows it. Reuses the
    /// hash `session::edit_history::FileSnapshot` already computes.
    pub revision: Option<String>,
    /// The edit checkpoint this came from, when recorded through one.
    pub checkpoint: Option<u64>,
    /// Evidence that discharged it, if any.
    pub satisfied_by: Option<EvidenceId>,
    /// Set when the file was removed: there is no longer anything to check.
    pub retired: bool,
}

impl Obligation {
    pub fn outstanding(&self) -> bool {
        self.satisfied_by.is_none() && !self.retired
    }
}

/// A recorded verification attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub id: EvidenceId,
    pub kind: EvidenceKind,
    pub scope: Scope,
    pub outcome: Outcome,
    /// Ledger position when the run STARTED. This is the execution snapshot,
    /// and it is what makes stale results detectable.
    pub snapshot_seq: Seq,
    /// Ledger position when the result was recorded.
    pub recorded_seq: Seq,
    pub recorded_at_ms: u64,
    /// Where to find the run: a log path, a job id, a command line.
    pub artifact: Option<String>,
}

/// A handle taken when a verification run begins.
///
/// Taking this *before* the run is the whole point: it pins the ledger position
/// the run is about to observe. Recording evidence without one would let a
/// result claim to cover changes that landed after it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub seq: Seq,
}

/// Why a piece of evidence did not discharge a given obligation. Surfaced so a
/// human can see the reasoning rather than an unexplained outstanding count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsatisfied {
    WrongKind,
    OutOfScope,
    Failed,
    /// The run started before the change existed.
    PredatesChange,
    /// The path changed while the run was in flight.
    RacedAnEdit,
    /// The run executed but did not report what it exercised, so whether this
    /// path was covered is unknown. Distinct from `OutOfScope`, which is a
    /// definite no.
    CoverageUnknown,
    /// The path's current content was not produced by a recorded change, so the
    /// ledger does not know what is in it.
    RevisionUnknown,
}

/// Append-only ledger of obligations and evidence.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    seq: Seq,
    next_obligation: u64,
    next_evidence: u64,
    obligations: Vec<Obligation>,
    evidence: Vec<Evidence>,
    /// Every change to a path, by sequence. Used to detect a run that raced an
    /// edit, which the obligation list alone cannot answer once obligations are
    /// retired.
    changes_by_path: BTreeMap<PathBuf, Vec<Seq>>,
    /// Changes the ledger did not make, and therefore cannot describe.
    external_changes: BTreeMap<PathBuf, Vec<Seq>>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    fn tick(&mut self) -> Seq {
        self.seq += 1;
        self.seq
    }

    /// Current position. Take one before starting a verification run.
    pub fn snapshot(&self) -> RunSnapshot {
        RunSnapshot { seq: self.seq }
    }

    /// Record a mutation to a file. This is the only thing that creates debt.
    ///
    /// Returns both obligations: an edit owes a read AND a test, and they are
    /// discharged independently.
    pub fn record_change(
        &mut self,
        path: impl Into<PathBuf>,
        line_count: usize,
        turn_index: usize,
        recorded_at_ms: u64,
    ) -> [ObligationId; 2] {
        let path = path.into();
        let seq = self.tick();
        self.changes_by_path
            .entry(path.clone())
            .or_default()
            .push(seq);

        let mut ids = [ObligationId(0); 2];
        for (slot, kind) in [
            ObligationKind::UnreviewedChange,
            ObligationKind::UntestedLogic,
        ]
        .into_iter()
        .enumerate()
        {
            let id = ObligationId(self.next_obligation);
            self.next_obligation += 1;
            ids[slot] = id;
            self.obligations.push(Obligation {
                id,
                kind,
                path: path.clone(),
                line_count,
                turn_index,
                seq,
                recorded_at_ms,
                revision: None,
                checkpoint: None,
                satisfied_by: None,
                retired: false,
            });
        }
        ids
    }

    /// Attach the content hash and originating checkpoint to the obligations
    /// just recorded for a path, so the ledger can cite a revision.
    pub fn annotate_revision(
        &mut self,
        ids: &[ObligationId],
        revision: impl Into<String>,
        checkpoint: Option<u64>,
    ) {
        let revision = revision.into();
        for obligation in self.obligations.iter_mut() {
            if ids.contains(&obligation.id) {
                obligation.revision = Some(revision.clone());
                obligation.checkpoint = checkpoint;
            }
        }
    }

    /// Record a file removal. The code is gone, so its outstanding obligations
    /// are retired rather than left forever unsatisfiable — but the removal is
    /// itself a change somebody should read.
    pub fn record_deletion(
        &mut self,
        path: impl Into<PathBuf>,
        turn_index: usize,
        recorded_at_ms: u64,
    ) -> ObligationId {
        let path = path.into();
        let seq = self.tick();
        self.changes_by_path
            .entry(path.clone())
            .or_default()
            .push(seq);
        for obligation in self.obligations.iter_mut() {
            if obligation.path == path && obligation.outstanding() {
                obligation.retired = true;
            }
        }
        let id = ObligationId(self.next_obligation);
        self.next_obligation += 1;
        self.obligations.push(Obligation {
            id,
            kind: ObligationKind::UnreviewedChange,
            path,
            line_count: 0,
            turn_index,
            seq,
            recorded_at_ms,
            revision: None,
            checkpoint: None,
            satisfied_by: None,
            retired: false,
        });
        id
    }

    /// Record an executed test run.
    pub fn record_test_run(
        &mut self,
        snapshot: RunSnapshot,
        scope: Scope,
        outcome: Outcome,
        artifact: Option<String>,
        recorded_at_ms: u64,
    ) -> EvidenceId {
        self.record_evidence(
            EvidenceKind::TestsExecuted,
            snapshot,
            scope,
            outcome,
            artifact,
            recorded_at_ms,
        )
    }

    /// Record a human review. Paths are required, not optional: an unscoped
    /// confirmation discharges nothing, so the API does not allow expressing
    /// one.
    pub fn record_human_review(
        &mut self,
        snapshot: RunSnapshot,
        paths: Vec<PathBuf>,
        outcome: Outcome,
        artifact: Option<String>,
        recorded_at_ms: u64,
    ) -> EvidenceId {
        self.record_evidence(
            EvidenceKind::HumanReviewed,
            snapshot,
            Scope::Paths(paths),
            outcome,
            artifact,
            recorded_at_ms,
        )
    }

    fn record_evidence(
        &mut self,
        kind: EvidenceKind,
        snapshot: RunSnapshot,
        scope: Scope,
        outcome: Outcome,
        artifact: Option<String>,
        recorded_at_ms: u64,
    ) -> EvidenceId {
        let recorded_seq = self.tick();
        let id = EvidenceId(self.next_evidence);
        self.next_evidence += 1;
        let evidence = Evidence {
            id,
            kind,
            scope,
            outcome,
            snapshot_seq: snapshot.seq,
            recorded_seq,
            recorded_at_ms,
            artifact,
        };
        self.apply(&evidence);
        self.evidence.push(evidence);
        id
    }

    /// Record a change the ledger did not make: a human editing in their
    /// editor, a `git checkout`, a formatter, another process.
    ///
    /// Line count is deliberately not a parameter. The ledger did not see the
    /// diff, so claiming a size would be inventing one. What it records is that
    /// the path's content is no longer attributable to anything it knows, which
    /// invalidates evidence taken over it.
    pub fn record_external_change(
        &mut self,
        path: impl Into<PathBuf>,
        recorded_at_ms: u64,
    ) -> ObligationId {
        let path = path.into();
        let seq = self.tick();
        self.changes_by_path
            .entry(path.clone())
            .or_default()
            .push(seq);
        self.external_changes
            .entry(path.clone())
            .or_default()
            .push(seq);
        let id = ObligationId(self.next_obligation);
        self.next_obligation += 1;
        self.obligations.push(Obligation {
            id,
            kind: ObligationKind::UnreviewedChange,
            path,
            line_count: 0,
            turn_index: usize::MAX,
            seq,
            recorded_at_ms,
            revision: None,
            checkpoint: None,
            satisfied_by: None,
            retired: false,
        });
        id
    }

    /// Has an unattributed edit landed on `path` at or after `since`?
    fn externally_changed_since(&self, path: &Path, since: Seq) -> bool {
        self.external_changes
            .get(path)
            .is_some_and(|seqs| seqs.iter().any(|s| *s >= since))
    }

    /// Did `path` change strictly inside `(after, up_to]`?
    fn changed_between(&self, path: &Path, after: Seq, up_to: Seq) -> bool {
        self.changes_by_path
            .get(path)
            .is_some_and(|seqs| seqs.iter().any(|s| *s > after && *s <= up_to))
    }

    /// Why this evidence does or does not discharge this obligation.
    ///
    /// Split out from [`Self::apply`] so the reasoning can be shown to a human
    /// and tested directly, rather than inferred from a count.
    pub fn assess(&self, evidence: &Evidence, obligation: &Obligation) -> Result<(), Unsatisfied> {
        if evidence.kind.discharges() != obligation.kind {
            return Err(Unsatisfied::WrongKind);
        }
        match evidence.scope.coverage_of(&obligation.path) {
            Coverage::Covered => {}
            Coverage::NotCovered => return Err(Unsatisfied::OutOfScope),
            Coverage::Unknown => return Err(Unsatisfied::CoverageUnknown),
        }
        if evidence.outcome != Outcome::Passed {
            return Err(Unsatisfied::Failed);
        }
        if evidence.snapshot_seq < obligation.seq {
            return Err(Unsatisfied::PredatesChange);
        }
        if self.changed_between(
            &obligation.path,
            evidence.snapshot_seq,
            evidence.recorded_seq,
        ) {
            return Err(Unsatisfied::RacedAnEdit);
        }
        // An edit the ledger did not see means it does not know what is in the
        // file now. Evidence taken over an unknown revision proves nothing
        // about the change this obligation records.
        if self.externally_changed_since(&obligation.path, obligation.seq) {
            return Err(Unsatisfied::RevisionUnknown);
        }
        Ok(())
    }

    fn apply(&mut self, evidence: &Evidence) {
        let discharged: Vec<ObligationId> = self
            .obligations
            .iter()
            .filter(|o| o.outstanding() && self.assess(evidence, o).is_ok())
            .map(|o| o.id)
            .collect();
        for obligation in self.obligations.iter_mut() {
            if discharged.contains(&obligation.id) {
                obligation.satisfied_by = Some(evidence.id);
            }
        }
    }

    /// Everything still owed, oldest first — the order a steward would work in.
    pub fn outstanding(&self) -> Vec<&Obligation> {
        let mut out: Vec<&Obligation> = self
            .obligations
            .iter()
            .filter(|o| o.outstanding())
            .collect();
        out.sort_by_key(|o| o.seq);
        out
    }

    pub fn obligations(&self) -> &[Obligation] {
        &self.obligations
    }

    pub fn evidence(&self) -> &[Evidence] {
        &self.evidence
    }

    /// Unsatisfied lines, by kind — the raw material a debt projection uses.
    /// Deliberately not normalised here: the projection belongs to whatever is
    /// displaying it, not to the record.
    pub fn outstanding_lines(&self, kind: ObligationKind) -> usize {
        self.obligations
            .iter()
            .filter(|o| o.outstanding() && o.kind == kind)
            .map(|o| o.line_count)
            .sum()
    }

    /// One line per outstanding obligation, citing where it came from.
    pub fn citations(&self) -> Vec<String> {
        self.outstanding()
            .iter()
            .map(|o| {
                let kind = match o.kind {
                    ObligationKind::UnreviewedChange => "unreviewed",
                    ObligationKind::UntestedLogic => "untested",
                };
                format!(
                    "{} {} ({} lines, turn {})",
                    kind,
                    o.path.display(),
                    o.line_count,
                    o.turn_index
                )
            })
            .collect()
    }
}
