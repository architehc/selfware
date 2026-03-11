//! E2E test for all tools on a test project

use selfware::api::types::{ToolCall, ToolFunction};
use selfware::safety::SafetyChecker;
use selfware::tools::ToolRegistry;
use std::fs;
use tempfile::{tempdir, NamedTempFile};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PNG_1X1_RED: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
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

#[tokio::test]
async fn test_e2e_file_tools() {
    let cfg = selfware::config::SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..Default::default()
    };
    selfware::tools::file::init_safety_config(&cfg);
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
