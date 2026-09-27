//! Talking to the host about a registered resource.
//!
//! [`ResourceDriver`] is the seam between the teardown/reaper logic and the
//! host: tests inject a fake, production uses [`SystemDriver`], which shells
//! out to docker/podman and signals processes. Two invariants hold for every
//! [`SystemDriver`] action:
//!
//! - a container is only stopped or removed while it still carries the
//!   `selfware.task` label it was started with;
//! - a pid (or its process group) is only signalled while the OS start time
//!   of that pid still equals the one recorded at spawn — a reused pid is a
//!   foreign process and is never touched.

use super::registry::SessionRecord;
use super::{Resource, ResourceHandle};
use crate::tools::process_guard::GroupedOutputExt;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::process::Output;
use std::time::Duration;

/// What a probe found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// Still running / present.
    Running,
    /// Confirmed not running any more (or never existed).
    Gone,
    /// The handle now refers to something selfware does not own (reused pid,
    /// relabelled container). Never acted on.
    Foreign(String),
    /// Could not determine (runtime unavailable, no recorded start time…).
    Unknown(String),
}

/// A container found by label (`selfware.task`), for reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelledContainer {
    pub runtime: String,
    pub id: String,
    pub task: String,
    pub session: String,
    pub agent: Option<String>,
    pub running: bool,
    pub name: String,
    pub image: String,
}

/// Host operations the teardown engine and reaper need.
#[async_trait]
pub trait ResourceDriver: Send + Sync {
    /// Is the resource still there, and still ours?
    async fn probe(&self, resource: &Resource) -> Probe;
    /// Ask it to stop (SIGTERM, `docker kill -s TERM`, `compose down`).
    async fn polite_stop(&self, resource: &Resource) -> Result<()>;
    /// Make it stop (SIGKILL, `docker kill`).
    async fn force_stop(&self, resource: &Resource) -> Result<()>;
    /// After it stopped: remove what remains (a stopped container) and
    /// confirm. An error means release could not be confirmed.
    async fn finalize(&self, _resource: &Resource) -> Result<()> {
        Ok(())
    }
    /// Containers carrying a `selfware.task` label, across runtimes.
    async fn labelled_containers(&self) -> Result<Vec<LabelledContainer>>;
    /// Is the selfware process behind `session` still running?
    fn session_alive(&self, session: &SessionRecord) -> bool;
}

// ---------------------------------------------------------------------------
// Process identity
// ---------------------------------------------------------------------------

/// OS start time (seconds since the epoch) of `pid`, if it exists.
pub fn process_start_time(pid: u32) -> Option<u64> {
    match pid_status(pid) {
        PidStatus::Alive { start_time } => Some(start_time),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PidStatus {
    Absent,
    Zombie,
    Alive { start_time: u64 },
}

fn pid_status(pid: u32) -> PidStatus {
    use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, System};
    let target = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[target]), true);
    match sys.process(target) {
        None => PidStatus::Absent,
        Some(p) if matches!(p.status(), ProcessStatus::Zombie | ProcessStatus::Dead) => {
            PidStatus::Zombie
        }
        Some(p) => PidStatus::Alive {
            start_time: p.start_time(),
        },
    }
}

/// Probe a pid against its recorded start time.
pub fn probe_pid(pid: u32, recorded_start: Option<u64>) -> Probe {
    match pid_status(pid) {
        PidStatus::Absent | PidStatus::Zombie => Probe::Gone,
        PidStatus::Alive { start_time } => match recorded_start {
            // A different start time: our process exited and the pid was
            // reused. Ours is gone; the new one is not ours to touch.
            Some(recorded) if recorded != start_time => Probe::Gone,
            Some(_) => Probe::Running,
            None => Probe::Unknown(format!(
                "pid {pid} is running but its start time was not recorded; not signalling it"
            )),
        },
    }
}

/// Signal a verified pid (and its process group when it leads one).
fn signal_verified(pid: u32, pgid: Option<u32>, start: Option<u64>, force: bool) -> Result<()> {
    match probe_pid(pid, start) {
        Probe::Running => {}
        Probe::Gone => return Ok(()),
        Probe::Foreign(why) | Probe::Unknown(why) => anyhow::bail!(why),
    }
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, killpg, Signal};
        use nix::unistd::Pid;
        let signal = if force {
            Signal::SIGKILL
        } else {
            Signal::SIGTERM
        };
        let raw = i32::try_from(pid).context("pid does not fit pid_t")?;
        // Only signal the group the verified pid leads (pgid == pid); a
        // recorded pgid naming some other group is never signalled.
        if pgid == Some(pid) && raw > 1 && killpg(Pid::from_raw(raw), signal).is_ok() {
            return Ok(());
        }
        kill(Pid::from_raw(raw), signal).with_context(|| format!("signal pid {pid}"))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = pgid;
        use sysinfo::{Pid, ProcessesToUpdate, System};
        let target = Pid::from_u32(pid);
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::Some(&[target]), true);
        let process = sys.process(target).context("process vanished")?;
        let sent = if force {
            process.kill()
        } else {
            process
                .kill_with(sysinfo::Signal::Term)
                .unwrap_or_else(|| process.kill())
        };
        anyhow::ensure!(sent, "could not signal pid {pid}");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SystemDriver
// ---------------------------------------------------------------------------

/// The production driver (see the module docs for its invariants).
#[derive(Debug, Clone)]
pub struct SystemDriver {
    /// Bound on each docker/podman/compose invocation.
    pub command_timeout: Duration,
}

impl Default for SystemDriver {
    fn default() -> Self {
        Self {
            command_timeout: Duration::from_secs(20),
        }
    }
}

impl SystemDriver {
    async fn run(&self, program: &str, args: &[&str], cwd: Option<&Path>) -> Result<Output> {
        use crate::safety::process_env::SanitizedEnvExt;
        let mut cmd = tokio::process::Command::new(program);
        cmd.sanitized_env_preserve(crate::safety::process_env::CONTAINER_RUNTIME_ENV);
        cmd.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        tokio::time::timeout(self.command_timeout, cmd.output_grouped())
            .await
            .with_context(|| format!("{program} {} timed out", args.join(" ")))?
            .with_context(|| format!("run {program}"))
    }

    fn compose_cmd<'a>(
        runtime: &'a str,
        file: Option<&'a str>,
        tail: &[&'a str],
    ) -> (&'a str, Vec<&'a str>) {
        let (program, mut args) = if runtime == "podman" {
            ("podman-compose", Vec::new())
        } else {
            ("docker", vec!["compose"])
        };
        if let Some(f) = file {
            args.push("-f");
            args.push(f);
        }
        args.extend_from_slice(tail);
        (program, args)
    }

    async fn probe_container(&self, runtime: &str, id: &str, task_label: &str) -> Probe {
        let format = "{{.State.Running}}|{{index .Config.Labels \"selfware.task\"}}";
        let out = match self
            .run(
                runtime,
                &["inspect", "--type", "container", "--format", format, id],
                None,
            )
            .await
        {
            Ok(out) => out,
            Err(e) => return Probe::Unknown(format!("{e:#}")),
        };
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.to_ascii_lowercase().contains("no such") {
                return Probe::Gone;
            }
            return Probe::Unknown(format!("{runtime} inspect failed: {}", stderr.trim()));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let (running, label) = text.trim().split_once('|').unwrap_or((text.trim(), ""));
        if label != task_label {
            return Probe::Foreign(format!(
                "container {id} no longer carries selfware.task={task_label}; not touched"
            ));
        }
        if running == "true" {
            Probe::Running
        } else {
            Probe::Gone
        }
    }
}

#[async_trait]
impl ResourceDriver for SystemDriver {
    async fn probe(&self, resource: &Resource) -> Probe {
        match &resource.handle {
            ResourceHandle::Container {
                runtime,
                id,
                task_label,
            } => self.probe_container(runtime, id, task_label).await,
            ResourceHandle::Compose { runtime, dir, file } => {
                let (program, args) = Self::compose_cmd(runtime, file.as_deref(), &["ps", "-q"]);
                match self.run(program, &args, Some(dir)).await {
                    Ok(out) if out.status.success() => {
                        if String::from_utf8_lossy(&out.stdout).trim().is_empty() {
                            Probe::Gone
                        } else {
                            Probe::Running
                        }
                    }
                    Ok(out) => Probe::Unknown(format!(
                        "compose ps failed: {}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    )),
                    Err(e) => Probe::Unknown(format!("{e:#}")),
                }
            }
            ResourceHandle::Process {
                pid,
                start_time,
                managed_id,
                ..
            } => {
                if resource.session == super::session_id() {
                    if let Some(id) = managed_id {
                        // Reaps the child if it exited, so no zombie lingers.
                        if let Some(exited) = crate::tools::process::managed_exited(id).await {
                            return if exited { Probe::Gone } else { Probe::Running };
                        }
                    }
                }
                probe_pid(*pid, *start_time)
            }
            ResourceHandle::Pty {
                session_id,
                pid,
                start_time,
                ..
            } => {
                if resource.session == super::session_id() {
                    if let Some(exited) = crate::tools::pty_shell::session_exited(session_id).await
                    {
                        return if exited { Probe::Gone } else { Probe::Running };
                    }
                }
                probe_pid(*pid, *start_time)
            }
            ResourceHandle::Port { .. } | ResourceHandle::Path { .. } => Probe::Unknown(format!(
                "{} resources are not stopped automatically",
                resource.kind
            )),
        }
    }

    async fn polite_stop(&self, resource: &Resource) -> Result<()> {
        self.stop(resource, false).await
    }

    async fn force_stop(&self, resource: &Resource) -> Result<()> {
        self.stop(resource, true).await
    }

    async fn finalize(&self, resource: &Resource) -> Result<()> {
        match &resource.handle {
            ResourceHandle::Container {
                runtime,
                id,
                task_label,
            } => {
                match self.probe_container(runtime, id, task_label).await {
                    Probe::Gone => {}
                    other => anyhow::bail!("container {id} not removable: {other:?}"),
                }
                // Exited but still present? Remove it (label verified above).
                let out = self.run(runtime, &["rm", id], None).await?;
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if !stderr.to_ascii_lowercase().contains("no such") {
                        anyhow::bail!("stopped but not removed: {}", stderr.trim());
                    }
                }
                Ok(())
            }
            ResourceHandle::Compose { runtime, dir, file } => {
                let (program, args) = Self::compose_cmd(runtime, file.as_deref(), &["down"]);
                let out = self.run(program, &args, Some(dir)).await?;
                anyhow::ensure!(
                    out.status.success(),
                    "compose down failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
                Ok(())
            }
            ResourceHandle::Pty { session_id, .. } => {
                crate::tools::pty_shell::forget_session(session_id).await;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn labelled_containers(&self) -> Result<Vec<LabelledContainer>> {
        let format = "{{.ID}}|{{.Label \"selfware.task\"}}|{{.Label \"selfware.session\"}}|{{.Label \"selfware.agent\"}}|{{.State}}|{{.Names}}|{{.Image}}";
        let mut found = Vec::new();
        let mut errors = Vec::new();
        let mut any_runtime = false;
        for runtime in ["docker", "podman"] {
            let out = match self
                .run(
                    runtime,
                    &[
                        "ps",
                        "-a",
                        "--no-trunc",
                        "--filter",
                        "label=selfware.task",
                        "--format",
                        format,
                    ],
                    None,
                )
                .await
            {
                Ok(out) => out,
                Err(e) => {
                    let missing = e
                        .chain()
                        .filter_map(|c| c.downcast_ref::<std::io::Error>())
                        .any(|io| io.kind() == std::io::ErrorKind::NotFound);
                    if !missing {
                        errors.push(format!("{runtime}: {e:#}"));
                    }
                    continue;
                }
            };
            any_runtime = true;
            if !out.status.success() {
                errors.push(format!(
                    "{runtime} ps: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
                continue;
            }
            found.extend(parse_labelled(
                runtime,
                &String::from_utf8_lossy(&out.stdout),
            ));
        }
        if found.is_empty() && !errors.is_empty() {
            anyhow::bail!(errors.join("; "));
        }
        if !any_runtime && errors.is_empty() {
            return Ok(Vec::new());
        }
        Ok(found)
    }

    fn session_alive(&self, session: &SessionRecord) -> bool {
        if session.id == super::session_id() {
            return true;
        }
        match probe_pid(session.pid, session.start_time) {
            Probe::Gone => false,
            // Running, or alive with an unknown start time: assume alive so
            // another session's resources are never reaped under it.
            _ => true,
        }
    }
}

impl SystemDriver {
    async fn stop(&self, resource: &Resource, force: bool) -> Result<()> {
        match &resource.handle {
            ResourceHandle::Container {
                runtime,
                id,
                task_label,
            } => {
                match self.probe_container(runtime, id, task_label).await {
                    Probe::Running => {}
                    Probe::Gone => return Ok(()),
                    Probe::Foreign(why) | Probe::Unknown(why) => anyhow::bail!(why),
                }
                let signal = if force { "KILL" } else { "TERM" };
                let out = self
                    .run(runtime, &["kill", "--signal", signal, id], None)
                    .await?;
                anyhow::ensure!(
                    out.status.success(),
                    "{runtime} kill: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
                Ok(())
            }
            ResourceHandle::Compose { runtime, dir, file } => {
                let tail: &[&str] = if force { &["kill"] } else { &["stop"] };
                let (program, args) = Self::compose_cmd(runtime, file.as_deref(), tail);
                let out = self.run(program, &args, Some(dir)).await?;
                anyhow::ensure!(
                    out.status.success(),
                    "compose {}: {}",
                    tail[0],
                    String::from_utf8_lossy(&out.stderr).trim()
                );
                Ok(())
            }
            ResourceHandle::Process {
                pid,
                pgid,
                start_time,
                managed_id,
            } => {
                if resource.session == super::session_id() {
                    if let Some(id) = managed_id {
                        if crate::tools::process::signal_managed(id, force).await? {
                            return Ok(());
                        }
                    }
                }
                signal_verified(*pid, *pgid, *start_time, force)
            }
            ResourceHandle::Pty {
                session_id,
                pid,
                pgid,
                start_time,
            } => {
                if resource.session == super::session_id()
                    && crate::tools::pty_shell::signal_session(session_id, force).await
                {
                    return Ok(());
                }
                signal_verified(*pid, *pgid, *start_time, force)
            }
            ResourceHandle::Port { .. } | ResourceHandle::Path { .. } => {
                anyhow::bail!("{} resources are not stopped automatically", resource.kind)
            }
        }
    }
}

/// Parse `docker ps --format` output produced by
/// [`SystemDriver::labelled_containers`]. Lines without both the
/// `selfware.task` and `selfware.session` labels are dropped: an unlabelled
/// container is never ours.
pub fn parse_labelled(runtime: &str, text: &str) -> Vec<LabelledContainer> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.trim().splitn(7, '|').collect();
            // Both ownership labels must be present: a lone `selfware.task`
            // (e.g. inherited from somewhere else) is not proof enough.
            if parts.len() < 7 || parts[0].is_empty() || parts[1].is_empty() || parts[2].is_empty()
            {
                return None;
            }
            Some(LabelledContainer {
                runtime: runtime.to_string(),
                id: parts[0].to_string(),
                task: parts[1].to_string(),
                session: parts[2].to_string(),
                agent: (!parts[3].is_empty()).then(|| parts[3].to_string()),
                running: parts[4].eq_ignore_ascii_case("running"),
                name: parts[5].to_string(),
                image: parts[6].to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/resources/driver_test.rs"]
mod tests;
