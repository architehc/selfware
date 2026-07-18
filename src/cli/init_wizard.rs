//! Interactive setup wizard and template-based config generation.

use anyhow::Result;

use crate::ui::style::Glyphs;

/// Print helper that respects the global `--quiet` flag.
macro_rules! wizard_print {
    () => {
        if !crate::output::is_quiet() {
            print!("\n");
        }
    };
    ($($arg:tt)*) => {
        if !crate::output::is_quiet() {
            print!($($arg)*);
            print!("\n");
        }
    };
}

pub(crate) fn run_init_wizard(template: Option<String>, scaffold: bool) -> Result<()> {
    use std::io::{self, BufRead, IsTerminal, Write};
    use std::path::PathBuf;

    // If a template is provided, skip the interactive wizard
    if let Some(ref tmpl) = template {
        return write_template_config(tmpl);
    }

    // The wizard reads every answer from stdin; without a terminal each
    // read_line hits EOF and silently selects the default, persisting an
    // all-defaults localhost config (shadowing any working global config)
    // that the user never asked for. Refuse instead — mirrors the headless
    // fail-fast guards on the chat/run entry paths.
    ensure_interactive_stdin(std::io::stdin().is_terminal())?;

    if scaffold {
        run_scaffold_interview()?;
    }

    wizard_print!();
    wizard_print!(
        "{} Welcome to Selfware! Let's set up your workspace.",
        Glyphs::seedling()
    );
    wizard_print!();

    // Detect project type
    let project_type = if std::path::Path::new("Cargo.toml").exists() {
        "Rust (Cargo.toml)"
    } else if std::path::Path::new("package.json").exists() {
        "Node.js (package.json)"
    } else if std::path::Path::new("pyproject.toml").exists()
        || std::path::Path::new("setup.py").exists()
    {
        "Python (pyproject.toml)"
    } else if std::path::Path::new("go.mod").exists() {
        "Go (go.mod)"
    } else {
        "Unknown"
    };
    wizard_print!("  Detecting project type... Found: {}", project_type);
    wizard_print!();

    // Step 1: Endpoint
    wizard_print!("Step 1/4: API Endpoint");
    wizard_print!("  Where should Selfware connect?");
    wizard_print!("  [1] Local (http://127.0.0.1:1234/v1) - LM Studio, Ollama, vLLM");
    wizard_print!("  [2] OpenAI-compatible API (https://api.openai.com/v1)");
    wizard_print!("  [3] Custom endpoint");
    print!("  > ");
    io::stdout().flush()?;
    let mut choice = String::new();
    io::stdin().lock().read_line(&mut choice)?;
    let endpoint = match choice.trim() {
        "2" => "https://api.openai.com/v1".to_string(),
        "3" => {
            print!("  Enter endpoint URL: ");
            io::stdout().flush()?;
            let mut url = String::new();
            io::stdin().lock().read_line(&mut url)?;
            url.trim().to_string()
        }
        _ => "http://127.0.0.1:1234/v1".to_string(),
    };
    wizard_print!();

    // Step 2: Model
    wizard_print!("Step 2/4: Model");
    let default_model = if endpoint.contains("openai") {
        "gpt-4"
    } else {
        "qwen3-coder"
    };
    print!("  Which model should Selfware use? [{}]: ", default_model);
    io::stdout().flush()?;
    let mut model = String::new();
    io::stdin().lock().read_line(&mut model)?;
    let model = if model.trim().is_empty() {
        default_model.to_string()
    } else {
        model.trim().to_string()
    };
    wizard_print!();

    // Step 2.5: API key (optional) — stored in the OS keyring, never the config.
    wizard_print!("Step: API Key (optional)");
    wizard_print!("  If your endpoint needs an API key (e.g. OpenRouter), paste it now.");
    wizard_print!("  It is stored in your OS keyring — NOT written to the config file.");
    wizard_print!("  Leave blank to skip (you can set SELFWARE_API_KEY later).");
    print!("  > ");
    io::stdout().flush()?;
    let mut api_key = String::new();
    io::stdin().lock().read_line(&mut api_key)?;
    let api_key = api_key.trim();
    if !api_key.is_empty() {
        match crate::config::save_api_key_to_keyring(&endpoint, api_key) {
            Ok(()) => wizard_print!("  {} API key saved to your OS keyring.", Glyphs::bloom()),
            Err(e) => wizard_print!(
                "  {} Could not save to keyring ({}). Set SELFWARE_API_KEY=<key> in your environment instead.",
                Glyphs::frost(),
                e
            ),
        }
    }
    wizard_print!();

    // Step 3: Allowed paths
    wizard_print!("Step 3/4: Allowed Paths");
    wizard_print!("  Which directories can Selfware access?");
    wizard_print!("  [1] Current directory only (.)");
    wizard_print!("  [2] Home directory (~)");
    wizard_print!("  [3] Custom paths");
    print!("  > ");
    io::stdout().flush()?;
    let mut path_choice = String::new();
    io::stdin().lock().read_line(&mut path_choice)?;
    let allowed_paths = match path_choice.trim() {
        "2" => {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            allowed_paths_toml([dir_allow_entry(&home)])
        }
        "3" => {
            print!("  Enter paths (comma-separated): ");
            io::stdout().flush()?;
            let mut paths = String::new();
            io::stdin().lock().read_line(&mut paths)?;
            allowed_paths_toml(
                paths
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(custom_allow_entry),
            )
        }
        _ => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            allowed_paths_toml([dir_allow_entry(&cwd)])
        }
    };
    wizard_print!();

    // Step 4: Execution mode
    wizard_print!("Step 4/4: Execution Mode");
    wizard_print!("  How should Selfware handle file changes?");
    wizard_print!("  [1] Normal - Ask before every edit (safest)");
    wizard_print!("  [2] AutoEdit - Auto-approve file edits, confirm commands");
    wizard_print!("  [3] YOLO - Auto-approve everything (use with caution!)");
    print!("  > ");
    io::stdout().flush()?;
    let mut mode_choice = String::new();
    io::stdin().lock().read_line(&mut mode_choice)?;
    let mode = match mode_choice.trim() {
        "2" => "autoedit",
        "3" => "yolo",
        _ => "normal",
    };
    wizard_print!();

    // Write config
    write_config_file(&endpoint, &model, mode, &allowed_paths)
}

/// Refuse to run the interactive wizard without a terminal on stdin: every
/// `read_line` would hit EOF and silently select the default answer,
/// persisting an all-defaults localhost config the user never asked for.
fn ensure_interactive_stdin(is_terminal: bool) -> Result<()> {
    if !is_terminal {
        anyhow::bail!(
            "`selfware init` is an interactive wizard and needs a terminal on stdin. \
             For a non-interactive setup use `selfware init --template <rust|python|node|minimal>`, \
             or re-run in a terminal."
        );
    }
    Ok(())
}

/// Quote `s` as a TOML basic-string literal. Backslashes, double quotes, and
/// control characters are escaped, so Windows paths (`C:\Users\...`) don't
/// produce invalid TOML escape sequences (`\U`, `\s`) in the generated config.
fn toml_quote(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Allow-list entry for a concrete directory: `<dir>/**`. The path validator
/// matches allow-list entries as exact globs, so a bare directory would match
/// only the directory itself and DENY every file inside it — the descendant
/// glob is what actually grants the project the user picked.
fn dir_allow_entry(dir: &std::path::Path) -> String {
    format!("{}/**", dir.display())
}

/// User-supplied custom path → allow-list entry. Entries that already carry
/// glob metacharacters are left alone; plain directories get the descendant
/// glob (`/**`) appended, for the same reason as [`dir_allow_entry`].
fn custom_allow_entry(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.contains(['*', '?', '[']) {
        return trimmed.to_string();
    }
    // Strip trailing separators so we don't emit `foo//**`, but never turn a
    // Windows drive root (`C:\`) into a relative-looking `C:/**`.
    let base = trimmed.trim_end_matches(['/', '\\']);
    if base.len() == 2 && base.ends_with(':') {
        format!("{}**", trimmed)
    } else {
        format!("{}/**", base)
    }
}

/// Render allow-list entries as a TOML array literal with each entry quoted
/// via [`toml_quote`].
fn allowed_paths_toml<I, S>(entries: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let quoted: Vec<String> = entries
        .into_iter()
        .map(|e| toml_quote(e.as_ref()))
        .collect();
    format!("[{}]", quoted.join(", "))
}

/// Ask structured questions about the project to build, then scaffold it into
/// the current directory via the interview-driven template engine.
fn run_scaffold_interview() -> Result<()> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        wizard_print!(
            "  {} --scaffold needs an interactive terminal; skipping.",
            Glyphs::frost()
        );
        return Ok(());
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let ctx = crate::interview::run_interview("Scaffold a new project", &cwd)?;
    match crate::templates::scaffold_from_context(&ctx, &cwd) {
        Ok(files) => {
            wizard_print!("  {} Scaffolded {} files:", Glyphs::bloom(), files.len());
            for f in &files {
                wizard_print!("    {}", f);
            }
        }
        Err(e) => wizard_print!("  {} Could not scaffold: {}", Glyphs::frost(), e),
    }
    Ok(())
}

fn write_template_config(template: &str) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Scaffold project files for known language templates
    match template {
        "rust" | "python" | "node" | "nodejs" | "typescript" => {
            wizard_print!("  {} Using '{}' template...", Glyphs::gear(), template);

            let lang_key = match template {
                "node" | "nodejs" | "typescript" => "nodejs",
                other => other,
            };

            let project_name = cwd
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("my-project")
                .to_string();

            let engine = crate::templates::TemplateEngine::new();
            let opts = crate::templates::ScaffoldOptions {
                description: format!("A {} project scaffolded by Selfware", template),
                framework: None,
                with_ci: true,
                with_tests: true,
                qa_profile: "standard".into(),
            };

            match engine.scaffold_project(lang_key, &project_name, &cwd, &opts) {
                Ok(files) => {
                    wizard_print!("  {} Scaffolded {} files:", Glyphs::bloom(), files.len());
                    for f in &files {
                        wizard_print!("    {}", f);
                    }
                }
                Err(e) => {
                    wizard_print!("  {} Could not scaffold project: {}", Glyphs::frost(), e);
                }
            }
        }
        "minimal" => {
            wizard_print!("  {} Using 'minimal' template...", Glyphs::gear());
        }
        other => {
            anyhow::bail!(
                "Unknown template '{}'. Available templates: rust, python, node, nodejs, typescript, minimal",
                other
            );
        }
    }

    let (endpoint, model, mode, allowed_paths) = match template {
        "rust" | "python" | "node" | "nodejs" | "typescript" => (
            "http://127.0.0.1:1234/v1".to_string(),
            "qwen3-coder".to_string(),
            "normal",
            allowed_paths_toml([dir_allow_entry(&cwd)]),
        ),
        _ => (
            "http://127.0.0.1:1234/v1".to_string(),
            "qwen3-coder".to_string(),
            "normal",
            "[\"./**\"]".to_string(),
        ),
    };

    write_config_file(&endpoint, &model, mode, &allowed_paths)
}

/// Build the generated `selfware.toml` body. Execution mode is deliberately
/// NOT written as a live key — it's `#[serde(skip)]` (runtime-only, so a repo
/// config can't silently enable auto-approval), so emitting it would be a dead
/// key that misleads the user. A comment documents how to select it per run.
fn build_config_content(endpoint: &str, model: &str, allowed_paths: &str) -> String {
    format!(
        r#"# Selfware Configuration
# Generated by `selfware init`

endpoint = "{}"
model = "{}"

# Token limits. `context_length` MUST match your server's real context window
# (vLLM --max-model-len, LM Studio's context slider, Ollama num_ctx); raise it
# when your model serves more. `max_tokens` is the per-response output budget
# and must fit inside context_length with room to spare for conversation.
# Leaving these unset is NOT safe for models selfware doesn't recognize: the
# context window falls back to a conservative 32k while max_tokens defaults
# to 64k, which doesn't fit.
context_length = 32768
max_tokens = 8192

# Execution mode is chosen per RUN, not stored here — a config file (this one
# lives in the repo) must never be able to silently enable auto-approval.
# Start Selfware in your chosen mode with a flag or env var:
#   Normal (ask first): selfware
#   AutoEdit:           selfware -m auto-edit   (or SELFWARE_MODE=auto-edit)
#   YOLO (auto all):    selfware -y             (or SELFWARE_MODE=yolo)

[safety]
allowed_paths = {}

[agent]
# token_budget defaults to 60% of context_length — set explicitly to override
# token_budget = 19660
"#,
        endpoint, model, allowed_paths
    )
}

/// Probe whether something is listening on the endpoint's host:port.
/// TCP-connect only — no HTTP, no async — so it is safe to call from the
/// sync wizard. A `false` result means "nothing answered in time", i.e. the
/// endpoint is almost certainly dead; it does not prove the URL path is
/// right, only that a server is (not) there.
fn probe_endpoint_tcp(endpoint: &str, timeout: std::time::Duration) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};

    let Ok(url) = url::Url::parse(endpoint) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let Some(port) = url.port_or_known_default() else {
        return false;
    };
    let Ok(mut addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs.any(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok())
}

fn write_config_file(endpoint: &str, model: &str, mode: &str, allowed_paths: &str) -> Result<()> {
    use std::path::PathBuf;

    // Write the workspace config to `./selfware.toml` so the loader finds it
    // before falling back to the global config directory.
    let config_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("selfware.toml");

    // Check if config already exists
    if config_path.exists() {
        use std::io::{self, BufRead, Write};

        wizard_print!(
            "  {} Configuration already exists at {}",
            Glyphs::frost(),
            config_path.display()
        );
        print!("  Overwrite? [y/N]: ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            wizard_print!("  Aborted. Existing configuration preserved.");
            return Ok(());
        }
    }

    let content = build_config_content(endpoint, model, allowed_paths);

    // Never write a config selfware itself would reject: validate the exact
    // TOML body the way the loader would after reading it from disk, and fail
    // loudly instead of persisting garbage.
    crate::config::Config::validate_generated_toml(&content)?;

    // Probe the endpoint before declaring success — a config pointing at a
    // dead endpoint is the most common first-run failure. TCP-connect only
    // (no HTTP, no async runtime), so this is safe in the sync wizard.
    let endpoint_reachable = probe_endpoint_tcp(endpoint, std::time::Duration::from_secs(2));

    std::fs::write(&config_path, &content)?;
    wizard_print!(
        "  {} Configuration saved to {}",
        Glyphs::bloom(),
        config_path.display()
    );

    // Trust the config we just wrote. It was created by the user, in this
    // directory, by an explicit `init` — so the untrusted-checkout
    // restriction (which silently resets allowed_paths/hooks/MCP to defaults)
    // must not strip it on the next run. This is the same store
    // `selfware trust` writes (~/.selfware/trusted_repos).
    match crate::config::trust::add_trusted_config(&config_path) {
        Ok(()) => wizard_print!(
            "  {} Trusted this config — its [safety] settings take effect on the next run.",
            Glyphs::bloom()
        ),
        Err(e) => {
            wizard_print!(
                "  {} Could not record repo trust ({}). Run `selfware trust` in this directory,",
                Glyphs::frost(),
                e
            );
            wizard_print!(
                "     otherwise the untrusted-checkout protection resets allowed_paths/hooks/MCP to defaults."
            );
        }
    }

    // The new project-local file shadows any global config in this directory —
    // name that explicitly so a working global setup doesn't look "lost".
    if let Some(home_config) = dirs::home_dir().map(|h| h.join(".config/selfware/config.toml")) {
        if home_config.is_file() {
            wizard_print!(
                "  {} Note: {} takes precedence over your global config ({}) in this directory.",
                Glyphs::frost(),
                config_path.display(),
                home_config.display()
            );
        }
    }

    if endpoint_reachable {
        wizard_print!("  {} Endpoint {} is reachable.", Glyphs::bloom(), endpoint);
    } else {
        wizard_print!();
        wizard_print!(
            "  {} WARNING: could not reach {} — the config was written UNVERIFIED.",
            Glyphs::frost(),
            endpoint
        );
        wizard_print!(
            "     Start your LLM server (or fix the endpoint), then verify with `selfware llm-doctor`."
        );
    }
    wizard_print!();
    // Echo the run command for the mode the user picked — the mode isn't
    // persisted (see the config comment), so tell them exactly how to get it.
    match mode {
        "yolo" => wizard_print!(
            "  {} You chose YOLO — start it with `selfware -y` (mode isn't saved in config, for safety).",
            Glyphs::sprout()
        ),
        "autoedit" | "auto-edit" | "auto_edit" => wizard_print!(
            "  {} You chose AutoEdit — start it with `selfware -m auto-edit`.",
            Glyphs::sprout()
        ),
        _ => wizard_print!(
            "  {} Run `selfware` to start your workshop!",
            Glyphs::sprout()
        ),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_config_content;

    #[test]
    fn generated_default_config_passes_agent_context_check() {
        // P0-1: the wizard's own suggested defaults (unknown model
        // `qwen3-coder`) previously produced a config the agent refused to
        // start (32k unknown-model fallback vs 64k default max_tokens →
        // max_context_tokens = 0). The generated body must carry explicit,
        // mutually-fitting token limits.
        let content = build_config_content("http://127.0.0.1:1234/v1", "qwen3-coder", "[\"./**\"]");
        let cfg: crate::config::Config =
            toml::from_str(&content).expect("generated TOML must parse into Config");
        let (budget, reserved) = cfg
            .derive_context_budget()
            .expect("wizard defaults must pass the agent's context check");
        assert_eq!(
            reserved, cfg.max_tokens,
            "wizard defaults must fit without relying on the runtime clamp"
        );
        assert!(budget >= 2048, "conversation budget too small: {budget}");
        // …and through the strict generated-validation path (fallback + fit).
        crate::config::Config::validate_generated_toml(&content).unwrap();
    }

    #[test]
    fn allowed_paths_use_descendant_glob_that_matches_files() {
        // A bare directory allow-list entry matches only the directory itself
        // and DENIES every file inside it — the wizard must emit `<dir>/**`.
        let dir = tempfile::tempdir().unwrap();
        let entry = super::dir_allow_entry(dir.path());
        assert!(entry.ends_with("/**"), "must be a descendant glob: {entry}");

        let canonical_dir = dir.path().canonicalize().unwrap();
        let child = canonical_dir.join("src").join("main.rs");
        let child_str = child.to_string_lossy().to_string();

        let config = crate::config::SafetyConfig {
            allowed_paths: vec![entry],
            ..Default::default()
        };
        let validator =
            crate::safety::path_validator::PathValidator::new(&config, dir.path().to_path_buf());
        assert!(
            validator
                .is_path_in_allowed_list(&child_str, "src/main.rs")
                .unwrap(),
            "<dir>/** must match files inside the chosen project"
        );
        assert!(
            validator
                .is_path_in_allowed_list(&canonical_dir.to_string_lossy(), ".")
                .unwrap(),
            "<dir>/** must also match the chosen directory itself"
        );

        // Regression guard: the old bare-dir form must NOT match children.
        let bare = crate::config::SafetyConfig {
            allowed_paths: vec![dir.path().to_string_lossy().to_string()],
            ..Default::default()
        };
        let bare_validator =
            crate::safety::path_validator::PathValidator::new(&bare, dir.path().to_path_buf());
        assert!(
            !bare_validator
                .is_path_in_allowed_list(&child_str, "src/main.rs")
                .unwrap(),
            "bare dir must not match children (this was the onboarding bug)"
        );
    }

    #[test]
    fn custom_allow_entry_appends_glob_but_keeps_existing_globs() {
        assert_eq!(super::custom_allow_entry("/tmp/proj"), "/tmp/proj/**");
        assert_eq!(super::custom_allow_entry("/tmp/proj/"), "/tmp/proj/**");
        assert_eq!(super::custom_allow_entry(" /data "), "/data/**");
        // Already-globbed entries pass through untouched.
        assert_eq!(super::custom_allow_entry("/tmp/**"), "/tmp/**");
        assert_eq!(super::custom_allow_entry("./**"), "./**");
        assert_eq!(super::custom_allow_entry("~/**"), "~/**");
    }

    #[test]
    fn windows_style_paths_are_toml_escaped() {
        // Backslashes in a basic TOML string form invalid escapes (\U, \s);
        // the generated config must quote them so it parses on Windows.
        let entry = super::custom_allow_entry(r"C:\Users\ada\project");
        assert_eq!(entry, r"C:\Users\ada\project/**");

        let array = super::allowed_paths_toml([entry.clone()]);
        let doc: toml::Value = toml::from_str(&format!("allowed_paths = {array}"))
            .expect("escaped array must be valid TOML");
        let decoded = doc
            .get("allowed_paths")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(decoded, entry, "TOML round-trip must preserve the path");

        // Sanity: WITHOUT escaping the same body is invalid TOML (\U escape).
        assert!(toml::from_str::<toml::Value>(&format!("allowed_paths = [\"{entry}\"]")).is_err());

        // The full generated config with such a path must pass validation.
        let content = build_config_content("http://127.0.0.1:1234/v1", "qwen3-coder", &array);
        crate::config::Config::validate_generated_toml(&content).unwrap();
    }

    #[test]
    fn quotes_in_paths_are_toml_escaped() {
        let array = super::allowed_paths_toml([r#"weird"path"#.to_string()]);
        let doc: toml::Value = toml::from_str(&format!("allowed_paths = {array}")).unwrap();
        let decoded = doc
            .get("allowed_paths")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(decoded, r#"weird"path"#);
    }

    #[test]
    fn non_terminal_stdin_is_refused() {
        // Piped/EOF stdin must not silently write an all-defaults config.
        assert!(super::ensure_interactive_stdin(true).is_ok());
        let err = super::ensure_interactive_stdin(false).unwrap_err();
        assert!(
            err.to_string().contains("interactive"),
            "error should explain the wizard needs a terminal: {err}"
        );
    }

    #[test]
    fn generated_config_has_no_live_execution_mode_key() {
        let content = build_config_content("http://localhost:8000/v1", "mock", "[\"./**\"]");
        // A live `execution_mode = "..."` key would be silently ignored
        // (#[serde(skip)]) and mislead the user — it must not be emitted.
        assert!(
            !content.contains("execution_mode ="),
            "wizard must not write a live execution_mode key: {content}"
        );
        // The endpoint/model still land, and the mode guidance comment is present.
        assert!(content.contains("http://localhost:8000/v1"));
        assert!(content.contains("model = \"mock\""));
        assert!(content.contains("selfware -y"));
    }

    #[test]
    fn generated_config_has_no_api_key() {
        // The API key is stored in the OS keyring, never written to the config
        // TOML — verify the generated body has no api_key field.
        let content = build_config_content("https://openrouter.ai/api/v1", "gpt-4", "[\".\"]");
        assert!(
            !content.contains("api_key"),
            "config must never contain the api key: {content}"
        );
    }

    #[test]
    fn generated_config_passes_loader_validation() {
        // P0-7: the wizard must never emit a config selfware rejects. Every
        // endpoint/model variant the wizard offers goes through the same
        // validation the loader applies on disk reads.
        for (endpoint, model) in [
            ("http://127.0.0.1:1234/v1", "qwen3-coder"),
            ("https://api.openai.com/v1", "gpt-4"),
            ("https://openrouter.ai/api/v1", "z-ai/glm-5.2"),
        ] {
            let content = build_config_content(endpoint, model, "[\".\"]");
            crate::config::Config::validate_generated_toml(&content).unwrap_or_else(|e| {
                panic!("generated config for {} must validate: {}", endpoint, e)
            });
        }
    }

    #[test]
    fn probe_endpoint_tcp_detects_listener_and_dead_port() {
        // A bound listener is reachable; a closed loopback port is not.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(super::probe_endpoint_tcp(
            &format!("http://127.0.0.1:{}/v1", port),
            std::time::Duration::from_millis(500)
        ));
        // Port 1 on loopback is (virtually) always closed → fast `false`.
        assert!(!super::probe_endpoint_tcp(
            "http://127.0.0.1:1/v1",
            std::time::Duration::from_millis(200)
        ));
        // Unparseable endpoint → false, never a panic.
        assert!(!super::probe_endpoint_tcp(
            "not a url",
            std::time::Duration::from_millis(50)
        ));
    }
}
