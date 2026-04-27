//! E2E test for all tools on a test project

use once_cell::sync::Lazy;
use selfware::api::types::{ToolCall, ToolFunction};
use selfware::safety::SafetyChecker;
use selfware::tools::ToolRegistry;
use std::env;
use std::fs;
use std::path::Path;
use tempfile::{tempdir, NamedTempFile};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const PNG_1X1_RED: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

static BROWSER_ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

const BROWSER_ENV_KEYS: &[&str] = &[
    "SELFWARE_BROWSER_NO_SANDBOX",
    "SELFWARE_PLAYWRIGHT_NODE_PATH",
    "SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH",
    "SELFWARE_CHROME_EXECUTABLE_PATH",
    "SELFWARE_WORKSPACE_ROOT",
];

const CHROME_CANDIDATES: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
];

fn make_tool_call(name: &str, arguments: String) -> ToolCall {
    ToolCall {
        id: "test".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: name.to_string(),
            arguments,
        },
    }
}

async fn spawn_static_response_server(
    body: String,
    content_type: &'static str,
) -> (tokio::task::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = [0_u8; 2048];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    content_type,
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    (handle, format!("http://127.0.0.1:{}", addr.port()))
}

struct EnvRestore {
    prior: Vec<(&'static str, Option<String>)>,
}

impl EnvRestore {
    fn capture(keys: &[&'static str]) -> Self {
        let prior = keys.iter().map(|key| (*key, env::var(key).ok())).collect();
        Self { prior }
    }

    fn set_var(&mut self, key: &'static str, value: impl Into<String>) {
        env::set_var(key, value.into());
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, value) in self.prior.drain(..) {
            if let Some(value) = value {
                env::set_var(key, value);
            } else {
                env::remove_var(key);
            }
        }
    }
}

fn find_chrome_executable() -> Option<String> {
    for candidate in CHROME_CANDIDATES {
        if Path::new(candidate).exists() {
            return Some((*candidate).to_string());
        }
        let Ok(output) = std::process::Command::new(candidate)
            .arg("--version")
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        if let Ok(which_output) = std::process::Command::new("which").arg(candidate).output() {
            if which_output.status.success() {
                let resolved = String::from_utf8_lossy(&which_output.stdout)
                    .trim()
                    .to_string();
                if !resolved.is_empty() && Path::new(&resolved).exists() {
                    return Some(resolved);
                }
            }
        }
    }

    None
}

fn playwright_runtime_ready() -> bool {
    let mut cmd = std::process::Command::new("node");
    if let Ok(node_path) = env::var("SELFWARE_PLAYWRIGHT_NODE_PATH") {
        let merged = match env::var("NODE_PATH") {
            Ok(existing) if !existing.is_empty() => format!("{}:{}", node_path, existing),
            _ => node_path,
        };
        cmd.env("NODE_PATH", merged);
    }
    cmd.args([
        "-e",
        "try { require('playwright'); process.exit(0); } catch (_) { try { require('playwright-core'); process.exit(0); } catch (_) { process.exit(1); } }",
    ]);
    cmd.status().map(|status| status.success()).unwrap_or(false)
}

fn is_missing_browser_runtime_message(message: &str) -> bool {
    message.contains("No browser automation tool found")
        || message.contains("Playwright runtime unavailable")
        || message.contains("Playwright not installed")
        || message.contains("requires Chrome or Playwright")
        || message.contains("requires Chrome/Chromium")
        || message.contains("Failed to spawn playwright-bridge")
}

fn maybe_skip_browser_error(error: anyhow::Error) -> Option<()> {
    if is_missing_browser_runtime_message(&error.to_string()) {
        eprintln!("skipping browser E2E test: {}", error);
        Some(())
    } else {
        None
    }
}

fn maybe_skip_page_control_result(result: &serde_json::Value) -> bool {
    result["success"].as_bool() == Some(false)
        && result["error"]
            .as_str()
            .is_some_and(is_missing_browser_runtime_message)
}

#[tokio::test]
async fn test_e2e_file_tools() {
    let cfg = selfware::config::SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..Default::default()
    };
    selfware::tools::file::init_safety_config(&cfg);
    selfware::tools::file_read::init_safety_config(&cfg);
    let dir = tempdir().unwrap();
    let test_file = dir.path().join("test.rs");

    // Create test file
    fs::write(
        &test_file,
        r#"
fn hello() -> &'static str {
    "Hello, World!"
}

fn main() {
    println!("{}", hello());
}
"#,
    )
    .unwrap();

    let registry = ToolRegistry::new();

    // Test FileRead
    let file_read = registry.get("file_read").unwrap();
    let result = file_read
        .execute(serde_json::json!({
            "path": test_file.to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(result["content"]
        .as_str()
        .unwrap()
        .contains("Hello, World!"));
    println!("✓ FileRead works");

    // Test FileEdit
    let file_edit = registry.get("file_edit").unwrap();
    let result = file_edit
        .execute(serde_json::json!({
            "path": test_file.to_str().unwrap(),
            "old_str": "Hello, World!",
            "new_str": "Hello, Rust!"
        }))
        .await
        .unwrap();
    assert_eq!(result["success"], true);
    println!("✓ FileEdit works");

    // Verify edit
    let content = fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("Hello, Rust!"));
    println!("✓ FileEdit verified");

    // Test DirectoryTree
    let dir_tree = registry.get("directory_tree").unwrap();
    let result = dir_tree
        .execute(serde_json::json!({
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(result["total"].as_i64().unwrap() >= 1);
    println!("✓ DirectoryTree works");
}

#[tokio::test]
async fn test_e2e_search_tools() {
    let dir = tempdir().unwrap();

    // Create test files
    fs::write(
        dir.path().join("main.rs"),
        r#"
fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

struct Calculator {
    value: i32,
}

fn main() {
    let result = calculate_sum(1, 2);
}
"#,
    )
    .unwrap();

    fs::write(
        dir.path().join("lib.rs"),
        r#"
pub fn helper_function() -> bool {
    true
}
"#,
    )
    .unwrap();

    let registry = ToolRegistry::new();

    // Test GrepSearch
    let grep = registry.get("grep_search").unwrap();
    let result = grep
        .execute(serde_json::json!({
            "pattern": "calculate",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert!(result["count"].as_i64().unwrap() >= 1);
    println!("✓ GrepSearch works - found {} matches", result["count"]);

    // Test GlobFind
    let glob = registry.get("glob_find").unwrap();
    let result = glob
        .execute(serde_json::json!({
            "pattern": "*.rs",
            "path": dir.path().to_str().unwrap()
        }))
        .await
        .unwrap();
    assert_eq!(result["count"], 2);
    println!("✓ GlobFind works - found {} files", result["count"]);

    // Test SymbolSearch
    let symbol = registry.get("symbol_search").unwrap();
    let result = symbol
        .execute(serde_json::json!({
            "name": "calculate",
            "path": dir.path().to_str().unwrap(),
            "symbol_type": "function"
        }))
        .await
        .unwrap();
    assert!(!result["symbols"].as_array().unwrap().is_empty());
    println!(
        "✓ SymbolSearch works - found {} symbols",
        result["symbols"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn test_e2e_cargo_tools() {
    // Use our actual project for cargo tools
    let registry = ToolRegistry::new();

    // Test CargoCheck (just schema, actual run needs cargo project)
    let cargo_check = registry.get("cargo_check").unwrap();
    assert_eq!(cargo_check.name(), "cargo_check");
    println!("✓ CargoCheck registered");

    // Test CargoTest (just schema)
    let cargo_test = registry.get("cargo_test").unwrap();
    assert_eq!(cargo_test.name(), "cargo_test");
    println!("✓ CargoTest registered");

    // Test CargoClippy (just schema)
    let cargo_clippy = registry.get("cargo_clippy").unwrap();
    assert_eq!(cargo_clippy.name(), "cargo_clippy");
    println!("✓ CargoClippy registered");
}

#[tokio::test]
async fn test_e2e_shell_tool() {
    let registry = ToolRegistry::new();

    let shell = registry.get("shell_exec").unwrap();
    let result = shell
        .execute(serde_json::json!({
            "command": "echo 'E2E test successful'",
            "timeout_secs": 5
        }))
        .await
        .unwrap();

    assert_eq!(result["exit_code"], 0);
    assert!(result["stdout"]
        .as_str()
        .unwrap()
        .contains("E2E test successful"));
    println!("✓ ShellExec works");
}

#[tokio::test]
async fn test_e2e_all_tools_registered() {
    let registry = ToolRegistry::new();
    let tools = registry.list();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

    // Expected tools
    let expected = vec![
        "file_read",
        "file_write",
        "file_edit",
        "directory_tree",
        "git_status",
        "git_diff",
        "git_commit",
        "git_checkpoint",
        "cargo_check",
        "cargo_test",
        "cargo_clippy",
        "shell_exec",
        "grep_search",
        "glob_find",
        "symbol_search",
        "http_request",
    ];

    for tool in &expected {
        assert!(tool_names.contains(tool), "Missing tool: {}", tool);
    }

    println!(
        "✓ All {} tools registered: {:?}",
        tool_names.len(),
        tool_names
    );
}

#[test]
fn test_e2e_safety_allows_local_page_control_targets() {
    let checker = SafetyChecker::new(&selfware::config::SafetyConfig::default());

    let workspace_file = NamedTempFile::new_in(std::env::current_dir().unwrap()).unwrap();
    fs::write(workspace_file.path(), "<html><body>ok</body></html>").unwrap();
    let file_url = format!("file://{}", workspace_file.path().display());

    let file_call = make_tool_call(
        "page_control",
        format!(r#"{{"action":"goto","url":"{}"}}"#, file_url),
    );
    assert!(checker.check_tool_call(&file_call).is_ok());

    let localhost_call = make_tool_call(
        "page_control",
        r#"{"action":"goto","url":"http://localhost:8888/chart.html"}"#.to_string(),
    );
    assert!(checker.check_tool_call(&localhost_call).is_ok());
}

#[test]
fn test_e2e_safety_allows_local_http_and_browser_targets() {
    let checker = SafetyChecker::new(&selfware::config::SafetyConfig::default());

    let http_call = make_tool_call(
        "http_request",
        r#"{"url":"http://localhost:8888/health"}"#.to_string(),
    );
    assert!(checker.check_tool_call(&http_call).is_ok());

    let browser_call = make_tool_call(
        "browser_fetch",
        r#"{"url":"http://127.0.0.1:8888/chart.html"}"#.to_string(),
    );
    assert!(checker.check_tool_call(&browser_call).is_ok());
}

#[test]
fn test_e2e_safety_blocks_untrusted_local_artifacts() {
    let checker = SafetyChecker::new(&selfware::config::SafetyConfig::default());

    let outside = tempdir().unwrap();
    let outside_file = outside.path().join("secret.html");
    fs::write(&outside_file, "<html><body>blocked</body></html>").unwrap();
    let outside_call = make_tool_call(
        "page_control",
        format!(
            r#"{{"action":"goto","url":"file://{}"}}"#,
            outside_file.display()
        ),
    );
    assert!(checker.check_tool_call(&outside_call).is_err());

    let private_vision = make_tool_call(
        "vision_analyze",
        r#"{"endpoint":"http://192.168.1.170:1234/v1","prompt":"test","model":"vlm"}"#.to_string(),
    );
    assert!(checker.check_tool_call(&private_vision).is_err());
}

#[tokio::test]
async fn test_e2e_localhost_http_and_browser_fetch_round_trip() {
    let body = "<html><body><h1>local chart</h1><p>browser smoke</p></body></html>".to_string();
    let (server, base_url) = spawn_static_response_server(body, "text/html").await;
    let url = format!("{}/chart.html", base_url);

    let registry = ToolRegistry::new();

    let http_request = registry.get("http_request").unwrap();
    let http_result = http_request
        .execute(serde_json::json!({
            "url": url,
            "timeout_secs": 5
        }))
        .await
        .unwrap();
    assert_eq!(http_result["status"].as_u64(), Some(200));
    assert!(http_result["body"]
        .as_str()
        .unwrap_or_default()
        .contains("local chart"));

    let browser_fetch = registry.get("browser_fetch").unwrap();
    let browser_result = browser_fetch
        .execute(serde_json::json!({
            "url": format!("{}/chart.html", base_url),
            "timeout_secs": 5
        }))
        .await
        .unwrap();
    assert_eq!(browser_result["success"].as_bool(), Some(true));
    assert!(browser_result["text"]
        .as_str()
        .unwrap_or_default()
        .contains("local chart"));

    server.abort();
}

#[tokio::test]
async fn test_e2e_browser_screenshot_round_trip() {
    let _env_lock = BROWSER_ENV_LOCK.lock().await;
    let mut env_restore = EnvRestore::capture(BROWSER_ENV_KEYS);
    env_restore.set_var("SELFWARE_BROWSER_NO_SANDBOX", "1");
    if let Some(chrome_path) = find_chrome_executable() {
        env_restore.set_var("SELFWARE_CHROME_EXECUTABLE_PATH", chrome_path);
    }

    let body = "<html><head><title>chart shot</title></head><body><main style='width:100%;height:100%;background:#f5e7d8'><h1>chart shot</h1></main></body></html>".to_string();
    let (server, base_url) = spawn_static_response_server(body, "text/html").await;
    let url = format!("{}/chart.html", base_url);
    let output_dir = tempdir().unwrap();
    let output_path = output_dir.path().join("chart-shot.png");

    let registry = ToolRegistry::new();
    let screenshot = registry.get("browser_screenshot").unwrap();
    let result = screenshot
        .execute(serde_json::json!({
            "url": url,
            "output_path": output_path.to_str().unwrap(),
            "width": 1280,
            "height": 720,
            "timeout_secs": 10
        }))
        .await;

    let result = match result {
        Ok(result) => result,
        Err(err) => {
            server.abort();
            if maybe_skip_browser_error(err).is_some() {
                return;
            }
            panic!("browser_screenshot failed");
        }
    };

    assert_eq!(result["success"].as_bool(), Some(true));
    assert_eq!(result["file_exists"].as_bool(), Some(true));
    assert!(fs::metadata(&output_path).unwrap().len() > 0);

    server.abort();
}

#[tokio::test]
async fn test_e2e_browser_pdf_round_trip() {
    let _env_lock = BROWSER_ENV_LOCK.lock().await;
    let mut env_restore = EnvRestore::capture(BROWSER_ENV_KEYS);
    env_restore.set_var("SELFWARE_BROWSER_NO_SANDBOX", "1");
    if let Some(chrome_path) = find_chrome_executable() {
        env_restore.set_var("SELFWARE_CHROME_EXECUTABLE_PATH", &chrome_path);
        env_restore.set_var("SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH", &chrome_path);
    }

    let body = "<html><head><title>chart pdf</title></head><body><article><h1>chart pdf</h1><p>pdf smoke</p></article></body></html>".to_string();
    let (server, base_url) = spawn_static_response_server(body, "text/html").await;
    let url = format!("{}/chart.html", base_url);
    let output_dir = tempdir().unwrap();
    let output_path = output_dir.path().join("chart.pdf");

    let registry = ToolRegistry::new();
    let pdf = registry.get("browser_pdf").unwrap();
    let result = pdf
        .execute(serde_json::json!({
            "url": url,
            "output_path": output_path.to_str().unwrap(),
            "timeout_secs": 10
        }))
        .await;

    let result = match result {
        Ok(result) => result,
        Err(err) => {
            server.abort();
            if maybe_skip_browser_error(err).is_some() {
                return;
            }
            panic!("browser_pdf failed");
        }
    };

    assert_eq!(result["success"].as_bool(), Some(true));
    assert_eq!(result["file_exists"].as_bool(), Some(true));
    assert!(fs::metadata(&output_path).unwrap().len() > 0);

    server.abort();
}

#[tokio::test]
async fn test_e2e_browser_eval_round_trip() {
    let _env_lock = BROWSER_ENV_LOCK.lock().await;
    let Some(chrome_path) = find_chrome_executable() else {
        eprintln!("skipping browser_eval E2E test: Chrome not available");
        return;
    };
    if !playwright_runtime_ready() {
        eprintln!("skipping browser_eval E2E test: Playwright runtime unavailable");
        return;
    }
    let mut env_restore = EnvRestore::capture(BROWSER_ENV_KEYS);
    env_restore.set_var("SELFWARE_BROWSER_NO_SANDBOX", "1");
    env_restore.set_var("SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH", chrome_path);

    let body =
        "<html><head><title>chart eval</title></head><body><div id='value'>42</div></body></html>"
            .to_string();
    let (server, base_url) = spawn_static_response_server(body, "text/html").await;
    let url = format!("{}/chart.html", base_url);

    let registry = ToolRegistry::new();
    let browser_eval = registry.get("browser_eval").unwrap();
    let result = browser_eval
        .execute(serde_json::json!({
            "url": url,
            "script": "return document.querySelector('#value').textContent;",
            "timeout_secs": 10
        }))
        .await
        .unwrap();

    assert_eq!(result["success"].as_bool(), Some(true));
    assert_eq!(result["browser"].as_str(), Some("playwright"));
    assert_eq!(result["result"].as_str(), Some("42"));

    server.abort();
}

#[tokio::test]
async fn test_e2e_page_control_local_file_round_trip() {
    let _env_lock = BROWSER_ENV_LOCK.lock().await;
    let Some(chrome_path) = find_chrome_executable() else {
        eprintln!("skipping page_control E2E test: Chrome not available");
        return;
    };
    if !playwright_runtime_ready() {
        eprintln!("skipping page_control E2E test: Playwright runtime unavailable");
        return;
    }
    let mut env_restore = EnvRestore::capture(BROWSER_ENV_KEYS);
    env_restore.set_var("SELFWARE_BROWSER_NO_SANDBOX", "1");
    env_restore.set_var("SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH", chrome_path);
    env_restore.set_var(
        "SELFWARE_WORKSPACE_ROOT",
        std::env::current_dir().unwrap().display().to_string(),
    );

    let html = "<html><head><title>Chart Control</title></head><body><main><h1 id='headline'>Chart Control</h1><p>page control smoke</p></main></body></html>";
    let workspace_dir = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let workspace_file = workspace_dir.path().join("chart-control.html");
    fs::write(&workspace_file, html).unwrap();
    let file_url = format!("file://{}", workspace_file.display());
    let output_dir = tempdir().unwrap();
    let screenshot_path = output_dir.path().join("page-control.png");

    let registry = ToolRegistry::new();
    let page_control = registry.get("page_control").unwrap();

    let goto = page_control
        .execute(serde_json::json!({
            "action": "goto",
            "url": file_url,
            "timeout_ms": 10_000
        }))
        .await
        .unwrap();
    if maybe_skip_page_control_result(&goto) {
        return;
    }
    assert_eq!(goto["success"].as_bool(), Some(true));

    let text = page_control
        .execute(serde_json::json!({
            "action": "text",
            "selector": "#headline"
        }))
        .await
        .unwrap();
    assert_eq!(text["success"].as_bool(), Some(true));
    assert_eq!(text["result"]["text"].as_str(), Some("Chart Control"));

    let current_url = page_control
        .execute(serde_json::json!({
            "action": "url"
        }))
        .await
        .unwrap();
    assert_eq!(current_url["success"].as_bool(), Some(true));
    assert!(current_url["result"]["url"]
        .as_str()
        .unwrap_or_default()
        .starts_with("file://"));

    let screenshot = page_control
        .execute(serde_json::json!({
            "action": "screenshot",
            "path": screenshot_path.to_str().unwrap()
        }))
        .await
        .unwrap();
    assert_eq!(screenshot["success"].as_bool(), Some(true));
    assert!(fs::metadata(&screenshot_path).unwrap().len() > 0);

    let shutdown = page_control
        .execute(serde_json::json!({
            "action": "shutdown"
        }))
        .await
        .unwrap();
    assert_eq!(shutdown["success"].as_bool(), Some(true));
}

#[tokio::test]
async fn test_e2e_mock_vision_analyze_round_trip() {
    let response = serde_json::json!({
        "choices": [{
            "message": {
                "content": "mock visual analysis"
            }
        }],
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 4,
            "total_tokens": 16
        }
    })
    .to_string();
    let (server, endpoint) = spawn_static_response_server(response, "application/json").await;

    let dir = tempdir().unwrap();
    let image_path = dir.path().join("pixel.png");
    fs::write(&image_path, PNG_1X1_RED).unwrap();

    let registry = ToolRegistry::new();
    let vision = registry.get("vision_analyze").unwrap();
    let result = vision
        .execute(serde_json::json!({
            "image_path": image_path.to_str().unwrap(),
            "prompt": "Describe this image",
            "endpoint": format!("{}/v1", endpoint),
            "model": "mock-vision"
        }))
        .await
        .unwrap();

    assert_eq!(result["success"].as_bool(), Some(true));
    assert_eq!(result["analysis"].as_str(), Some("mock visual analysis"));

    server.abort();
}
