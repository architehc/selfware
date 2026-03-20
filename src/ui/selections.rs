//! Selection / Recommendation Menus
//!
//! A guided wizard system where the agent presents scored options and the user
//! picks from them.  Think of it as a recommendation engine for project setup:
//! the agent analyses the task description, scores each option for relevance,
//! marks the top-scored one as "recommended", and lets the user confirm or
//! override.
//!
//! # Usage
//!
//! ```ignore
//! use selfware::ui::selections::*;
//!
//! let menu = recommend_project_template("Build a REST API for user management");
//! let result = present_selection(&menu)?;
//! ```
//!
//! The module also exposes [`guided_project_setup`], which chains multiple
//! selection menus together (template, architecture, database, testing,
//! deployment) and returns a [`ProjectPlan`] suitable for injection into a
//! system prompt.

use anyhow::Result;
use colored::*;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal;
use std::io::{self, IsTerminal, Write as IoWrite};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single option inside a selection menu.
#[derive(Debug, Clone)]
pub struct SelectionOption {
    /// Single-character shortcut key ('a' .. 'z').
    pub key: char,
    /// Short human-readable label.
    pub label: String,
    /// Longer description shown next to the label.
    pub description: String,
    /// Tags such as "recommended", "advanced", "popular".
    pub tags: Vec<String>,
    /// Arbitrary extra data attached to this option.
    pub metadata: Option<serde_json::Value>,
}

/// A complete selection menu with title, options, and behaviour flags.
#[derive(Debug, Clone)]
pub struct SelectionMenu {
    /// Bold title rendered at the top of the box.
    pub title: String,
    /// Optional subtitle / context line below the title.
    pub description: Option<String>,
    /// The available choices.
    pub options: Vec<SelectionOption>,
    /// When `true`, the user may type free-form text instead of picking a
    /// letter.
    pub allow_custom: bool,
    /// When `true`, the user may pick more than one option (comma-separated
    /// letters).
    pub allow_multiple: bool,
}

/// The result of presenting a selection menu to the user.
#[derive(Debug, Clone)]
pub struct SelectionResult {
    /// The option(s) the user chose.
    pub selected: Vec<SelectionOption>,
    /// Free-form input when `allow_custom` is true and the user typed text.
    pub custom_input: Option<String>,
}

/// Aggregated choices from the guided project-setup wizard.
#[derive(Debug, Clone)]
pub struct ProjectPlan {
    pub template: String,
    pub language: String,
    pub framework: String,
    pub architecture: String,
    pub database: Option<String>,
    pub testing_strategy: String,
    pub deployment: Option<String>,
}

impl ProjectPlan {
    /// Format the plan as a system-prompt section for injection into the
    /// agent's context.
    pub fn to_system_prompt_section(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push("## Project Plan (user-selected preferences)".to_string());
        lines.push(String::new());
        lines.push(format!("- **Template**: {}", self.template));
        lines.push(format!("- **Language**: {}", self.language));
        lines.push(format!("- **Framework**: {}", self.framework));
        lines.push(format!("- **Architecture**: {}", self.architecture));
        if let Some(ref db) = self.database {
            lines.push(format!("- **Database**: {db}"));
        }
        lines.push(format!("- **Testing strategy**: {}", self.testing_strategy));
        if let Some(ref deploy) = self.deployment {
            lines.push(format!("- **Deployment**: {deploy}"));
        }
        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Task analysis & scoring
// ---------------------------------------------------------------------------

/// Internal scores for each recommendation category.
#[derive(Debug, Default, Clone)]
struct TaskScores {
    // Template scores (higher = better fit)
    rust_axum: f64,
    python_fastapi: f64,
    typescript_express: f64,
    go_gin: f64,

    // Architecture scores
    layered: f64,
    hexagonal: f64,
    clean: f64,
    microservices: f64,
    monolith: f64,

    // Database scores
    postgresql: f64,
    sqlite: f64,
    mongodb: f64,
    redis: f64,
    no_db: f64,

    // Testing scores
    comprehensive: f64,
    standard: f64,
    minimal: f64,
    tdd: f64,

    // Deployment scores
    docker: f64,
    serverless: f64,
    binary: f64,
    registry: f64,
}

/// Analyse a task description and return relevance scores.
fn score_task(task: &str) -> TaskScores {
    let lower = task.to_lowercase();
    let mut s = TaskScores::default();

    // --- Template / language signals ------------------------------------------

    // Rust signals
    let rust_signals = ["rust", "cargo", "crate", "tokio", "axum", "actix"];
    for kw in &rust_signals {
        if lower.contains(kw) {
            s.rust_axum += 3.0;
        }
    }

    // Python signals
    let python_signals = [
        "python", "pip", "django", "flask", "fastapi", "pandas", "numpy",
    ];
    for kw in &python_signals {
        if lower.contains(kw) {
            s.python_fastapi += 3.0;
        }
    }

    // TypeScript / JS signals
    let ts_signals = [
        "typescript",
        "javascript",
        "node",
        "express",
        "next.js",
        "react",
        "npm",
        "yarn",
        "bun",
        "deno",
    ];
    for kw in &ts_signals {
        if lower.contains(kw) {
            s.typescript_express += 3.0;
        }
    }

    // Go signals
    let go_signals = ["golang", "gin", "echo"];
    for kw in &go_signals {
        if lower.contains(kw) {
            s.go_gin += 3.0;
        }
    }
    // Word-boundary check for "go"
    {
        use std::sync::LazyLock;
        static GO_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"(?i)\bgo\b").expect("invalid go regex"));
        if GO_RE.is_match(&lower) {
            s.go_gin += 2.0;
        }
    }

    // API / web signals boost all templates, but especially Rust + Go for
    // performance and Python/TS for quick prototyping.
    let api_signals = [
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
    let is_api = api_signals.iter().any(|kw| lower.contains(kw));
    if is_api {
        s.rust_axum += 2.0;
        s.python_fastapi += 2.0;
        s.typescript_express += 1.5;
        s.go_gin += 2.0;
    }

    // Performance-critical signals
    let perf_signals = [
        "performance",
        "high-performance",
        "fast",
        "low latency",
        "concurrent",
        "concurrency",
    ];
    if perf_signals.iter().any(|kw| lower.contains(kw)) {
        s.rust_axum += 2.0;
        s.go_gin += 2.0;
    }

    // Quick prototyping signals
    let proto_signals = [
        "prototype",
        "quick",
        "mvp",
        "proof of concept",
        "poc",
        "hack",
        "script",
    ];
    if proto_signals.iter().any(|kw| lower.contains(kw)) {
        s.python_fastapi += 2.0;
        s.typescript_express += 1.5;
    }

    // CLI tool signals
    let cli_signals = [
        "cli",
        "command line",
        "command-line",
        "terminal tool",
        "shell tool",
    ];
    if cli_signals.iter().any(|kw| lower.contains(kw)) {
        s.rust_axum += 1.5; // Rust is great for CLIs
        s.go_gin += 1.0;
    }

    // If no signal at all, give a slight baseline to Rust (project default).
    if s.rust_axum == 0.0
        && s.python_fastapi == 0.0
        && s.typescript_express == 0.0
        && s.go_gin == 0.0
    {
        s.rust_axum = 1.0;
    }

    // --- Architecture signals -------------------------------------------------

    // Default: layered is generally a good starting point
    s.layered = 2.0;
    s.monolith = 1.0;

    if is_api {
        s.layered += 2.0;
        s.hexagonal += 1.0;
        s.clean += 1.0;
    }

    let complex_signals = [
        "enterprise",
        "large scale",
        "large-scale",
        "complex",
        "domain driven",
        "ddd",
    ];
    if complex_signals.iter().any(|kw| lower.contains(kw)) {
        s.hexagonal += 2.0;
        s.clean += 2.0;
        s.microservices += 1.5;
    }

    if lower.contains("microservice") {
        s.microservices += 5.0;
    }

    if lower.contains("simple")
        || lower.contains("small")
        || lower.contains("quick")
        || lower.contains("script")
    {
        s.monolith += 2.0;
        s.layered += 1.0;
    }

    // --- Database signals -----------------------------------------------------

    let db_signals = [
        "database",
        "storage",
        "persist",
        "store",
        "data",
        "user management",
        "users",
        "accounts",
        "crud",
    ];
    let needs_db = db_signals.iter().any(|kw| lower.contains(kw));
    if needs_db {
        s.postgresql = 3.0;
        s.sqlite = 1.5;
        s.mongodb = 1.0;
    }

    if lower.contains("embedded") || lower.contains("lightweight") || lower.contains("sqlite") {
        s.sqlite += 3.0;
    }
    if lower.contains("document") || lower.contains("nosql") || lower.contains("mongo") {
        s.mongodb += 3.0;
    }
    if lower.contains("cache") || lower.contains("session") || lower.contains("redis") {
        s.redis += 3.0;
    }
    // Use word-boundary check for "sql" to avoid matching "nosql".
    {
        use std::sync::LazyLock;
        static SQL_RE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"(?i)\bsql\b").expect("invalid sql regex"));
        if lower.contains("relational") || SQL_RE.is_match(&lower) || lower.contains("postgres") {
            s.postgresql += 3.0;
        }
    }

    if !needs_db
        && !lower.contains("sqlite")
        && !lower.contains("mongo")
        && !lower.contains("redis")
        && !lower.contains("postgres")
    {
        s.no_db = 2.0;
    }

    // --- Testing signals ------------------------------------------------------

    s.standard = 2.0; // sensible default

    if lower.contains("tdd") || lower.contains("test-driven") || lower.contains("test driven") {
        s.tdd += 4.0;
    }
    if lower.contains("comprehensive") || lower.contains("thorough") || lower.contains("e2e") {
        s.comprehensive += 3.0;
    }
    if lower.contains("minimal") || lower.contains("quick") || lower.contains("script") {
        s.minimal += 2.0;
    }
    if is_api {
        s.comprehensive += 1.5;
        s.standard += 1.0;
    }

    // --- Deployment signals ---------------------------------------------------

    s.docker = 2.0; // sensible default for most projects

    if lower.contains("docker") || lower.contains("container") || lower.contains("kubernetes") {
        s.docker += 3.0;
    }
    if lower.contains("serverless") || lower.contains("lambda") || lower.contains("cloud function")
    {
        s.serverless += 3.0;
    }
    if lower.contains("binary") || lower.contains("release") || lower.contains("cli") {
        s.binary += 3.0;
    }
    if lower.contains("publish")
        || lower.contains("crates.io")
        || lower.contains("npm")
        || lower.contains("pypi")
    {
        s.registry += 3.0;
    }

    s
}

/// Return the key of the recommended (highest-scored) option, given
/// `(key, score)` pairs.
fn pick_recommended(scores: &[(char, f64)]) -> char {
    scores
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(k, _)| *k)
        .unwrap_or('a')
}

/// Build a `SelectionOption`, marking it recommended if its key matches.
fn opt(key: char, label: &str, description: &str, recommended_key: char) -> SelectionOption {
    let mut tags = Vec::new();
    if key == recommended_key {
        tags.push("recommended".to_string());
    }
    SelectionOption {
        key,
        label: label.to_string(),
        description: description.to_string(),
        tags,
        metadata: None,
    }
}

// ---------------------------------------------------------------------------
// Pre-built recommendation menus
// ---------------------------------------------------------------------------

/// Recommend a project template based on task analysis.
pub fn recommend_project_template(task: &str) -> SelectionMenu {
    let scores = score_task(task);
    let rec = pick_recommended(&[
        ('a', scores.rust_axum),
        ('b', scores.python_fastapi),
        ('c', scores.typescript_express),
        ('d', scores.go_gin),
    ]);

    SelectionMenu {
        title: "Project Template Recommendations".to_string(),
        description: Some(format!("Based on: \"{}\"", task)),
        options: vec![
            opt('a', "Rust + Axum", "Fast, type-safe API", rec),
            opt('b', "Python + FastAPI", "Quick prototyping", rec),
            opt('c', "TypeScript + Express", "JS ecosystem", rec),
            opt('d', "Go + Gin", "High concurrency", rec),
            {
                let mut o = opt('e', "Let me specify...", "", rec);
                // Never recommended
                o.tags.clear();
                o
            },
        ],
        allow_custom: true,
        allow_multiple: false,
    }
}

/// Recommend an architecture pattern.
pub fn recommend_architecture(project_type: &str) -> SelectionMenu {
    let scores = score_task(project_type);
    let rec = pick_recommended(&[
        ('a', scores.layered),
        ('b', scores.hexagonal),
        ('c', scores.clean),
        ('d', scores.microservices),
        ('e', scores.monolith),
    ]);

    SelectionMenu {
        title: "Architecture Pattern".to_string(),
        description: None,
        options: vec![
            opt('a', "Layered", "routes / services / models", rec),
            opt('b', "Hexagonal", "ports & adapters", rec),
            opt('c', "Clean Architecture", "use-case centric", rec),
            opt(
                'd',
                "Microservices",
                "independently deployable services",
                rec,
            ),
            opt('e', "Monolith (simple)", "single deployable unit", rec),
        ],
        allow_custom: false,
        allow_multiple: false,
    }
}

/// Recommend a database based on requirements.
pub fn recommend_database(requirements: &str) -> SelectionMenu {
    let scores = score_task(requirements);
    let rec = pick_recommended(&[
        ('a', scores.postgresql),
        ('b', scores.sqlite),
        ('c', scores.mongodb),
        ('d', scores.redis),
        ('e', scores.no_db),
    ]);

    SelectionMenu {
        title: "Database".to_string(),
        description: None,
        options: vec![
            opt('a', "PostgreSQL", "Relational, feature-rich", rec),
            opt('b', "SQLite", "Embedded, zero-config", rec),
            opt('c', "MongoDB", "Document-oriented", rec),
            opt('d', "Redis", "In-memory, caching", rec),
            opt('e', "None", "No database needed", rec),
        ],
        allow_custom: false,
        allow_multiple: false,
    }
}

/// Recommend a testing strategy.
pub fn recommend_testing_strategy(project_size: &str) -> SelectionMenu {
    let scores = score_task(project_size);
    let rec = pick_recommended(&[
        ('a', scores.comprehensive),
        ('b', scores.standard),
        ('c', scores.minimal),
        ('d', scores.tdd),
    ]);

    SelectionMenu {
        title: "Testing Strategy".to_string(),
        description: None,
        options: vec![
            opt('a', "Comprehensive", "unit + integration + e2e", rec),
            opt('b', "Standard", "unit + integration", rec),
            opt('c', "Minimal", "unit only", rec),
            opt('d', "TDD", "tests first", rec),
        ],
        allow_custom: false,
        allow_multiple: false,
    }
}

/// Recommend a deployment strategy.
pub fn recommend_deployment(project_type: &str) -> SelectionMenu {
    let scores = score_task(project_type);
    let rec = pick_recommended(&[
        ('a', scores.docker),
        ('b', scores.serverless),
        ('c', scores.binary),
        ('d', scores.registry),
    ]);

    SelectionMenu {
        title: "Deployment".to_string(),
        description: None,
        options: vec![
            opt('a', "Docker container", "Containerised deploy", rec),
            opt('b', "Serverless", "Lambda / Cloud Functions", rec),
            opt('c', "Binary release", "Standalone executable", rec),
            opt('d', "Package registry", "crates.io / npm / pypi", rec),
            {
                let mut o = opt('e', "Not now", "Decide later", rec);
                o.tags.clear();
                o
            },
        ],
        allow_custom: false,
        allow_multiple: false,
    }
}

/// Dynamic "what next?" menu based on current context.
pub fn recommend_next_action(context: &str) -> SelectionMenu {
    let lower = context.to_lowercase();

    // Simple heuristic: if tests are mentioned, recommend running them.
    let rec = if lower.contains("test") || lower.contains("failing") {
        'a'
    } else if lower.contains("bug") || lower.contains("issue") || lower.contains("error") {
        'd'
    } else if lower.contains("deploy") || lower.contains("release") {
        'e'
    } else if lower.contains("doc") {
        'f'
    } else {
        'b'
    };

    SelectionMenu {
        title: "What would you like to do next?".to_string(),
        description: None,
        options: vec![
            opt('a', "Run tests", "Verify current state", rec),
            opt('b', "Add more features", "Continue building", rec),
            opt('c', "Refactor code", "Improve structure", rec),
            opt('d', "Fix issues", "Address problems", rec),
            opt('e', "Deploy", "Ship it", rec),
            opt('f', "Generate documentation", "Write docs", rec),
        ],
        allow_custom: true,
        allow_multiple: false,
    }
}

// ---------------------------------------------------------------------------
// Terminal rendering
// ---------------------------------------------------------------------------

/// Render and present a selection menu, returning the user's choice(s).
///
/// - Single-select: type a letter (e.g. `b`) and press Enter.
/// - Multi-select:  type comma-separated letters (e.g. `a,c,e`) and press
///   Enter.
/// - Custom input:  type free-form text when `allow_custom` is true.
/// - Default:       press Enter alone to accept the recommended option.
/// - Cancel:        press Esc to skip.
pub fn present_selection(menu: &SelectionMenu) -> Result<SelectionResult> {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // --- Determine box width ---
    let min_width = 50usize;
    let content_width = compute_content_width(menu);
    let box_width = content_width.max(min_width);

    let horiz_bar: String = "\u{2500}".repeat(box_width);

    // --- Top border ---
    writeln!(out)?;
    writeln!(
        out,
        "  {}{}{}",
        "\u{256d}\u{2500} ".custom_color(colored::CustomColor {
            r: 212,
            g: 163,
            b: 115
        }),
        menu.title.bold().custom_color(colored::CustomColor {
            r: 212,
            g: 163,
            b: 115
        }),
        format!(
            " {}{}",
            horiz_bar
                .chars()
                .take(box_width.saturating_sub(menu.title.len() + 2))
                .collect::<String>(),
            "\u{256e}"
        )
        .custom_color(colored::CustomColor {
            r: 212,
            g: 163,
            b: 115
        }),
    )?;

    // --- Description ---
    let vert = "\u{2502}".custom_color(colored::CustomColor {
        r: 212,
        g: 163,
        b: 115,
    });
    if let Some(ref desc) = menu.description {
        writeln!(out, "  {}", vert)?;
        writeln!(out, "  {}  {}", vert, desc.dimmed())?;
        writeln!(out, "  {}", vert,)?;
    } else {
        writeln!(out, "  {}", vert)?;
    }

    // --- Options ---
    let recommended_key = menu
        .options
        .iter()
        .find(|o| o.tags.contains(&"recommended".to_string()))
        .map(|o| o.key);

    for option in &menu.options {
        let is_rec = option.tags.contains(&"recommended".to_string());
        let star = if is_rec { "\u{2b50} " } else { "   " };
        let key_str = format!("{})", option.key);

        let label = if is_rec {
            option.label.bold().to_string()
        } else {
            option.label.clone()
        };

        let desc_part = if option.description.is_empty() {
            String::new()
        } else {
            format!("  {}", option.description.dimmed())
        };

        writeln!(
            out,
            "  {}  {}{} {}{}",
            vert,
            star.green(),
            key_str.cyan(),
            label,
            desc_part,
        )?;
    }

    // --- Bottom border ---
    writeln!(out, "  {}", vert)?;
    writeln!(
        out,
        "  {}",
        format!("\u{2570}{}{}", horiz_bar, "\u{256f}").custom_color(colored::CustomColor {
            r: 212,
            g: 163,
            b: 115
        }),
    )?;

    // --- Prompt ---
    let multi_hint = if menu.allow_multiple {
        " (comma-separated for multiple)"
    } else {
        ""
    };
    let default_hint = match recommended_key {
        Some(k) => format!(" [{}]", k),
        None => String::new(),
    };

    write!(
        out,
        "\n  {} {}",
        ">>".green().bold(),
        format!("Your choice{}{}: ", default_hint, multi_hint).dimmed(),
    )?;
    out.flush()?;
    drop(out);

    // --- Read input ---
    let input = read_selection_input()?;
    match input {
        SelectionInput::Cancelled => Ok(SelectionResult {
            selected: vec![],
            custom_input: None,
        }),
        SelectionInput::Text(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                // Default: pick the recommended option.
                if let Some(rec_key) = recommended_key {
                    if let Some(option) = menu.options.iter().find(|o| o.key == rec_key) {
                        return Ok(SelectionResult {
                            selected: vec![option.clone()],
                            custom_input: None,
                        });
                    }
                }
                // No recommendation — return empty.
                return Ok(SelectionResult {
                    selected: vec![],
                    custom_input: None,
                });
            }

            // Check for multi-select (comma-separated single chars)
            if menu.allow_multiple && trimmed.contains(',') {
                let keys: Vec<char> = trimmed
                    .split(',')
                    .filter_map(|s| {
                        let s = s.trim();
                        if s.len() == 1 {
                            Some(s.chars().next().unwrap().to_ascii_lowercase())
                        } else {
                            None
                        }
                    })
                    .collect();

                let selected: Vec<SelectionOption> = keys
                    .iter()
                    .filter_map(|k| menu.options.iter().find(|o| o.key == *k).cloned())
                    .collect();

                if !selected.is_empty() {
                    return Ok(SelectionResult {
                        selected,
                        custom_input: None,
                    });
                }
            }

            // Single-char selection
            if trimmed.len() == 1 {
                let ch = trimmed.chars().next().unwrap().to_ascii_lowercase();
                if let Some(option) = menu.options.iter().find(|o| o.key == ch) {
                    // If this is the custom/"specify" option, prompt for text
                    if option.label.contains("specify") || option.label.contains("Let me") {
                        let custom = read_custom_input()?;
                        return Ok(SelectionResult {
                            selected: vec![option.clone()],
                            custom_input: custom,
                        });
                    }
                    return Ok(SelectionResult {
                        selected: vec![option.clone()],
                        custom_input: None,
                    });
                }
            }

            // Free-form text
            if menu.allow_custom {
                Ok(SelectionResult {
                    selected: vec![],
                    custom_input: Some(trimmed.to_string()),
                })
            } else {
                // Treat as first-match attempt
                let lower = trimmed.to_lowercase();
                if let Some(option) = menu
                    .options
                    .iter()
                    .find(|o| o.label.to_lowercase().contains(&lower))
                {
                    return Ok(SelectionResult {
                        selected: vec![option.clone()],
                        custom_input: None,
                    });
                }
                Ok(SelectionResult {
                    selected: vec![],
                    custom_input: None,
                })
            }
        }
    }
}

/// Compute the minimum content width needed to render every option without
/// wrapping.
fn compute_content_width(menu: &SelectionMenu) -> usize {
    let mut max = menu.title.len() + 4;
    if let Some(ref desc) = menu.description {
        max = max.max(desc.len() + 4);
    }
    for opt in &menu.options {
        //  "   ⭐ a) Label  Description"
        let w = 6 + 3 + opt.label.len() + 2 + opt.description.len();
        max = max.max(w);
    }
    max
}

// ---------------------------------------------------------------------------
// Terminal I/O helpers
// ---------------------------------------------------------------------------

enum SelectionInput {
    Cancelled,
    Text(String),
}

/// Read a line of input, supporting Esc cancellation in TTY mode.
fn read_selection_input() -> Result<SelectionInput> {
    let stdin = io::stdin();
    if !stdin.is_terminal() {
        // Piped input — just read a line.
        let mut buf = String::new();
        stdin.read_line(&mut buf)?;
        return Ok(SelectionInput::Text(buf));
    }

    terminal::enable_raw_mode()?;
    let result = read_raw_line();
    let _ = terminal::disable_raw_mode();
    result
}

/// Raw-mode line reader that recognises Esc as cancellation.
fn read_raw_line() -> Result<SelectionInput> {
    let mut buf = String::new();
    let mut stdout = io::stdout();

    loop {
        if event::poll(std::time::Duration::from_millis(5000))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => {
                        write!(stdout, "\r\n")?;
                        stdout.flush()?;
                        return Ok(SelectionInput::Cancelled);
                    }
                    KeyCode::Enter => {
                        write!(stdout, "\r\n")?;
                        stdout.flush()?;
                        return Ok(SelectionInput::Text(buf));
                    }
                    KeyCode::Char(c) => {
                        buf.push(c);
                        write!(stdout, "{c}")?;
                        stdout.flush()?;
                    }
                    KeyCode::Backspace => {
                        if buf.pop().is_some() {
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

/// Prompt for custom / free-form text after the user chose the "specify" option.
fn read_custom_input() -> Result<Option<String>> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write!(out, "  {} ", "Type your answer:".dimmed())?;
    out.flush()?;
    drop(out);

    let input = read_selection_input()?;
    match input {
        SelectionInput::Cancelled => Ok(None),
        SelectionInput::Text(t) => {
            let trimmed = t.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Guided workflow
// ---------------------------------------------------------------------------

/// Run the full guided project-setup wizard.
///
/// Chains: template -> architecture -> database -> testing -> deployment.
/// Returns a [`ProjectPlan`] with all user selections.
pub async fn guided_project_setup(task: &str) -> Result<ProjectPlan> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out)?;
    writeln!(
        out,
        "  {}",
        "Guided Project Setup"
            .bold()
            .underline()
            .custom_color(colored::CustomColor {
                r: 212,
                g: 163,
                b: 115
            })
    )?;
    writeln!(
        out,
        "  {}",
        "Answer each step to build your project plan. Press Esc to skip any step.".dimmed()
    )?;
    drop(out);

    // 1. Template
    let template_menu = recommend_project_template(task);
    let template_result = present_selection(&template_menu)?;
    let (template, language, framework) = parse_template_result(&template_result);

    // 2. Architecture
    let arch_menu = recommend_architecture(task);
    let arch_result = present_selection(&arch_menu)?;
    let architecture = first_label_or(&arch_result, "Layered");

    // 3. Database
    let db_menu = recommend_database(task);
    let db_result = present_selection(&db_menu)?;
    let database = {
        let db = first_label_or(&db_result, "None");
        if db == "None" {
            None
        } else {
            Some(db)
        }
    };

    // 4. Testing
    let test_menu = recommend_testing_strategy(task);
    let test_result = present_selection(&test_menu)?;
    let testing_strategy = first_label_or(&test_result, "Standard");

    // 5. Deployment
    let deploy_menu = recommend_deployment(task);
    let deploy_result = present_selection(&deploy_menu)?;
    let deployment = {
        let dep = first_label_or(&deploy_result, "Not now");
        if dep == "Not now" {
            None
        } else {
            Some(dep)
        }
    };

    let plan = ProjectPlan {
        template,
        language,
        framework,
        architecture,
        database,
        testing_strategy,
        deployment,
    };

    // Summary
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out)?;
    writeln!(
        out,
        "  {}",
        "Project Plan Summary"
            .bold()
            .underline()
            .custom_color(colored::CustomColor {
                r: 96,
                g: 108,
                b: 56
            })
    )?;
    writeln!(out, "  {} {}", "Template:".bold(), plan.template)?;
    writeln!(out, "  {} {}", "Language:".bold(), plan.language)?;
    writeln!(out, "  {} {}", "Framework:".bold(), plan.framework)?;
    writeln!(out, "  {} {}", "Architecture:".bold(), plan.architecture)?;
    if let Some(ref db) = plan.database {
        writeln!(out, "  {} {}", "Database:".bold(), db)?;
    }
    writeln!(out, "  {} {}", "Testing:".bold(), plan.testing_strategy)?;
    if let Some(ref dep) = plan.deployment {
        writeln!(out, "  {} {}", "Deployment:".bold(), dep)?;
    }
    writeln!(out)?;
    drop(out);

    Ok(plan)
}

/// Extract template/language/framework from a template selection result.
fn parse_template_result(result: &SelectionResult) -> (String, String, String) {
    if let Some(ref custom) = result.custom_input {
        return (custom.clone(), custom.clone(), custom.clone());
    }
    if let Some(option) = result.selected.first() {
        match option.key {
            'a' => (
                "Rust + Axum".to_string(),
                "Rust".to_string(),
                "Axum".to_string(),
            ),
            'b' => (
                "Python + FastAPI".to_string(),
                "Python".to_string(),
                "FastAPI".to_string(),
            ),
            'c' => (
                "TypeScript + Express".to_string(),
                "TypeScript".to_string(),
                "Express".to_string(),
            ),
            'd' => ("Go + Gin".to_string(), "Go".to_string(), "Gin".to_string()),
            _ => (
                option.label.clone(),
                option.label.clone(),
                option.label.clone(),
            ),
        }
    } else {
        (
            "Rust + Axum".to_string(),
            "Rust".to_string(),
            "Axum".to_string(),
        )
    }
}

/// Return the label of the first selected option, or a default.
fn first_label_or(result: &SelectionResult, default: &str) -> String {
    if let Some(ref custom) = result.custom_input {
        return custom.clone();
    }
    result
        .selected
        .first()
        .map(|o| o.label.clone())
        .unwrap_or_else(|| default.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Menu construction tests ---

    #[test]
    fn test_recommend_project_template_has_options() {
        let menu = recommend_project_template("Build a REST API");
        assert_eq!(menu.options.len(), 5);
        assert!(menu.allow_custom);
        assert!(!menu.allow_multiple);
    }

    #[test]
    fn test_recommend_project_template_description() {
        let menu = recommend_project_template("Build a REST API for user management");
        assert!(menu.description.is_some());
        assert!(menu
            .description
            .as_ref()
            .unwrap()
            .contains("Build a REST API"));
    }

    #[test]
    fn test_recommend_architecture_has_options() {
        let menu = recommend_architecture("web api");
        assert_eq!(menu.options.len(), 5);
        assert!(!menu.allow_custom);
    }

    #[test]
    fn test_recommend_database_has_options() {
        let menu = recommend_database("user management with data storage");
        assert_eq!(menu.options.len(), 5);
    }

    #[test]
    fn test_recommend_testing_strategy_has_options() {
        let menu = recommend_testing_strategy("large enterprise project");
        assert_eq!(menu.options.len(), 4);
    }

    #[test]
    fn test_recommend_deployment_has_options() {
        let menu = recommend_deployment("web api");
        assert_eq!(menu.options.len(), 5);
    }

    #[test]
    fn test_recommend_next_action_has_options() {
        let menu = recommend_next_action("just finished coding");
        assert_eq!(menu.options.len(), 6);
    }

    // --- Task scoring / recommendation tests ---

    #[test]
    fn test_score_rest_api_recommends_backend_template() {
        let menu = recommend_project_template("Build a REST API for user management");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some(), "Should have a recommended option");
        // For a REST API, Rust+Axum or Go+Gin should be top (both score high)
        let rec = rec.unwrap();
        assert!(
            rec.label.contains("Rust") || rec.label.contains("Go"),
            "REST API should recommend Rust or Go, got: {}",
            rec.label
        );
    }

    #[test]
    fn test_score_python_script_recommends_python() {
        let menu = recommend_project_template("Write a Python script to process CSV files");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Python"),
            "Python task should recommend Python"
        );
    }

    #[test]
    fn test_score_typescript_recommends_typescript() {
        let menu = recommend_project_template("Build a Node.js Express API");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("TypeScript"),
            "Node.js task should recommend TypeScript"
        );
    }

    #[test]
    fn test_score_go_recommends_go() {
        let menu = recommend_project_template("Build a high-concurrency golang microservice");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Go"),
            "Go task should recommend Go"
        );
    }

    #[test]
    fn test_score_database_postgres_for_user_mgmt() {
        let menu = recommend_database("user management with relational data");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("PostgreSQL"),
            "Relational data should recommend PostgreSQL"
        );
    }

    #[test]
    fn test_score_database_redis_for_cache() {
        let menu = recommend_database("session caching layer with redis");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Redis"),
            "Cache task should recommend Redis"
        );
    }

    #[test]
    fn test_score_database_mongodb_for_nosql() {
        let menu = recommend_database("document store with nosql mongo");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("MongoDB"),
            "NoSQL task should recommend MongoDB"
        );
    }

    #[test]
    fn test_score_database_no_db_for_cli() {
        let menu = recommend_database("simple CLI calculator");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("None"),
            "CLI without data signals should recommend None"
        );
    }

    #[test]
    fn test_score_architecture_microservices() {
        let menu = recommend_architecture("microservice architecture with independent deploys");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Microservices"),
            "Microservice mention should recommend Microservices"
        );
    }

    #[test]
    fn test_score_architecture_monolith_for_simple() {
        let menu = recommend_architecture("simple quick script");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        let label = &rec.unwrap().label;
        assert!(
            label.contains("Monolith") || label.contains("Layered"),
            "Simple project should recommend Monolith or Layered, got: {}",
            label
        );
    }

    #[test]
    fn test_score_testing_tdd() {
        let menu = recommend_testing_strategy("TDD approach with test-driven development");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("TDD"),
            "TDD mention should recommend TDD"
        );
    }

    #[test]
    fn test_score_testing_comprehensive_for_api() {
        let menu = recommend_testing_strategy("REST API with comprehensive e2e testing");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Comprehensive"),
            "e2e mention should recommend Comprehensive"
        );
    }

    #[test]
    fn test_score_deployment_docker() {
        let menu = recommend_deployment("docker containerised deployment on kubernetes");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Docker"),
            "Docker mention should recommend Docker"
        );
    }

    #[test]
    fn test_score_deployment_serverless() {
        let menu = recommend_deployment("serverless lambda cloud function");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Serverless"),
            "Serverless mention should recommend Serverless"
        );
    }

    #[test]
    fn test_score_deployment_binary_for_cli() {
        let menu = recommend_deployment("cli binary release");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Binary"),
            "Binary/CLI mention should recommend Binary release"
        );
    }

    // --- Next action recommendation ---

    #[test]
    fn test_next_action_test_context() {
        let menu = recommend_next_action("tests are failing");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Run tests"),
            "Failing tests context should recommend Run tests"
        );
    }

    #[test]
    fn test_next_action_bug_context() {
        let menu = recommend_next_action("there is a bug in the code");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Fix issues"),
            "Bug context should recommend Fix issues"
        );
    }

    #[test]
    fn test_next_action_deploy_context() {
        let menu = recommend_next_action("time to deploy");
        let rec = menu
            .options
            .iter()
            .find(|o| o.tags.contains(&"recommended".to_string()));
        assert!(rec.is_some());
        assert!(
            rec.unwrap().label.contains("Deploy"),
            "Deploy context should recommend Deploy"
        );
    }

    // --- Structural tests ---

    #[test]
    fn test_option_keys_are_unique() {
        let menus = vec![
            recommend_project_template("test"),
            recommend_architecture("test"),
            recommend_database("test"),
            recommend_testing_strategy("test"),
            recommend_deployment("test"),
            recommend_next_action("test"),
        ];
        for menu in menus {
            let mut keys: Vec<char> = menu.options.iter().map(|o| o.key).collect();
            let before = keys.len();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), before, "Duplicate keys in menu: {}", menu.title);
        }
    }

    #[test]
    fn test_exactly_one_recommended_per_menu() {
        let menus = vec![
            recommend_project_template("Build a REST API"),
            recommend_architecture("web api"),
            recommend_database("user management"),
            recommend_testing_strategy("large project"),
            recommend_deployment("web api"),
            recommend_next_action("just finished"),
        ];
        for menu in menus {
            let rec_count = menu
                .options
                .iter()
                .filter(|o| o.tags.contains(&"recommended".to_string()))
                .count();
            assert!(
                rec_count >= 1,
                "Menu '{}' should have at least one recommended option, has {}",
                menu.title,
                rec_count
            );
        }
    }

    #[test]
    fn test_pick_recommended_helper() {
        assert_eq!(pick_recommended(&[('a', 1.0), ('b', 3.0), ('c', 2.0)]), 'b');
        assert_eq!(pick_recommended(&[('x', 5.0)]), 'x');
    }

    #[test]
    fn test_compute_content_width() {
        let menu = SelectionMenu {
            title: "Short".to_string(),
            description: None,
            options: vec![SelectionOption {
                key: 'a',
                label: "Test".to_string(),
                description: "A test option".to_string(),
                tags: vec![],
                metadata: None,
            }],
            allow_custom: false,
            allow_multiple: false,
        };
        let width = compute_content_width(&menu);
        assert!(width >= 9, "Width should be at least title + padding");
    }

    // --- ProjectPlan tests ---

    #[test]
    fn test_project_plan_to_system_prompt_section() {
        let plan = ProjectPlan {
            template: "Rust + Axum".to_string(),
            language: "Rust".to_string(),
            framework: "Axum".to_string(),
            architecture: "Layered".to_string(),
            database: Some("PostgreSQL".to_string()),
            testing_strategy: "Comprehensive".to_string(),
            deployment: Some("Docker container".to_string()),
        };
        let section = plan.to_system_prompt_section();
        assert!(section.contains("Rust + Axum"));
        assert!(section.contains("Rust"));
        assert!(section.contains("Axum"));
        assert!(section.contains("Layered"));
        assert!(section.contains("PostgreSQL"));
        assert!(section.contains("Comprehensive"));
        assert!(section.contains("Docker container"));
        assert!(section.contains("## Project Plan"));
    }

    #[test]
    fn test_project_plan_without_optional_fields() {
        let plan = ProjectPlan {
            template: "Go + Gin".to_string(),
            language: "Go".to_string(),
            framework: "Gin".to_string(),
            architecture: "Monolith (simple)".to_string(),
            database: None,
            testing_strategy: "Minimal".to_string(),
            deployment: None,
        };
        let section = plan.to_system_prompt_section();
        assert!(section.contains("Go + Gin"));
        assert!(!section.contains("Database"));
        assert!(!section.contains("Deployment"));
    }

    // --- parse_template_result tests ---

    #[test]
    fn test_parse_template_result_rust() {
        let result = SelectionResult {
            selected: vec![SelectionOption {
                key: 'a',
                label: "Rust + Axum".to_string(),
                description: "Fast, type-safe API".to_string(),
                tags: vec!["recommended".to_string()],
                metadata: None,
            }],
            custom_input: None,
        };
        let (template, language, framework) = parse_template_result(&result);
        assert_eq!(template, "Rust + Axum");
        assert_eq!(language, "Rust");
        assert_eq!(framework, "Axum");
    }

    #[test]
    fn test_parse_template_result_python() {
        let result = SelectionResult {
            selected: vec![SelectionOption {
                key: 'b',
                label: "Python + FastAPI".to_string(),
                description: "Quick prototyping".to_string(),
                tags: vec![],
                metadata: None,
            }],
            custom_input: None,
        };
        let (template, language, framework) = parse_template_result(&result);
        assert_eq!(template, "Python + FastAPI");
        assert_eq!(language, "Python");
        assert_eq!(framework, "FastAPI");
    }

    #[test]
    fn test_parse_template_result_custom() {
        let result = SelectionResult {
            selected: vec![],
            custom_input: Some("Ruby on Rails".to_string()),
        };
        let (template, language, framework) = parse_template_result(&result);
        assert_eq!(template, "Ruby on Rails");
        assert_eq!(language, "Ruby on Rails");
    }

    #[test]
    fn test_parse_template_result_empty() {
        let result = SelectionResult {
            selected: vec![],
            custom_input: None,
        };
        let (template, language, framework) = parse_template_result(&result);
        // Falls back to Rust + Axum
        assert_eq!(template, "Rust + Axum");
    }

    #[test]
    fn test_first_label_or_with_selection() {
        let result = SelectionResult {
            selected: vec![SelectionOption {
                key: 'b',
                label: "Hexagonal".to_string(),
                description: "ports & adapters".to_string(),
                tags: vec![],
                metadata: None,
            }],
            custom_input: None,
        };
        assert_eq!(first_label_or(&result, "default"), "Hexagonal");
    }

    #[test]
    fn test_first_label_or_with_default() {
        let result = SelectionResult {
            selected: vec![],
            custom_input: None,
        };
        assert_eq!(first_label_or(&result, "Layered"), "Layered");
    }

    #[test]
    fn test_first_label_or_with_custom() {
        let result = SelectionResult {
            selected: vec![],
            custom_input: Some("Event sourcing".to_string()),
        };
        assert_eq!(first_label_or(&result, "Layered"), "Event sourcing");
    }

    // --- Score edge cases ---

    #[test]
    fn test_score_empty_task() {
        let scores = score_task("");
        // Should have baseline Rust score
        assert!(
            scores.rust_axum > 0.0,
            "Empty task should have Rust baseline"
        );
    }

    #[test]
    fn test_score_multiple_language_signals() {
        let scores = score_task("Port a Python FastAPI service to Rust Axum");
        assert!(scores.rust_axum > 0.0);
        assert!(scores.python_fastapi > 0.0);
    }

    #[test]
    fn test_selection_option_clone() {
        let opt = SelectionOption {
            key: 'a',
            label: "Test".to_string(),
            description: "A test".to_string(),
            tags: vec!["recommended".to_string()],
            metadata: Some(serde_json::json!({"key": "value"})),
        };
        let cloned = opt.clone();
        assert_eq!(cloned.key, 'a');
        assert_eq!(cloned.label, "Test");
        assert!(cloned.tags.contains(&"recommended".to_string()));
    }

    #[test]
    fn test_selection_menu_clone() {
        let menu = recommend_project_template("test");
        let cloned = menu.clone();
        assert_eq!(cloned.title, menu.title);
        assert_eq!(cloned.options.len(), menu.options.len());
    }

    #[test]
    fn test_selection_result_clone() {
        let result = SelectionResult {
            selected: vec![],
            custom_input: Some("custom".to_string()),
        };
        let cloned = result.clone();
        assert_eq!(cloned.custom_input, Some("custom".to_string()));
    }
}
