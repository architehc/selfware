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
    use std::io::{self, BufRead, Write};
    use std::path::PathBuf;

    // If a template is provided, skip the interactive wizard
    if let Some(ref tmpl) = template {
        return write_template_config(tmpl);
    }

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
            format!("[\"{}\"]", home.display())
        }
        "3" => {
            print!("  Enter paths (comma-separated): ");
            io::stdout().flush()?;
            let mut paths = String::new();
            io::stdin().lock().read_line(&mut paths)?;
            let paths: Vec<String> = paths
                .trim()
                .split(',')
                .map(|p| format!("\"{}\"", p.trim()))
                .collect();
            format!("[{}]", paths.join(", "))
        }
        _ => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            format!("[\"{}\"]", cwd.display())
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
            format!("[\"{}\"]", cwd.display()),
        ),
        _ => (
            "http://127.0.0.1:1234/v1".to_string(),
            "qwen3-coder".to_string(),
            "normal",
            "[\".\"]".to_string(),
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

# Execution mode is chosen per RUN, not stored here — a config file (this one
# lives in the repo) must never be able to silently enable auto-approval.
# Start Selfware in your chosen mode with a flag or env var:
#   Normal (ask first): selfware
#   AutoEdit:           selfware -m auto-edit   (or SELFWARE_MODE=auto-edit)
#   YOLO (auto all):    selfware -y             (or SELFWARE_MODE=yolo)

[safety]
allowed_paths = {}

[agent]
# token_budget defaults to max_tokens — set explicitly to match your model's context window
# token_budget = 131072
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
