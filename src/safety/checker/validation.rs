//! Safety Validation Logic
//!
//! Core validation functions for checking tool calls, shell commands, and paths.

use crate::errors::{Result, SafetyError, SelfwareError};
use regex::Regex;
use std::path::PathBuf;
use std::sync::LazyLock;

use crate::api::types::ToolCall;
use crate::config::{is_local_endpoint, SafetyConfig};
use crate::safety::scanner::{SecurityCategory, SecuritySeverity};

use super::types::*;

/// Environment variables denied in BOTH the inline command-string check
/// (`VAR=value cmd`, `export VAR=`, `env VAR=`) and the shell_exec
/// structured env map: PATH hijack, dynamic-loader injection/audit, and
/// interpreter startup hooks. Single source of truth —
/// `src/tools/shell_exec/mod.rs` uses this same list. Deliberately NOT
/// denied: HOME/USER/TERM/SHELL/CLASSPATH (everyday overrides), ENV
/// (breaks `ENV=production`), LD_DEBUG (legitimate loader debugging).
pub(crate) const DENIED_ENV_VARS: &[&str] = &[
    "PATH",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "BASH_ENV",
    "PYTHONPATH",
    "NODE_PATH",
    "PERL5LIB",
    // Legacy alias of PERL5LIB — perl honors both (red-team wave-78).
    "PERLLIB",
    "RUBYLIB",
    "IFS",
    // Interpreter startup hooks (red-team wave 4): every one of these loads
    // attacker-controlled code at interpreter boot without touching the
    // command's visible payload.
    "PYTHONSTARTUP",
    "PYTHONINSPECT",
    "NODE_OPTIONS",
    "PERL5OPT",
    "RUBYOPT",
    // Interpreter home / load-path hijacks (red-team wave-12): same class as
    // PYTHONPATH/RUBYLIB above — point the interpreter or package manager at
    // attacker-controlled trees. PYTHONHOME relocates the whole stdlib,
    // GEM_HOME/GEM_PATH relocate gem resolution, BUNDLE_GEMFILE executes an
    // attacker-written Gemfile at bundle time.
    "PYTHONHOME",
    "GEM_HOME",
    "GEM_PATH",
    "BUNDLE_GEMFILE",
    // zsh function/autoload path and startup-dir hijacks (red-team
    // wave-24): `export FPATH=/tmp; autoload evil; evil` — zsh's
    // equivalent of BASH_ENV/PYTHONSTARTUP.
    "FPATH",
    "ZDOTDIR",
    // Shell prompt/startup hooks (wave-35): PROMPT_COMMAND runs before
    // every prompt (bash/zsh), PHPRC + PHP_INI_SCAN_DIR relocate php.ini
    // (auto_prepend_file class), SSH_AUTH_SOCK pointed at an attacker's
    // socket is credential phishing (git.rs re-adds it via Command env,
    // not shell, so blocking shell assignments costs nothing).
    "PROMPT_COMMAND",
    "PHPRC",
    "PHP_INI_SCAN_DIR",
    "PHP_AUTO_PREPEND_FILE",
    "SSH_AUTH_SOCK",
    // Helper-app and config-relocation injection (wave-37): tools invoke
    // these through sh -c, so a value like `vim;id` executes; git/npm/tls
    // config relocation swaps trust anchors and package sources.
    // (GIT_EDITOR was already here; EDITOR/VISUAL/PAGER/BROWSER are the
    // same vector for crontab -e, less, xdg-open.)
    "EDITOR",
    "VISUAL",
    "PAGER",
    "BROWSER",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "CURL_CA_BUNDLE",
    "SSL_CERT_FILE",
    "NODE_EXTRA_CA_CERTS",
    "NPM_CONFIG_PREFIX",
    "NPM_CONFIG_REGISTRY",
    // GUI toolkit module injection (wave-38b: GTK_MODULES/QT_PLUGIN_PATH
    // load .so from arbitrary paths — LD_PRELOAD via the toolkit), askpass
    // phishing (SSH_ASKPASS/SUDO_ASKPASS harvest credentials; GIT_ASKPASS
    // was already here), pip package-source swap (NPM_CONFIG_REGISTRY's
    // twin), and interpreter-environment relocation (fake venv/conda
    // activate scripts run attacker code).
    "GTK_MODULES",
    "QT_PLUGIN_PATH",
    "SSH_ASKPASS",
    "SUDO_ASKPASS",
    "PIP_INDEX_URL",
    "PIP_EXTRA_INDEX_URL",
    "VIRTUAL_ENV",
    "CONDA_PREFIX",
    // Editor startup hooks (wave-58b: `VIMINIT='!cat /tmp/payload | sh'
    // vim` runs on every vim/ex start — BASH_ENV via the editor) and
    // compiler toolchain relocation (GCC_EXEC_PREFIX finds attacker
    // cc1/as/ld).
    "VIMINIT",
    "EXINIT",
    "GCC_EXEC_PREFIX",
    // Config-redirection and helper-app twins (wave-69): numbered git
    // config (GIT_CONFIG_COUNT + KEY_n/VALUE_n — the regex below covers
    // the numbered vars), kubectl context hijack, pager/preprocessor
    // execution (PAGER's family), and the third JVM-options twin.
    "GIT_CONFIG_COUNT",
    "KUBECONFIG",
    "MANPAGER",
    "LESSCLOSE",
    "LESSOPEN",
    "JAVA_OPTS",
    "JDK_JAVA_OPTIONS",
    // Proxy/git-transport/prompt-expansion channels (wave-46):
    // HTTP(S)_PROXY/ALL_PROXY route every request through the attacker
    // (MITM by configuration, the CA-bundle twins); GIT_PROXY_COMMAND
    // pipes git traffic through a chosen binary; PS1/PS2/PS4 undergo
    // command substitution on every prompt — `PS4="+$(curl evil)"` is a
    // callback on every xtrace line (PROMPT_COMMAND's twin).
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "GIT_PROXY_COMMAND",
    // Git execution vectors: agent git work goes through the git tools, not
    // env-injected helpers.
    "GIT_SSH_COMMAND",
    "GIT_ASKPASS",
    "GIT_EDITOR",
    "GIT_PAGER",
    // JVM equivalent of LD_PRELOAD.
    "JAVA_TOOL_OPTIONS",
    "_JAVA_OPTIONS",
];

/// Programs whose `eval $(<program> …)` shell-integration line is a
/// known-safe everyday pattern (`eval $(ssh-agent)`,
/// `eval "$(direnv hook bash)"`, prompt and version-manager init).
const SAFE_EVAL_SUBSTITUTION_PROGRAMS: &[&str] = &[
    "ssh-agent",
    "direnv",
    "keychain",
    "starship",
    "zoxide",
    "fnm",
    "mise",
    "rbenv",
    "pyenv",
    "conda",
];

static EVAL_WITH_SUBSTITUTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\beval\b[^\n;]*\$\(").expect("Invalid regex"));
static EVAL_SUBSTITUTION_PROGRAM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\beval\b[^\n;]*?\$\(\s*([a-z0-9_.-]+)").expect("Invalid regex"));

/// Extract a shell command from tool arguments in either wire form: a plain
/// string ("command": "cargo test") or an argv array ("command":
/// ["/bin/sh", "-c", "..."]). The array form previously skipped the shell
/// check entirely because only `as_str()` was consulted (red-team finding).
fn command_arg_string(args: &serde_json::Value) -> String {
    match args.get("command") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

/// Base64-looking query-param values (len >= 40) in a URL, for the
/// credential-blob exfil check in check_endpoint_url.
fn base64_like_query_blobs(url: &str) -> Vec<String> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter_map(|pair| pair.split_once('=').map(|(_, v)| v))
        .flat_map(|v| {
            v.split(|c: char| {
                !c.is_alphanumeric() && c != '_' && c != '-' && c != '+' && c != '/' && c != '='
            })
            .map(str::to_string)
            .collect::<Vec<_>>()
        })
        .filter(|v| v.len() >= 40)
        .collect()
}

/// Lenient base64 decode (standard + URL-safe, padding optional) → UTF-8.
fn decode_base64_lenient(input: &str) -> Option<String> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let v: Vec<u8> = chunk.iter().map(|b| val(*b).unwrap_or(0)).collect();
        let n = ((v[0] as u32) << 18)
            | ((v[1] as u32) << 12)
            | (((*v.get(2).unwrap_or(&0)) as u32) << 6)
            | ((*v.get(3).unwrap_or(&0)) as u32);
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    String::from_utf8(out).ok()
}

/// Extract bind-mount specs from a raw `docker run`-style shell command:
/// `-v /h:/c`, `-v=/h:/c`, `--volume /h:/c`, `--volume=/h:/c`, and
/// `--mount type=bind,source=/h,target=/c` (yields the host side, which is
/// what check_volume_mount inspects).
fn extract_shell_volume_specs(cmd: &str) -> Vec<String> {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    let unquote = |s: &str| s.trim_matches(['"', '\'']).to_string();
    let mut specs = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        if tok == "-v" || tok == "--volume" {
            if let Some(spec) = tokens.get(i + 1) {
                specs.push(unquote(spec));
                i += 1;
            }
        } else if let Some(rest) = tok
            .strip_prefix("-v=")
            .or_else(|| tok.strip_prefix("--volume="))
        {
            specs.push(unquote(rest));
        } else if tok == "--mount" || tok.starts_with("--mount=") {
            let mount_str = if tok == "--mount" {
                i += 1;
                tokens.get(i).copied().unwrap_or("")
            } else {
                tok.strip_prefix("--mount=").unwrap_or(tok)
            };
            for field in mount_str.split(',') {
                if let Some(src) = field
                    .strip_prefix("source=")
                    .or_else(|| field.strip_prefix("src="))
                {
                    specs.push(unquote(src));
                }
            }
        }
        i += 1;
    }
    specs
}

/// Whether an `eval` invokes command substitution with a program outside
/// the known-safe shell-integration list. `eval` without substitution is
/// not handled here. (The command is first run through
/// `normalize_shell_command`, which rewrites backticks to `$(…)`.)
fn eval_substitution_is_blocked(cmd: &str) -> bool {
    if !EVAL_WITH_SUBSTITUTION.is_match(cmd) {
        return false;
    }
    match EVAL_SUBSTITUTION_PROGRAM.captures(cmd) {
        Some(caps) => {
            let program = caps[1].to_string();
            !SAFE_EVAL_SUBSTITUTION_PROGRAMS.contains(&program.as_str())
        }
        // eval + substitution whose program we can't identify: keep blocking.
        None => true,
    }
}

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
                if tool_name == "file_write" || tool_name == "file_edit" {
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
                let cmd = command_arg_string(&args);
                self.check_shell_command(&cmd)?;

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
                // An explicit URL remote ships the whole workspace to an
                // arbitrary endpoint (wave-60: `remote:
                // "https://github.com/victim-org/repo"`). Named remotes
                // (origin, upstream) are user-configured and pass.
                if let Some(remote) = args.get("remote").and_then(|v| v.as_str()) {
                    if remote.starts_with("http://")
                        || remote.starts_with("https://")
                        || remote.starts_with("ssh://")
                        || remote.starts_with("git@")
                        || remote.starts_with("file://")
                    {
                        return Err(SelfwareError::Safety(
                            SafetyError::DangerousCommandPattern {
                                description: format!(
                                    "git push to an explicit URL remote (workspace exfil): {remote}"
                                ),
                            },
                        ));
                    }
                }
                // Only checkable here when the branch is explicit in the call --
                // when omitted the tool pushes whatever branch is currently
                // checked out, which GitPush::execute() itself re-checks
                // after resolving it (see src/tools/git.rs).
                if let Some(branch) = args.get("branch").and_then(|v| v.as_str()) {
                    // Normalize ref-qualified names: `refs/heads/main` used to
                    // slip past the exact-string compare and push to protected
                    // main (red-team wave-11 finding).
                    let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
                    if self
                        .config
                        .protected_branches
                        .iter()
                        .any(|b| b == branch)
                    {
                        return Err(SelfwareError::Safety(
                            SafetyError::BlockedProtectedBranchPush {
                                branch: branch.to_string(),
                                protected: self.config.protected_branches.clone(),
                            },
                        ));
                    }
                }
            }
            "container_exec" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                let cmd = command_arg_string(&args);
                if !cmd.is_empty() {
                    self.check_shell_command(&cmd)?;
                }
            }
            "container_run" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                let cmd = command_arg_string(&args);
                if !cmd.is_empty() {
                    self.check_shell_command(&cmd)?;
                }
                // `--privileged` is blocked in shell docker-run commands as
                // never agent-legitimate; the structured arg deserves the
                // same stance even though the tool currently ignores it
                // (red-team wave-29: privileged:true + nsenter host escape).
                if args.get("privileged").and_then(|v| v.as_bool()) == Some(true) {
                    return Err(SelfwareError::Safety(
                        SafetyError::DangerousCommandPattern {
                            description: "container run with privileged: true".to_string(),
                        },
                    ));
                }
                // Check for dangerous volume mounts — both the string form
                // ("/etc:/host") and the object form
                // ({"host": "/etc", "container": "/host"} / docker's
                // {"source": ..., "target": ...}); the object form previously
                // bypassed this check entirely (red-team finding).
                // volumes arrives as an array, OR as a stringified JSON
                // array ("[\"/etc:/host\"]"), OR as a bare "host:container"
                // string — the string forms previously skipped every check
                // (red-team wave-5 finding).
                let volumes_json: Option<serde_json::Value> = match args.get("volumes") {
                    Some(v @ serde_json::Value::Array(_)) => Some(v.clone()),
                    Some(serde_json::Value::String(s)) => serde_json::from_str(s)
                        .ok()
                        .or_else(|| Some(serde_json::json!([s.clone()]))),
                    // Dict form {"<host-path>": "<container-path>"} — a
                    // root-mount smuggled as an object key bypassed the
                    // array-only extraction (red-team wave-28 finding).
                    Some(v @ serde_json::Value::Object(_)) => Some(serde_json::json!([v.clone()])),
                    _ => None,
                };
                if let Some(serde_json::Value::Array(volumes)) = volumes_json {
                    for vol in volumes {
                        if let Some(mount) = vol.as_str() {
                            self.check_volume_mount(mount)?;
                        } else if let Some(obj) = vol.as_object() {
                            // Dict form: KEYS are host paths.
                            if obj
                                .get("host")
                                .or_else(|| obj.get("source"))
                                .and_then(|v| v.as_str())
                                .is_none()
                            {
                                for host_path in obj.keys() {
                                    self.check_volume_mount(host_path)?;
                                }
                                continue;
                            }
                            let host = obj
                                .get("host")
                                .or_else(|| obj.get("source"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !host.is_empty() {
                                self.check_volume_mount(host)?;
                            }
                        }
                    }
                }
            }
            "process_start" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                let cmd = command_arg_string(&args);
                if !cmd.is_empty() {
                    self.check_shell_command(&cmd)?;
                }
                // The tool also takes an argv array ("args": ["-c", "…"]) —
                // its own metacharacter defense only rejects ;&|`$()<>, so
                // metachar-free payloads (`sh -c "cat .env"`) sail through
                // both layers unless the checker sees them (red-team wave-22
                // finding). Check the joined command line.
                let argv: Vec<String> = args
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if !argv.is_empty() {
                    let joined = format!("{} {}", cmd, argv.join(" "));
                    self.check_shell_command(&joined)?;
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
            | "tool_search" | "process_list" | "process_logs" | "port_check" | "pip_list"
            | "pip_freeze" | "npm_scripts" | "container_list" | "container_logs"
            | "container_images" | "knowledge_query" | "knowledge_stats" | "knowledge_export"
            => {
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
            // pty_shell is a shell tool — apply the same command checks as shell_exec.
            "pty_shell" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                    self.check_shell_command(cmd)?;
                }
            }
            // file_multi_edit mutates several files — validate EVERY edit path and
            // secret-scan each replacement, not just a top-level "path".
            "file_multi_edit" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                    for edit in edits {
                        if let Some(path) = edit.get("path").and_then(|v| v.as_str()) {
                            self.check_path(path)?;
                        }
                        if let Some(new_str) = edit.get("new_str").and_then(|v| v.as_str()) {
                            if !new_str.is_empty() {
                                self.check_content_for_secrets(new_str)?;
                            }
                        }
                    }
                }
            }
            // patch_apply targets paths embedded in the unified diff — validate the
            // `+++ b/<path>` targets and secret-scan the diff body. File
            // deletions (`+++ /dev/null`) target the OLD file (`--- a/<path>`):
            // that path is what the operation removes, so it is what must be
            // validated against denied/allowed paths.
            "patch_apply" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(diff) = args.get("diff").and_then(|v| v.as_str()) {
                    let mut old_path: Option<String> = None;
                    for line in diff.lines() {
                        if let Some(rest) = line.strip_prefix("--- ") {
                            let p = rest.split('\t').next().unwrap_or("").trim();
                            let p = p.strip_prefix("a/").unwrap_or(p);
                            old_path = if p.is_empty() || p == "/dev/null" {
                                None
                            } else {
                                Some(p.to_string())
                            };
                        } else if let Some(rest) = line.strip_prefix("+++ ") {
                            let p = rest.trim().trim_start_matches("b/");
                            if p.is_empty() || p == "/dev/null" {
                                // Deletion: validate the old file path.
                                if let Some(old) = old_path.take() {
                                    self.check_path(&old)?;
                                }
                            } else {
                                self.check_path(p)?;
                            }
                            old_path = None;
                        }
                    }
                    self.check_content_for_secrets(diff)?;
                }
            }
            // Worktree tools operate on a worktree path — validate it.
            "enter_worktree" | "exit_worktree" => {
                let args: serde_json::Value = serde_json::from_str(&call.function.arguments)?;
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    self.check_path(path)?;
                }
            }
            // Runtime dynamic-library loader (feature-gated, off by default) —
            // a native-code execution vector. Validate the library path it
            // loads so it can't pull code in from a denied location.
            "hot_reload" => {
                if let Ok(args) =
                    serde_json::from_str::<serde_json::Value>(&call.function.arguments)
                {
                    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                        self.check_path(path)?;
                    }
                }
            }
            // Read-only / introspection tools: no filesystem or shell mutation.
            "list_worktrees"
            | "lsp_goto_definition"
            | "lsp_goto_implementation"
            | "lsp_find_references"
            | "lsp_hover"
            | "lsp_document_symbols"
            | "lsp_workspace_symbols"
            | "lsp_diagnostics"
            // Analysis / context / interaction tools: no filesystem or shell
            // mutation (metadata-classified read-only).
            | "code_metrics"
            | "code_map"
            | "code_diff_plan"
            | "context_budget"
            | "context_action"
            | "graph_summary"
            | "context_pack"
            | "hotspots"
            | "impact"
            | "neighbors"
            | "test_map"
            | "cycles"
            | "dups"
            | "localize_issue"
            | "ask_user"
            | "knowledge_auto_extract" => {
                // Metadata-classified as read-only / network probes; nothing to
                // path- or command-check.
            }
            unknown => {
                // MCP tools are dynamically named `mcp_<server>_<tool>`; they are
                // registered (user-configured servers), so don't hard-block them.
                // Apply generic argument checks (path + command) and allow.
                if unknown.starts_with("mcp_") {
                    if let Ok(args) =
                        serde_json::from_str::<serde_json::Value>(&call.function.arguments)
                    {
                        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                            self.check_path(path)?;
                        }
                        if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                            self.check_shell_command(cmd)?;
                        }
                    }
                    return Ok(());
                }
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
        // Quote-masked scaffold: quoted segments stay as opaque placeholders,
        // so quoted PROSE (`git commit -m "revert the rm -rf / guard"`,
        // `echo "never run chown -R /"`) cannot trip the dangerous-command
        // patterns. Unquoted dangerous content still matches.
        let masked = normalize_shell_command_masked(cmd);
        // $IFS obfuscation: shells expand `$IFS`/`${IFS}` to whitespace, so
        // `rm$IFS-rf$IFS/target` EXECUTES as `rm -rf /target` while matching
        // no literal `rm\s+` pattern. Expand it in a CHECK-ONLY copy used
        // solely for pattern matching — the command that runs is untouched.
        let masked_ifs = expand_ifs_for_matching(&masked);

        // SECURITY: Check for dangerous patterns (against the masked form).
        for (pattern, description) in DANGEROUS_COMMAND_PATTERNS.iter() {
            if pattern.is_match(&masked) || pattern.is_match(&masked_ifs) {
                return Err(SelfwareError::Safety(
                    SafetyError::DangerousCommandPattern {
                        description: (*description).to_string(),
                    },
                ));
            }
        }

        // Payload patterns inspect QUOTED content (python -c '…', eval "…"),
        // so they match the quote-restored and dequoted forms instead.
        for (pattern, description) in PAYLOAD_COMMAND_PATTERNS.iter() {
            if pattern.is_match(&normalized) || pattern.is_match(&dequoted) {
                return Err(SelfwareError::Safety(
                    SafetyError::DangerousCommandPattern {
                        description: (*description).to_string(),
                    },
                ));
            }
        }

        // eval with command substitution: allowed for a small set of
        // known-safe shell-integration programs (`eval $(ssh-agent)`,
        // `eval "$(direnv hook bash)"`), blocked otherwise.
        if eval_substitution_is_blocked(&normalized) || eval_substitution_is_blocked(&dequoted) {
            return Err(SelfwareError::Safety(
                SafetyError::DangerousCommandPattern {
                    description: "eval with command substitution".to_string(),
                },
            ));
        }

        // Check for base64-encoded command execution
        if BASE64_EXEC_PATTERN.is_match(&normalized) || BASE64_EXEC_PATTERN.is_match(&dequoted) {
            return Err(SelfwareError::Safety(SafetyError::BlockedBase64Command));
        }

        // Interpreter-embedded decode+execute — `python3 -c "exec(__import__(
        // 'base64').b64decode(...))"`, JS `eval(atob(...))`. The pipe-based
        // pattern above cannot see these: nothing pipes to a shell (red-team
        // finding: base64 reverse shell via process_start).
        if (dequoted.contains("b64decode") || dequoted.contains("atob("))
            && (dequoted.contains("exec(")
                || dequoted.contains("eval(")
                || dequoted.contains("subprocess")
                || dequoted.contains("os.system")
                || dequoted.contains("popen"))
        {
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

        // Raw `docker run` / `podman run` / `nerdctl run` in a shell bypasses
        // the container_run TOOL's volume checks entirely (red-team wave-2
        // finding). Extract bind-mount specs and run them through the same
        // check_volume_mount gate; --privileged is never agent-legitimate.
        if normalized.contains("docker run")
            || normalized.contains("podman run")
            || normalized.contains("nerdctl run")
        {
            if normalized.contains("--privileged") {
                return Err(SelfwareError::Safety(
                    SafetyError::DangerousCommandPattern {
                        description: "container run with --privileged".to_string(),
                    },
                ));
            }
            for spec in extract_shell_volume_specs(&normalized) {
                self.check_volume_mount(&spec)?;
            }
        }

        // denied_paths must also guard shell output redirects: without this,
        // `echo x > denied_file` bypasses the file-tool path checks entirely
        // (P1-8). Parse `>`/`>>` targets from the RAW command (preserving
        // case and quoting) and match them against the deny globs.
        for target in shell_output_redirect_targets(cmd) {
            if let Some(pattern) = redirect_target_matches_denied(
                &target,
                &self.working_dir,
                &self.config.denied_paths,
            ) {
                return Err(SelfwareError::Safety(SafetyError::PathDeniedPattern {
                    pattern,
                }));
            }
        }

        // `tee`/`sponge` writes bypass the redirect guard entirely — no `>`
        // appears in the command, so `echo KEY | tee -a ~/.ssh/authorized_keys`
        // sailed through BOTH write guards (whole-repo review, Safety P1).
        // Extract their file targets from every pipeline segment and run
        // them through the same denied-path matching as redirects.
        for target in shell_tee_write_targets(cmd) {
            if let Some(pattern) = redirect_target_matches_denied(
                &target,
                &self.working_dir,
                &self.config.denied_paths,
            ) {
                return Err(SelfwareError::Safety(SafetyError::PathDeniedPattern {
                    pattern,
                }));
            }
        }

        // The command BODY is also not covered by the allowed_paths
        // allow-list: only file_* tool `path` args and shell_exec's `cwd`
        // were validated, so `cat /etc/passwd` or `cp x ~/.ssh/y` succeeded
        // where the equivalent file_read/file_write was blocked (Safety P1).
        // Apply a conservative lexical check to file-targeting tokens.
        self.check_shell_command_paths(cmd)?;

        // Check command chaining (against the masked form, same rationale
        // as the main dangerous-pattern pass above).
        for part in split_shell_commands(&masked) {
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

        // Check for environment variable injection. Besides the bare
        // `VAR=value` prefix form, cover the `export VAR=` and `env VAR=`
        // wrappers — both assign the variable for subsequent commands
        // (`export` persists it in the shell, `env` sets it for the child),
        // so they are the same injection with extra steps. The list is
        // shared with the shell_exec structured env map (DENIED_ENV_VARS).
        // `env` may carry flags before the assignments (`env -i GEM_HOME=…`
        // slipped past the flag-free form — red-team wave-12 finding).
        static DANGEROUS_ENV_VARS: LazyLock<Regex> = LazyLock::new(|| {
            // readonly/declare/typeset wrappers assign exactly like export
            // (red-team wave-78: `readonly PATH=/tmp:$PATH`). `local` is
            // deliberately excluded — function-scope PATH customization is a
            // legitimate shell idiom and bare `local` outside functions is a
            // no-op error anyway.
            Regex::new(&format!(
                r"(?i)^\s*(?:(?:export|env|readonly|declare|typeset)(?:\s+-\w+)*\s+)?({})\s*=",
                DENIED_ENV_VARS.join("|")
            ))
            .expect("Invalid regex")
        });

        for part in split_shell_commands(&normalized) {
            let part_trimmed = part.trim();
            if DANGEROUS_ENV_VARS.is_match(part_trimmed) {
                return Err(SelfwareError::Safety(SafetyError::BlockedEnvInjection));
            }
        }

        // `env` segments with flag arguments in between (`env -u FOO
        // GEM_HOME=…`) defeat the prefix-anchored form above — catch a denied
        // assignment anywhere in an env invocation (wave-12 sibling sweep).
        static DANGEROUS_ENV_SEGMENT: LazyLock<Regex> = LazyLock::new(|| {
            // Unanchored: export/env forms not at segment start too (wave-17:
            // `(export LD_LIBRARY_PATH=/tmp; cat /etc/hosts)` — the subshell
            // paren broke the old ^ anchor).
            Regex::new(&format!(
                r"(?i)\b(export|env|readonly|declare|typeset)(?:\s+-\w+)*\s+({})\s*=",
                DENIED_ENV_VARS.join("|")
            ))
            .expect("Invalid regex")
        });
        // env-segment catch-all (wave-12 form, restored after the unanchored
        // rewrite narrowed it): `env -u LD_PRELOAD -i LD_PRELOAD=/tmp/x.so
        // bash` — flag ARGUMENTS sit between env and the assignment, which
        // the tight form above cannot skip.
        static DANGEROUS_ENV_CATCHALL: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(&format!(
                r"(?i)^\s*env\b.*\b({})\s*=",
                DENIED_ENV_VARS.join("|")
            ))
            .expect("Invalid regex")
        });
        for part in split_shell_commands(&normalized) {
            let part_trimmed = part.trim();
            if DANGEROUS_ENV_VARS.is_match(part_trimmed)
                || DANGEROUS_ENV_SEGMENT.is_match(part_trimmed)
                || DANGEROUS_ENV_CATCHALL.is_match(part_trimmed)
            {
                return Err(SelfwareError::Safety(SafetyError::BlockedEnvInjection));
            }
        }

        // ENV is deliberately NOT in DENIED_ENV_VARS (`ENV=production` is
        // everyday), but the sh/bash startup-file attack lives in the same
        // var: `ENV=/tmp/evil.sh; sh -c 'id'` (wave-35). Refuse only
        // PATH-SHAPED values (absolute, dot-relative, home-relative) —
        // plain identifiers like production/staging stay legal.
        // R_PROFILE/R_PROFILE_USER are the same class for the R interpreter
        // (red-team wave-78: `export R_PROFILE=./.profile && R -q` sources
        // the file at interpreter start, BASH_ENV-style).
        static ENV_PATH_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"(?i)(^|\||;|&&|\()\s*(export\s+)?(env|r_profile|r_profile_user)\s*=\s*['"]?(?://|/|\./|\.\./|~)"#)
                .expect("Invalid regex")
        });
        for part in split_shell_commands(&normalized) {
            if ENV_PATH_ASSIGNMENT.is_match(part.trim()) {
                return Err(SelfwareError::Safety(SafetyError::BlockedEnvInjection));
            }
        }

        // Indirect assignment through a variable NAME (wave-35:
        // `VAR='LD_PRELOAD'; export $VAR=/tmp/exploit.so`) — no static list
        // can know the name, but `export $<name>=` is itself the tell.
        // Numbered git config injection (wave-69: GIT_CONFIG_KEY_0=
        // core.editor + GIT_CONFIG_VALUE_0=<payload>) — the exact-match
        // list cannot wildcard the numbering.
        static GIT_CONFIG_NUMBERED: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?i)\bgit_config_(key|value)_\d+\s*=").expect("Invalid regex")
        });
        for part in split_shell_commands(&normalized) {
            if GIT_CONFIG_NUMBERED.is_match(part.trim()) {
                return Err(SelfwareError::Safety(SafetyError::BlockedEnvInjection));
            }
        }

        static INDIRECT_EXPORT: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\bexport\s+\$\w+\s*=").expect("Invalid regex"));
        // Substitution-built assignment (wave-64: `export $(echo -n 'LD_'
        // 'PRELOAD=' '/tmp/v.so')`, `export $(printf '%s=%s' 'BASH_ENV'
        // '/tmp/rc')`, `export $(cat /tmp/env.txt)`). Documented trade-off:
        // the `export $(cat config.env)` idiom is refused too — agents load
        // env files through the structured env map or file tools instead
        // (AGENTS.md rule 5: the bug class, not the file).
        static INDIRECT_EXPORT_SUBST: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\bexport\s+\$\(").expect("Invalid regex"));
        for part in split_shell_commands(&normalized) {
            if INDIRECT_EXPORT.is_match(part.trim()) || INDIRECT_EXPORT_SUBST.is_match(part.trim())
            {
                return Err(SelfwareError::Safety(SafetyError::BlockedEnvInjection));
            }
        }

        // PS1/PS2/PS4 assignments are everyday theming — but the prompt
        // vars undergo command substitution on every print, so a value
        // containing $(…) or backticks is a callback channel (wave-46:
        // `PS4="+$(id)"`). Refuse substitution-bearing values only.
        static PS_SUBST_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?i)\b(ps1|ps2|ps4)\s*=\s*[^;&|]*(\$\(|`)").expect("Invalid regex")
        });
        for part in split_shell_commands(&normalized) {
            if PS_SUBST_ASSIGNMENT.is_match(part.trim()) {
                return Err(SelfwareError::Safety(SafetyError::BlockedEnvInjection));
            }
        }

        // The dedicated git_push tool enforces protected_branches, but an
        // agent can bypass that entirely by invoking git directly through
        // shell_exec/container_exec -- catch explicit protected-branch
        // targets here too (see git_push_protected_branch_target's doc
        // comment for what this does and doesn't catch). Split on pipeline
        // boundaries (not just `;`/`&&`/`||`) so `… | git push origin main`
        // still sees git as the segment's command word.
        for part in split_shell_pipeline(&normalized) {
            if let Some(branch) =
                git_push_protected_branch_target(part.trim(), &self.config.protected_branches)
            {
                return Err(SelfwareError::Safety(
                    SafetyError::BlockedProtectedBranchPush {
                        branch,
                        protected: self.config.protected_branches.clone(),
                    },
                ));
            }
        }

        Ok(())
    }

    /// Conservative `allowed_paths`/`denied_paths` check for shell command
    /// bodies (whole-repo review, Safety P1).
    ///
    /// `file_*` tools validate their `path` argument, but shell tools
    /// (`shell_exec`, `pty_shell`, `container_exec`, `process_start`, npm/pip
    /// scripts) used to check only the `cwd` argument — the command body
    /// could read or write any path, asymmetric with the blocked
    /// `file_read`/`file_write` of the very same path. This closes the gap
    /// with a deliberately conservative heuristic (fail-open where unsure,
    /// to keep everyday commands working):
    ///
    /// - Candidate paths are (a) the non-flag operands of common file verbs
    ///   (see [`FILE_TARGET_VERBS`] — `dd` contributes its `if=`/`of=`
    ///   values, `chmod` skips its mode operand) and (b) any token that
    ///   *looks like an explicit path*: absolute (`/x`, `C:/x`),
    ///   home-relative (`~/x`), or dot-relative (`./x`, `../x`). Bare words
    ///   (`README.md`) are only candidates after a file verb, so strings,
    ///   URLs, and parenthesized arguments are never treated as paths.
    /// - The command word itself (running system binaries like
    ///   `/usr/bin/python` stays allowed), redirect targets (`>`/`>>` —
    ///   owned by the denied-path redirect guard above, which deliberately
    ///   allows scratch writes like `2> /tmp/x.log`), and `tee`/`sponge`
    ///   operands (owned by the tee guard above) are excluded.
    /// - Each candidate is matched against `denied_paths` with the same
    ///   matcher the redirect guard uses, and — only when `allowed_paths`
    ///   is non-empty — must match the allow-list after lexical resolution
    ///   against the working dir (with a leading `~` expanded). Failures
    ///   return the same errors the file tools return
    ///   (`PathDeniedPattern`/`PathNotAllowed`).
    /// - Operands of READ-ONLY commands (see [`READ_ONLY_COMMANDS`] — and
    ///   `find` without `-delete`/`-exec`) are exempt from the allow-list:
    ///   reads can't destroy anything, so `cat /etc/hosts`,
    ///   `tail /tmp/build.log`, or `ls /usr/local/bin` work even with the
    ///   default `allowed_paths = ["./**"]`. The `denied_paths` check
    ///   (`.env`, `.ssh`, `secrets`, …) still fires on those reads.
    ///   Write/execute verbs keep full allow-list enforcement.
    ///
    /// Documented fail-open gaps: paths hidden in `--flag=VALUE` pairs,
    /// command substitution `$(…)`, nested `sh -c '…'`, and remote
    /// `host:path` operands are not extracted.
    fn check_shell_command_paths(&self, cmd: &str) -> Result<()> {
        for segment in split_shell_pipeline(cmd) {
            let Some(tokens) = shlex::split(segment) else {
                continue;
            };
            let Some(cmd_idx) = command_word_index(&tokens) else {
                continue;
            };
            let verb = command_basename(&tokens[cmd_idx]);
            // tee/sponge operands are write targets owned by the tee guard
            // (denied-only semantics, same as redirects) — skip them here.
            if verb == "tee" || verb == "sponge" {
                continue;
            }
            // Read-only commands may read absolute paths outside the cwd;
            // `find` only when it can't write/execute (`-delete`, `-exec`).
            let read_only = READ_ONLY_COMMANDS.contains(&verb)
                || (verb == "find"
                    && !tokens.iter().any(|t| {
                        matches!(
                            t.as_str(),
                            "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir"
                        )
                    }));
            let enforce_allowlist = !read_only;
            let is_file_verb = FILE_TARGET_VERBS.contains(&verb);
            let mut chmod_mode_seen = false;
            let mut flags_done = false;
            // What to do with the token after a redirect operator: `>`
            // targets belong to the redirect guard (skip), `<` targets are
            // reads we must check (take), `<<` heredoc delimiters are not
            // paths (skip).
            enum Redirect {
                None,
                Skip,
                Take,
            }
            let mut pending = Redirect::None;
            for (i, tok) in tokens.iter().enumerate() {
                if i == cmd_idx {
                    continue; // the program being run is not a path operand
                }
                match pending {
                    Redirect::Skip => {
                        pending = Redirect::None;
                        continue;
                    }
                    Redirect::Take => {
                        pending = Redirect::None;
                        self.check_shell_path_candidate(tok, enforce_allowlist)?;
                        continue;
                    }
                    Redirect::None => {}
                }
                let stripped = tok.trim_start_matches(|c: char| c.is_ascii_digit());
                if let Some(rest) = stripped.strip_prefix("<<") {
                    // Heredoc: `<<EOF` glues the delimiter word onto the
                    // token; `<< EOF` takes the next one. Neither is a path.
                    if rest.is_empty() {
                        pending = Redirect::Skip;
                    }
                    continue;
                }
                if let Some(rest) = stripped.strip_prefix('<') {
                    // Input redirect = a read target, glued (`</etc/passwd`)
                    // or separate (`< /etc/passwd`).
                    if rest.is_empty() {
                        pending = Redirect::Take;
                    } else {
                        self.check_shell_path_candidate(rest, enforce_allowlist)?;
                    }
                    continue;
                }
                if matches!(stripped, ">" | ">>" | ">|") {
                    // A separated output-redirect target (`> file`) belongs
                    // to the redirect guard — skip it here. Glued forms
                    // (`>file`, `2>file`, `>&1`) keep no state: their
                    // targets are extracted from the raw command by the
                    // redirect guard.
                    pending = Redirect::Skip;
                    continue;
                }
                if stripped.starts_with('>') {
                    continue;
                }
                if tok == "--" {
                    flags_done = true;
                    continue;
                }
                if !flags_done && tok.starts_with('-') && tok.len() > 1 {
                    continue; // flag (possibly glued `-oFILE` — a documented gap)
                }
                if is_file_verb {
                    if verb == "dd" {
                        if let Some(v) = tok.strip_prefix("if=").or_else(|| tok.strip_prefix("of="))
                        {
                            self.check_shell_path_candidate(v, enforce_allowlist)?;
                        }
                        continue; // bs=/count=/… operands are not paths
                    }
                    if verb == "chmod" && !chmod_mode_seen {
                        chmod_mode_seen = true; // the mode operand is not a path
                        continue;
                    }
                    self.check_shell_path_candidate(tok, enforce_allowlist)?;
                    continue;
                }
                if looks_like_explicit_path(tok) {
                    self.check_shell_path_candidate(tok, enforce_allowlist)?;
                }
            }
        }
        Ok(())
    }

    /// Validate one extracted shell-command path token against
    /// `denied_paths` (same matcher as redirect targets) and, when
    /// `allowed_paths` is non-empty AND `enforce_allowlist` is set, against
    /// the allow-list. A leading `~` is expanded so home-relative tokens
    /// see the same absolute path the shell would use. Operands of
    /// read-only commands pass `enforce_allowlist = false` (reads can't
    /// destroy; the denied-path check still applies).
    fn check_shell_path_candidate(&self, candidate: &str, enforce_allowlist: bool) -> Result<()> {
        let expanded = expand_home_token(candidate);
        {
            let forms: &[&str] = if expanded == candidate {
                &[candidate]
            } else {
                &[candidate, expanded.as_str()]
            };
            for form in forms {
                if let Some(pattern) = redirect_target_matches_denied(
                    form,
                    &self.working_dir,
                    &self.config.denied_paths,
                ) {
                    return Err(SelfwareError::Safety(SafetyError::PathDeniedPattern {
                        pattern,
                    }));
                }
            }
        }
        if !enforce_allowlist || self.config.allowed_paths.is_empty() {
            return Ok(());
        }
        let resolved = resolve_redirect_target_lexical(&expanded, &self.working_dir);
        use crate::safety::path_validator::PathValidator;
        let validator = PathValidator::new(&self.config, self.working_dir.clone());
        let allowed = validator
            .is_path_in_allowed_list(&resolved, candidate)
            .unwrap_or(false);
        if !allowed {
            return Err(SelfwareError::Safety(SafetyError::PathNotAllowed {
                path: resolved,
            }));
        }
        Ok(())
    }

    /// Scan content for hardcoded secrets
    fn check_content_for_secrets(&self, content: &str) -> Result<()> {
        let result = self.security_scanner.scan_content(content, None, "");
        // Only actual secrets block a write. Compliance findings (e.g. the
        // OWASP-A02 `md5(|sha1(` Cryptography rule) must not stop legitimate
        // code from being written, and must not be misreported as secrets.
        let blocked: Vec<_> = result
            .findings
            .iter()
            .filter(|f| {
                f.severity >= SecuritySeverity::High
                    && f.category == SecurityCategory::HardcodedSecret
            })
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
            "/opt", "/run", "/dev",
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
        // Credential directories are the .ssh class (red-team wave-76:
        // `/home/user/.aws:/root/.aws` ships host cloud credentials into the
        // container). Same stance for every cloud/tooling credential home.
        const CREDENTIAL_DIRS: &[&str] = &[
            "/.aws",
            "/.gnupg",
            "/.azure",
            "/.kube",
            "/.config/gcloud",
            "/.docker",
        ];
        if CREDENTIAL_DIRS.iter().any(|d| {
            host_path.contains(d)
                || host_path == d.trim_start_matches('/')
                || host_path.starts_with(&format!("~{d}/"))
                || host_path == format!("~{d}")
        }) {
            return Err(SelfwareError::Safety(
                SafetyError::ContainerCredentialMount {
                    mount: mount.to_string(),
                },
            ));
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

    /// Check an endpoint URL for SSRF.
    ///
    /// Shared body of the HTTP-request, browser, and vision-endpoint checks
    /// (page-control URLs additionally allow `file://`, so they are separate).
    fn check_endpoint_url(&self, url: &str) -> Result<()> {
        // URLs carrying shell substitution (`$(…)`, backticks, `${…}`) are an
        // exfiltration channel when any layer builds the request through a
        // shell (red-team finding: k8s serviceaccount token smuggled into a
        // query param via $(head … token | base64)).
        if url.contains("$(") || url.contains('`') || url.contains("${") {
            return Err(SelfwareError::Safety(
                SafetyError::BlockedUrlShellSubstitution,
            ));
        }
        // Sensitive local paths referenced in the URL — the agent shipping
        // `/root/.ssh/id_rsa` or `/etc/hostname` as a query param to a
        // remote host is exfiltration, not an API call (wave-9 finding).
        let lower_url = url.to_lowercase();
        for marker in [
            "/etc/", "/root/", "/home/", ".ssh", ".env", "id_rsa", "passwd",
        ] {
            if lower_url.contains(marker) {
                return Err(SelfwareError::Safety(
                    SafetyError::BlockedUrlShellSubstitution,
                ));
            }
        }
        // Webhook URLs are secrets AND exfil sinks: posting to a Slack
        // webhook-shape URL sends whatever the agent includes straight out.
        // Discord webhooks are the same class (wave-32). The /services/
        // T…/B…/ path signature is blocked on ANY host — a lookalike domain
        // (slack-webhook.evil.com) is the same sink in a costume (wave-57).
        static WEBHOOK_PATH_SIG: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?i)/services/t[0-9a-z]{5,}/b[0-9a-z]{5,}/").expect("Invalid regex")
        });
        if lower_url.contains("hooks.slack.com/services/")
            || lower_url.contains("discord.com/api/webhooks/")
            || lower_url.contains("discordapp.com/api/webhooks/")
            || WEBHOOK_PATH_SIG.is_match(&lower_url)
        {
            return Err(SelfwareError::Safety(
                SafetyError::BlockedUrlShellSubstitution,
            ));
        }
        // Base64 blobs in query params that DECODE to credential-shaped
        // content are smuggled exfil (red-team wave-5:
        // ?data=eyJ1c2Vy...=  -> {"username":"admin","password":"..."}). A
        // blob that decodes to something credential-free (pagination
        // cursors, state params) passes.
        for blob in base64_like_query_blobs(url) {
            if let Some(decoded) = decode_base64_lenient(&blob) {
                let lower = decoded.to_lowercase();
                if ["pass", "secret", "token", "api_key", "apikey", "credential"]
                    .iter()
                    .any(|marker| lower.contains(marker))
                {
                    return Err(SelfwareError::Safety(
                        SafetyError::BlockedUrlShellSubstitution,
                    ));
                }
            }
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

    /// Check HTTP request URL for SSRF
    fn check_http_request_url(&self, url: &str) -> Result<()> {
        self.check_endpoint_url(url)
    }

    /// Check browser URL
    fn check_browser_url(&self, url: &str) -> Result<()> {
        self.check_endpoint_url(url)
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
        self.check_endpoint_url(url)
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

/// Normalize a shell command, with quoted segments restored into the result.
pub fn normalize_shell_command(cmd: &str) -> String {
    normalize_shell_command_impl(cmd, true)
}

/// Normalize a shell command but leave quoted segments as opaque
/// placeholders. This is the form dangerous-command patterns are matched
/// against, so quoted prose — commit messages, echoed strings — cannot
/// trigger them (e.g. `git commit -m "revert the rm -rf / guard"` or
/// `echo "never run chown -R /"`). Patterns that must inspect quoted
/// payloads (python -c '…', eval "…") match the restored form from
/// [`normalize_shell_command`] instead.
pub(crate) fn normalize_shell_command_masked(cmd: &str) -> String {
    normalize_shell_command_impl(cmd, false)
}

fn normalize_shell_command_impl(cmd: &str, restore_quotes: bool) -> String {
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

    // Restore quoted segments (unless the caller wants the masked scaffold
    // for dangerous-pattern matching — see normalize_shell_command_masked).
    if restore_quotes {
        for (idx, segment) in quoted_segments.iter().enumerate() {
            let placeholder = format!("\x00\x01{}\x00", idx);
            result = result.replace(&placeholder, segment);
        }
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

/// Replace `$IFS`/`${IFS}` (either case) with a space in a CHECK-ONLY copy
/// of a shell command. Shells expand `$IFS` to whitespace, so
/// `rm$IFS-rf$IFS/target` runs as `rm -rf /target` while evading literal
/// `rm\s+` patterns; matching against this copy closes that gap. This is
/// only ever applied to strings used for pattern matching — never to the
/// command that executes.
fn expand_ifs_for_matching(cmd: &str) -> String {
    cmd.replace("${IFS}", " ")
        .replace("$IFS", " ")
        .replace("${ifs}", " ")
        .replace("$ifs", " ")
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

/// Split a shell command into pipeline segments: splits on `|`, `||`,
/// `&&`, `;` and a backgrounding `&` appearing outside single/double
/// quotes. Unlike [`split_shell_commands`] this also splits on a bare `|`
/// so each stage of a pipeline can be inspected on its own (e.g. the `tee`
/// in `echo x | tee file`).
fn split_shell_pipeline(cmd: &str) -> Vec<&str> {
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

        if !in_quotes && matches!(c, b';' | b'|' | b'&') {
            // `||` and `&&` are two-byte operators (`;;` is not a thing).
            let is_double = c != b';' && i + 1 < bytes.len() && bytes[i + 1] == c;
            if start < i {
                parts.push(&cmd[start..i]);
            }
            start = i + if is_double { 2 } else { 1 };
            if is_double {
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

/// Whether a token is a leading `VAR=value` environment assignment prefix
/// (`FOO=bar cmd …`), which is not the command word.
fn is_env_assignment(tok: &str) -> bool {
    let Some(eq) = tok.find('=') else {
        return false;
    };
    let name = &tok[..eq];
    !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The basename of a command word (`/usr/bin/tee` → `tee`).
fn command_basename(word: &str) -> &str {
    word.rsplit(['/', '\\']).next().unwrap_or(word)
}

/// Locate the command word of one pipeline segment: the first token after
/// any `VAR=value` prefixes, looking through ONE leading `sudo`/`doas`
/// wrapper (the canonical `… | sudo tee /root/file` form). Flags known to
/// take a separate value (`sudo -u root …`) consume one extra token.
fn command_word_index(tokens: &[String]) -> Option<usize> {
    let mut idx = 0;
    while tokens.get(idx).is_some_and(|t| is_env_assignment(t)) {
        idx += 1;
    }
    if tokens
        .get(idx)
        .is_some_and(|t| matches!(command_basename(t), "sudo" | "doas"))
    {
        idx += 1;
        while let Some(t) = tokens.get(idx) {
            if !t.starts_with('-') || t.as_str() == "-" {
                break;
            }
            idx += 1;
            if matches!(
                t.as_str(),
                "-u" | "--user"
                    | "-g"
                    | "--group"
                    | "-h"
                    | "--host"
                    | "-p"
                    | "--prompt"
                    | "-C"
                    | "-D"
                    | "--chdir"
                    | "-R"
                    | "-U"
            ) {
                idx += 1;
            }
        }
    }
    (idx < tokens.len()).then_some(idx)
}

/// Whether a token is shell plumbing rather than an operand: redirections
/// (`>`, `2>`, `>>`, `<`, `<<`) or command separators that survived
/// quoting.
fn is_shell_metachar_token(tok: &str) -> bool {
    let stripped = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    stripped.starts_with('>')
        || stripped.starts_with('<')
        || stripped.starts_with('|')
        || stripped.starts_with('&')
        || stripped.starts_with(';')
}

/// Extract the file targets of `tee` and `sponge` invocations anywhere in a
/// shell command's pipeline. Both write (or append, `-a`) their stdin to
/// the named files, so they are write primitives exactly like `>`/`>>` —
/// but the redirect guard cannot see them because no `>` appears in the
/// command. Quoting is handled via `shlex`; the command word is found per
/// pipeline segment after `VAR=value` prefixes and one `sudo`/`doas`
/// wrapper; flags (`-a`, `--append`) are skipped and collection stops at
/// shell plumbing tokens. Everything else after the command word is a file
/// by the tools' own syntax.
pub fn shell_tee_write_targets(cmd: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for segment in split_shell_pipeline(cmd) {
        let Some(tokens) = shlex::split(segment) else {
            continue;
        };
        let Some(cmd_idx) = command_word_index(&tokens) else {
            continue;
        };
        if !matches!(command_basename(&tokens[cmd_idx]), "tee" | "sponge") {
            continue;
        }
        let mut flags_done = false;
        for tok in &tokens[cmd_idx + 1..] {
            if is_shell_metachar_token(tok) {
                break;
            }
            if tok == "--" {
                flags_done = true;
                continue;
            }
            if !flags_done && tok.starts_with('-') && tok.len() > 1 {
                continue; // -a, --append, …
            }
            targets.push(tok.clone());
        }
    }
    targets
}

/// File verbs whose non-flag operands are file targets by their own
/// syntax. `dd` is special-cased (`if=`/`of=` operands) and `chmod` skips
/// its mode operand; see [`SafetyChecker::check_shell_command_paths`].
const FILE_TARGET_VERBS: &[&str] = &[
    "cat", "cp", "mv", "rm", "chmod", "ln", "mkdir", "touch", "head", "tail", "less", "more", "dd",
    "install", "rsync", "scp",
    // Content-dumping readers (red-team wave-15): `grep -i password .env`
    // bypassed the denied-path check because grep was READ_ONLY but not a
    // file verb, so its `.env` operand was never a candidate — the same
    // read `cat .env` is denied. Their first operand is a PATTERN, but a
    // spurious candidate is harmless: bare words resolve cwd-relative and
    // pass the allow-list, while denied patterns (.env/.ssh/secrets) still
    // fire. awk/sed/jq/xargs remain documented fail-open (see above).
    "grep", "rg", "diff",
    // Encoding readers (wave-19): `base64 -w0 .env | xargs curl` — base64
    // with a file OPERAND reads the file; not being a file verb left .env
    // unchecked. xxd/od/hexdump are the same content channel.
    "base64", "xxd", "od", "hexdump",
];

/// Commands whose operands are only ever READ: their absolute-path operands
/// outside the working dir are exempt from the `allowed_paths` allow-list
/// (`cat /etc/hosts`, `tail /tmp/build.log`, `ls /usr/local/bin` are
/// everyday reads). `denied_paths` still applies to them. `find` is NOT in
/// this list — it is handled separately because `-delete`/`-exec` make it
/// a write/execute primitive; only find invocations without those flags
/// get the read-only exemption.
const READ_ONLY_COMMANDS: &[&str] = &[
    "cat",
    "head",
    "tail",
    "less",
    "more",
    "ls",
    "file",
    "stat",
    "wc",
    "grep",
    "rg",
    "diff",
    "md5",
    "md5sum",
    "shasum",
    "sha1sum",
    "sha256sum",
];

/// Whether a shell token has the shape of an explicit filesystem path:
/// absolute (`/x`, `C:/x`, `C:\x`), home-relative (`~/x`), or dot-relative
/// (`./x`, `../x`). Tokens containing substitution characters (`$`,
/// backtick, parentheses) are excluded — they are shell-expansion
/// artifacts, not literal paths. Bare words (`README.md`) deliberately do
/// not qualify: outside file-verb operands they are far more often strings
/// than paths.
fn looks_like_explicit_path(tok: &str) -> bool {
    if tok.contains(['$', '`', '(', ')']) {
        return false;
    }
    if tok.starts_with('/')
        || tok.starts_with("~/")
        || tok.starts_with("./")
        || tok.starts_with("../")
        || tok.starts_with(".\\")
        || tok.starts_with("..\\")
    {
        return true;
    }
    // Windows drive-letter absolute path (both separator flavors).
    let b = tok.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

/// Expand a leading `~`/`~/` against the user's home directory so a
/// home-relative shell token is validated against the same absolute path
/// the shell would resolve it to. `~user/...` forms are left untouched.
fn expand_home_token(tok: &str) -> String {
    if tok == "~" || tok.starts_with("~/") {
        let home =
            std::env::var_os("HOME").or_else(|| dirs::home_dir().map(|p| p.into_os_string()));
        if let Some(home) = home {
            return format!("{}{}", home.to_string_lossy(), &tok[1..]);
        }
    }
    tok.to_string()
}

/// Whether the platform's default shell treats `\` as an escape character
/// outside quotes. Unix `sh` does; Windows `cmd`/`powershell` do not — there
/// `\` is a literal path separator, so `C:\work\.env` must keep its
/// backslashes or the redirect target gets mangled (`C:work.env`) and evades
/// the denied_paths match entirely.
fn shell_treats_backslash_as_escape() -> bool {
    !cfg!(target_os = "windows")
}

/// Extract the file targets of output redirections (`>`, `>>`) from a shell
/// command. `>` inside single/double quotes is ignored, as is descriptor
/// duplication that writes no file (`2>&1`, `>&2`) and process substitution
/// (`>(...)`). Fd-qualified redirects that DO write a file (`2> err.log`,
/// `&> all.log`, `>| clobbered`) are included, quotes around the target are
/// stripped. Input redirects (`<`) and here-docs (`<<`) are not writes and
/// are ignored.
pub fn shell_output_redirect_targets(cmd: &str) -> Vec<String> {
    let chars: Vec<char> = cmd.chars().collect();
    let mut targets = Vec::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // Backslash escapes the next character (outside single quotes) in
        // Unix shells; on Windows it is a literal path separator.
        if c == '\\' && !in_single && shell_treats_backslash_as_escape() {
            i += 2;
            continue;
        }
        if c == '\'' && !in_double {
            in_single = !in_single;
            i += 1;
            continue;
        }
        if c == '"' && !in_single {
            in_double = !in_double;
            i += 1;
            continue;
        }
        if c != '>' || in_single || in_double {
            i += 1;
            continue;
        }

        // A `>` outside quotes. Consume the operator: `>`, `>>`, then an
        // optional `|` (`>|` noclobber override still writes a file).
        let mut j = i + 1;
        if j < chars.len() && chars[j] == '>' {
            j += 1;
        }
        // Descriptor duplication (`>&1`, `>>&2`) and process substitution
        // (`>(...)`) write no file.
        if j < chars.len() && (chars[j] == '&' || chars[j] == '(') {
            i = j + 1;
            continue;
        }
        if j < chars.len() && chars[j] == '|' {
            j += 1;
        }
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }

        // Read the target token, stripping quotes (quoted segments may
        // contain whitespace) and resolving backslash escapes the way the
        // platform's shell would (Unix only — Windows shells treat `\` as a
        // literal path separator).
        let mut token = String::new();
        let mut token_quote: Option<char> = None;
        while j < chars.len() {
            let t = chars[j];
            if let Some(q) = token_quote {
                if t == q {
                    token_quote = None;
                } else {
                    token.push(t);
                }
                j += 1;
                continue;
            }
            if t.is_whitespace() || matches!(t, ';' | '|' | '&' | '<' | '>' | '(' | ')') {
                break;
            }
            if t == '\'' || t == '"' {
                token_quote = Some(t);
                j += 1;
                continue;
            }
            if t == '\\' && j + 1 < chars.len() && shell_treats_backslash_as_escape() {
                token.push(chars[j + 1]);
                j += 2;
                continue;
            }
            token.push(t);
            j += 1;
        }
        // A token still starting with `&` (`foo>&2` without whitespace) is
        // descriptor duplication, not a file.
        if !token.is_empty() && !token.starts_with('&') {
            targets.push(token);
        }
        i = j.max(i + 1);
    }
    targets
}

/// Match a shell output-redirect target against the configured `denied_paths`
/// globs, mirroring `PathValidator`'s matching: full-path glob match on both
/// the raw target and the working-dir-resolved form, plus basename matching
/// for filename-only patterns (e.g. `.env`). Resolution is lexical only (no
/// filesystem access), so prospective output paths work. Returns the matched
/// pattern, or `None` when the target is not denied.
///
/// Matching is separator-agnostic: both `/` and `\` are treated as path
/// separators and drive-letter prefixes (`C:\...`, `C:/...`) count as
/// absolute, so Windows-shaped targets resolve and match identically on every
/// host OS. Relying on `std::path::Path` alone would only recognize the HOST
/// separator and let `echo x > C:\work\.env` slip past a `.env` deny glob.
pub(super) fn redirect_target_matches_denied(
    target: &str,
    working_dir: &std::path::Path,
    denied_paths: &[String],
) -> Option<String> {
    if denied_paths.is_empty() {
        return None;
    }
    let resolved = resolve_redirect_target_lexical(target, working_dir);
    let raw_glob = target.trim_start_matches("./").replace('\\', "/");

    for pattern in denied_paths {
        let compiled = match glob::Pattern::new(&super::to_glob_form(pattern)) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if compiled.matches(&resolved) || compiled.matches(&raw_glob) {
            return Some(pattern.clone());
        }
        // Filename-only patterns (no separator, e.g. ".env") also match by
        // basename; the basename is taken across both separator kinds.
        if !pattern.contains('/') && !pattern.contains('\\') {
            let base = resolved.rsplit('/').next().unwrap_or(resolved.as_str());
            if !base.is_empty() && compiled.matches(base) {
                return Some(pattern.clone());
            }
        }
    }
    None
}

/// Lexically resolve a shell redirect target against `working_dir`, treating
/// both `/` and `\` as path separators, and return the result in
/// forward-slash (glob) form with `.`/`..` segments resolved. Purely lexical
/// — no filesystem access — so prospective output paths and foreign-OS path
/// shapes resolve identically on any host.
fn resolve_redirect_target_lexical(target: &str, working_dir: &std::path::Path) -> String {
    let target_fwd = target.replace('\\', "/");
    let joined = if redirect_target_is_absolute(&target_fwd) {
        target_fwd
    } else {
        let wd = working_dir.to_string_lossy().replace('\\', "/");
        format!("{}/{}", wd.trim_end_matches('/'), target_fwd)
    };

    // Resolve `.` and `..` segments lexically. `..` never pops past the
    // root, a drive letter (`C:`), or a leading `..` of a relative path.
    let mut segments: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if matches!(segments.last(), Some(&last) if last != ".." && !is_drive_letter_segment(last))
                {
                    segments.pop();
                }
            }
            s => segments.push(s),
        }
    }
    let prefix = if joined.starts_with('/') { "/" } else { "" };
    format!("{}{}", prefix, segments.join("/"))
}

/// A redirect target is absolute when rooted (`/x`, or `\x` after separator
/// normalization) or drive-qualified (`C:/x`).
fn redirect_target_is_absolute(target_fwd: &str) -> bool {
    if target_fwd.starts_with('/') {
        return true;
    }
    let bytes = target_fwd.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

/// Whether a normalized path segment is a Windows drive letter (`C:`).
fn is_drive_letter_segment(seg: &str) -> bool {
    let bytes = seg.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Detect `git push` invocations (via shell_exec/container_exec, not the
/// dedicated git_push tool) that explicitly target a protected branch, and
/// return the offending branch name if found.
///
/// `cmd` is ONE chain segment (already split on `;`/`&&`/`||`). Git only
/// counts when it IS the command — the first token of the segment, either
/// bare (`git`) or an absolute/relative path ending in `/git`
/// (`/usr/bin/git`). Searching for `git` anywhere would both false-positive
/// on `echo git push origin main` and miss the `/usr/bin/git` spelling.
///
/// Only catches an *explicit* branch argument (`git push origin main`,
/// `git push origin --delete main`, `git push origin :main`,
/// `git push --force-with-lease origin main`, etc.). A bare `git push` with
/// no branch argument pushes whatever the current branch's configured
/// upstream is, which can't be determined from the command string alone --
/// that case isn't caught here.
fn git_push_protected_branch_target(cmd: &str, protected_branches: &[String]) -> Option<String> {
    if protected_branches.is_empty() {
        return None;
    }
    // Unbalanced quotes make shlex give up — and previously made this guard
    // give up too: `git push -f origin main'` returned None from split() and
    // sailed through (red-team wave-2 finding). Fall back to whitespace
    // tokens with quotes stripped — slightly cruder, strictly safer.
    let tokens = shlex::split(cmd).unwrap_or_else(|| {
        cmd.split_whitespace()
            .map(|t| t.trim_matches(['"', '\'']).to_string())
            .collect()
    });
    // Resolve the actual command word, skipping `VAR=value` prefixes and a
    // `sudo`/`doas` wrapper (with its flags) — those prefixes must not defeat
    // the guard (`sudo git push origin main` pushes just as hard).
    let cmd_idx = command_word_index(&tokens)?;
    let command = &tokens[cmd_idx];
    if command != "git" && !command.ends_with("/git") {
        return None;
    }
    let rest = &tokens[cmd_idx + 1..];
    let push_pos = rest.iter().position(|t| t == "push")?;
    let args = &rest[push_pos + 1..];

    let mut delete_next = false;
    let mut candidates: Vec<&str> = Vec::new();
    for arg in args {
        if delete_next {
            candidates.push(arg.as_str());
            delete_next = false;
            continue;
        }
        if arg == "--delete" || arg == "-d" {
            delete_next = true;
            continue;
        }
        if let Some(stripped) = arg.strip_prefix(':') {
            // `git push origin :branch` (delete-by-refspec syntax)
            candidates.push(stripped);
            continue;
        }
        if arg.starts_with('-') {
            continue; // other flags: --force, --force-with-lease, -u, etc.
        }
        candidates.push(arg.as_str());
    }

    // The remaining non-flag tokens are typically [remote] [branch[:refspec]].
    // Check both sides of a local:remote refspec against protected_branches.
    for candidate in candidates {
        let (local, remote) = match candidate.split_once(':') {
            Some((l, r)) => (l, Some(r)),
            None => (candidate, None),
        };
        for name in [Some(local), remote].into_iter().flatten() {
            if protected_branches.iter().any(|b| b == name) {
                return Some(name.to_string());
            }
        }
    }
    None
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
