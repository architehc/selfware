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

    let mut selected = None;
    for (cmd, args) in CANDIDATES {
        let available = tokio::process::Command::new("which")
            .arg(cmd)
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
        if available {
            selected = Some((*cmd, *args));
            break;
        }
    }

    let Some((cmd, args)) = selected else {
        anyhow::bail!("No clipboard tool found (pbcopy, xclip, xsel, or wl-copy)");
    };

    let mut child = tokio::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes()).await?;
    }
    child.wait().await?;
    Ok(())
}
