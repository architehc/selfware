//! Shared xdotool / WSL-PowerShell detection and execution helpers.
//!
//! Canonical home for the input-backend probes and the `xdotool` runner that
//! the keyboard and mouse controllers previously each carried a copy of.

#[cfg(target_os = "linux")]
use anyhow::Result;
#[cfg(all(target_os = "linux", not(test)))]
use anyhow::{bail, Context};

/// Whether the `xdotool` binary is on PATH.
pub(crate) fn xdotool_available() -> bool {
    std::process::Command::new("which")
        .arg("xdotool")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Whether we are running under WSL with a working `powershell.exe` interop.
pub(crate) fn can_use_wsl_powershell() -> bool {
    is_wsl_environment()
        && std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Write-Output ok"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn is_wsl_environment() -> bool {
    std::env::var_os("WSL_INTEROP").is_some()
        || std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|r| r.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

/// Execute an xdotool command. Returns error if xdotool is not available.
#[cfg(all(target_os = "linux", not(test)))]
pub(crate) async fn run_xdotool(args: &[String]) -> Result<()> {
    use tokio::process::Command;

    // If no input backend was detected, fail with a clear message instead of
    // shelling out to a missing xdotool binary (which produces a raw ENOENT).
    if !xdotool_available() && !can_use_wsl_powershell() {
        bail!(
            "no input backend available: install xdotool (Linux X11) or run \
             under a supported environment"
        );
    }

    let output = Command::new("xdotool")
        .args(args)
        .output()
        .await
        .context("Failed to execute xdotool. Is xdotool installed? (apt install xdotool)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("xdotool failed (exit {}): {}", output.status, stderr.trim());
    }

    Ok(())
}

/// No-op xdotool stub for tests (avoids requiring xdotool in CI).
#[cfg(all(target_os = "linux", test))]
pub(crate) async fn run_xdotool(_args: &[String]) -> Result<()> {
    Ok(())
}
