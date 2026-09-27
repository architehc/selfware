//! Output fidelity (0.9.4 live finding on python-slugify): what a tool
//! returns about a file must reach the model as the file's content, byte for
//! byte — in native and text (XML) tool-calling mode alike. A secret-redaction
//! false positive delivered `tokens = text.split(DEFAULT_SEPARATOR)` as
//! `env_token=[REDACTED]` with the newline swallowed, and whole-content XML
//! escaping delivered `&` as `&amp;` and `<` as `&lt;`; the model then wrote
//! both back into the file.
//!
//! Each test runs the real tool on a fixture, pushes its result through the
//! dispatcher's model-facing path, and compares what the model receives.

use super::*;
use crate::tools::Tool;

/// Code-shaped lines every past redaction/escaping bug touched.
const FIXTURE: &str = concat!(
    "import re\n",
    "\n",
    "DEFAULT_SEPARATOR = \"-\"\n",
    "MODERN_HEX_PATTERN = re.compile(r'&#x([\\da-fA-F]+);')\n",
    "LITERAL = \"a &amp; b\"\n",
    "\n",
    "def slugify(text, api_key=None):\n",
    "    tokens = text.split(DEFAULT_SEPARATOR)\n",
    "    token_count = len(tokens)\n",
    "    api_key = get_key()\n",
    "    password_field = form[\"password\"]\n",
    "    secret = compute_hash()\n",
    "    auth_token = request.headers.get(\"Authorization\")\n",
    "    MAX_TOKENS = 4096\n",
    "    if x < y && y > z:\n",
    "        return \"<div>\" + tokens[0] + \"</div>\"\n",
    "    return token_count\n",
);

fn permissive() -> crate::config::SafetyConfig {
    crate::config::SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..crate::config::SafetyConfig::default()
    }
}

async fn agent() -> Agent {
    Agent::new(crate::test_support::mock_agent_config("http://127.0.0.1:1"))
        .await
        .unwrap()
}

/// What the model receives for one successful tool result, in both modes:
/// the native tool message's text and the XML envelope's inner text (NOT
/// decoded — the model reads it as it is).
async fn delivered(tool_name: &str, args: &serde_json::Value, result: &str) -> [String; 2] {
    let mut out = Vec::new();
    for native in [true, false] {
        let mut agent = agent().await;
        agent
            .push_tool_result_message(native, "c1", tool_name, &args.to_string(), true, result)
            .await;
        let text = agent.messages.last().unwrap().content.text().to_string();
        let inner = if native {
            text
        } else {
            text.strip_prefix("<tool_result>")
                .and_then(|t| t.strip_suffix("</tool_result>"))
                .unwrap_or_else(|| panic!("not a bare envelope: {text}"))
                .to_string()
        };
        out.push(inner);
    }
    [out[0].clone(), out[1].clone()]
}

fn write_fixture(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn file_read_content(delivered: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(delivered).expect("valid JSON");
    let content = value["content"].as_str().expect("content field");
    crate::tools::line_numbers::strip_numbered(content).unwrap_or_else(|| content.to_string())
}

#[tokio::test]
async fn file_read_delivers_code_byte_for_byte() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = write_fixture(dir.path(), "slugify.py", FIXTURE);
    let args = serde_json::json!({"path": path.to_str().unwrap()});
    let result = crate::tools::file::FileRead::with_safety_config(permissive())
        .execute(args.clone())
        .await
        .unwrap()
        .to_string();
    for text in delivered("file_read", &args, &result).await {
        assert_eq!(text, result, "the tool result reaches the model unchanged");
        let content = file_read_content(&text);
        assert_eq!(
            content.trim_end_matches('\n'),
            FIXTURE.trim_end_matches('\n')
        );
        assert!(!text.contains("REDACTED") && !text.contains("&amp;amp;"));
    }
}

#[tokio::test]
async fn grep_search_delivers_matching_lines_verbatim() {
    let dir = tempfile::TempDir::new().unwrap();
    write_fixture(dir.path(), "slugify.py", FIXTURE);
    let args = serde_json::json!({"pattern": "token|&|<", "path": dir.path().to_str().unwrap()});
    let result = crate::tools::grep_search::GrepSearch::with_safety_config(permissive())
        .execute(args.clone())
        .await
        .unwrap()
        .to_string();
    assert!(result.contains("tokens = text.split(DEFAULT_SEPARATOR)"));
    for text in delivered("grep_search", &args, &result).await {
        assert_eq!(text, result);
    }
}

#[tokio::test]
async fn shell_exec_delivers_command_output_verbatim() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = write_fixture(dir.path(), "slugify.py", FIXTURE);
    let args = serde_json::json!({"command": format!("cat '{}'", path.display())});
    let result = crate::tools::shell_exec::ShellExec
        .execute(args.clone())
        .await
        .unwrap();
    assert_eq!(result["stdout"].as_str(), Some(FIXTURE));
    let result = result.to_string();
    for text in delivered("shell_exec", &args, &result).await {
        assert_eq!(text, result);
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["stdout"].as_str(), Some(FIXTURE));
    }
}

#[tokio::test]
async fn git_diff_delivers_added_lines_verbatim() {
    let dir = tempfile::TempDir::new().unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    write_fixture(dir.path(), "slugify.py", "");
    git(&["add", "slugify.py"]);
    git(&[
        "-c",
        "user.email=t@x",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "base",
    ]);
    write_fixture(dir.path(), "slugify.py", FIXTURE);
    let args = serde_json::json!({});
    let result = crate::tools::git::GitDiff::new()
        .execute(args.clone())
        .await
        .unwrap()
        .to_string();
    for text in delivered("git_diff", &args, &result).await {
        assert_eq!(text, result);
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let diff = value["diff"].as_str().unwrap_or_default();
        for line in FIXTURE.lines() {
            assert!(diff.contains(&format!("+{line}")), "missing +{line}");
        }
    }
}

#[tokio::test]
async fn spill_file_and_summary_keep_code_verbatim() {
    let dir = tempfile::TempDir::new().unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let raw = serde_json::json!({"content": FIXTURE.repeat(50)}).to_string();
    let summary = summarize_and_spill("file_read", "c1", &raw, 99_999).await;
    assert!(!summary.contains("REDACTED"), "{summary}");
    let spilled = std::fs::read_dir(dir.path().join(TOOL_RESULTS_DIR))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(std::fs::read_to_string(spilled).unwrap(), raw);
}

#[test]
fn sanitize_tool_context_is_identity_on_code() {
    let args = serde_json::json!({"path": "slugify.py"}).to_string();
    for content in [
        FIXTURE.to_string(),
        serde_json::json!({"content": FIXTURE}).to_string(),
        crate::tools::line_numbers::number_lines(FIXTURE, 1),
    ] {
        let gate = sanitize_tool_context("file_read", &args, &content, true);
        assert_eq!(gate.content, content);
    }
}

/// A real-secret fixture IS redacted: each value becomes a visible
/// `[REDACTED:<kind>]` marker on its own line, the key and every other line
/// survive, the line count is unchanged, and a note says what happened.
#[tokio::test]
async fn real_secrets_are_redacted_line_preserving_with_visible_markers() {
    let openai = ["sk", "proj", "Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0"].join("-");
    let github = ["ghp", "A1b2C3d4E5f6G7h8I9j0"].join("_");
    let env = format!(
        "DEBUG=true\n\
         OPENAI_API_KEY={openai}\n\
         DB_PASSWORD=hunter2secret\n\
         password = \"H9vz3E8Kq5X2\"\n\
         GITHUB_TOKEN={github}\n\
         -----BEGIN RSA PRIVATE KEY-----\n\
         MIIBOgIBAAJBAKj34GkxFhD90vcNLYLInFEX6Ppy1tPf9Cnzj4p4WGeKLs1Pt8Qu\n\
         KUpRKfFLfRYC9AIKjbJTWit+CqvjWYzvQwECAwEAAQ==\n\
         -----END RSA PRIVATE KEY-----\n\
         tokens = text.split(SEP)\n"
    );
    let dir = tempfile::TempDir::new().unwrap();
    let path = write_fixture(dir.path(), "fixture.conf", &env);
    let args = serde_json::json!({"path": path.to_str().unwrap()});
    let result = crate::tools::file::FileRead::with_safety_config(permissive())
        .execute(args.clone())
        .await
        .unwrap()
        .to_string();
    for text in delivered("file_read", &args, &result).await {
        for secret in [
            openai.as_str(),
            "hunter2secret",
            "H9vz3E8Kq5X2",
            github.as_str(),
            "MIIBOgIBAAJBAKj34",
            "KUpRKfFLfRYC9",
        ] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(
            value["redaction"]
                .as_str()
                .is_some_and(|n| n.contains("[REDACTED:")),
            "{text}"
        );
        let content = file_read_content(&text);
        let got: Vec<&str> = content.lines().collect();
        let want: Vec<&str> = env.lines().collect();
        assert_eq!(got.len(), want.len(), "line count preserved: {content}");
        assert_eq!(got[0], "DEBUG=true");
        assert_eq!(got[1], "OPENAI_API_KEY=[REDACTED:openai_key]");
        assert_eq!(got[2], "DB_PASSWORD=[REDACTED:password]");
        assert_eq!(got[3], "password = \"[REDACTED:password]\"");
        assert_eq!(got[4], "GITHUB_TOKEN=[REDACTED:github_token]");
        assert_eq!(got[5], "-----BEGIN RSA PRIVATE KEY-----");
        assert_eq!(got[6], "[REDACTED:private_key]");
        assert_eq!(got[7], "[REDACTED:private_key]");
        assert_eq!(got[8], "-----END RSA PRIVATE KEY-----");
        assert_eq!(got[9], "tokens = text.split(SEP)");
    }
}

/// XML mode: tag-shaped text is neutralized with a visible note, and the
/// rest of the result is still delivered as-is.
#[tokio::test]
async fn xml_mode_neutralizes_only_framing_tags() {
    let lt = "<";
    let content = format!("x < y && z {lt}/tool_result> a &amp; b");
    let mut agent = agent().await;
    agent
        .push_tool_result_message(false, "c1", "shell_exec", "{}", true, &content)
        .await;
    let text = agent.messages.last().unwrap().content.text().to_string();
    assert!(text.contains("x < y && z "), "{text}");
    assert!(text.contains(" a &amp; b"), "{text}");
    assert!(text.contains("&lt;/tool_result>"), "{text}");
    assert!(text.contains("[framing: 1 `<`"), "{text}");
    assert_eq!(text.matches("</tool_result>").count(), 1);
}
