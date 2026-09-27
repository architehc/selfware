//! The resource lifecycle (formal/DESIGN.md §4.3; the timed part is proved
//! as P3–P5 in `formal/TaskFsm.lean`).
//!
//! ```text
//!  Requested ─start─▶ Starting ─ready─▶ Live ─drain─▶ Draining ─stopped─▶ Released
//!      │ drain            │ fail           │ owner_gone     │ deadline_passed
//!      ▼                  ▼                ▼                ▼
//!   Released           Leaked          Orphaned ─reap─▶ Draining      Leaked
//!                  (Starting ─drain─▶ Draining;  Live/Orphaned ─stopped─▶ Released;
//!                   Leaked ─reap─▶ Draining;     Leaked ─stopped─▶ Released)
//! ```
//!
//! `released` is terminal and sticky. `leaked` is terminal for teardown
//! purposes (it raises [`Effect::LeakAlarm`]) but the reaper may retry it
//! (`reap`), or reconciliation may find it already gone (`stopped`).

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
    /// A bound TCP port.
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
}

impl ResourceEvent {
    /// Every event.
    pub const ALL: [ResourceEvent; 8] = [
        ResourceEvent::Start,
        ResourceEvent::Ready,
        ResourceEvent::Fail,
        ResourceEvent::Drain,
        ResourceEvent::Stopped,
        ResourceEvent::DeadlinePassed,
        ResourceEvent::OwnerGone,
        ResourceEvent::Reap,
    ];
}

/// The resource machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceMachine;

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

    /// `released` is sticky; `leaked` is left only by `reap`/`stopped`; a
    /// `draining` resource always has its deadline exit to a settled state
    /// (the transition-table half of P4).
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
        Ok(())
    }
}
