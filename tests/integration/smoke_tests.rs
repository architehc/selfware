//! Smoke tests — quick validation that selfware works end-to-end on a live endpoint.
//!
//! These tests verify core agent capabilities against the actual model:
//! - Simple Q&A (no tools needed)
//! - File operations (create, read, edit)
//! - Project scaffolding (multi-step)
//! - Code generation + verification
//! - Context tools
//!
//! Run with:
//!   cargo test --features integration --test integration smoke_ -- --ignored
//!
//! Requires vLLM running on localhost:8000 with qwen3.5-27b.

use std::path::Path;
use tempfile::TempDir;

async fn smoke_config(work_dir: &Path) -> Option<selfware::config::Config> {
    let endpoint = std::env::var("SELFWARE_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8000/v1".to_string());
    let model = std::env::var("SELFWARE_MODEL").unwrap_or_else(|_| "qwen3.5-27b".to_string());

    // Quick check: is the endpoint up and serving our model?
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;
    let resp = client
        .get(format!("{}/models", endpoint))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    if !body.contains(&model) {
        eprintln!("Model '{}' not found at {}", model, endpoint);
        return None;
    }

    Some(selfware::config::Config {
        endpoint,
        model,
        max_tokens: 65536,
        temperature: 0.7,
        agent: selfware::config::AgentConfig {
            max_iterations: 15,
            step_timeout_secs: 120,
            stream_stall_timeout_secs: None,
            token_budget: 65536,
            streaming: true,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        safety: selfware::config::SafetyConfig {
            allowed_paths: vec![format!("{}/**", work_dir.display()), "./**".to_string()],
            ..Default::default()
        },
        execution_mode: selfware::config::ExecutionMode::Yolo,
        ..Default::default()
    })
}

// ─── Simple Q&A (no tools) ──────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_simple_qa() {
    let tmp = TempDir::new().unwrap();
    let config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let result = agent
        .run_task("What is 2 + 2? Just answer with the number.")
        .await;

    assert!(
        result.is_ok(),
        "Simple Q&A should complete: {:?}",
        result.err()
    );
}

// ─── File Creation ──────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_create_file() {
    let tmp = TempDir::new().unwrap();
    let mut config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };
    // Allow writing to tmp dir
    config.safety.allowed_paths = vec![format!("{}/**", tmp.path().display()), "./**".to_string()];

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let task = format!(
        "Create a file at {}/hello.txt with the content 'Hello from selfware!'",
        tmp.path().display()
    );
    let result = agent.run_task(&task).await;

    assert!(
        result.is_ok(),
        "File creation should complete: {:?}",
        result.err()
    );

    let file_path = tmp.path().join("hello.txt");
    if file_path.exists() {
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(
            content.contains("Hello") || content.contains("selfware"),
            "File should contain expected content, got: {}",
            content
        );
        println!("PASS: File created with content: {}", content.trim());
    } else {
        println!("WARN: File not created at expected path (model may have used different path)");
    }
}

// ─── Read Existing File ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_read_file() {
    let tmp = TempDir::new().unwrap();
    // Pre-create a file for the model to read
    let test_file = tmp.path().join("data.txt");
    std::fs::write(&test_file, "The secret number is 42.").unwrap();

    let config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let task = format!(
        "Read the file at {} and tell me what the secret number is.",
        test_file.display()
    );
    let result = agent.run_task(&task).await;

    assert!(
        result.is_ok(),
        "File reading should complete: {:?}",
        result.err()
    );
    println!("PASS: Read file task completed");
}

// ─── Rust Project Scaffolding ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_create_rust_project() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("myapp");

    let mut config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };
    config.safety.allowed_paths = vec![format!("{}/**", tmp.path().display()), "./**".to_string()];
    config.agent.max_iterations = 20; // More room for multi-step

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let task = format!(
        "Create a new Rust project at {} using 'cargo init'. \
         Then add a function `fn greet(name: &str) -> String` that returns \
         \"Hello, {{name}}!\" and add a test for it. \
         Run cargo check to verify it compiles.",
        project_dir.display()
    );
    let result = agent.run_task(&task).await;

    // Check if project was created
    let cargo_toml = project_dir.join("Cargo.toml");
    let main_rs = project_dir.join("src/main.rs");

    if cargo_toml.exists() {
        println!("PASS: Cargo.toml created");
    } else {
        println!("WARN: Cargo.toml not found at {}", cargo_toml.display());
    }

    if main_rs.exists() {
        let content = std::fs::read_to_string(&main_rs).unwrap();
        if content.contains("greet") {
            println!("PASS: greet function found in main.rs");
        } else {
            println!("WARN: greet function not found in main.rs");
        }
        if content.contains("#[test]") || content.contains("mod tests") {
            println!("PASS: Tests found in main.rs");
        }
    }

    match result {
        Ok(()) => println!("PASS: Rust project scaffolding completed"),
        Err(e) => println!("PARTIAL: Task ended with error (may be partial): {}", e),
    }
}

// ─── Python Script Generation ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_create_python_script() {
    let tmp = TempDir::new().unwrap();

    let mut config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };
    config.safety.allowed_paths = vec![format!("{}/**", tmp.path().display()), "./**".to_string()];

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let task = format!(
        "Create a Python script at {}/fibonacci.py that:\n\
         1. Defines a function fibonacci(n) that returns the nth fibonacci number\n\
         2. Has a main block that prints fibonacci(10)\n\
         3. Includes a docstring",
        tmp.path().display()
    );
    let result = agent.run_task(&task).await;

    let script = tmp.path().join("fibonacci.py");
    if script.exists() {
        let content = std::fs::read_to_string(&script).unwrap();
        assert!(
            content.contains("fibonacci"),
            "Should contain fibonacci function"
        );
        assert!(
            content.contains("def "),
            "Should contain a function definition"
        );
        println!("PASS: Python script created ({} bytes)", content.len());
    } else {
        println!("WARN: Script not created at expected path");
    }

    match result {
        Ok(()) => println!("PASS: Python script generation completed"),
        Err(e) => println!("PARTIAL: {}", e),
    }
}

// ─── Multi-File Edit ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_multi_file_edit() {
    let tmp = TempDir::new().unwrap();

    // Pre-create files for the model to edit
    let config_file = tmp.path().join("config.json");
    std::fs::write(
        &config_file,
        r#"{"name": "myapp", "version": "1.0.0", "debug": true}"#,
    )
    .unwrap();

    let readme = tmp.path().join("README.md");
    std::fs::write(&readme, "# MyApp\n\nA simple app.\n").unwrap();

    let mut config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };
    config.safety.allowed_paths = vec![format!("{}/**", tmp.path().display()), "./**".to_string()];

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let task = format!(
        "In the directory {}, do these changes:\n\
         1. Read config.json and change \"debug\": true to \"debug\": false\n\
         2. Update README.md to add a '## Configuration' section that mentions the debug flag\n\
         Then verify both files look correct by reading them.",
        tmp.path().display()
    );
    let result = agent.run_task(&task).await;

    // Check config.json was modified
    if config_file.exists() {
        let content = std::fs::read_to_string(&config_file).unwrap();
        if content.contains("false") {
            println!("PASS: config.json debug flag updated");
        } else {
            println!("WARN: config.json not updated as expected: {}", content);
        }
    }

    // Check README was modified
    if readme.exists() {
        let content = std::fs::read_to_string(&readme).unwrap();
        if content.len() > 30 {
            println!("PASS: README.md expanded ({} bytes)", content.len());
        } else {
            println!("WARN: README.md not significantly changed");
        }
    }

    match result {
        Ok(()) => println!("PASS: Multi-file edit completed"),
        Err(e) => println!("PARTIAL: {}", e),
    }
}

// ─── Search and Analyze ─────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_search_codebase() {
    let tmp = TempDir::new().unwrap();
    let config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    // This task operates on the selfware codebase itself
    let result = agent
        .run_task("Search for 'strip_think_blocks' in the codebase and tell me which files contain it and what it does.")
        .await;

    assert!(
        result.is_ok(),
        "Search task should complete: {:?}",
        result.err()
    );
    println!("PASS: Search and analyze completed");
}

// ─── Shell Command Execution ────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_shell_command() {
    let tmp = TempDir::new().unwrap();
    let config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let result = agent
        .run_task("Run 'rustc --version' and 'cargo --version' and tell me the versions.")
        .await;

    assert!(
        result.is_ok(),
        "Shell command should complete: {:?}",
        result.err()
    );
    println!("PASS: Shell command execution completed");
}

// ─── Context Status Tool ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_context_status() {
    let tmp = TempDir::new().unwrap();
    let config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let result = agent
        .run_task("Use context_status to check your context window budget and report the numbers.")
        .await;

    match result {
        Ok(()) => println!("PASS: Context status tool worked"),
        Err(e) => println!("PARTIAL: Context status task: {}", e),
    }
}

// ─── Cargo Check on Selfware ────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live endpoint"]
async fn smoke_cargo_check_selfware() {
    let tmp = TempDir::new().unwrap();
    let config = match smoke_config(tmp.path()).await {
        Some(c) => c,
        None => {
            eprintln!("SKIPPED: endpoint not available");
            return;
        }
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let result = agent
        .run_task("Run cargo check on this project and report if it compiles successfully.")
        .await;

    assert!(
        result.is_ok(),
        "Cargo check should complete: {:?}",
        result.err()
    );
    println!("PASS: Cargo check task completed");
}
