use super::*;

#[test]
fn test_browser_fetch_schema() {
    let tool = BrowserFetch;
    let schema = tool.schema();
    assert!(schema["properties"].get("url").is_some());
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("url")));
}

#[test]
fn test_browser_screenshot_schema() {
    let tool = BrowserScreenshot;
    let schema = tool.schema();
    assert!(schema["properties"].get("url").is_some());
    assert!(schema["properties"].get("output_path").is_some());
    assert!(schema["properties"].get("width").is_some());
}

#[test]
fn test_browser_eval_schema() {
    let tool = BrowserEval;
    let schema = tool.schema();
    assert!(schema["properties"].get("url").is_some());
    assert!(schema["properties"].get("script").is_some());
}

#[test]
fn test_tool_names() {
    assert_eq!(BrowserFetch.name(), "browser_fetch");
    assert_eq!(BrowserScreenshot.name(), "browser_screenshot");
    assert_eq!(BrowserPdf.name(), "browser_pdf");
    assert_eq!(BrowserEval.name(), "browser_eval");
    assert_eq!(BrowserLinks.name(), "browser_links");
}

#[test]
fn test_tool_descriptions() {
    assert!(!BrowserFetch.description().is_empty());
    assert!(BrowserFetch.description().contains("web page"));
    assert!(BrowserScreenshot.description().contains("screenshot"));
}

#[test]
fn test_extract_text_from_html() {
    let html = "<html><body><script>alert('hi')</script><p>Hello <b>World</b></p></body></html>";
    let text = extract_text_from_html(html);
    assert!(text.contains("Hello"));
    assert!(text.contains("World"));
    assert!(!text.contains("alert"));
    assert!(!text.contains("<"));
}

#[test]
fn test_resolve_and_pin_target_rejects_non_http_scheme() {
    let result = resolve_and_pin_target("file:///etc/passwd");
    assert!(result.is_err());
}

#[test]
fn test_resolve_and_pin_target_blocks_private_ip() {
    let result = resolve_and_pin_target("http://192.168.1.10/test");
    assert!(result.is_err());
}

#[test]
fn test_resolve_and_pin_target_allows_localhost() {
    let pinned = resolve_and_pin_target("http://localhost/test").unwrap();
    assert_eq!(pinned.host, "localhost");
    assert!(is_private_network_ip(&pinned.ip));
}

#[test]
fn test_resolve_and_pin_target_allows_loopback_ip() {
    let pinned = resolve_and_pin_target("http://127.0.0.1/test").unwrap();
    assert_eq!(pinned.ip.to_string(), "127.0.0.1");
    assert!(pinned.host_is_ip);
}

#[test]
fn test_resolve_and_pin_target_allows_public_ip() {
    let pinned = resolve_and_pin_target("https://1.1.1.1/").unwrap();
    assert_eq!(pinned.ip.to_string(), "1.1.1.1");
    assert!(pinned.host_is_ip);
}

#[test]
fn test_extract_text_entities() {
    let html = "<p>Hello &amp; World &lt;test&gt;</p>";
    let text = extract_text_from_html(html);
    assert!(text.contains("Hello & World <test>"));
}

#[test]
fn test_truncate_output() {
    let short = "hello";
    assert_eq!(truncate_output(short, 100), short);

    let long = "a".repeat(200);
    let result = truncate_output(&long, 50);
    assert!(result.contains("truncated"));
    assert!(result.len() < 200);
}

#[tokio::test]
async fn test_browser_fetch_no_url() {
    let tool = BrowserFetch;
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("url is required"));
}

#[tokio::test]
async fn test_browser_eval_no_script() {
    let tool = BrowserEval;
    let result = tool.execute(json!({"url": "http://example.com"})).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("script is required"));
}

#[test]
fn test_browser_pdf_schema() {
    let tool = BrowserPdf;
    let schema = tool.schema();
    assert!(schema["properties"].get("url").is_some());
    assert!(schema["properties"].get("output_path").is_some());
    assert!(schema["properties"].get("timeout_secs").is_some());
}

#[test]
fn test_browser_links_schema() {
    let tool = BrowserLinks;
    let schema = tool.schema();
    assert!(schema["properties"].get("url").is_some());
    assert!(schema["properties"].get("filter").is_some());
}

#[test]
fn test_extract_text_removes_scripts() {
    let html = "<html><script>var x = 1;</script><body>Hello</body></html>";
    let text = extract_text_from_html(html);
    assert!(text.contains("Hello"));
    assert!(!text.contains("var x"));
}

#[test]
fn test_extract_text_removes_styles() {
    let html = "<html><style>.foo { color: red; }</style><body>World</body></html>";
    let text = extract_text_from_html(html);
    assert!(text.contains("World"));
    assert!(!text.contains("color"));
}

#[test]
fn test_extract_text_preserves_content() {
    let html = "<div><p>Paragraph 1</p><p>Paragraph 2</p></div>";
    let text = extract_text_from_html(html);
    assert!(text.contains("Paragraph 1"));
    assert!(text.contains("Paragraph 2"));
}

#[test]
fn test_extract_text_entity_nbsp() {
    let html = "Hello&nbsp;World";
    let text = extract_text_from_html(html);
    assert!(text.contains("Hello World"));
}

#[test]
fn test_extract_text_entity_quote() {
    let html = "&quot;quoted&quot;";
    let text = extract_text_from_html(html);
    assert!(text.contains("\"quoted\""));
}

#[test]
fn test_extract_text_entity_apostrophe() {
    let html = "it&#39;s";
    let text = extract_text_from_html(html);
    assert!(text.contains("it's"));
}

#[test]
fn test_truncate_output_exact() {
    let s = "12345";
    assert_eq!(truncate_output(s, 5), "12345");
}

#[test]
fn test_truncate_output_with_info() {
    let s = "a".repeat(100);
    let result = truncate_output(&s, 20);
    assert!(result.contains("100 total chars"));
    assert!(result.contains("truncated"));
}

#[test]
fn test_browser_type_debug() {
    let chrome = BrowserType::Chrome("/usr/bin/chrome".to_string());
    let debug_str = format!("{:?}", chrome);
    assert!(debug_str.contains("Chrome"));

    let playwright = BrowserType::Playwright;
    let debug_str = format!("{:?}", playwright);
    assert!(debug_str.contains("Playwright"));

    let curl = BrowserType::Curl;
    let debug_str = format!("{:?}", curl);
    assert!(debug_str.contains("Curl"));
}

#[test]
fn test_browser_type_clone() {
    let chrome = BrowserType::Chrome("/usr/bin/chrome".to_string());
    let cloned = chrome.clone();
    if let BrowserType::Chrome(path) = cloned {
        assert_eq!(path, "/usr/bin/chrome");
    } else {
        panic!("Clone should preserve variant");
    }
}

#[tokio::test]
async fn test_browser_screenshot_no_url() {
    let tool = BrowserScreenshot;
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("url is required"));
}

#[tokio::test]
async fn test_browser_pdf_no_url() {
    let tool = BrowserPdf;
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("url is required"));
}

#[tokio::test]
async fn test_browser_eval_no_url() {
    let tool = BrowserEval;
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("url is required"));
}

#[test]
fn test_all_tool_descriptions_non_empty() {
    assert!(!BrowserFetch.description().is_empty());
    assert!(!BrowserScreenshot.description().is_empty());
    assert!(!BrowserPdf.description().is_empty());
    assert!(!BrowserEval.description().is_empty());
    assert!(!BrowserLinks.description().is_empty());
}

#[test]
fn test_browser_fetch_schema_complete() {
    let tool = BrowserFetch;
    let schema = tool.schema();
    assert!(schema["properties"].get("wait_for").is_some());
    assert!(schema["properties"].get("timeout_secs").is_some());
    assert!(schema["properties"].get("javascript").is_some());
    assert!(schema["properties"].get("user_agent").is_some());
}

#[test]
fn test_should_stage_chrome_output_for_non_home_absolute_paths() {
    // Use platform-appropriate absolute path literals so the test works
    // identically on Linux, macOS, and Windows.
    #[cfg(unix)]
    let (home_dir, outside_abs, inside_abs, relative_abs) = (
        Path::new("/home/tester"),
        Path::new("/tmp/output.png"),
        Path::new("/home/tester/output.png"),
        Path::new("relative/output.png"),
    );
    #[cfg(windows)]
    let (home_dir, outside_abs, inside_abs, relative_abs) = (
        Path::new(r"C:\Users\tester"),
        Path::new(r"D:\tmp\output.png"),
        Path::new(r"C:\Users\tester\output.png"),
        Path::new(r"relative\output.png"),
    );

    assert!(should_stage_chrome_output_for_home(
        outside_abs,
        Some(home_dir)
    ));
    assert!(!should_stage_chrome_output_for_home(
        inside_abs,
        Some(home_dir)
    ));
    assert!(!should_stage_chrome_output_for_home(
        relative_abs,
        Some(home_dir)
    ));
}

#[test]
fn test_chrome_staging_output_preserves_filename() {
    // Use a platform-relative-but-still-valid absolute path so this works
    // on Windows too. We just need *some* absolute path with a filename.
    #[cfg(unix)]
    let input = Path::new("/tmp/chart-shot.png");
    #[cfg(windows)]
    let input = Path::new(r"C:\tmp\chart-shot.png");

    let staged = chrome_staging_output_path(input).unwrap();
    assert_eq!(
        staged.file_name().and_then(|name| name.to_str()),
        Some("chart-shot.png")
    );

    // Compare on a forward-slash form so the assertion holds on Windows
    // (where the staged path uses `\`).
    let staged_str = staged.to_string_lossy().replace('\\', "/");
    assert!(staged_str.contains(".selfware/browser-output"));
}

#[test]
fn test_browser_screenshot_schema_complete() {
    let tool = BrowserScreenshot;
    let schema = tool.schema();
    assert!(schema["properties"]["output_path"]["description"]
        .as_str()
        .unwrap()
        .contains(".selfware/browser-output/screenshot.png"));
    assert!(schema["properties"].get("height").is_some());
    assert!(schema["properties"].get("full_page").is_some());
    assert!(schema["properties"].get("timeout_secs").is_some());
}

#[test]
fn test_validate_browser_output_path_rejects_unsafe_absolute_path() {
    crate::tools::file::reset_safety_config_for_tests();
    let err = validate_browser_output_path("/etc/selfware-shot.png", "browser_screenshot")
        .expect_err("absolute output outside allowed paths must be rejected");
    assert!(err.to_string().contains("output_path validation failed"));
}

#[test]
fn test_extract_text_empty_html() {
    let text = extract_text_from_html("");
    assert!(text.is_empty());
}

#[test]
fn test_extract_text_whitespace_collapse() {
    let html = "Hello     World\n\n\nTest";
    let text = extract_text_from_html(html);
    // Multiple whitespaces should be collapsed
    assert!(!text.contains("     "));
}

#[test]
fn test_truncate_output_empty() {
    assert_eq!(truncate_output("", 10), "");
}

#[test]
fn test_extract_text_nested_tags() {
    let html = "<div><span><b><i>Nested</i></b></span></div>";
    let text = extract_text_from_html(html);
    assert!(text.contains("Nested"));
}

#[test]
fn test_extract_text_multiline_script() {
    let html = r#"<script>
            function test() {
                return "hidden";
            }
        </script>
        <p>Visible</p>"#;
    let text = extract_text_from_html(html);
    assert!(text.contains("Visible"));
    assert!(!text.contains("hidden"));
    assert!(!text.contains("function"));
}

// =========================================================================
// escape_js_string tests
// =========================================================================

#[test]
fn test_escape_js_string_plain() {
    assert_eq!(escape_js_string("hello"), "hello");
}

#[test]
fn test_escape_js_string_backslash() {
    assert_eq!(escape_js_string(r"path\to\file"), r"path\\to\\file");
}

#[test]
fn test_escape_js_string_single_quote() {
    assert_eq!(escape_js_string("it's"), r"it\'s");
}

#[test]
fn test_escape_js_string_double_quote() {
    assert_eq!(escape_js_string(r#"say "hi""#), r#"say \"hi\""#);
}

#[test]
fn test_escape_js_string_newline() {
    assert_eq!(escape_js_string("line1\nline2"), r"line1\nline2");
}

#[test]
fn test_escape_js_string_carriage_return() {
    assert_eq!(escape_js_string("line1\rline2"), r"line1\rline2");
}

#[test]
fn test_escape_js_string_tab() {
    assert_eq!(escape_js_string("col1\tcol2"), r"col1\tcol2");
}

#[test]
fn test_escape_js_string_null() {
    assert_eq!(escape_js_string("before\0after"), r"before\0after");
}

#[test]
fn test_escape_js_string_line_separator() {
    let input = "text\u{2028}more";
    let escaped = escape_js_string(input);
    assert!(escaped.contains("\\u2028"));
}

#[test]
fn test_escape_js_string_paragraph_separator() {
    let input = "text\u{2029}more";
    let escaped = escape_js_string(input);
    assert!(escaped.contains("\\u2029"));
}

#[test]
fn test_escape_js_string_backtick() {
    assert_eq!(escape_js_string("hello `world`"), r"hello \`world\`");
}

#[test]
fn test_escape_js_string_dollar() {
    assert_eq!(escape_js_string("cost $5"), r"cost \$5");
}

#[test]
fn test_escape_js_string_empty() {
    assert_eq!(escape_js_string(""), "");
}

#[test]
fn test_escape_js_string_all_special_chars() {
    let input = "'\"\\\n\r\t\0`$";
    let escaped = escape_js_string(input);
    // Verify all special characters are escaped
    assert!(escaped.contains(r"\'"));
    assert!(escaped.contains(r#"\""#));
    assert!(escaped.contains(r"\\"));
    assert!(escaped.contains(r"\n"));
    assert!(escaped.contains(r"\r"));
    assert!(escaped.contains(r"\t"));
    assert!(escaped.contains(r"\0"));
    assert!(escaped.contains(r"\`"));
    assert!(escaped.contains(r"\$"));
    // Verify no raw special chars remain (check length changed)
    assert!(escaped.len() > input.len());
}

// =========================================================================
// is_private_network_ip tests
// =========================================================================

#[test]
fn test_private_ip_loopback_v4() {
    let ip: IpAddr = "127.0.0.1".parse().unwrap();
    assert!(is_private_network_ip(&ip));
}

#[test]
fn test_private_ip_loopback_v6() {
    let ip: IpAddr = "::1".parse().unwrap();
    assert!(is_private_network_ip(&ip));
}

#[test]
fn test_private_ip_10_range() {
    let ip: IpAddr = "10.0.0.1".parse().unwrap();
    assert!(is_private_network_ip(&ip));
}

#[test]
fn test_private_ip_172_range() {
    let ip: IpAddr = "172.16.0.1".parse().unwrap();
    assert!(is_private_network_ip(&ip));
}

#[test]
fn test_private_ip_192_168_range() {
    let ip: IpAddr = "192.168.1.1".parse().unwrap();
    assert!(is_private_network_ip(&ip));
}

#[test]
fn test_private_ip_link_local() {
    let ip: IpAddr = "169.254.1.1".parse().unwrap();
    assert!(is_private_network_ip(&ip));
}

#[test]
fn test_private_ip_unspecified_v4() {
    let ip: IpAddr = "0.0.0.0".parse().unwrap();
    assert!(is_private_network_ip(&ip));
}

#[test]
fn test_private_ip_unspecified_v6() {
    let ip: IpAddr = "::".parse().unwrap();
    assert!(is_private_network_ip(&ip));
}

#[test]
fn test_public_ip_not_private() {
    let ip: IpAddr = "8.8.8.8".parse().unwrap();
    assert!(!is_private_network_ip(&ip));
}

#[test]
fn test_public_ip_1111() {
    let ip: IpAddr = "1.1.1.1".parse().unwrap();
    assert!(!is_private_network_ip(&ip));
}

#[test]
fn test_public_ip_93() {
    let ip: IpAddr = "93.184.216.34".parse().unwrap();
    assert!(!is_private_network_ip(&ip));
}

// =========================================================================
// is_trusted_local_browser_host tests
// =========================================================================

#[test]
fn test_trusted_localhost() {
    assert!(is_trusted_local_browser_host("localhost"));
}

#[test]
fn test_trusted_127001() {
    assert!(is_trusted_local_browser_host("127.0.0.1"));
}

#[test]
fn test_trusted_ipv6_loopback() {
    assert!(is_trusted_local_browser_host("::1"));
}

#[test]
fn test_trusted_ipv6_loopback_bracketed() {
    assert!(is_trusted_local_browser_host("[::1]"));
}

#[test]
fn test_trusted_0000() {
    assert!(is_trusted_local_browser_host("0.0.0.0"));
}

#[test]
fn test_trusted_subdomain_localhost() {
    assert!(is_trusted_local_browser_host("app.localhost"));
}

#[test]
fn test_not_trusted_example() {
    assert!(!is_trusted_local_browser_host("example.com"));
}

#[test]
fn test_not_trusted_private_ip() {
    assert!(!is_trusted_local_browser_host("192.168.1.1"));
}

#[test]
fn test_not_trusted_empty() {
    assert!(!is_trusted_local_browser_host(""));
}

// =========================================================================
// resolve_and_pin_target edge cases
// =========================================================================

#[test]
fn test_resolve_rejects_ftp_scheme() {
    assert!(resolve_and_pin_target("ftp://example.com/file").is_err());
}

#[test]
fn test_resolve_rejects_javascript_scheme() {
    assert!(resolve_and_pin_target("javascript:alert(1)").is_err());
}

#[test]
fn test_resolve_rejects_data_scheme() {
    assert!(resolve_and_pin_target("data:text/html,<h1>hi</h1>").is_err());
}

#[test]
fn test_resolve_allows_https() {
    // This may fail in CI with no DNS, but the scheme check happens before DNS
    let result = resolve_and_pin_target("https://1.1.1.1/");
    assert!(result.is_ok());
}

#[test]
fn test_resolve_pin_target_port_default_http() {
    let pinned = resolve_and_pin_target("http://127.0.0.1/test").unwrap();
    assert_eq!(pinned.port, 80);
}

#[test]
fn test_resolve_pin_target_port_default_https() {
    let pinned = resolve_and_pin_target("https://1.1.1.1/").unwrap();
    assert_eq!(pinned.port, 443);
}

#[test]
fn test_resolve_pin_target_custom_port() {
    let pinned = resolve_and_pin_target("http://127.0.0.1:8080/api").unwrap();
    assert_eq!(pinned.port, 8080);
}

#[test]
fn test_resolve_pin_target_host_is_ip_true() {
    let pinned = resolve_and_pin_target("http://127.0.0.1/").unwrap();
    assert!(pinned.host_is_ip);
}

#[test]
fn test_resolve_pin_target_host_is_ip_false() {
    let pinned = resolve_and_pin_target("http://localhost/").unwrap();
    assert!(!pinned.host_is_ip);
}

#[test]
fn test_resolve_pin_target_resolver_rule_format() {
    let pinned = resolve_and_pin_target("http://localhost:3000/api").unwrap();
    assert!(pinned.resolver_rule.starts_with("MAP localhost "));
    assert!(pinned.resolver_rule.contains("EXCLUDE localhost"));
}

// =========================================================================
// should_stage_chrome_output tests
// =========================================================================

#[test]
fn test_should_stage_relative_path_no_staging() {
    assert!(!should_stage_chrome_output_for_home(
        Path::new("output.png"),
        Some(Path::new("/home/user"))
    ));
}

#[test]
fn test_should_stage_no_home_dir() {
    assert!(!should_stage_chrome_output_for_home(
        Path::new("/tmp/output.png"),
        None
    ));
}

#[test]
fn test_should_stage_home_subpath_no_staging() {
    assert!(!should_stage_chrome_output_for_home(
        Path::new("/home/user/project/output.png"),
        Some(Path::new("/home/user"))
    ));
}

#[test]
fn test_should_stage_outside_home_triggers_staging() {
    #[cfg(unix)]
    let (outside, home) = (Path::new("/var/output.png"), Path::new("/home/user"));
    #[cfg(windows)]
    let (outside, home) = (Path::new(r"D:\var\output.png"), Path::new(r"C:\Users\user"));
    assert!(should_stage_chrome_output_for_home(outside, Some(home)));
}

// =========================================================================
// chrome_staging_output_path tests
// =========================================================================

#[test]
fn test_chrome_staging_preserves_extension() {
    let staged = chrome_staging_output_path(Path::new("/tmp/test.pdf")).unwrap();
    assert_eq!(staged.extension().and_then(|e| e.to_str()), Some("pdf"));
}

#[test]
fn test_chrome_staging_empty_filename_fallback() {
    let staged = chrome_staging_output_path(Path::new("/tmp/")).unwrap();
    assert!(staged.to_string_lossy().contains("browser-output"));
}

// =========================================================================
// extract_text_from_html more edge cases
// =========================================================================

#[test]
fn test_extract_text_multiple_scripts() {
    let html = "<script>a()</script>Hello<script>b()</script>World";
    let text = extract_text_from_html(html);
    assert!(text.contains("Hello"));
    assert!(text.contains("World"));
    assert!(!text.contains("a()"));
    assert!(!text.contains("b()"));
}

#[test]
fn test_extract_text_inline_style() {
    let html = r#"<style>.x{display:none}</style><p style="color:red">Text</p>"#;
    let text = extract_text_from_html(html);
    assert!(text.contains("Text"));
    assert!(!text.contains("display:none"));
}

#[test]
fn test_extract_text_all_entities() {
    let html = "&amp;&lt;&gt;&quot;&#39;&nbsp;";
    let text = extract_text_from_html(html);
    assert!(text.contains("&"));
    assert!(text.contains("<"));
    assert!(text.contains(">"));
    assert!(text.contains("\""));
    assert!(text.contains("'"));
}

#[test]
fn test_extract_text_only_tags() {
    let html = "<div><span></span></div>";
    let text = extract_text_from_html(html);
    assert!(text.trim().is_empty());
}

#[test]
fn test_extract_text_large_html() {
    let html = "<p>word </p>".repeat(1000);
    let text = extract_text_from_html(&html);
    assert!(text.contains("word"));
}
