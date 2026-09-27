//! One tracked entity: its id, current state and the log its transitions go
//! to.

use super::{Effect, EventLog, InvalidTransition, Label, Machine, RecordedUsage, TransitionRecord};

/// A live entity of machine `M`. [`Tracked::apply`] is the only way its state
/// changes: it computes the next state, checks the machine's invariants as a
/// runtime oracle, records the transition and returns the effects entering
/// the new state requests.
#[derive(Debug, Clone)]
pub struct Tracked<M: Machine> {
    id: String,
    state: M::State,
    owner: Option<String>,
    task_type: Option<String>,
    parent: Option<String>,
    usage: Option<RecordedUsage>,
    log: EventLog,
}

impl<M: Machine> Tracked<M> {
    /// A handle on entity `id` in `state`. Records nothing; call
    /// [`Tracked::record_created`] for a new entity or segment, or use it as
    /// is to continue from a state read back from the log.
    pub fn new(id: impl Into<String>, state: M::State, log: EventLog) -> Self {
        Self {
            id: id.into(),
            state,
            owner: None,
            task_type: None,
            parent: None,
            usage: None,
            log,
        }
    }

    /// Set the owning entity written with every record (a resource's task
    /// id, a task's agent id).
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Set the task type written with every record.
    pub fn with_task_type(mut self, task_type: impl Into<String>) -> Self {
        self.task_type = Some(task_type.into());
        self
    }

    /// Set the task this one was forked from, written with every record.
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// The task this one was forked from, if any.
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// Attach measured usage to the next record only (a terminal transition
    /// or an edit); later records carry none until set again.
    pub fn attach_usage(&mut self, usage: RecordedUsage) {
        self.usage = Some(usage);
    }

    /// Record that the entity (or a new segment of it) now exists in its
    /// current state (`from: null`).
    pub fn record_created(&self, cause: &str) {
        let rec = self.record(None, None, cause);
        self.log.append(&rec, false);
    }

    /// The entity id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The current state.
    pub fn state(&self) -> &M::State {
        &self.state
    }

    /// Whether the current state is terminal.
    pub fn is_terminal(&self) -> bool {
        M::is_terminal(&self.state)
    }

    /// The log this entity writes to.
    pub fn log(&self) -> &EventLog {
        &self.log
    }

    /// Apply `event`. On success the state changes, one record is appended
    /// (fsynced when the new state is terminal) and the effects of entering
    /// the new state are returned. A refused event returns the typed error
    /// and changes nothing (and records nothing).
    ///
    /// Runtime oracle: the machine's proved invariants
    /// ([`Machine::check_step`]) are checked on every accepted transition —
    /// a violation panics in debug/test builds and is logged as an error in
    /// release builds (the transition still happens there: the table, not
    /// the oracle, is authoritative).
    pub fn apply(
        &mut self,
        event: M::Event,
        cause: &str,
    ) -> Result<Vec<Effect>, InvalidTransition> {
        let next = M::next(&self.state, &event)?;
        if let Err(violation) = M::check_step(&self.state, &event, &next) {
            debug_assert!(
                false,
                "lifecycle oracle: {violation} ({} {})",
                M::ENTITY,
                self.id
            );
            tracing::error!("lifecycle oracle: {violation} ({} {})", M::ENTITY, self.id);
        }
        let rec = self.record(Some(&next), Some(event.label()), cause);
        self.usage = None;
        self.log.append(&rec, M::is_terminal(&next));
        self.state = next;
        Ok(M::on_enter(&self.state))
    }

    fn record(
        &self,
        next: Option<&M::State>,
        event: Option<&str>,
        cause: &str,
    ) -> TransitionRecord {
        let (from, to) = match next {
            Some(n) => (Some(self.state.label()), n.label()),
            None => (None, self.state.label()),
        };
        let mut rec = TransitionRecord::now(M::ENTITY, &self.id, from, to, event, cause);
        rec.owner = self.owner.clone();
        rec.task_type = self.task_type.clone();
        rec.parent = self.parent.clone();
        rec.usage = self.usage;
        rec
    }
}
