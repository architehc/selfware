//! Selfware Doctor — system diagnostics and dependency checker.
//!
//! Checks that all required and optional tools are available on the system.
//! Run via `selfware doctor` or automatically on first launch.

use std::fmt;
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

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Category of a doctor check.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Category {
    Core,
    Languages,
    RustTools,
    PythonTools,
    NodeTools,
    GoTools,
    ComputerControl,
    ContainerTools,
    BrowserAutomation,
    Security,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Category::Core => write!(f, "Core (Required)"),
            Category::Languages => write!(f, "Languages (Optional)"),
            Category::RustTools => write!(f, "Rust Tools"),
            Category::PythonTools => write!(f, "Python Tools"),
            Category::NodeTools => write!(f, "Node Tools"),
            Category::GoTools => write!(f, "Go Tools"),
            Category::ComputerControl => write!(f, "Computer Control / Image Processing"),
            Category::ContainerTools => write!(f, "Container Tools"),
            Category::BrowserAutomation => write!(f, "Browser Automation"),
            Category::Security => write!(f, "Security"),
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

/// Check a tool by running `program --version` (or custom args).
async fn check_tool(
    name: &str,
    category: Category,
    required: bool,
    programs: &[&str],
    version_args: &[&str],
) -> DoctorCheck {
    for program in programs {
        if let Some(output) = run_cmd(program, version_args).await {
            let version = extract_version(&output);
            return DoctorCheck {
                name: name.to_string(),
                category,
                status: CheckStatus::Ok,
                version,
                message: format!("Found: {}", output.lines().next().unwrap_or(&output)),
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
        };
    }
    DoctorCheck {
        name: name.to_string(),
        category,
        status: CheckStatus::Warning,
        version: None,
        message: "Not found (optional)".to_string(),
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
        },
        None => DoctorCheck {
            name: "Accessibility (macOS)".to_string(),
            category: Category::ComputerControl,
            status: CheckStatus::Warning,
            version: None,
            message: "Accessibility permissions not granted — needed for computer control"
                .to_string(),
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
        }
    } else {
        DoctorCheck {
            name: "screen capture".to_string(),
            category: Category::ComputerControl,
            status: CheckStatus::Warning,
            version: None,
            message: "Screen capture unavailable — check display server / permissions".to_string(),
        }
    }
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
pub async fn run_doctor() -> DoctorReport {
    // We group checks by category and run each category in parallel.
    // Within each category, checks also run in parallel via join_all.
    // All futures are boxed via `ck()` so they share a single type.

    let core: Vec<CheckFut> = vec![
        ck("rustc", Category::Core, true, &["rustc"], &["--version"]),
        ck("cargo", Category::Core, true, &["cargo"], &["--version"]),
        ck("git", Category::Core, true, &["git"], &["--version"]),
    ];

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
                }
            }
        }
    }

    // Assemble all checks in category order.
    let mut checks = Vec::new();
    checks.extend(core_results);
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
    /// Print a nicely formatted, colored report to stdout.
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

            let (icon, style_msg) = match check.status {
                CheckStatus::Ok => {
                    let ver = check
                        .version
                        .as_deref()
                        .map(|v| format!(" ({})", v))
                        .unwrap_or_default();
                    (
                        "  \u{2713}".green().bold().to_string(),
                        format!("{}{}", check.name, ver).green().to_string(),
                    )
                }
                CheckStatus::Warning => (
                    "  \u{26a0}".yellow().bold().to_string(),
                    format!("{} — {}", check.name, check.message)
                        .yellow()
                        .to_string(),
                ),
                CheckStatus::Missing => (
                    "  \u{2717}".red().bold().to_string(),
                    format!("{} — {}", check.name, check.message)
                        .red()
                        .to_string(),
                ),
            };

            println!("  {} {}", icon, style_msg);
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
            }],
            health: OverallHealth::Healthy,
        };
        assert_eq!(report.health, OverallHealth::Healthy);

        // Missing required => Broken
        let report = DoctorReport {
            checks: vec![DoctorCheck {
                name: "test".into(),
                category: Category::Core,
                status: CheckStatus::Missing,
                version: None,
                message: "missing".into(),
            }],
            health: OverallHealth::Broken,
        };
        assert_eq!(report.health, OverallHealth::Broken);
    }

    #[tokio::test]
    async fn test_run_doctor_completes() {
        // Smoke test: just ensure it doesn't panic or hang.
        let report = run_doctor().await;
        assert!(!report.checks.is_empty());
        // rustc must be present in a Rust build environment
        let rustc = report.checks.iter().find(|c| c.name == "rustc");
        assert!(rustc.is_some());
        assert_eq!(rustc.unwrap().status, CheckStatus::Ok);
    }
}
