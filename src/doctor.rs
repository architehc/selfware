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
mod tests {
    use super::*;

    #[test]
    fn test_extract_version_semver() {
        assert_eq!(
            extract_version("rustc 1.79.0 (129f3b996 2024-06-10)"),
            Some("1.79.0".to_string())
        );
    }

    #[test]
    fn test_extract_version_with_v_prefix() {
        assert_eq!(extract_version("v20.11.1"), Some("20.11.1".to_string()));
    }

    #[test]
    fn test_extract_version_git() {
        assert_eq!(
            extract_version("git version 2.45.2"),
            Some("2.45.2".to_string())
        );
    }

    #[test]
    fn test_extract_version_none() {
        assert_eq!(extract_version("no version here"), None);
    }

    #[test]
    fn test_category_display() {
        assert_eq!(Category::Core.to_string(), "Core (Required)");
        assert_eq!(Category::Languages.to_string(), "Languages (Optional)");
        assert_eq!(Category::Platform.to_string(), "Platform Tools");
        assert_eq!(Category::Configuration.to_string(), "Configuration");
    }

    #[test]
    fn test_overall_health_display() {
        assert_eq!(OverallHealth::Healthy.to_string(), "healthy");
        assert_eq!(OverallHealth::Degraded.to_string(), "degraded");
        assert_eq!(OverallHealth::Broken.to_string(), "broken");
    }

    #[test]
    fn test_health_determination() {
        // All OK => Healthy
        let report = DoctorReport {
            checks: vec![DoctorCheck {
                name: "test".into(),
                category: Category::Core,
                status: CheckStatus::Ok,
                version: Some("1.0.0".into()),
                message: "ok".into(),
                fix_hint: None,
            }],
            health: OverallHealth::Healthy,
        };
        assert_eq!(report.health, OverallHealth::Healthy);
        assert_eq!(report.exit_code(), 0);

        // Missing required => Broken => exit 1
        let report = DoctorReport {
            checks: vec![DoctorCheck {
                name: "test".into(),
                category: Category::Core,
                status: CheckStatus::Missing,
                version: None,
                message: "missing".into(),
                fix_hint: Some("install it".into()),
            }],
            health: OverallHealth::Broken,
        };
        assert_eq!(report.health, OverallHealth::Broken);
        assert_eq!(report.exit_code(), 1);
    }

    // ---- version_at_least ----

    #[test]
    fn test_version_at_least_equal() {
        assert!(version_at_least("1.91.0", "1.91"));
        assert!(version_at_least("1.91.0", "1.91.0"));
    }

    #[test]
    fn test_version_at_least_higher() {
        assert!(version_at_least("1.95.0", "1.91"));
        assert!(version_at_least("2.0.0", "1.91.0"));
    }

    #[test]
    fn test_version_at_least_lower() {
        assert!(!version_at_least("1.79.0", "1.91"));
        assert!(!version_at_least("1.91.0", "1.95.0"));
    }

    #[test]
    fn test_version_at_least_with_suffix() {
        // rustc 1.95.0-nightly should satisfy >= 1.91
        assert!(version_at_least("1.95.0-nightly", "1.91"));
    }

    // ---- install_hint_for ----

    #[test]
    fn test_install_hint_for_known() {
        assert!(install_hint_for("rustc", &["rustc"]).is_some());
        assert!(install_hint_for("git", &["git"]).is_some());
        assert!(install_hint_for("wmctrl", &["wmctrl"]).is_some());
        assert!(install_hint_for("xdotool", &["xdotool"]).is_some());
    }

    #[test]
    fn test_install_hint_for_unknown() {
        assert!(install_hint_for("totally-made-up", &["totally-made-up"]).is_none());
    }

    // ---- glob_root ----

    #[test]
    fn test_glob_root_simple() {
        assert_eq!(glob_root("./**"), ".");
    }

    #[test]
    fn test_glob_root_absolute() {
        assert_eq!(glob_root("/srv/foo/**/*.rs"), "/srv/foo");
    }

    #[test]
    fn test_glob_root_pure_glob() {
        assert_eq!(glob_root("**"), "");
    }

    #[test]
    fn test_glob_root_no_glob() {
        assert_eq!(glob_root("/etc/passwd"), "/etc/passwd");
    }

    // ---- config_checks ----

    #[test]
    fn test_config_checks_default_endpoint_local() {
        // A localhost endpoint with no api_key should NOT fail (treated as
        // local). Constructed explicitly because the default endpoint is now
        // the remote OpenRouter GLM-5.2 stack, which DOES require a key (that
        // path is covered by test_config_checks_remote_no_api_key_fails).
        let cfg = crate::config::Config {
            endpoint: "http://127.0.0.1:1234/v1".to_string(),
            api_key: None,
            ..crate::config::Config::default()
        };
        let checks = config_checks(&cfg);
        // endpoint URL should pass
        assert!(checks
            .iter()
            .any(|c| c.name == "endpoint URL" && c.status == CheckStatus::Ok));
        // api_key should not be Missing for local default
        let api_check = checks.iter().find(|c| c.name == "api_key").unwrap();
        assert_ne!(api_check.status, CheckStatus::Missing);
    }

    #[test]
    fn test_config_checks_invalid_endpoint() {
        let cfg = crate::config::Config {
            endpoint: "not a url".to_string(),
            ..crate::config::Config::default()
        };
        let checks = config_checks(&cfg);
        let url_check = checks.iter().find(|c| c.name == "endpoint URL").unwrap();
        assert_eq!(url_check.status, CheckStatus::Missing);
        assert!(url_check.fix_hint.is_some());
    }

    #[test]
    fn test_config_checks_remote_no_api_key_fails() {
        let cfg = crate::config::Config {
            endpoint: "https://api.example.com/v1".to_string(),
            api_key: None,
            ..crate::config::Config::default()
        };
        let checks = config_checks(&cfg);
        let api_check = checks.iter().find(|c| c.name == "api_key").unwrap();
        assert_eq!(api_check.status, CheckStatus::Missing);
        assert!(api_check.fix_hint.is_some());
    }

    #[tokio::test]
    async fn test_run_doctor_completes() {
        // Smoke test: just ensure it doesn't panic or hang.
        let report = run_doctor(None).await;
        assert!(!report.checks.is_empty());
        // rustc must be present in a Rust build environment
        let rustc = report.checks.iter().find(|c| c.name == "rustc");
        assert!(rustc.is_some());
        assert_eq!(rustc.unwrap().status, CheckStatus::Ok);
        // MSRV check present
        assert!(report.checks.iter().any(|c| c.name == "rustc MSRV"));
    }
}
