//! Small shared helpers with no narrower home.
//!
//! Canonical implementations for cross-cutting one-liners that were
//! previously copy-pasted across `cognitive`, `analysis`, and friends.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

/// Current Unix epoch time in seconds.
///
/// Returns 0 if the system clock is set before the Unix epoch.
pub fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Serialize `value` as pretty JSON and write it to `path`, creating parent
/// directories as needed.
pub fn save_json_pretty<T: Serialize + ?Sized>(value: &T, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Copy text to the system clipboard using the first available platform tool
/// (pbcopy, xclip, xsel, wl-copy). Async on tokio workers.
pub async fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;

    const CANDIDATES: &[(&str, &[&str])] = &[
        ("pbcopy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("wl-copy", &[]),
    ];

    for &(bin, args) in CANDIDATES {
        let mut cmd = std::process::Command::new(bin);
        crate::safety::process_env::sanitize_std_command_env(&mut cmd);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());

        let mut child = match tokio::process::Command::from(cmd).spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).await?;
        }
        let output = child.wait_with_output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "Clipboard tool '{}' exited with {}: {}",
                bin,
                output.status,
                stderr.trim()
            );
        }
        return Ok(());
    }

    anyhow::bail!("No clipboard tool found (pbcopy, xclip, xsel, or wl-copy)")
}
