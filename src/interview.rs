//! Interview / Clarification Mode
//!
//! Asks users structured questions before starting a task so the agent does not
//! have to guess at language, framework, project type, testing strategy, etc.
//!
//! # Usage
//!
//! ```ignore
//! use selfware::interview::{run_interview, InterviewContext};
//!
//! let ctx = run_interview("Build a REST API for user management").await?;
//! let section = ctx.to_system_prompt_section();
//! // inject `section` into the system prompt before handing off to the agent
//! ```
//!
//! The interview is designed to be invoked from `src/cli.rs` (via a `--interview`
//! flag) or from `src/agent/interactive.rs` (via a `/interview` command).  This
//! module intentionally does **not** modify those files — integration is left to
//! the caller.

use anyhow::Result;
use colored::*;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// All answers collected during an interview session.
#[derive(Debug, Clone, Default)]
pub struct InterviewSession {
    pub language: Option<String>,
    pub framework: Option<String>,
    pub project_type: Option<ProjectType>,
    pub testing_preference: Option<TestingPreference>,
    pub output_dir: Option<String>,
    pub scope: Option<ProjectScope>,
    /// Free-form notes the user typed in response to open-ended questions.
    pub extra_notes: Vec<String>,
}

/// The final, validated context ready for injection into a system prompt.
#[derive(Debug, Clone)]
pub struct InterviewContext {
    pub language: Option<String>,
    pub framework: Option<String>,
    pub project_type: Option<ProjectType>,
    pub testing_preference: Option<TestingPreference>,
    pub output_dir: Option<String>,
    pub scope: Option<ProjectScope>,
    pub extra_notes: Vec<String>,
    /// The original task description, kept for reference.
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectType {
    CliTool,
    WebApi,
    FrontendUi,
    Library,
    FullStack,
    Script,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestingPreference {
    /// Full TDD — tests first.
    Tdd,
    /// Tests after implementation.
    TestsAfter,
    /// Minimal tests — critical paths only.
    Minimal,
    /// No tests.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectScope {
    /// Quick script (< 100 lines).
    Quick,
    /// Small project (100–500 lines).
    Small,
    /// Medium project (500–2000 lines).
    Medium,
    /// Large project (2000+ lines).
    Large,
}

// ---------------------------------------------------------------------------
// Display impls
// ---------------------------------------------------------------------------

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::CliTool => write!(f, "CLI tool"),
            ProjectType::WebApi => write!(f, "Web API / backend"),
            ProjectType::FrontendUi => write!(f, "Frontend / UI"),
            ProjectType::Library => write!(f, "Library / crate"),
            ProjectType::FullStack => write!(f, "Full-stack application"),
            ProjectType::Script => write!(f, "Script / automation"),
            ProjectType::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::fmt::Display for TestingPreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestingPreference::Tdd => write!(f, "Full TDD (tests first)"),
            TestingPreference::TestsAfter => write!(f, "Tests after implementation"),
            TestingPreference::Minimal => write!(f, "Minimal tests (critical paths)"),
            TestingPreference::None => write!(f, "No tests"),
        }
    }
}

impl std::fmt::Display for ProjectScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectScope::Quick => write!(f, "Quick script (< 100 lines)"),
            ProjectScope::Small => write!(f, "Small project (100-500 lines)"),
            ProjectScope::Medium => write!(f, "Medium project (500-2000 lines)"),
            ProjectScope::Large => write!(f, "Large project (2000+ lines)"),
        }
    }
}

// ---------------------------------------------------------------------------
// InterviewContext — system-prompt serialization
// ---------------------------------------------------------------------------

impl InterviewContext {
    /// Format the collected context as a section suitable for injection into the
    /// agent's system prompt.
    pub fn to_system_prompt_section(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push("## Interview Context (user-specified preferences)".to_string());
        lines.push(String::new());

        if let Some(ref lang) = self.language {
            lines.push(format!("- **Language**: {lang}"));
        }
        if let Some(ref fw) = self.framework {
            lines.push(format!("- **Framework / stack**: {fw}"));
        }
        if let Some(ref pt) = self.project_type {
            lines.push(format!("- **Project type**: {pt}"));
        }
        if let Some(ref tp) = self.testing_preference {
            lines.push(format!("- **Testing approach**: {tp}"));
        }
        if let Some(ref od) = self.output_dir {
            lines.push(format!("- **Output location**: {od}"));
        }
        if let Some(ref sc) = self.scope {
            lines.push(format!("- **Expected scope**: {sc}"));
        }
        for note in &self.extra_notes {
            lines.push(format!("- **Note**: {note}"));
        }

        if lines.len() <= 2 {
            // Only the header — user skipped everything.
            lines.push("_(no preferences specified — use your best judgement)_".to_string());
        }

        lines.join("\n")
    }

    /// Returns `true` when the user provided no answers at all (e.g. pressed
    /// Esc to skip the entire interview).
    pub fn is_empty(&self) -> bool {
        self.language.is_none()
            && self.framework.is_none()
            && self.project_type.is_none()
            && self.testing_preference.is_none()
            && self.output_dir.is_none()
            && self.scope.is_none()
            && self.extra_notes.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Task-description analysis (heuristic keyword matching)
// ---------------------------------------------------------------------------

/// Hints extracted from the task description that influence which questions are
/// asked and what defaults are offered.
#[derive(Debug, Default)]
struct TaskHints {
    suggests_web_api: bool,
    suggests_cli: bool,
    suggests_frontend: bool,
    suggests_library: bool,
    suggests_script: bool,
    /// Languages explicitly mentioned in the task.
    mentioned_languages: Vec<String>,
}

fn analyze_task(task: &str) -> TaskHints {
    let lower = task.to_lowercase();
    let mut hints = TaskHints::default();

    // Web API / backend signals
    let web_signals = [
        "rest api",
        "restful",
        "graphql",
        "web server",
        "http server",
        "endpoint",
        "backend",
        "microservice",
        "web service",
        "api server",
    ];
    hints.suggests_web_api = web_signals.iter().any(|s| lower.contains(s));

    // CLI signals
    let cli_signals = [
        "cli",
        "command line",
        "command-line",
        "terminal tool",
        "shell tool",
    ];
    hints.suggests_cli = cli_signals.iter().any(|s| lower.contains(s));

    // Frontend signals
    let frontend_signals = [
        "frontend",
        "front-end",
        "ui",
        "user interface",
        "dashboard",
        "web app",
        "webapp",
        "spa",
        "single page",
    ];
    hints.suggests_frontend = frontend_signals.iter().any(|s| lower.contains(s));

    // Library signals
    let lib_signals = ["library", "crate", "package", "module", "sdk"];
    hints.suggests_library = lib_signals.iter().any(|s| lower.contains(s));

    // Script signals
    let script_signals = ["script", "automation", "automate", "quick", "one-off"];
    hints.suggests_script = script_signals.iter().any(|s| lower.contains(s));

    // Language mentions
    let lang_map = [
        ("rust", "Rust"),
        ("python", "Python"),
        ("typescript", "TypeScript"),
        ("javascript", "JavaScript"),
        ("node", "Node.js"),
        ("golang", "Go"),
    ];
    for (keyword, canonical) in lang_map {
        if lower.contains(keyword) {
            hints.mentioned_languages.push(canonical.to_string());
        }
    }

    // "Go" requires word-boundary detection because it is too short for a
    // naive substring check.  We look for the pattern surrounded by
    // non-alphabetic characters (or at string boundaries).
    if !hints.mentioned_languages.contains(&"Go".to_string()) {
        use std::sync::LazyLock;
        static GO_WORD_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"(?i)\bgo\b").expect("invalid go regex"));
        if GO_WORD_RE.is_match(&lower) {
            hints.mentioned_languages.push("Go".to_string());
        }
    }

    hints
}

// ---------------------------------------------------------------------------
// Project-environment detection
// ---------------------------------------------------------------------------

/// Detect the dominant language of an existing project by looking for marker
/// files in `dir`.
fn detect_existing_project(dir: &Path) -> Option<String> {
    if dir.join("Cargo.toml").exists() {
        return Some("Rust".to_string());
    }
    if dir.join("package.json").exists() {
        // Distinguish TS vs JS
        if dir.join("tsconfig.json").exists() {
            return Some("TypeScript".to_string());
        }
        return Some("JavaScript/Node.js".to_string());
    }
    if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("requirements.txt").exists()
    {
        return Some("Python".to_string());
    }
    if dir.join("go.mod").exists() {
        return Some("Go".to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// Terminal I/O helpers
// ---------------------------------------------------------------------------

/// Describes a single option in a multiple-choice question.
struct QuestionOption {
    key: char,
    label: String,
}

/// Present a multiple-choice question and read the user's answer.
///
/// Returns `Ok(None)` if the user presses Esc (skip question) and
/// `Err(Skipped)` if the user pressed Esc at the very first question (skip
/// entire interview — propagated by the caller).
///
/// The last option is always the write-your-own option.
fn ask_multiple_choice(
    question: &str,
    options: &[QuestionOption],
    default: Option<char>,
) -> Result<Option<String>> {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // Print question
    writeln!(out)?;
    writeln!(out, "  {}", question.bold())?;
    writeln!(out)?;

    for opt in options {
        let marker = if Some(opt.key) == default {
            format!("  {})", opt.key).cyan().bold().to_string()
        } else {
            format!("  {})", opt.key).cyan().to_string()
        };
        let suffix = if Some(opt.key) == default {
            " (default)".dimmed().to_string()
        } else {
            String::new()
        };
        writeln!(out, "{} {}{}", marker, opt.label, suffix)?;
    }

    writeln!(out)?;
    let default_hint = match default {
        Some(c) => format!(" [{}]", c),
        None => String::new(),
    };
    write!(
        out,
        "  {} {}",
        ">>".green().bold(),
        format!("Your choice{}: ", default_hint).dimmed()
    )?;
    out.flush()?;

    // Read one line of input from stdin (works in both TTY and piped modes).
    let response = read_line_or_esc()?;
    match response {
        LineInput::Esc => Ok(None),
        LineInput::Line(line) => {
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                // Use default
                if let Some(d) = default {
                    if let Some(opt) = options.iter().find(|o| o.key == d) {
                        return Ok(Some(opt.label.clone()));
                    }
                }
                return Ok(None);
            }
            // Single-char match?
            if trimmed.len() == 1 {
                let ch = trimmed
                    .chars()
                    .next()
                    .expect("len == 1 guarantees a char")
                    .to_ascii_lowercase();
                if let Some(opt) = options.iter().find(|o| o.key == ch) {
                    // If this is the last option (write-your-own), prompt for text.
                    if Some(ch) == options.last().map(|o| o.key) {
                        return ask_freeform("  Type your answer: ");
                    }
                    return Ok(Some(opt.label.clone()));
                }
            }
            // Treat the full string as a free-form answer.
            Ok(Some(trimmed))
        }
    }
}

/// Ask for free-form text input (single line).
fn ask_freeform(prompt: &str) -> Result<Option<String>> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write!(out, "{}", prompt.dimmed())?;
    out.flush()?;

    let response = read_line_or_esc()?;
    match response {
        LineInput::Esc => Ok(None),
        LineInput::Line(line) => {
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
    }
}

enum LineInput {
    Esc,
    Line(String),
}

/// Read a line from stdin.  If the terminal is a TTY, briefly enter raw mode
/// to detect an Esc keypress; otherwise fall back to a plain `read_line`.
fn read_line_or_esc() -> Result<LineInput> {
    let stdin = io::stdin();
    if !stdin.is_terminal() {
        // Non-interactive — just read a line.
        let mut buf = String::new();
        stdin.lock().read_line(&mut buf)?;
        return Ok(LineInput::Line(buf));
    }

    // In a terminal: enter raw mode so we can detect Esc immediately, but
    // otherwise accumulate characters into a line buffer until Enter.
    terminal::enable_raw_mode()?;
    let result = read_line_raw();
    let _ = terminal::disable_raw_mode();
    result
}

/// Raw-mode line reader that recognises Esc as a cancellation signal.
fn read_line_raw() -> Result<LineInput> {
    let mut buf = String::new();
    let mut stdout = io::stdout();

    loop {
        if event::poll(std::time::Duration::from_millis(5000))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => {
                        // Print a newline so subsequent output is clean.
                        write!(stdout, "\r\n")?;
                        stdout.flush()?;
                        return Ok(LineInput::Esc);
                    }
                    KeyCode::Enter => {
                        write!(stdout, "\r\n")?;
                        stdout.flush()?;
                        return Ok(LineInput::Line(buf));
                    }
                    KeyCode::Char(c) => {
                        buf.push(c);
                        write!(stdout, "{c}")?;
                        stdout.flush()?;
                    }
                    KeyCode::Backspace => {
                        if buf.pop().is_some() {
                            // Move cursor back, overwrite, move back again.
                            write!(stdout, "\x08 \x08")?;
                            stdout.flush()?;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Question builders
// ---------------------------------------------------------------------------

fn language_options() -> Vec<QuestionOption> {
    vec![
        QuestionOption {
            key: 'a',
            label: "Rust".to_string(),
        },
        QuestionOption {
            key: 'b',
            label: "Python".to_string(),
        },
        QuestionOption {
            key: 'c',
            label: "TypeScript / Node.js".to_string(),
        },
        QuestionOption {
            key: 'd',
            label: "Go".to_string(),
        },
        QuestionOption {
            key: 'e',
            label: "Let me specify...".to_string(),
        },
    ]
}

fn framework_options_for(language: &str) -> Vec<QuestionOption> {
    let lower = language.to_lowercase();
    let opts: Vec<(&str, &str)> = if lower.contains("rust") {
        vec![
            ("a", "axum"),
            ("b", "actix-web"),
            ("c", "tokio (async runtime only)"),
            ("d", "warp"),
            ("e", "clap (CLI)"),
            ("f", "Let me specify..."),
        ]
    } else if lower.contains("python") {
        vec![
            ("a", "FastAPI"),
            ("b", "Flask"),
            ("c", "Django"),
            ("d", "click / typer (CLI)"),
            ("e", "Let me specify..."),
        ]
    } else if lower.contains("typescript") || lower.contains("node") || lower.contains("javascript")
    {
        vec![
            ("a", "Next.js"),
            ("b", "Express"),
            ("c", "Deno"),
            ("d", "Bun"),
            ("e", "Let me specify..."),
        ]
    } else if lower.contains("go") {
        vec![
            ("a", "gin"),
            ("b", "echo"),
            ("c", "standard library (net/http)"),
            ("d", "Let me specify..."),
        ]
    } else {
        vec![("a", "Let me specify...")]
    };

    opts.into_iter()
        .map(|(k, l)| {
            let key = k.chars().next().expect("option key is non-empty");
            QuestionOption {
                key,
                label: l.to_string(),
            }
        })
        .collect()
}

fn project_type_options() -> Vec<QuestionOption> {
    vec![
        QuestionOption {
            key: 'a',
            label: "CLI tool".to_string(),
        },
        QuestionOption {
            key: 'b',
            label: "Web API / backend".to_string(),
        },
        QuestionOption {
            key: 'c',
            label: "Frontend / UI".to_string(),
        },
        QuestionOption {
            key: 'd',
            label: "Library / crate".to_string(),
        },
        QuestionOption {
            key: 'e',
            label: "Full-stack application".to_string(),
        },
        QuestionOption {
            key: 'f',
            label: "Script / automation".to_string(),
        },
        QuestionOption {
            key: 'g',
            label: "Let me specify...".to_string(),
        },
    ]
}

fn testing_options() -> Vec<QuestionOption> {
    vec![
        QuestionOption {
            key: 'a',
            label: "Full TDD (tests first)".to_string(),
        },
        QuestionOption {
            key: 'b',
            label: "Tests after implementation".to_string(),
        },
        QuestionOption {
            key: 'c',
            label: "Minimal tests (just critical paths)".to_string(),
        },
        QuestionOption {
            key: 'd',
            label: "No tests".to_string(),
        },
    ]
}

fn output_location_options() -> Vec<QuestionOption> {
    vec![
        QuestionOption {
            key: 'a',
            label: "Current directory".to_string(),
        },
        QuestionOption {
            key: 'b',
            label: "New subdirectory (specify name)".to_string(),
        },
        QuestionOption {
            key: 'c',
            label: "Temporary directory".to_string(),
        },
    ]
}

fn scope_options() -> Vec<QuestionOption> {
    vec![
        QuestionOption {
            key: 'a',
            label: "Quick script (< 100 lines)".to_string(),
        },
        QuestionOption {
            key: 'b',
            label: "Small project (100-500 lines)".to_string(),
        },
        QuestionOption {
            key: 'c',
            label: "Medium project (500-2000 lines)".to_string(),
        },
        QuestionOption {
            key: 'd',
            label: "Large project (2000+ lines)".to_string(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Mapping user selections to typed values
// ---------------------------------------------------------------------------

fn parse_project_type(answer: &str) -> ProjectType {
    let lower = answer.to_lowercase();
    if lower.contains("cli") {
        ProjectType::CliTool
    } else if lower.contains("api")
        || lower.contains("backend")
        || lower.contains("web") && !lower.contains("app")
    {
        ProjectType::WebApi
    } else if lower.contains("frontend") || lower.contains("ui") {
        ProjectType::FrontendUi
    } else if lower.contains("library") || lower.contains("crate") {
        ProjectType::Library
    } else if lower.contains("full-stack")
        || lower.contains("full stack")
        || lower.contains("fullstack")
    {
        ProjectType::FullStack
    } else if lower.contains("script") || lower.contains("automation") {
        ProjectType::Script
    } else {
        ProjectType::Other(answer.to_string())
    }
}

fn parse_testing_preference(answer: &str) -> TestingPreference {
    let lower = answer.to_lowercase();
    if lower.contains("tdd") || lower.contains("tests first") {
        TestingPreference::Tdd
    } else if lower.contains("after") {
        TestingPreference::TestsAfter
    } else if lower.contains("minimal") || lower.contains("critical") {
        TestingPreference::Minimal
    } else {
        TestingPreference::None
    }
}

fn parse_scope(answer: &str) -> ProjectScope {
    let lower = answer.to_lowercase();
    if lower.contains("quick") || lower.contains("< 100") {
        ProjectScope::Quick
    } else if lower.contains("small") || lower.contains("100-500") || lower.contains("100–500") {
        ProjectScope::Small
    } else if lower.contains("medium") || lower.contains("500-2000") || lower.contains("500–2000")
    {
        ProjectScope::Medium
    } else {
        ProjectScope::Large
    }
}

// ---------------------------------------------------------------------------
// Core interview flow
// ---------------------------------------------------------------------------

/// Run a structured interview, asking the user questions relevant to `task`.
///
/// The working directory (`cwd`) is used for project-environment detection
/// (e.g. presence of `Cargo.toml`).
///
/// Returns an [`InterviewContext`] that can be injected into the system prompt
/// via [`InterviewContext::to_system_prompt_section`].
///
/// If the user presses Esc at any point, remaining questions are skipped and
/// the answers collected so far are returned.
pub fn run_interview(task: &str, cwd: &Path) -> Result<InterviewContext> {
    let hints = analyze_task(task);
    let existing_lang = detect_existing_project(cwd);
    let mut session = InterviewSession::default();

    // Banner
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out)?;
    writeln!(out, "  {}", "Interview Mode".bold().underline())?;
    writeln!(
        out,
        "  {}",
        "Answer a few questions so I can tailor my approach.".dimmed()
    )?;
    writeln!(
        out,
        "  {}",
        "Press Esc at any time to skip remaining questions.".dimmed()
    )?;
    drop(out);

    // ---- Language ----
    let should_ask_language = existing_lang.is_none() && hints.mentioned_languages.is_empty();
    if should_ask_language {
        let default = infer_language_default(&hints);
        match ask_multiple_choice(
            "What language should this be written in?",
            &language_options(),
            default,
        )? {
            Some(answer) => session.language = Some(answer),
            None => {
                // Esc — skip remaining questions
                return Ok(session_to_context(session, task));
            }
        }
    } else if let Some(ref lang) = existing_lang {
        // Auto-detected — inform the user.
        let stdout = io::stdout();
        let mut out = stdout.lock();
        writeln!(out)?;
        writeln!(
            out,
            "  {} Detected existing {} project — skipping language question.",
            ">>".green().bold(),
            lang.cyan()
        )?;
        drop(out);
        session.language = Some(lang.clone());
    } else if hints.mentioned_languages.len() == 1 {
        session.language = Some(hints.mentioned_languages[0].clone());
    }

    // ---- Framework ----
    let is_simple = hints.suggests_script
        || session
            .language
            .as_deref()
            .map(|l| l.to_lowercase().contains("script"))
            .unwrap_or(false);

    if !is_simple {
        if let Some(ref lang) = session.language {
            let fw_opts = framework_options_for(lang);
            if fw_opts.len() > 1 {
                match ask_multiple_choice(
                    &format!("Which {} framework / stack?", lang),
                    &fw_opts,
                    None,
                )? {
                    Some(answer) => session.framework = Some(answer),
                    None => return Ok(session_to_context(session, task)),
                }
            }
        }
    }

    // ---- Project type ----
    let default_project_type = infer_project_type_default(&hints);
    match ask_multiple_choice(
        "What type of project is this?",
        &project_type_options(),
        default_project_type,
    )? {
        Some(answer) => session.project_type = Some(parse_project_type(&answer)),
        None => return Ok(session_to_context(session, task)),
    }

    // ---- Testing preference ----
    match ask_multiple_choice("Testing approach?", &testing_options(), Some('b'))? {
        Some(answer) => session.testing_preference = Some(parse_testing_preference(&answer)),
        None => return Ok(session_to_context(session, task)),
    }

    // ---- Output location ----
    // Skip if inside an existing project (output to current dir is obvious).
    if existing_lang.is_none() {
        match ask_multiple_choice(
            "Where should the code go?",
            &output_location_options(),
            Some('a'),
        )? {
            Some(answer) => {
                let lower = answer.to_lowercase();
                if lower.contains("subdirectory") || lower.contains("new sub") {
                    if let Some(name) = ask_freeform("  Directory name: ")? {
                        session.output_dir = Some(name);
                    }
                } else if lower.contains("temporary") {
                    session.output_dir = Some("<temp>".to_string());
                } else {
                    session.output_dir = Some(".".to_string());
                }
            }
            None => return Ok(session_to_context(session, task)),
        }
    }

    // ---- Scope ----
    let default_scope = infer_scope_default(&hints);
    match ask_multiple_choice("Expected scope?", &scope_options(), default_scope)? {
        Some(answer) => session.scope = Some(parse_scope(&answer)),
        None => return Ok(session_to_context(session, task)),
    }

    Ok(session_to_context(session, task))
}

// ---------------------------------------------------------------------------
// Default inference helpers
// ---------------------------------------------------------------------------

fn infer_language_default(hints: &TaskHints) -> Option<char> {
    if !hints.mentioned_languages.is_empty() {
        return None; // already handled
    }
    // No strong signal — don't presume.
    None
}

fn infer_project_type_default(hints: &TaskHints) -> Option<char> {
    if hints.suggests_web_api {
        Some('b')
    } else if hints.suggests_cli {
        Some('a')
    } else if hints.suggests_frontend {
        Some('c')
    } else if hints.suggests_library {
        Some('d')
    } else if hints.suggests_script {
        Some('f')
    } else {
        None
    }
}

fn infer_scope_default(hints: &TaskHints) -> Option<char> {
    if hints.suggests_script {
        Some('a')
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Session → Context conversion
// ---------------------------------------------------------------------------

fn session_to_context(session: InterviewSession, task: &str) -> InterviewContext {
    InterviewContext {
        language: session.language,
        framework: session.framework,
        project_type: session.project_type,
        testing_preference: session.testing_preference,
        output_dir: session.output_dir,
        scope: session.scope,
        extra_notes: session.extra_notes,
        task: task.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_task_web_api() {
        let hints = analyze_task("Build a REST API for user management");
        assert!(hints.suggests_web_api);
        assert!(!hints.suggests_cli);
    }

    #[test]
    fn test_analyze_task_cli() {
        let hints = analyze_task("Create a CLI tool that converts images");
        assert!(hints.suggests_cli);
        assert!(!hints.suggests_web_api);
    }

    #[test]
    fn test_analyze_task_language_mention() {
        let hints = analyze_task("Write a Python script to parse CSV files");
        assert_eq!(hints.mentioned_languages, vec!["Python"]);
        assert!(hints.suggests_script);
    }

    #[test]
    fn test_analyze_task_multiple_languages() {
        let hints = analyze_task("Port this Rust library to Go");
        assert!(hints.mentioned_languages.contains(&"Rust".to_string()));
        assert!(hints.mentioned_languages.contains(&"Go".to_string()));
    }

    #[test]
    fn test_detect_existing_project_rust() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(
            detect_existing_project(dir.path()),
            Some("Rust".to_string())
        );
    }

    #[test]
    fn test_detect_existing_project_typescript() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        assert_eq!(
            detect_existing_project(dir.path()),
            Some("TypeScript".to_string())
        );
    }

    #[test]
    fn test_detect_existing_project_node() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(
            detect_existing_project(dir.path()),
            Some("JavaScript/Node.js".to_string())
        );
    }

    #[test]
    fn test_detect_existing_project_python() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[project]").unwrap();
        assert_eq!(
            detect_existing_project(dir.path()),
            Some("Python".to_string())
        );
    }

    #[test]
    fn test_detect_existing_project_go() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module example").unwrap();
        assert_eq!(detect_existing_project(dir.path()), Some("Go".to_string()));
    }

    #[test]
    fn test_detect_existing_project_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_existing_project(dir.path()), None);
    }

    #[test]
    fn test_parse_project_type() {
        assert_eq!(parse_project_type("CLI tool"), ProjectType::CliTool);
        assert_eq!(parse_project_type("Web API / backend"), ProjectType::WebApi);
        assert_eq!(
            parse_project_type("Script / automation"),
            ProjectType::Script
        );
        assert_eq!(parse_project_type("Library / crate"), ProjectType::Library);
    }

    #[test]
    fn test_parse_testing_preference() {
        assert_eq!(
            parse_testing_preference("Full TDD (tests first)"),
            TestingPreference::Tdd
        );
        assert_eq!(
            parse_testing_preference("Tests after implementation"),
            TestingPreference::TestsAfter
        );
        assert_eq!(
            parse_testing_preference("Minimal tests (just critical paths)"),
            TestingPreference::Minimal
        );
        assert_eq!(
            parse_testing_preference("No tests"),
            TestingPreference::None
        );
    }

    #[test]
    fn test_parse_scope() {
        assert_eq!(
            parse_scope("Quick script (< 100 lines)"),
            ProjectScope::Quick
        );
        assert_eq!(
            parse_scope("Small project (100-500 lines)"),
            ProjectScope::Small
        );
        assert_eq!(
            parse_scope("Medium project (500-2000 lines)"),
            ProjectScope::Medium
        );
        assert_eq!(
            parse_scope("Large project (2000+ lines)"),
            ProjectScope::Large
        );
    }

    #[test]
    fn test_interview_context_to_system_prompt_section() {
        let ctx = InterviewContext {
            language: Some("Rust".to_string()),
            framework: Some("axum".to_string()),
            project_type: Some(ProjectType::WebApi),
            testing_preference: Some(TestingPreference::Tdd),
            output_dir: Some(".".to_string()),
            scope: Some(ProjectScope::Medium),
            extra_notes: vec![],
            task: "Build a REST API".to_string(),
        };
        let section = ctx.to_system_prompt_section();
        assert!(section.contains("Rust"));
        assert!(section.contains("axum"));
        assert!(section.contains("Web API"));
        assert!(section.contains("TDD"));
        assert!(section.contains("Medium project"));
    }

    #[test]
    fn test_interview_context_empty() {
        let ctx = InterviewContext {
            language: None,
            framework: None,
            project_type: None,
            testing_preference: None,
            output_dir: None,
            scope: None,
            extra_notes: vec![],
            task: "do something".to_string(),
        };
        assert!(ctx.is_empty());
        let section = ctx.to_system_prompt_section();
        assert!(section.contains("no preferences specified"));
    }

    #[test]
    fn test_interview_context_not_empty() {
        let ctx = InterviewContext {
            language: Some("Python".to_string()),
            framework: None,
            project_type: None,
            testing_preference: None,
            output_dir: None,
            scope: None,
            extra_notes: vec![],
            task: "do something".to_string(),
        };
        assert!(!ctx.is_empty());
    }

    #[test]
    fn test_framework_options_for_rust() {
        let opts = framework_options_for("Rust");
        assert!(opts.len() >= 5);
        assert!(opts.iter().any(|o| o.label.contains("axum")));
    }

    #[test]
    fn test_framework_options_for_python() {
        let opts = framework_options_for("Python");
        assert!(opts.iter().any(|o| o.label.contains("FastAPI")));
    }

    #[test]
    fn test_framework_options_for_typescript() {
        let opts = framework_options_for("TypeScript / Node.js");
        assert!(opts.iter().any(|o| o.label.contains("Next.js")));
    }

    #[test]
    fn test_framework_options_for_go() {
        let opts = framework_options_for("Go");
        assert!(opts.iter().any(|o| o.label.contains("gin")));
    }

    #[test]
    fn test_framework_options_for_unknown() {
        let opts = framework_options_for("Haskell");
        assert_eq!(opts.len(), 1);
        assert!(opts[0].label.contains("specify"));
    }

    #[test]
    fn test_infer_project_type_default_web_api() {
        let hints = analyze_task("Build a REST API");
        assert_eq!(infer_project_type_default(&hints), Some('b'));
    }

    #[test]
    fn test_infer_project_type_default_cli() {
        let hints = analyze_task("Create a CLI tool");
        assert_eq!(infer_project_type_default(&hints), Some('a'));
    }

    #[test]
    fn test_infer_scope_default_script() {
        let hints = analyze_task("Write a quick script");
        assert_eq!(infer_scope_default(&hints), Some('a'));
    }

    #[test]
    fn test_display_impls() {
        assert_eq!(format!("{}", ProjectType::CliTool), "CLI tool");
        assert_eq!(
            format!("{}", TestingPreference::Tdd),
            "Full TDD (tests first)"
        );
        assert_eq!(
            format!("{}", ProjectScope::Medium),
            "Medium project (500-2000 lines)"
        );
    }

    #[test]
    fn test_session_to_context() {
        let session = InterviewSession {
            language: Some("Rust".to_string()),
            framework: None,
            project_type: Some(ProjectType::CliTool),
            testing_preference: None,
            output_dir: None,
            scope: None,
            extra_notes: vec!["keep it simple".to_string()],
        };
        let ctx = session_to_context(session, "build something");
        assert_eq!(ctx.task, "build something");
        assert_eq!(ctx.language.as_deref(), Some("Rust"));
        assert_eq!(ctx.project_type, Some(ProjectType::CliTool));
        assert_eq!(ctx.extra_notes, vec!["keep it simple"]);
    }
}
