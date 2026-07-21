//! Selfware Doctor — system diagnostics and dependency checker.
//!
//! Checks that all required and optional tools are available on the system,
//! verifies platform-specific binaries, the project Rust MSRV, and validates
//! the loaded configuration. Each check carries a `fix_hint` so the user gets
//! actionable remediation advice without having to read documentation.
//!
//! Run via `selfware doctor`. Exits non-zero if any required check FAILs.

use std::fmt;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

use colored::Colorize;
use futures::future::join_all;
use tokio::process::Command;

/// A boxed future that yields a `DoctorCheck`. Used so we can collect
/// heterogeneous async checks into a single `Vec` for `join_all`.
type CheckFut = Pin<Box<dyn std::future::Future<Output = DoctorCheck> + Send>>;

/// Maximum time to wait for a single tool check.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Project minimum supported Rust version (kept in sync with `Cargo.toml`'s
/// `rust-version` field).
pub const MSRV: &str = "1.91";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Category of a doctor check.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Category {
    Core,
    Platform,
    Languages,
    RustTools,
    PythonTools,
    NodeTools,
    GoTools,
    ComputerControl,
    ContainerTools,
    BrowserAutomation,
    Security,
    Configuration,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Category::Core => write!(f, "Core (Required)"),
            Category::Platform => write!(f, "Platform Tools"),
            Category::Languages => write!(f, "Languages (Optional)"),
            Category::RustTools => write!(f, "Rust Tools"),
            Category::PythonTools => write!(f, "Python Tools"),
            Category::NodeTools => write!(f, "Node Tools"),
            Category::GoTools => write!(f, "Go Tools"),
            Category::ComputerControl => write!(f, "Computer Control / Image Processing"),
            Category::ContainerTools => write!(f, "Container Tools"),
            Category::BrowserAutomation => write!(f, "Browser Automation"),
            Category::Security => write!(f, "Security"),
            Category::Configuration => write!(f, "Configuration"),
        }
    }
}

/// Result status of a single check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warning,
    Missing,
}

/// A single diagnostic check result.
#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub category: Category,
    pub status: CheckStatus,
    pub version: Option<String>,
    pub message: String,
    /// Optional one-line "How to fix:" hint shown beneath FAIL/WARN entries.
    pub fix_hint: Option<String>,
}

/// Overall health of the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverallHealth {
    Healthy,
    Degraded,
    Broken,
}

impl fmt::Display for OverallHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverallHealth::Healthy => write!(f, "healthy"),
            OverallHealth::Degraded => write!(f, "degraded"),
            OverallHealth::Broken => write!(f, "broken"),
        }
    }
}

/// Full diagnostic report.
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
    pub health: OverallHealth,
}

// ---------------------------------------------------------------------------
// Check helpers
// ---------------------------------------------------------------------------

/// Run a command with a timeout, returning stdout on success.
async fn run_cmd(program: &str, args: &[&str]) -> Option<String> {
    let result = tokio::time::timeout(CHECK_TIMEOUT, Command::new(program).args(args).output())
        .await
        .ok()?
        .ok()?;

    let out = if result.status.success() {
        String::from_utf8_lossy(&result.stdout).to_string()
    } else {
        // Some tools print version to stderr (e.g. rustfmt)
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        if stderr.is_empty() {
            return None;
        }
        stderr
    };
    Some(out.trim().to_string())
}

/// Extract a version-like string from output (first token matching semver-ish).
fn extract_version(output: &str) -> Option<String> {
    for token in output.split_whitespace() {
        let cleaned = token.trim_start_matches('v').trim_end_matches(',');
        // Accept tokens that start with a digit and contain at least one dot.
        if cleaned
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
            && cleaned.contains('.')
        {
            return Some(cleaned.to_string());
        }
    }
    None
}

/// Compare two semver-ish version strings — returns true if `actual >= required`.
pub(crate) fn version_at_least(actual: &str, required: &str) -> bool {
    fn parts(s: &str) -> Vec<u64> {
        s.split('.')
            .map(|x| {
                x.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            })
            .filter_map(|x| x.parse::<u64>().ok())
            .collect()
    }
    let a = parts(actual);
    let r = parts(required);
    for i in 0..r.len().max(a.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let rv = r.get(i).copied().unwrap_or(0);
        if av > rv {
            return true;
        }
        if av < rv {
            return false;
        }
    }
    true
}

/// Suggest a per-OS install command for a missing binary.
///
/// Returns a single human-readable line; the caller prefixes "How to fix: ".
pub(crate) fn install_hint_for(_name: &str, programs: &[&str]) -> Option<String> {
    let primary = programs.first().copied().unwrap_or("");

    let s: Option<&'static str> = match primary {
        "rustc" | "cargo" => Some(
            "Install via rustup: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`",
        ),
        "git" => {
            if cfg!(target_os = "linux") {
                Some("Linux: `sudo apt install git` (Debian/Ubuntu) / `sudo dnf install git` (Fedora) / `sudo pacman -S git` (Arch)")
            } else if cfg!(target_os = "macos") {
                Some("macOS: `brew install git` or install Xcode Command Line Tools (`xcode-select --install`)")
            } else {
                Some("Windows: install Git for Windows from https://git-scm.com/download/win")
            }
        }
        "wmctrl" => Some(
            "Linux: `sudo apt install wmctrl` (Debian/Ubuntu) / `sudo dnf install wmctrl` (Fedora) / `sudo pacman -S wmctrl` (Arch)",
        ),
        "xdotool" => Some(
            "Linux: `sudo apt install xdotool` (Debian/Ubuntu) / `sudo dnf install xdotool` (Fedora) / `sudo pacman -S xdotool` (Arch)",
        ),
        "osascript" => Some(
            "macOS only — ships with the OS. If missing, your install is broken; reinstall macOS tools.",
        ),
        "powershell" | "pwsh" => Some(
            "Windows: PowerShell ships with Windows 7+. Install PowerShell 7 with `winget install --id Microsoft.Powershell`.",
        ),
        "docker" => Some(
            "Install Docker Desktop (https://docker.com) or `sudo apt install docker.io` on Linux.",
        ),
        "node" | "nodejs" => Some(
            "Install Node.js LTS: https://nodejs.org or use a version manager (`nvm`, `fnm`).",
        ),
        "python3" | "python" => Some(
            "Install Python 3 from https://python.org or your distro: `sudo apt install python3`.",
        ),
        _ => None,
    };

    s.map(|x| x.to_string())
}

/// Check a tool by running `program --version` (or custom args).
async fn check_tool(
    name: &str,
    category: Category,
    required: bool,
    programs: &[&str],
    version_args: &[&str],
) -> DoctorCheck {
    let fix_hint = install_hint_for(name, programs);
    for program in programs {
        if let Some(output) = run_cmd(program, version_args).await {
            let version = extract_version(&output);
            return DoctorCheck {
                name: name.to_string(),
                category,
                status: CheckStatus::Ok,
                version,
                message: format!("Found: {}", output.lines().next().unwrap_or(&output)),
                fix_hint: None,
            };
        }
    }
    DoctorCheck {
        name: name.to_string(),
        category,
        status: if required {
            CheckStatus::Missing
        } else {
            CheckStatus::Warning
        },
        version: None,
        message: if required {
            format!(
                "REQUIRED — install {}",
                programs
                    .iter()
                    .map(|s| format!("`{}`", s))
                    .collect::<Vec<_>>()
                    .join(" or ")
            )
        } else {
            "Not found (optional)".to_string()
        },
        fix_hint,
    }
}

/// Check a tool that uses `npx <tool> --version`.
async fn check_npx_tool(name: &str, category: Category, tool: &str) -> DoctorCheck {
    if let Some(output) = run_cmd("npx", &[tool, "--version"]).await {
        let version = extract_version(&output);
        return DoctorCheck {
            name: name.to_string(),
            category,
            status: CheckStatus::Ok,
            version,
            message: format!("Found: {}", output.lines().next().unwrap_or(&output)),
            fix_hint: None,
        };
    }
    DoctorCheck {
        name: name.to_string(),
        category,
        status: CheckStatus::Warning,
        version: None,
        message: "Not found (optional)".to_string(),
        fix_hint: Some(format!(
            "Install Node.js, then `npm install -g {}` (or invoke via `npx {}`).",
            tool, tool
        )),
    }
}

// ---------------------------------------------------------------------------
// macOS accessibility check
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
async fn check_accessibility() -> DoctorCheck {
    // osascript returns 0 if system events are accessible
    let result = run_cmd(
        "osascript",
        &[
            "-e",
            "tell application \"System Events\" to return name of first process",
        ],
    )
    .await;
    match result {
        Some(_) => DoctorCheck {
            name: "Accessibility (macOS)".to_string(),
            category: Category::ComputerControl,
            status: CheckStatus::Ok,
            version: None,
            message: "Accessibility permissions granted".to_string(),
            fix_hint: None,
        },
        None => DoctorCheck {
            name: "Accessibility (macOS)".to_string(),
            category: Category::ComputerControl,
            status: CheckStatus::Warning,
            version: None,
            message: "Accessibility permissions not granted — needed for computer control"
                .to_string(),
            fix_hint: Some(
                "System Settings → Privacy & Security → Accessibility → enable your terminal/IDE."
                    .to_string(),
            ),
        },
    }
}

#[cfg(not(target_os = "macos"))]
async fn check_accessibility() -> DoctorCheck {
    DoctorCheck {
        name: "Accessibility".to_string(),
        category: Category::ComputerControl,
        status: CheckStatus::Ok,
        version: None,
        message: "Not applicable on this platform".to_string(),
        fix_hint: None,
    }
}

// ---------------------------------------------------------------------------
// xcap crate availability check
// ---------------------------------------------------------------------------

async fn check_xcap() -> DoctorCheck {
    let backend = tokio::task::spawn_blocking(crate::computer::ScreenCapture::available_backend)
        .await
        .ok()
        .flatten();

    if let Some(backend) = backend {
        DoctorCheck {
            name: format!("screen capture ({})", backend.doctor_name()),
            category: Category::ComputerControl,
            status: CheckStatus::Ok,
            version: None,
            message: backend.doctor_message().to_string(),
            fix_hint: None,
        }
    } else {
        DoctorCheck {
            name: "screen capture".to_string(),
            category: Category::ComputerControl,
            status: CheckStatus::Warning,
            version: None,
            message: "Screen capture unavailable — check display server / permissions".to_string(),
            fix_hint: Some(
                "Linux: ensure X11 or Wayland is running. macOS: grant Screen Recording in System Settings → Privacy & Security."
                    .to_string(),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Platform-specific binary checks
// ---------------------------------------------------------------------------

/// Per-OS GUI / window-management binary checks.
fn platform_checks() -> Vec<CheckFut> {
    #[cfg(target_os = "linux")]
    {
        vec![
            ck("wmctrl", Category::Platform, false, &["wmctrl"], &["-h"]),
            ck(
                "xdotool",
                Category::Platform,
                false,
                &["xdotool"],
                &["--version"],
            ),
        ]
    }

    #[cfg(target_os = "macos")]
    {
        vec![ck(
            "osascript",
            Category::Platform,
            false,
            &["osascript"],
            &["-e", "return 1"],
        )]
    }

    #[cfg(target_os = "windows")]
    {
        vec![ck(
            "powershell",
            Category::Platform,
            true,
            &["powershell", "pwsh"],
            &["-Command", "$PSVersionTable.PSVersion.ToString()"],
        )]
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// Note explicitly stating Windows GUI tools are not supported on this platform.
#[cfg(target_os = "windows")]
fn windows_gui_note() -> DoctorCheck {
    DoctorCheck {
        name: "GUI computer-control tools".to_string(),
        category: Category::Platform,
        status: CheckStatus::Warning,
        version: None,
        message: "GUI window management (wmctrl/xdotool/osascript) is not supported on Windows"
            .to_string(),
        fix_hint: Some(
            "Use the Linux or macOS port for full computer-control. Selfware core features still work on Windows."
                .to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Rust MSRV check
// ---------------------------------------------------------------------------

/// Check that the installed `rustc` meets the project MSRV.
async fn check_msrv() -> DoctorCheck {
    let output = run_cmd("rustc", &["--version"]).await;
    match output {
        None => DoctorCheck {
            name: "rustc MSRV".to_string(),
            category: Category::Core,
            status: CheckStatus::Missing,
            version: None,
            message: format!("rustc not found (MSRV >= {})", MSRV),
            fix_hint: Some(
                "Install Rust via rustup: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`"
                    .to_string(),
            ),
        },
        Some(out) => match extract_version(&out) {
            Some(v) if version_at_least(&v, MSRV) => DoctorCheck {
                name: "rustc MSRV".to_string(),
                category: Category::Core,
                status: CheckStatus::Ok,
                version: Some(v.clone()),
                message: format!("rustc {} satisfies MSRV >= {}", v, MSRV),
                fix_hint: None,
            },
            Some(v) => DoctorCheck {
                name: "rustc MSRV".to_string(),
                category: Category::Core,
                status: CheckStatus::Warning,
                version: Some(v.clone()),
                message: format!("rustc {} is below project MSRV {}", v, MSRV),
                fix_hint: Some(format!(
                    "Update Rust: `rustup update stable` (need >= {}).",
                    MSRV
                )),
            },
            None => DoctorCheck {
                name: "rustc MSRV".to_string(),
                category: Category::Core,
                status: CheckStatus::Warning,
                version: None,
                message: "Could not parse rustc version".to_string(),
                fix_hint: None,
            },
        },
    }
}

// ---------------------------------------------------------------------------
// Selfware log health check (surfaces observability::log_analysis)
// ---------------------------------------------------------------------------

/// Directory where telemetry writes the daily-rolled `selfware.log` files
/// (kept in sync with the file appender in `observability::telemetry`).
#[cfg(feature = "log-analysis")]
fn selfware_log_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("selfware")
        .join("logs")
}

/// Locate the newest selfware log file in `dir`. The daily rolling appender
/// names files `selfware.log.YYYY-MM-DD`; a plain `*.log` extension is also
/// accepted.
#[cfg(feature = "log-analysis")]
fn newest_log_file(dir: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .map(|n| {
                        let n = n.to_string_lossy();
                        n.starts_with("selfware.log") || n.ends_with(".log")
                    })
                    .unwrap_or(false)
        })
        .max_by_key(|p| {
            p.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        })
}

/// Count error-level entries and anomalies in a log file (last `max_lines`
/// lines). Returns `None` when the file can't be read — doctor treats that as
/// "no logs yet", not a failure.
#[cfg(feature = "log-analysis")]
fn analyze_log_file(path: &Path, max_lines: usize) -> Option<(usize, u64)> {
    use crate::observability::log_analysis::{LogAnalyzer, LogFormat, LogLevel};

    let content = std::fs::read_to_string(path).ok()?;
    let analyzer = LogAnalyzer::new(LogFormat::Plain);
    let mut errors = 0usize;
    for line in content
        .lines()
        .rev()
        .take(max_lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if let Some(entry) = analyzer.process_line(line) {
            if entry.level == LogLevel::Error {
                errors += 1;
            }
        }
    }
    let summary = analyzer.summary();
    Some((errors, summary.anomalies.anomalies_detected))
}

/// Analyze the newest selfware log file for recent error lines and anomalies.
#[cfg(feature = "log-analysis")]
fn check_log_health() -> DoctorCheck {
    let no_logs = || DoctorCheck {
        name: "selfware log health".to_string(),
        category: Category::Core,
        status: CheckStatus::Ok,
        version: None,
        message: "no logs yet".to_string(),
        fix_hint: None,
    };

    let Some(path) = newest_log_file(&selfware_log_dir()) else {
        return no_logs();
    };
    let Some((errors, anomalies)) = analyze_log_file(&path, 500) else {
        return no_logs();
    };

    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    if errors > 0 {
        DoctorCheck {
            name: "selfware log health".to_string(),
            category: Category::Core,
            status: CheckStatus::Warning,
            version: None,
            message: format!(
                "{} error lines, {} anomalies in {}",
                errors, anomalies, file
            ),
            fix_hint: Some(format!(
                "Review recent errors: `tail -n 500 {}`",
                path.display()
            )),
        }
    } else {
        DoctorCheck {
            name: "selfware log health".to_string(),
            category: Category::Core,
            status: CheckStatus::Ok,
            version: None,
            message: format!(
                "{} error lines, {} anomalies in {}",
                errors, anomalies, file
            ),
            fix_hint: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration sanity checks
// ---------------------------------------------------------------------------

/// Run sanity checks against a loaded `Config`. Pure (no I/O against the LLM).
///
/// Verifies that:
///   * the configured endpoint URL parses,
///   * `allowed_paths` glob roots resolve to existing directories
///     (or contain a glob — in which case we accept the literal prefix),
///   * an api_key is present when the endpoint looks remote.
pub fn config_checks(config: &crate::config::Config) -> Vec<DoctorCheck> {
    let mut out = Vec::new();

    // Which config file is in effect — the single most useful piece of
    // provenance when debugging "why is my config ignored". Flags the
    // cwd-shadows-global trap explicitly.
    match config.loaded_config_path() {
        Some(path) => {
            let home_config = dirs::home_dir().map(|h| h.join(".config/selfware/config.toml"));
            let shadows_global = path.file_name() == Some(std::ffi::OsStr::new("selfware.toml"))
                && home_config.as_ref().map(|h| h.is_file()).unwrap_or(false)
                && Some(path) != home_config.as_deref();
            out.push(DoctorCheck {
                name: "config file".to_string(),
                category: Category::Configuration,
                status: CheckStatus::Ok,
                version: None,
                message: if shadows_global {
                    format!(
                        "config: {} (shadowing {})",
                        path.display(),
                        home_config.unwrap().display()
                    )
                } else {
                    format!("config: {}", path.display())
                },
                fix_hint: None,
            });
        }
        None => out.push(DoctorCheck {
            name: "config file".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Ok,
            version: None,
            message: "config: none found — using built-in defaults".to_string(),
            fix_hint: Some(
                "Run `selfware init` to create a config, or write ~/.config/selfware/config.toml."
                    .to_string(),
            ),
        }),
    }

    // Endpoint URL parses
    match url::Url::parse(&config.endpoint) {
        Ok(u) => out.push(DoctorCheck {
            name: "endpoint URL".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Ok,
            version: None,
            message: format!("{} (host: {})", u, u.host_str().unwrap_or("?")),
            fix_hint: None,
        }),
        Err(e) => out.push(DoctorCheck {
            name: "endpoint URL".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Missing,
            version: None,
            message: format!("invalid endpoint `{}` ({})", config.endpoint, e),
            fix_hint: Some(
                "Set a valid URL in selfware.toml `endpoint = \"http://127.0.0.1:8000/v1\"`."
                    .to_string(),
            ),
        }),
    }

    // allowed_paths glob roots exist
    let mut bad_roots: Vec<String> = Vec::new();
    for raw in &config.safety.allowed_paths {
        let root = glob_root(raw);
        if root.is_empty() {
            continue; // pure relative-like patterns ("**") — skip
        }
        let p = Path::new(&root);
        if !p.exists() {
            bad_roots.push(raw.clone());
        }
    }
    if bad_roots.is_empty() {
        out.push(DoctorCheck {
            name: "allowed_paths roots".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Ok,
            version: None,
            message: format!(
                "{} entr{} resolve",
                config.safety.allowed_paths.len(),
                if config.safety.allowed_paths.len() == 1 {
                    "y"
                } else {
                    "ies"
                }
            ),
            fix_hint: None,
        });
    } else {
        out.push(DoctorCheck {
            name: "allowed_paths roots".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Warning,
            version: None,
            message: format!("missing root(s): {}", bad_roots.join(", ")),
            fix_hint: Some(
                "Edit selfware.toml `[safety] allowed_paths` to point at directories that exist."
                    .to_string(),
            ),
        });
    }

    // api_key required for remote endpoints
    let endpoint_lower = config.endpoint.to_lowercase();
    let looks_local = endpoint_lower.contains("localhost")
        || endpoint_lower.contains("127.0.0.1")
        || endpoint_lower.contains("0.0.0.0")
        || endpoint_lower.starts_with("http://192.168.")
        || endpoint_lower.starts_with("http://10.")
        || endpoint_lower.starts_with("http://172.");
    if config.api_key.is_none() && !looks_local {
        out.push(DoctorCheck {
            name: "api_key".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Missing,
            version: None,
            message: "endpoint appears remote but no api_key is set".to_string(),
            fix_hint: Some(
                "Set `SELFWARE_API_KEY=...` in your environment, or `api_key = \"...\"` in selfware.toml."
                    .to_string(),
            ),
        });
    } else {
        out.push(DoctorCheck {
            name: "api_key".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Ok,
            version: None,
            message: if config.api_key.is_some() {
                "api_key present".to_string()
            } else {
                "api_key not set (endpoint is local — OK)".to_string()
            },
            fix_hint: None,
        });
    }

    out
}

/// Strip glob metacharacters from a pattern, returning the literal directory
/// prefix. Examples:
///   `./**`              -> `.`
///   `/srv/foo/**/*.rs`  -> `/srv/foo`
///   `~/code/*`          -> `~/code` (caller may expand `~`)
///   `**`                -> ``
pub(crate) fn glob_root(pattern: &str) -> String {
    let expanded = if let Some(rest) = pattern.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            home.join(rest).to_string_lossy().into_owned()
        } else {
            pattern.to_string()
        }
    } else {
        pattern.to_string()
    };

    let mut root = String::new();
    let starts_abs = expanded.starts_with('/');
    let segments: Vec<&str> = expanded.split('/').collect();
    for (i, seg) in segments.iter().enumerate() {
        if seg.contains('*') || seg.contains('?') || seg.contains('[') {
            break;
        }
        if i == 0 {
            if starts_abs {
                // first segment after leading '/' is empty — emit a single '/'
                root.push('/');
            } else {
                root.push_str(seg);
            }
            continue;
        }
        // For non-leading segments: separator is needed unless we already end in '/'
        if !root.ends_with('/') {
            root.push('/');
        }
        root.push_str(seg);
    }
    root
}

// ---------------------------------------------------------------------------
// Main doctor runner
// ---------------------------------------------------------------------------

/// Helper: wrap `check_tool` in a `CheckFut`.
fn ck(
    name: &'static str,
    category: Category,
    required: bool,
    programs: &'static [&'static str],
    version_args: &'static [&'static str],
) -> CheckFut {
    Box::pin(check_tool(name, category, required, programs, version_args))
}

/// Run all diagnostic checks and return a report.
pub async fn run_doctor(config_path: Option<&str>) -> DoctorReport {
    // We group checks by category and run each category in parallel.
    // Within each category, checks also run in parallel via join_all.
    // All futures are boxed via `ck()` so they share a single type.

    let core: Vec<CheckFut> = vec![
        ck("rustc", Category::Core, true, &["rustc"], &["--version"]),
        ck("cargo", Category::Core, true, &["cargo"], &["--version"]),
        ck("git", Category::Core, true, &["git"], &["--version"]),
        Box::pin(check_msrv()),
        #[cfg(feature = "log-analysis")]
        Box::pin(async move { check_log_health() }),
    ];

    let platform: Vec<CheckFut> = platform_checks();

    let languages: Vec<CheckFut> = vec![
        ck(
            "node",
            Category::Languages,
            false,
            &["node", "nodejs"],
            &["--version"],
        ),
        ck("npm", Category::Languages, false, &["npm"], &["--version"]),
        ck(
            "python",
            Category::Languages,
            false,
            &["python3", "python"],
            &["--version"],
        ),
        ck(
            "pip",
            Category::Languages,
            false,
            &["pip3", "pip"],
            &["--version"],
        ),
        ck("go", Category::Languages, false, &["go"], &["version"]),
    ];

    let rust_tools: Vec<CheckFut> = vec![
        ck(
            "cargo-clippy",
            Category::RustTools,
            false,
            &["cargo"],
            &["clippy", "--version"],
        ),
        ck(
            "cargo-fmt",
            Category::RustTools,
            false,
            &["cargo"],
            &["fmt", "--version"],
        ),
        ck(
            "cargo-tarpaulin",
            Category::RustTools,
            false,
            &["cargo"],
            &["tarpaulin", "--version"],
        ),
    ];

    let python_tools: Vec<CheckFut> = vec![
        ck(
            "ruff",
            Category::PythonTools,
            false,
            &["ruff"],
            &["--version"],
        ),
        ck(
            "mypy",
            Category::PythonTools,
            false,
            &["mypy"],
            &["--version"],
        ),
        ck(
            "pytest",
            Category::PythonTools,
            false,
            &["pytest"],
            &["--version"],
        ),
        ck(
            "black",
            Category::PythonTools,
            false,
            &["black"],
            &["--version"],
        ),
        ck(
            "bandit",
            Category::PythonTools,
            false,
            &["bandit"],
            &["--version"],
        ),
    ];

    let node_tools: Vec<CheckFut> = vec![
        ck("npx", Category::NodeTools, false, &["npx"], &["--version"]),
        ck(
            "eslint",
            Category::NodeTools,
            false,
            &["eslint"],
            &["--version"],
        ),
        ck(
            "prettier",
            Category::NodeTools,
            false,
            &["prettier"],
            &["--version"],
        ),
        ck("tsc", Category::NodeTools, false, &["tsc"], &["--version"]),
        ck(
            "vitest",
            Category::NodeTools,
            false,
            &["vitest"],
            &["--version"],
        ),
    ];

    let go_tools: Vec<CheckFut> = vec![
        ck(
            "golangci-lint",
            Category::GoTools,
            false,
            &["golangci-lint"],
            &["--version"],
        ),
        ck(
            "gofmt",
            Category::GoTools,
            false,
            &["gofmt"],
            &["-e", "/dev/null"],
        ),
    ];

    let container: Vec<CheckFut> = vec![
        ck(
            "docker",
            Category::ContainerTools,
            false,
            &["docker"],
            &["--version"],
        ),
        ck(
            "docker-compose",
            Category::ContainerTools,
            false,
            &["docker-compose"],
            &["--version"],
        ),
    ];

    let security: Vec<CheckFut> = vec![
        ck(
            "cargo-audit",
            Category::Security,
            false,
            &["cargo"],
            &["audit", "--version"],
        ),
        ck(
            "safety",
            Category::Security,
            false,
            &["safety"],
            &["--version"],
        ),
    ];

    let computer: Vec<CheckFut> = vec![
        ck(
            "ImageMagick",
            Category::ComputerControl,
            false,
            &["convert"],
            &["--version"],
        ),
        ck(
            "ffmpeg",
            Category::ComputerControl,
            false,
            &["ffmpeg"],
            &["-version"],
        ),
    ];

    let browser: Vec<CheckFut> = vec![
        ck(
            "chromium/chrome",
            Category::BrowserAutomation,
            false,
            &["chromium", "google-chrome", "chrome", "chromium-browser"],
            &["--version"],
        ),
        Box::pin(check_npx_tool(
            "playwright",
            Category::BrowserAutomation,
            "playwright",
        )),
    ];

    // Run all categories concurrently
    let (
        core_results,
        platform_results,
        lang_results,
        rust_results,
        py_results,
        node_results,
        go_results,
        container_results,
        security_results,
        computer_base,
        browser_results,
        xcap_result,
        accessibility_result,
    ) = tokio::join!(
        join_all(core),
        join_all(platform),
        join_all(languages),
        join_all(rust_tools),
        join_all(python_tools),
        join_all(node_tools),
        join_all(go_tools),
        join_all(container),
        join_all(security),
        join_all(computer),
        join_all(browser),
        check_xcap(),
        check_accessibility(),
    );

    // Also check `docker compose` (v2 plugin) if docker-compose was missing
    let mut container_checks: Vec<DoctorCheck> = container_results;
    {
        let dc_missing = container_checks
            .iter()
            .find(|c| c.name == "docker-compose")
            .map(|c| c.status != CheckStatus::Ok)
            .unwrap_or(true);
        if dc_missing {
            // Try `docker compose version` (v2 plugin)
            if let Some(output) = run_cmd("docker", &["compose", "version"]).await {
                let version = extract_version(&output);
                if let Some(existing) = container_checks
                    .iter_mut()
                    .find(|c| c.name == "docker-compose")
                {
                    existing.status = CheckStatus::Ok;
                    existing.version = version;
                    existing.message =
                        format!("Found: {}", output.lines().next().unwrap_or(&output));
                    existing.name = "docker compose (v2)".to_string();
                    existing.fix_hint = None;
                }
            }
        }
    }

    // Assemble all checks in category order.
    let mut checks = Vec::new();
    checks.extend(core_results);
    checks.extend(platform_results);
    #[cfg(target_os = "windows")]
    {
        checks.push(windows_gui_note());
    }
    // Best-effort: load and validate the user's config.
    match crate::config::Config::load(config_path) {
        Ok(cfg) => checks.extend(config_checks(&cfg)),
        Err(e) => checks.push(DoctorCheck {
            name: "config load".to_string(),
            category: Category::Configuration,
            status: CheckStatus::Warning,
            version: None,
            message: format!("could not load selfware.toml: {}", e),
            fix_hint: Some(
                "Run `selfware init`, or copy `selfware.example.toml` → `selfware.toml`."
                    .to_string(),
            ),
        }),
    }
    checks.extend(lang_results);
    checks.extend(rust_results);
    checks.extend(py_results);
    checks.extend(node_results);
    checks.extend(go_results);
    checks.extend(computer_base);
    checks.push(xcap_result);
    checks.push(accessibility_result);
    checks.extend(container_checks);
    checks.extend(browser_results);
    checks.extend(security_results);

    // Determine overall health.
    let has_missing = checks.iter().any(|c| c.status == CheckStatus::Missing);
    let has_warning = checks.iter().any(|c| c.status == CheckStatus::Warning);
    let health = if has_missing {
        OverallHealth::Broken
    } else if has_warning {
        OverallHealth::Degraded
    } else {
        OverallHealth::Healthy
    };

    DoctorReport { checks, health }
}

// ---------------------------------------------------------------------------
// Pretty-printing
// ---------------------------------------------------------------------------

impl DoctorReport {
    /// Process exit code: `0` if no FAIL (Missing) status, `1` otherwise.
    pub fn exit_code(&self) -> i32 {
        if self.checks.iter().any(|c| c.status == CheckStatus::Missing) {
            1
        } else {
            0
        }
    }

    /// Print a nicely formatted, colored report to stdout, using the unified
    /// `[PASS] / [WARN] / [FAIL]` tag format with per-FAIL "How to fix:" hints.
    pub fn print(&self) {
        println!();
        println!(
            "{}",
            "  Selfware Doctor — System Diagnostics".bold().underline()
        );
        println!();

        // Group by category, preserving insertion order.
        let mut current_category: Option<&Category> = None;

        for check in &self.checks {
            if current_category != Some(&check.category) {
                if current_category.is_some() {
                    println!(); // blank line between categories
                }
                current_category = Some(&check.category);
                println!("  {}:", check.category.to_string().bold());
            }

            let (tag, line) = match check.status {
                CheckStatus::Ok => {
                    let ver = check
                        .version
                        .as_deref()
                        .map(|v| format!(" ({})", v))
                        .unwrap_or_default();
                    (
                        "[PASS]".green().bold().to_string(),
                        format!("{}{}", check.name, ver).green().to_string(),
                    )
                }
                CheckStatus::Warning => (
                    "[WARN]".yellow().bold().to_string(),
                    format!("{} — {}", check.name, check.message)
                        .yellow()
                        .to_string(),
                ),
                CheckStatus::Missing => (
                    "[FAIL]".red().bold().to_string(),
                    format!("{} — {}", check.name, check.message)
                        .red()
                        .to_string(),
                ),
            };

            println!("  {} {}", tag, line);
            // Show "How to fix:" beneath FAIL/WARN entries.
            if check.status != CheckStatus::Ok {
                if let Some(hint) = &check.fix_hint {
                    println!("         {} {}", "How to fix:".bold().cyan(), hint);
                }
            }
        }

        println!();

        // Summary
        let total = self.checks.len();
        let ok_count = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Ok)
            .count();
        let warn_count = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Warning)
            .count();
        let missing_count = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Missing)
            .count();

        let summary_line = format!(
            "  {}/{} checks passed, {} warnings, {} missing",
            ok_count, total, warn_count, missing_count
        );

        match self.health {
            OverallHealth::Healthy => {
                println!("  {}", summary_line.green().bold());
                println!("  {}", "  Status: healthy — all systems go!".green().bold());
            }
            OverallHealth::Degraded => {
                println!("  {}", summary_line.yellow().bold());
                println!(
                    "  {}",
                    "  Status: degraded — some optional tools are missing"
                        .yellow()
                        .bold()
                );
            }
            OverallHealth::Broken => {
                println!("  {}", summary_line.red().bold());
                println!(
                    "  {}",
                    "  Status: broken — required tools are missing!"
                        .red()
                        .bold()
                );

                let missing: Vec<&str> = self
                    .checks
                    .iter()
                    .filter(|c| c.status == CheckStatus::Missing)
                    .map(|c| c.name.as_str())
                    .collect();
                println!();
                println!(
                    "  {}",
                    format!("  Missing required: {}", missing.join(", "))
                        .red()
                        .bold()
                );
            }
        }

        println!();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "../tests/unit/doctor/doctor_test.rs"]
mod tests;
