//! Applying evolve actions by driving the agent (`selfware run`) as a subprocess.
//!
//! A node-action like `merge_duplicate` describes *what* to change; applying it
//! means actually editing the code. Rather than reimplement editing, this spawns
//! `selfware run "<task>" --yolo` (the same agent, headless + auto-approve),
//! streams its output into a run registry, and reports status — so the UI can
//! kick off a consolidation and watch it land.
//!
//! Safety: the caller must ensure a clean working tree first (checked at the
//! endpoint), so the resulting diff is exactly the agent's work and reviewable.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    Running,
    Succeeded,
    Failed,
}

/// One agent-driven apply run.
#[derive(Debug, Clone, Serialize)]
pub struct ApplyRun {
    pub id: String,
    pub prompt: String,
    pub status: ApplyStatus,
    pub output: String,
    pub exit_code: Option<i32>,
}

/// Shared registry of in-flight / finished apply runs.
pub type ApplyRegistry = Arc<Mutex<HashMap<String, ApplyRun>>>;

pub fn new_registry() -> ApplyRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

/// The maximum output kept per run (bytes), so a chatty agent can't grow memory
/// unbounded; older output is dropped from the front.
const MAX_OUTPUT: usize = 200_000;

/// Spawn `selfware run "<prompt>" --yolo` in `project_root`, streaming stdout +
/// stderr into the run's output buffer. Returns the run id immediately; the run
/// continues in the background.
pub async fn spawn(
    prompt: String,
    project_root: PathBuf,
    registry: ApplyRegistry,
) -> Result<String> {
    let exe = std::env::current_exe()?;
    let id = format!("apply-{}", uuid::Uuid::new_v4().simple());
    registry.lock().await.insert(
        id.clone(),
        ApplyRun {
            id: id.clone(),
            prompt: prompt.clone(),
            status: ApplyStatus::Running,
            output: String::new(),
            exit_code: None,
        },
    );

    let mut child = Command::new(exe)
        .arg("run")
        .arg(&prompt)
        .arg("--yolo")
        .current_dir(&project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let reg = registry.clone();
    let rid = id.clone();
    tokio::spawn(async move {
        tokio::join!(
            pump_pipe(stdout, reg.clone(), rid.clone()),
            pump_pipe(stderr, reg.clone(), rid.clone()),
        );
        let code = child.wait().await.ok().and_then(|s| s.code());
        if let Some(run) = reg.lock().await.get_mut(&rid) {
            run.exit_code = code;
            run.status = if code == Some(0) {
                ApplyStatus::Succeeded
            } else {
                ApplyStatus::Failed
            };
        }
    });

    Ok(id)
}

/// Stream one pipe's lines into the run's output buffer (bounded). Generic over
/// stdout/stderr, which are distinct concrete types.
async fn pump_pipe<R>(pipe: Option<R>, reg: ApplyRegistry, rid: String)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let Some(pipe) = pipe else { return };
    let mut lines = BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(run) = reg.lock().await.get_mut(&rid) {
            run.output.push_str(&line);
            run.output.push('\n');
            if run.output.len() > MAX_OUTPUT {
                let cut = run.output.len() - MAX_OUTPUT;
                run.output.drain(..cut);
            }
        }
    }
}

pub async fn get(registry: &ApplyRegistry, id: &str) -> Option<ApplyRun> {
    registry.lock().await.get(id).cloned()
}
