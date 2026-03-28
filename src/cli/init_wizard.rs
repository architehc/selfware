//! Interactive setup wizard and template-based config generation.

use anyhow::Result;

use crate::ui::style::Glyphs;

pub(crate) fn run_init_wizard(template: Option<String>) -> Result<()> {
    use std::io::{self, BufRead, Write};
    use std::path::PathBuf;

    // If a template is provided, skip the interactive wizard
    if let Some(ref tmpl) = template {
        return write_template_config(tmpl);
    }

    println!();
    println!(
        "{} Welcome to Selfware! Let's set up your workspace.",
        Glyphs::seedling()
    );
    println!();

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
    println!("  Detecting project type... Found: {}", project_type);
    println!();

    // Step 1: Endpoint
    println!("Step 1/4: API Endpoint");
    println!("  Where should Selfware connect?");
    println!("  [1] Local (http://localhost:8080/v1) - Ollama, vLLM, LM Studio");
    println!("  [2] OpenAI-compatible API (https://api.openai.com/v1)");
    println!("  [3] Custom endpoint");
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
        _ => "http://localhost:8080/v1".to_string(),
    };
    println!();

    // Step 2: Model
    println!("Step 2/4: Model");
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
    println!();

    // Step 3: Allowed paths
    println!("Step 3/4: Allowed Paths");
    println!("  Which directories can Selfware access?");
    println!("  [1] Current directory only (.)");
    println!("  [2] Home directory (~)");
    println!("  [3] Custom paths");
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
    println!();

    // Step 4: Execution mode
    println!("Step 4/4: Execution Mode");
    println!("  How should Selfware handle file changes?");
    println!("  [1] Normal - Ask before every edit (safest)");
    println!("  [2] AutoEdit - Auto-approve file edits, confirm commands");
    println!("  [3] YOLO - Auto-approve everything (use with caution!)");
    print!("  > ");
    io::stdout().flush()?;
    let mut mode_choice = String::new();
    io::stdin().lock().read_line(&mut mode_choice)?;
    let mode = match mode_choice.trim() {
        "2" => "autoedit",
        "3" => "yolo",
        _ => "normal",
    };
    println!();

    // Write config
    write_config_file(&endpoint, &model, mode, &allowed_paths)
}

fn write_template_config(template: &str) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Scaffold project files for known language templates
    match template {
        "rust" | "python" | "node" | "nodejs" | "typescript" => {
            println!("  {} Using '{}' template...", Glyphs::gear(), template);

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
                    println!("  {} Scaffolded {} files:", Glyphs::bloom(), files.len());
                    for f in &files {
                        println!("    {}", f);
                    }
                }
                Err(e) => {
                    println!("  {} Could not scaffold project: {}", Glyphs::frost(), e);
                }
            }
        }
        "minimal" => {
            println!("  {} Using 'minimal' template...", Glyphs::gear());
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
            "http://localhost:8080/v1".to_string(),
            "qwen3-coder".to_string(),
            "normal",
            format!("[\"{}\"]", cwd.display()),
        ),
        _ => (
            "http://localhost:8080/v1".to_string(),
            "qwen3-coder".to_string(),
            "normal",
            "[\".\"]".to_string(),
        ),
    };

    write_config_file(&endpoint, &model, mode, &allowed_paths)
}

fn write_config_file(endpoint: &str, model: &str, mode: &str, allowed_paths: &str) -> Result<()> {
    use std::path::PathBuf;

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("selfware");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.toml");

    // Check if config already exists
    if config_path.exists() {
        use std::io::{self, BufRead, Write};

        println!(
            "  {} Configuration already exists at {}",
            Glyphs::frost(),
            config_path.display()
        );
        print!("  Overwrite? [y/N]: ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("  Aborted. Existing configuration preserved.");
            return Ok(());
        }
    }

    let content = format!(
        r#"# Selfware Configuration
# Generated by `selfware init`

endpoint = "{}"
model = "{}"
execution_mode = "{}"

[safety]
allowed_paths = {}

[agent]
# token_budget defaults to max_tokens — set explicitly to match your model's context window
# token_budget = 131072
"#,
        endpoint, model, mode, allowed_paths
    );

    std::fs::write(&config_path, &content)?;
    println!(
        "  {} Configuration saved to {}",
        Glyphs::bloom(),
        config_path.display()
    );
    println!();
    println!(
        "  {} Run `selfware` to start your workshop!",
        Glyphs::sprout()
    );

    Ok(())
}

