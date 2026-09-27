//! The resource lifecycle (formal/DESIGN.md §4.3). The table is the `step`
//! function of `formal/ResourceFsm.lean` (R1–R6 proved there) and is checked
//! against its export `formal/resource_table.json` by a conformance test;
//! the timed part of teardown is proved as P3–P5 in `formal/TaskFsm.lean`.
//!
//! ```text
//!  Requested ─start─▶ Starting ─ready─▶ Live ─drain─▶ Draining ─stopped─▶ Released
//!      │ drain            │ fail           │ owner_gone     │ deadline_passed | abandon
//!      ▼                  ▼                ▼                ▼
//!   Released           Leaked          Orphaned ─reap─▶ Draining      Leaked
//!                  (Starting ─drain─▶ Draining;  Live/Orphaned ─stopped─▶ Released;
//!                   Leaked ─reap─▶ Draining;     Leaked ─stopped─▶ Released)
//! ```
//!
//! `released` is terminal and sticky, and is reached only when the resource
//! was observed gone (`stopped`) or never started (R4). `leaked` is terminal
//! for teardown purposes (it raises [`Effect::LeakAlarm`]) but the reaper may
//! retry it (`reap`), or reconciliation may find it already gone (`stopped`).

use super::{Effect, Entity, InvalidTransition, Label, Machine};
use serde::{Deserialize, Serialize};

/// What kind of thing a resource is (the spawn sites of DESIGN §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// A Docker/Podman container.
    Container,
    /// A background OS process (process group).
    Process,
    /// A PTY shell session.
    Pty,
    /// A headless browser (and its driver process).
    Browser,
    /// A bound TCP port. (`server_port` is the label the first resource
    /// registry wrote to `resources.json`; it still reads as `port`.)
    #[serde(alias = "server_port")]
    Port,
    /// An MCP server child process.
    Mcp,
    /// An in-process HTTP server.
    Server,
    /// A git worktree.
    Worktree,
    /// A temporary directory.
    TempDir,
}

impl ResourceKind {
    /// The log label.
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceKind::Container => "container",
            ResourceKind::Process => "process",
            ResourceKind::Pty => "pty",
            ResourceKind::Browser => "browser",
            ResourceKind::Port => "port",
            ResourceKind::Mcp => "mcp",
            ResourceKind::Server => "server",
            ResourceKind::Worktree => "worktree",
            ResourceKind::TempDir => "tempdir",
        }
    }

    /// Kinds selfware can stop on its own. Worktrees, temp dirs, bound ports
    /// and in-process servers hold user work or die with the process; they
    /// are listed but never auto-drained.
    pub fn is_drainable(self) -> bool {
        matches!(
            self,
            ResourceKind::Container
                | ResourceKind::Process
                | ResourceKind::Pty
                | ResourceKind::Browser
                | ResourceKind::Mcp
        )
    }
}

impl std::fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a resource is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceState {
    /// Registered, not yet started.
    Requested,
    /// Being started.
    Starting,
    /// Running and owned by a live task.
    Live,
    /// Live, but its owner task is gone (found by reconciliation).
    Orphaned,
    /// Being stopped (polite first, then forced) within a deadline.
    Draining,
    /// Stopped cleanly. Terminal.
    Released,
    /// Teardown did not finish (or a start failed half-way). Terminal alarm;
    /// the reaper may retry.
    Leaked,
}

/// What can happen to a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceEvent {
    /// requested → starting.
    Start,
    /// starting → live.
    Ready,
    /// starting → leaked (a half-started resource may exist).
    Fail,
    /// requested → released; starting | live → draining.
    Drain,
    /// The resource is confirmed gone: draining | live | orphaned | leaked →
    /// released.
    Stopped,
    /// draining → leaked (the teardown deadline passed).
    DeadlinePassed,
    /// live → orphaned (reconciliation found no live owner).
    OwnerGone,
    /// orphaned | leaked → draining (the reaper takes it).
    Reap,
    /// draining → leaked before the deadline: the drain gave up (the handle
    /// is foreign or its state unknown, the kind is not stopped
    /// automatically, or the stopped resource could not be removed).
    Abandon,
}

impl Label for ResourceState {
    fn label(&self) -> &'static str {
        match self {
            ResourceState::Requested => "requested",
            ResourceState::Starting => "starting",
            ResourceState::Live => "live",
            ResourceState::Orphaned => "orphaned",
            ResourceState::Draining => "draining",
            ResourceState::Released => "released",
            ResourceState::Leaked => "leaked",
        }
    }
}

impl Label for ResourceEvent {
    fn label(&self) -> &'static str {
        match self {
            ResourceEvent::Start => "start",
            ResourceEvent::Ready => "ready",
            ResourceEvent::Fail => "fail",
            ResourceEvent::Drain => "drain",
            ResourceEvent::Stopped => "stopped",
            ResourceEvent::DeadlinePassed => "deadline_passed",
            ResourceEvent::OwnerGone => "owner_gone",
            ResourceEvent::Reap => "reap",
            ResourceEvent::Abandon => "abandon",
        }
    }
}

impl ResourceState {
    /// Every state.
    pub const ALL: [ResourceState; 7] = [
        ResourceState::Requested,
        ResourceState::Starting,
        ResourceState::Live,
        ResourceState::Orphaned,
        ResourceState::Draining,
        ResourceState::Released,
        ResourceState::Leaked,
    ];

    /// Settled = released or leaked (the Lean `settled`).
    pub fn is_settled(self) -> bool {
        matches!(self, ResourceState::Released | ResourceState::Leaked)
    }

    /// Confirmed gone; nothing left on the host.
    pub fn is_released(self) -> bool {
        matches!(self, ResourceState::Released)
    }

    /// The log / registry label.
    pub fn as_str(self) -> &'static str {
        self.label()
    }

    /// Parse a log / table name.
    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.label() == s)
    }
}

impl std::fmt::Display for ResourceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl ResourceEvent {
    /// Every event, in the Lean model's `allEvents` order.
    pub const ALL: [ResourceEvent; 9] = [
        ResourceEvent::Start,
        ResourceEvent::Ready,
        ResourceEvent::Fail,
        ResourceEvent::Drain,
        ResourceEvent::Stopped,
        ResourceEvent::DeadlinePassed,
        ResourceEvent::OwnerGone,
        ResourceEvent::Reap,
        ResourceEvent::Abandon,
    ];

    /// Parse a log / table name.
    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.label() == s)
    }
}

/// The full transition table as `(state, event, Some(next) | None)` for every
/// pair, in the Lean model's export order.
pub fn table() -> Vec<(ResourceState, ResourceEvent, Option<ResourceState>)> {
    let mut rows = Vec::with_capacity(ResourceState::ALL.len() * ResourceEvent::ALL.len());
    for s in ResourceState::ALL {
        for e in ResourceEvent::ALL {
            rows.push((s, e, step(s, e)));
        }
    }
    rows
}

/// The resource machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceMachine;

/// The Lean `step` function of `formal/ResourceFsm.lean`, arm for arm.
/// `None` = refused.
fn step(s: ResourceState, e: ResourceEvent) -> Option<ResourceState> {
    use ResourceEvent as E;
    use ResourceState as S;
    match (s, e) {
        (S::Requested, E::Start) => Some(S::Starting),
        (S::Requested, E::Drain) => Some(S::Released),
        (S::Starting, E::Ready) => Some(S::Live),
        (S::Starting, E::Fail) => Some(S::Leaked),
        (S::Starting, E::Drain) => Some(S::Draining),
        (S::Live, E::Drain) => Some(S::Draining),
        (S::Live, E::OwnerGone) => Some(S::Orphaned),
        (S::Live, E::Stopped) => Some(S::Released),
        (S::Orphaned, E::Reap) => Some(S::Draining),
        (S::Orphaned, E::Stopped) => Some(S::Released),
        (S::Draining, E::Stopped) => Some(S::Released),
        (S::Draining, E::DeadlinePassed) => Some(S::Leaked),
        (S::Draining, E::Abandon) => Some(S::Leaked),
        (S::Leaked, E::Reap) => Some(S::Draining),
        (S::Leaked, E::Stopped) => Some(S::Released),
        _ => None,
    }
}

impl Machine for ResourceMachine {
    type State = ResourceState;
    type Event = ResourceEvent;
    const ENTITY: Entity = Entity::Resource;

    fn next(
        state: &ResourceState,
        event: &ResourceEvent,
    ) -> Result<ResourceState, InvalidTransition> {
        step(*state, *event).ok_or_else(|| Self::refuse(state, event))
    }

    fn on_enter(state: &ResourceState) -> Vec<Effect> {
        match state {
            ResourceState::Leaked => vec![Effect::LeakAlarm],
            _ => Vec::new(),
        }
    }

    fn is_terminal(state: &ResourceState) -> bool {
        state.is_settled()
    }

    /// R1 `released` is sticky; R2 `leaked` is left only by
    /// `reap`/`stopped`; R3 a `draining` resource always has its deadline
    /// exit to a settled state (the transition-table half of P4); R4
    /// `released` is entered only on `stopped` or by draining a never-started
    /// resource.
    fn check_step(
        from: &ResourceState,
        event: &ResourceEvent,
        to: &ResourceState,
    ) -> Result<(), String> {
        if *from == ResourceState::Released {
            return Err(format!(
                "resource left terminal `released` via `{}`",
                event.label()
            ));
        }
        if *from == ResourceState::Leaked
            && !matches!(event, ResourceEvent::Reap | ResourceEvent::Stopped)
        {
            return Err(format!(
                "resource left `leaked` via `{}` (only reap/stopped may)",
                event.label()
            ));
        }
        if *to == ResourceState::Draining
            && !step(*to, ResourceEvent::DeadlinePassed).is_some_and(|s| s.is_settled())
        {
            return Err("draining has no deadline exit".to_string());
        }
        if *to == ResourceState::Released
            && !(*event == ResourceEvent::Stopped
                || (*from == ResourceState::Requested && *event == ResourceEvent::Drain))
        {
            return Err(format!(
                "resource released via `{}` from `{}` without confirmation",
                event.label(),
                from.label()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/lifecycle/resource_test.rs"]
mod tests;
