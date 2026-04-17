//! Safety Validation Logic
//!
//! Core validation functions for checking tool calls, shell commands, and paths.

use crate::errors::{Result, SafetyError, SelfwareError};
use regex::Regex;
use std::path::PathBuf;
use std::sync::LazyLock;

use crate::api::types::ToolCall;
use crate::config::{is_local_endpoint, SafetyConfig};
use crate::safety::scanner::SecuritySeverity;

use super::types::*;

impl SafetyChecker {
    /// Create a safety checker with the given configuration
    pub fn new(config: &SafetyConfig) -> Self {
        Self {
            config: config.clone(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            security_scanner: crate::safety::scanner::SecurityScanner::new(),
        }
    }

    /// Create a safety checker with a specific working directory (test helper)
    #[cfg(test)]
    pub fn with_working_dir(config: &SafetyConfig, working_dir: PathBuf) -> Self {
        Self {
            config: config.clone(),
            working_dir,
            security_scanner: crate::safety::scanner::SecurityScanner::new(),
        }
    }

    /// Check if a tool call is safe to execute
    pub fn check_tool_call(&self, call: &ToolCall) -> Result<()> {
        let raw_name = &call.function.name;
        let tool_name = raw_name.trim();
        if raw_name != tool_name {
            tracing::debug!(
                "Tool name had whitespace: '{}' -> '{}'",
                raw_name,
                tool_name
            );
        }
        match tool_name {
            "file_write" | "file_edit" | "file_read" | "file_delete" | "search"
            | "directory_tree" | "file_list" | "analyze" | "tech_debt_report" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.check_path(path)?;
                }
                // Scan content of file_write and file_edit for secrets
                if call.function.name == "file_write" || call.function.name == "file_edit" {
                    let content = args
                        .get("content")
                        .or_else(|| args.get("new_str"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !content.is_empty() {
                        self.check_content_for_secrets(content)?;
                    }
                }
            }
            "shell_exec" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                self.check_shell_command(cmd)?;

                if let Some(cwd) = args.get("cwd").and_then(|v| v.as_str()) {
                    self.check_path(cwd)?;
                }
            }
            "git_commit" | "git_checkpoint" => {
                // Git operations are generally safe
            }
            "git_push" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
                if force {
                    return Err(SelfwareError::Safety(SafetyError::BlockedForcePush));
                }
            }
            "container_exec" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                    self.check_shell_command(cmd)?;
                }
            }
            "container_run" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                    self.check_shell_command(cmd)?;
                }
                // Check for dangerous volume mounts
                if let Some(volumes) = args.get("volumes").and_then(|v| v.as_array()) {
                    for vol in volumes {
                        if let Some(mount) = vol.as_str() {
                            self.check_volume_mount(mount)?;
                        }
                    }
                }
            }
            "process_start" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                    self.check_shell_command(cmd)?;
                }
                if let Some(cwd) = args.get("cwd").and_then(|v| v.as_str()) {
                    self.check_path(cwd)?;
                }
            }
            "http_request" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
                    self.check_http_request_url(url)?;
                }
            }
            "browser_fetch" | "browser_links" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
                    self.check_browser_url(url)?;
                }
            }
            "browser_screenshot" | "browser_pdf" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
                    self.check_browser_url(url)?;
                }
                if let Some(output_path) = args.get("output_path").and_then(|v| v.as_str()) {
                    self.check_path(output_path)?;
                }
            }
            "screen_capture" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(output_path) = args.get("output_path").and_then(|v| v.as_str()) {
                    self.check_path(output_path)?;
                }
            }
            "browser_eval" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
                    self.check_browser_url(url)?;
                }
                if let Some(code) = args
                    .get("code")
                    .or_else(|| args.get("expression"))
                    .and_then(|v| v.as_str())
                {
                    self.check_browser_eval(code)?;
                }
            }
            "npm_install" | "pip_install" | "yarn_install" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(script) = args.get("script").and_then(|v| v.as_str()) {
                    self.check_shell_command(script)?;
                }
            }
            "git_status" | "git_diff" | "grep_search" | "glob_find" | "symbol_search"
            | "process_list" | "process_logs" | "port_check" | "pip_list" | "pip_freeze"
            | "npm_scripts" | "container_list" | "container_logs" | "container_images"
            | "knowledge_query" | "knowledge_stats" | "knowledge_export" => {
                // These are read-only operations, safe to execute
            }
            "knowledge_add" | "knowledge_relate" | "knowledge_remove" | "knowledge_clear" => {
                // Knowledge graph mutations are in-memory only, no filesystem risk
            }
            "cargo_test" | "cargo_check" | "cargo_clippy" | "cargo_fmt" => {
                // These run predefined cargo subcommands, not arbitrary shell
            }
            "npm_run" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(script) = args.get("script").and_then(|v| v.as_str()) {
                    self.check_shell_command(script)?;
                }
            }
            "process_stop" | "process_restart" => {
                // These affect running processes by ID, no shell injection risk
            }
            "container_stop" | "container_remove" | "container_pull" | "container_build"
            | "compose_up" | "compose_down" => {
                // Container management by name/ID
            }
            "vision_analyze" | "vision_compare" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(endpoint) = args.get("endpoint").and_then(|v| v.as_str()) {
                    self.check_vision_endpoint_url(endpoint)?;
                }
                for key in &["image_path", "image_a", "image_b"] {
                    if let Some(p) = args.get(*key).and_then(|v| v.as_str()) {
                        self.check_path(p)?;
                    }
                }
            }
            "file_fim_edit" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.check_path(path)?;
                }
            }
            "computer_screen" | "computer_window" => {
                // Screen capture returns base64 PNG in-memory
            }
            "code_introspect" | "code_query" | "code_plan" => {
                // These tools accept filesystem paths — validate them.
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(path) = args.get("target").and_then(|v| v.as_str()) {
                    self.check_path(path)?;
                }
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.check_path(path)?;
                }
            }
            "context_status"
            | "context_focus"
            | "context_evict"
            | "context_recommend"
            | "context_load_skeleton"
            | "context_bulk_read"
            | "context_summary" => {
                // Context tools interact with internal state only
            }
            "computer_mouse" | "computer_keyboard" => {
                // These manipulate the desktop
            }
            "page_control" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
                    self.check_page_control_url(url)?;
                }
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.check_path(path)?;
                }
                if let Some(expr) = args.get("expression").and_then(|v| v.as_str()) {
                    self.check_browser_eval(expr)?;
                }
            }
            unknown => {
                tracing::error!(
                    "Safety checker: unregistered tool '{}' blocked — add to checker.rs dispatch if legitimate.",
                    unknown
                );
                return Err(SelfwareError::Safety(SafetyError::UnregisteredTool {
                    tool: unknown.to_string(),
                }));
            }
        }

        Ok(())
    }

    /// Check if a shell command is safe to execute
    ///
    /// SECURITY: This function implements multiple layers of protection:
    /// 1. Pattern matching against known dangerous commands (rm -rf /, mkfs, etc.)
    /// 2. Base64/hex encoded command detection (prevents `echo <base64> | base64 -d | sh`)
    /// 3. Command chaining analysis (checks each segment of chained commands)
    /// 4. Environment variable injection prevention
    pub fn check_shell_command(&self, cmd: &str) -> Result<()> {
        let normalized = normalize_shell_command(cmd);
        let dequoted = dequote_and_lowercase(&normalized);

        // SECURITY: Check for dangerous patterns
        for (pattern, description) in DANGEROUS_COMMAND_PATTERNS.iter() {
            if pattern.is_match(&normalized) || pattern.is_match(&dequoted) {
                return Err(SelfwareError::Safety(
                    SafetyError::DangerousCommandPattern {
                        description: (*description).to_string(),
                    },
                ));
            }
        }

        // Check for base64-encoded command execution
        if BASE64_EXEC_PATTERN.is_match(&normalized) || BASE64_EXEC_PATTERN.is_match(&dequoted) {
            return Err(SelfwareError::Safety(SafetyError::BlockedBase64Command));
        }

        // Check for hex-encoded command execution
        if HEX_EXEC_PATTERN.is_match(&normalized) || HEX_EXEC_PATTERN.is_match(&dequoted) {
            return Err(SelfwareError::Safety(SafetyError::BlockedHexCommand));
        }

        // Check for other encoding/obfuscation
        if ENCODED_EXEC_PATTERN.is_match(&normalized) || ENCODED_EXEC_PATTERN.is_match(&dequoted) {
            return Err(SelfwareError::Safety(SafetyError::BlockedEncodedCommand));
        }

        // Check command chaining
        for part in split_shell_commands(&normalized) {
            let part_trimmed = part.trim();
            for (pattern, description) in DANGEROUS_COMMAND_PATTERNS.iter() {
                if pattern.is_match(part_trimmed) {
                    return Err(SelfwareError::Safety(
                        SafetyError::DangerousCommandPattern {
                            description: format!("{} (in chain)", *description),
                        },
                    ));
                }
            }
        }

        // Check for environment variable injection
        static DANGEROUS_ENV_VARS: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?i)^\s*(PATH|LD_PRELOAD|LD_LIBRARY_PATH|DYLD_INSERT_LIBRARIES|DYLD_LIBRARY_PATH|PYTHONPATH|NODE_PATH|PERL5LIB|RUBYLIB|CLASSPATH|HOME|SHELL|USER|TERM|IFS)\s*=")
                .expect("Invalid regex")
        });

        for part in split_shell_commands(&normalized) {
            let part_trimmed = part.trim();
            if DANGEROUS_ENV_VARS.is_match(part_trimmed) {
                return Err(SelfwareError::Safety(SafetyError::BlockedEnvInjection));
            }
        }

        Ok(())
    }

    /// Scan content for hardcoded secrets
    fn check_content_for_secrets(&self, content: &str) -> Result<()> {
        let result = self.security_scanner.scan_content(content, None, "");
        let blocked: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.severity >= SecuritySeverity::High)
            .collect();
        if !blocked.is_empty() {
            let titles: Vec<_> = blocked.iter().map(|f| f.title.as_str()).collect();
            return Err(SelfwareError::Safety(SafetyError::SecretDetected {
                finding: titles.join(", "),
            }));
        }
        Ok(())
    }

    /// Check a container volume mount
    fn check_volume_mount(&self, mount: &str) -> Result<()> {
        let host_path = mount.split(':').next().unwrap_or("");
        let dangerous_mounts = [
            "/", "/etc", "/boot", "/usr", "/var", "/root", "/sys", "/proc", "/lib", "/lib64",
            "/opt", "/run",
        ];
        if host_path.contains("/.ssh")
            || host_path == ".ssh"
            || host_path == "~/.ssh"
            || host_path.starts_with("~/.ssh/")
        {
            return Err(SelfwareError::Safety(SafetyError::ContainerSshMount {
                mount: mount.to_string(),
            }));
        }
        for dm in &dangerous_mounts {
            if host_path == *dm
                || (host_path.starts_with(dm) && host_path.as_bytes().get(dm.len()) == Some(&b'/'))
            {
                return Err(SelfwareError::Safety(SafetyError::ContainerSystemMount {
                    mount: mount.to_string(),
                    directory: (*dm).to_string(),
                }));
            }
        }
        Ok(())
    }

    /// Check HTTP request URL for SSRF
    fn check_http_request_url(&self, url: &str) -> Result<()> {
        self.check_url_ssrf_with_options(
            url,
            UrlSafetyOptions {
                allow_file_scheme: false,
                allow_localhost: true,
            },
            std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1",
        )
    }

    /// Check browser URL
    fn check_browser_url(&self, url: &str) -> Result<()> {
        self.check_url_ssrf_with_options(
            url,
            UrlSafetyOptions {
                allow_file_scheme: false,
                allow_localhost: true,
            },
            std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1",
        )
    }

    /// Check page control URL
    fn check_page_control_url(&self, url: &str) -> Result<()> {
        if url.starts_with("file://") {
            let parsed = url::Url::parse(url)?;
            let path = parsed
                .to_file_path()
                .map_err(|_| anyhow::anyhow!("file:// URL must point to a local absolute path"))?;
            let path_str = path.to_string_lossy();
            return self.check_path(&path_str);
        }

        self.check_url_ssrf_with_options(
            url,
            UrlSafetyOptions {
                allow_file_scheme: false,
                allow_localhost: true,
            },
            std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1",
        )
    }

    /// Check vision endpoint URL
    fn check_vision_endpoint_url(&self, url: &str) -> Result<()> {
        self.check_url_ssrf_with_options(
            url,
            UrlSafetyOptions {
                allow_file_scheme: false,
                allow_localhost: true,
            },
            std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1",
        )
    }

    /// Check URL for SSRF with options
    ///
    /// SECURITY: Implements SSRF (Server-Side Request Forgery) protection by:
    /// 1. Blocking dangerous URI schemes (file:, gopher:, dict:, ftp:)
    /// 2. Blocking cloud metadata endpoints (169.254.169.254, etc.)
    /// 3. Blocking encoded IP bypass attempts (hex, octal, decimal representations)
    /// 4. Blocking link-local addresses
    /// 5. Validating IP literals against private/internal ranges
    pub(crate) fn check_url_ssrf_with_options(
        &self,
        url: &str,
        options: UrlSafetyOptions,
        allow_private: bool,
    ) -> Result<()> {
        let lower = url.to_lowercase();

        // SECURITY: Block dangerous URI schemes that could access local resources
        for scheme in &["file:", "gopher:", "dict:", "ftp:"] {
            if *scheme == "file:" && options.allow_file_scheme && lower.starts_with("file:") {
                return Ok(());
            }
            if lower.starts_with(scheme) {
                return Err(SelfwareError::Safety(SafetyError::BlockedUrlScheme {
                    scheme: (*scheme).trim_end_matches(':').to_string(),
                }));
            }
        }

        // Block cloud metadata endpoints
        let blocked_hosts = [
            "169.254.169.254",
            "metadata.google.internal",
            "[fd00:ec2::254]",
            "100.100.100.200",
        ];
        for host in &blocked_hosts {
            if lower.contains(host) {
                return Err(SelfwareError::Safety(SafetyError::BlockedCloudMetadata {
                    host: (*host).to_string(),
                }));
            }
        }

        // Block encoded IP bypasses
        let encoded_bypasses = [
            "0xa9fea9fe",
            "0xa9.0xfe.0xa9.0xfe",
            "2852039166",
            "0251.0376.0251.0376",
            "0x646464c8",
            "0x64.0x64.0x64.0xc8",
            "1684300232",
            "0144.0144.0144.0310",
        ];
        for encoded in &encoded_bypasses {
            if lower.contains(encoded) {
                return Err(SelfwareError::Safety(SafetyError::BlockedEncodedMetadata));
            }
        }

        // Block link-local range
        if lower.contains("169.254.") {
            return Err(SelfwareError::Safety(SafetyError::BlockedLinkLocal));
        }

        // Check IP literals
        if let Ok(parsed) = url::Url::parse(url) {
            if options.allow_localhost && is_local_endpoint(url) {
                return Ok(());
            }
            if let Some(host) = parsed.host_str() {
                if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                    if is_private_or_internal(ip) && !allow_private {
                        return Err(SelfwareError::Safety(SafetyError::BlockedPrivateNetwork {
                            ip: ip.to_string(),
                        }));
                    }
                }
            }
        }

        Ok(())
    }

    /// Check browser eval for data exfiltration
    fn check_browser_eval(&self, code: &str) -> Result<()> {
        let lower = code.to_lowercase();
        if (lower.contains("fetch(") || lower.contains("xmlhttprequest"))
            && (lower.contains("document.cookie") || lower.contains("localstorage"))
        {
            return Err(SelfwareError::Safety(SafetyError::BlockedBrowserEval));
        }
        Ok(())
    }

    /// Check file path
    fn check_path(&self, path: &str) -> Result<()> {
        use crate::safety::path_validator::PathValidator;
        let validator = PathValidator::new(&self.config, self.working_dir.clone());
        validator.validate(path).map_err(|e| match e {
            crate::errors::SelfwareError::Safety(safety_err) => {
                crate::errors::SelfwareError::Safety(safety_err)
            }
            other => other,
        })
    }

    /// Check if path is in allowed list (test helper)
    #[cfg(test)]
    #[allow(dead_code)]
    fn is_path_in_allowed_list(&self, canonical_str: &str, _original_path: &str) -> Result<bool> {
        use crate::safety::path_validator::PathValidator;
        let validator = PathValidator::new(&self.config, self.working_dir.clone());
        validator
            .is_path_in_allowed_list(canonical_str, _original_path)
            .map_err(|e| match e {
                crate::errors::SelfwareError::Safety(safety_err) => {
                    crate::errors::SelfwareError::Safety(safety_err)
                }
                other => other,
            })
    }
}

/// Normalize a shell command
pub fn normalize_shell_command(cmd: &str) -> String {
    let mut quoted_segments: Vec<String> = Vec::new();
    let mut unquoted = String::with_capacity(cmd.len());
    let bytes = cmd.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if (c == b'"' || c == b'\'') && (i == 0 || bytes[i - 1] != b'\\') {
            let quote = c;
            let seg_start = i;
            i += 1;
            while i < bytes.len() && !(bytes[i] == quote && bytes[i - 1] != b'\\') {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            let placeholder = format!("\x00\x01{}\x00", quoted_segments.len());
            quoted_segments.push(cmd[seg_start..i].to_string());
            unquoted.push_str(&placeholder);
        } else {
            unquoted.push(c as char);
            i += 1;
        }
    }

    let mut result: String = unquoted.split_whitespace().collect::<Vec<_>>().join(" ");
    result = result.to_lowercase();
    while result.contains("//") {
        result = result.replace("//", "/");
    }
    result = result.replace("\\n", "").replace("\\t", " ");

    // Remove backslash escapes
    let mut deslashed = String::with_capacity(result.len());
    let result_bytes = result.as_bytes();
    let mut j = 0;
    while j < result_bytes.len() {
        if result_bytes[j] == b'\\' && j + 1 < result_bytes.len() {
            let next = result_bytes[j + 1];
            if next.is_ascii_alphanumeric() || next == b'_' || next == b'-' || next == b'/' {
                j += 1;
                continue;
            }
        }
        deslashed.push(result_bytes[j] as char);
        j += 1;
    }
    result = deslashed;

    result = result.replace('`', "$(");
    result = result.replace("$(", " $( ");
    result = result.replace(')', " ) ");
    result = result.replace(" | ", "|");
    result = result.replace("| ", "|");
    result = result.replace(" |", "|");
    result = result.replace('|', " | ");
    result = result.split_whitespace().collect::<Vec<_>>().join(" ");

    // Restore quoted segments
    for (idx, segment) in quoted_segments.iter().enumerate() {
        let placeholder = format!("\x00\x01{}\x00", idx);
        result = result.replace(&placeholder, segment);
    }
    result
}

/// Strip quotes and lowercase
pub(crate) fn dequote_and_lowercase(cmd: &str) -> String {
    let mut out = String::with_capacity(cmd.len());
    let bytes = cmd.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if (c == b'\'' || c == b'"') && (i == 0 || bytes[i - 1] != b'\\') {
            let quote = c;
            i += 1;
            while i < bytes.len() && !(bytes[i] == quote && (i == 0 || bytes[i - 1] != b'\\')) {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out.to_lowercase()
}

/// Split shell commands on separators
pub fn split_shell_commands(cmd: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let mut quote_char = b' ';
    let bytes = cmd.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];

        if (c == b'"' || c == b'\'') && (i == 0 || bytes[i - 1] != b'\\') {
            if !in_quotes {
                in_quotes = true;
                quote_char = c;
            } else if c == quote_char {
                in_quotes = false;
            }
        }

        if !in_quotes {
            if c == b';' {
                if start < i {
                    parts.push(&cmd[start..i]);
                }
                start = i + 1;
            } else if (c == b'&' || c == b'|') && i + 1 < bytes.len() && bytes[i + 1] == c {
                if start < i {
                    parts.push(&cmd[start..i]);
                }
                start = i + 2;
                i += 1;
            }
        }
        i += 1;
    }

    if start < cmd.len() {
        parts.push(&cmd[start..]);
    }

    parts
}

/// Check if an IP is private or internal
pub fn is_private_or_internal(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || (v4.octets()[0] == 169 && v4.octets()[1] == 254)
                || (v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 2)
                || (v4.octets()[0] == 198 && v4.octets()[1] == 51 && v4.octets()[2] == 100)
                || (v4.octets()[0] == 203 && v4.octets()[1] == 0 && v4.octets()[2] == 113)
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_private()
                        || v4.is_loopback()
                        || v4.is_link_local()
                        || v4.is_broadcast()
                        || v4.is_unspecified()
                        || (v4.octets()[0] == 169 && v4.octets()[1] == 254)
                        || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
                })
        }
    }
}

/// DNS resolver that pins resolved IPs
#[derive(Clone)]
pub struct PinnedDnsResolver {
    allow_private: bool,
}

impl PinnedDnsResolver {
    pub fn new(allow_private: bool) -> Self {
        Self { allow_private }
    }
}

impl reqwest::dns::Resolve for PinnedDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let allow_private = self.allow_private;
        Box::pin(async move {
            let addrs: Vec<std::net::SocketAddr> =
                tokio::net::lookup_host(format!("{}:0", name.as_str()))
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                    .collect();

            if allow_private {
                let iter: reqwest::dns::Addrs = Box::new(addrs.into_iter());
                return Ok(iter);
            }

            let safe_addrs: Vec<std::net::SocketAddr> = addrs
                .into_iter()
                .filter(|addr| !is_private_or_internal(addr.ip()))
                .collect();

            if safe_addrs.is_empty() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "DNS resolved to private/internal IP address",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }

            let iter: reqwest::dns::Addrs = Box::new(safe_addrs.into_iter());
            Ok(iter)
        })
    }
}
