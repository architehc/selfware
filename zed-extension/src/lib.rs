//! Selfware Zed Extension
//!
//! Starts `selfware lsp` for editor navigation and exposes `/selfware-graph`
//! in the Assistant for workspace graph exploration.

use zed_extension_api::{
    self as zed, process, serde_json,
    settings::{ContextServerSettings, LspSettings},
    ContextServerConfiguration, ContextServerId, LanguageServerId, Project, Result, SlashCommand,
    SlashCommandArgumentCompletion, SlashCommandOutput, SlashCommandOutputSection, Worktree,
};

const SELFWARE_CONTEXT_SERVER_ID: &str = "selfware";
const SELFWARE_SERVER_ID: &str = "selfware";
const GRAPH_COMMAND: &str = "selfware-graph";
const DEFAULT_GRAPH_FORMAT: &str = "mermaid";

struct SelfwareExtension {
    cached_binary_path: Option<String>,
    cached_worktree_root: Option<String>,
}

impl SelfwareExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
            cached_worktree_root: None,
        }
    }

    fn resolve_binary(
        &mut self,
        worktree: &Worktree,
        settings: Option<&serde_json::Value>,
    ) -> std::result::Result<String, String> {
        let root = worktree.root_path();
        if self.cached_worktree_root.as_deref() != Some(&root) {
            self.cached_worktree_root = Some(root.clone());
            self.cached_binary_path = None;
        }

        if let Some(path) = settings
            .and_then(serde_json::Value::as_object)
            .and_then(|config| config.get("binary_path"))
            .and_then(|value| value.as_str())
        {
            self.cached_binary_path = Some(path.to_string());
            return Ok(path.to_string());
        }

        if let Some(path) = &self.cached_binary_path {
            return Ok(path.clone());
        }

        let resolved = pick_binary(
            worktree.which("selfware"),
            &local_build_candidates(&root),
            binary_exists,
        );
        if resolved.cacheable {
            self.cached_binary_path = Some(resolved.path.clone());
        }
        Ok(resolved.path)
    }

    fn resolve_project_binary(
        &mut self,
        command_path: Option<&str>,
        settings: Option<&serde_json::Value>,
    ) -> std::result::Result<String, String> {
        if let Some(path) = command_path {
            self.cached_binary_path = Some(path.to_string());
            return Ok(path.to_string());
        }

        if let Some(path) = binary_path_from_settings(settings) {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        if let Some(path) = &self.cached_binary_path {
            return Ok(path.clone());
        }

        Ok("selfware".to_string())
    }

    fn graph_output(
        &mut self,
        args: Vec<String>,
        worktree: &Worktree,
    ) -> std::result::Result<SlashCommandOutput, String> {
        let settings = LspSettings::for_worktree(SELFWARE_SERVER_ID, worktree)
            .ok()
            .and_then(|s| s.settings);
        let binary = self.resolve_binary(worktree, settings.as_ref())?;

        let (format, focus) = parse_graph_args(args);
        let mut command = process::Command::new(binary)
            .arg("--quiet")
            .arg("--no-color")
            .arg("--ascii")
            .arg("graph")
            .arg(worktree.root_path())
            .arg("--format")
            .arg(format)
            .arg("--max-nodes")
            .arg("40")
            .env("RUST_LOG", "error")
            .env("NO_COLOR", "1");

        if !focus.is_empty() {
            command = command.arg("--focus").arg(focus.clone());
        }

        let output = command.output()?;
        let status = output.status.unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if status != 0 {
            let details = if stderr.is_empty() { stdout } else { stderr };
            return Err(format!("selfware graph failed: {details}"));
        }

        if stdout.is_empty() {
            return Err("selfware graph returned no output".into());
        }

        let label = if focus.is_empty() {
            "Workspace Graph".to_string()
        } else {
            format!("Graph: {focus}")
        };

        Ok(SlashCommandOutput {
            text: stdout.clone(),
            sections: vec![SlashCommandOutputSection {
                range: (0..stdout.len()).into(),
                label,
            }],
        })
    }

    fn base_env(settings: Option<&serde_json::Value>) -> Vec<(String, String)> {
        let mut env = vec![
            ("RUST_LOG".to_string(), "error".to_string()),
            ("NO_COLOR".to_string(), "1".to_string()),
        ];

        if let Some(endpoint) = endpoint_from_settings(settings) {
            push_env_if_missing(&mut env, "SELFWARE_ENDPOINT", endpoint);
        }
        if let Some(model) = model_from_settings(settings) {
            push_env_if_missing(&mut env, "SELFWARE_MODEL", model);
        }

        env
    }
}

impl zed::Extension for SelfwareExtension {
    fn new() -> Self {
        SelfwareExtension::new()
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.settings);
        let binary = self.resolve_binary(worktree, settings.as_ref())?;
        let env = SelfwareExtension::base_env(settings.as_ref());

        Ok(zed::Command {
            command: binary,
            args: vec!["lsp".to_string()],
            env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.settings);

        let endpoint = settings
            .as_ref()
            .and_then(|s| s.get("endpoint"))
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:8000/v1");

        let model = settings
            .as_ref()
            .and_then(|s| s.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("Qwen/Qwen3-Coder-Next-FP8");

        Ok(Some(serde_json::json!({
            "endpoint": endpoint,
            "model": model
        })))
    }

    fn context_server_command(
        &mut self,
        context_server_id: &ContextServerId,
        project: &Project,
    ) -> Result<zed::Command> {
        if context_server_id.as_ref() != SELFWARE_CONTEXT_SERVER_ID {
            return Err(format!(
                "unsupported context server '{}'",
                context_server_id.as_ref()
            ));
        }

        let settings = ContextServerSettings::for_project(context_server_id.as_ref(), project).ok();
        let command_settings = settings.as_ref().and_then(|s| s.command.as_ref());
        let server_settings = settings.as_ref().and_then(|s| s.settings.as_ref());
        let binary = self.resolve_project_binary(
            command_settings.and_then(|command| command.path.as_deref()),
            server_settings,
        )?;

        let args = command_settings
            .and_then(|command| command.arguments.clone())
            .filter(|args| !args.is_empty())
            .unwrap_or_else(|| vec!["mcp-server".to_string()]);

        let mut env = command_settings
            .and_then(|command| command.env.clone())
            .map(|env| env.into_iter().collect::<Vec<_>>())
            .unwrap_or_default();

        for (key, value) in SelfwareExtension::base_env(server_settings) {
            push_env_if_missing(&mut env, &key, value);
        }

        Ok(zed::Command {
            command: binary,
            args,
            env,
        })
    }

    fn context_server_configuration(
        &mut self,
        context_server_id: &ContextServerId,
        _project: &Project,
    ) -> Result<Option<ContextServerConfiguration>> {
        if context_server_id.as_ref() != SELFWARE_CONTEXT_SERVER_ID {
            return Ok(None);
        }

        Ok(Some(ContextServerConfiguration {
            installation_instructions:
                "Build `selfware` locally or point `context_servers.selfware.command.path` at the binary.".to_string(),
            settings_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "endpoint": {
                        "type": "string",
                        "description": "OpenAI-compatible base URL for Selfware model-backed tools."
                    },
                    "model": {
                        "type": "string",
                        "description": "Model ID used by the Selfware MCP server."
                    },
                    "binary_path": {
                        "type": "string",
                        "description": "Optional path to the `selfware` binary if you are not overriding command.path."
                    }
                },
                "additionalProperties": false
            })
            .to_string(),
            default_settings: serde_json::json!({
                "endpoint": "http://localhost:8000/v1",
                "model": "Qwen/Qwen3-Coder-Next-FP8"
            })
            .to_string(),
        }))
    }

    fn complete_slash_command_argument(
        &self,
        command: SlashCommand,
        args: Vec<String>,
    ) -> Result<Vec<SlashCommandArgumentCompletion>, String> {
        if command.name != GRAPH_COMMAND || !args.is_empty() {
            return Ok(Vec::new());
        }

        Ok(vec![
            SlashCommandArgumentCompletion {
                label: "mermaid whole workspace".to_string(),
                new_text: "mermaid".to_string(),
                run_command: true,
            },
            SlashCommandArgumentCompletion {
                label: "mermaid src/lib.rs".to_string(),
                new_text: "mermaid src/lib.rs".to_string(),
                run_command: true,
            },
            SlashCommandArgumentCompletion {
                label: "mermaid knowledge_graph".to_string(),
                new_text: "mermaid knowledge_graph".to_string(),
                run_command: true,
            },
            SlashCommandArgumentCompletion {
                label: "ascii knowledge_graph".to_string(),
                new_text: "ascii knowledge_graph".to_string(),
                run_command: true,
            },
        ])
    }

    fn run_slash_command(
        &self,
        command: SlashCommand,
        args: Vec<String>,
        worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        if command.name != GRAPH_COMMAND {
            return Err(format!("unsupported slash command '{}'", command.name));
        }

        let Some(worktree) = worktree else {
            return Err("selfware-graph requires an open worktree".into());
        };

        let mut extension = SelfwareExtension {
            cached_binary_path: self.cached_binary_path.clone(),
            cached_worktree_root: self.cached_worktree_root.clone(),
        };
        extension.graph_output(args, worktree)
    }
}

/// Bare command name used when no concrete binary was found: the host
/// resolves it against PATH at spawn time.
const PATH_FALLBACK: &str = "selfware";

/// Outcome of binary resolution.
#[derive(Debug, PartialEq, Eq)]
struct ResolvedBinary {
    path: String,
    /// Only a path that was verified to exist may be cached. The bare PATH
    /// fallback is never cached, so a later `cargo build` in the worktree is
    /// picked up on the next resolution instead of being shadowed forever.
    cacheable: bool,
}

/// Worktree-local build outputs, in preference order.
fn local_build_candidates(root: &str) -> [String; 2] {
    [
        format!("{root}/target/debug/selfware"),
        format!("{root}/target/release/selfware"),
    ]
}

/// Pick the binary to launch.
///
/// Order: a PATH hit reported by the worktree shell (`which`, already an
/// existence check), then worktree-local `target/debug` / `target/release`
/// builds — each only if `exists` confirms it — then the bare `selfware`
/// PATH fallback. Previously the first local candidate was returned (and
/// cached) unconditionally, so a release-only build or a PATH install
/// resolved to a non-existent `target/debug/selfware`.
fn pick_binary(
    which_hit: Option<String>,
    local_candidates: &[String],
    exists: impl Fn(&str) -> bool,
) -> ResolvedBinary {
    if let Some(path) = which_hit {
        return ResolvedBinary {
            path,
            cacheable: true,
        };
    }
    for candidate in local_candidates {
        if exists(candidate) {
            return ResolvedBinary {
                path: candidate.clone(),
                cacheable: true,
            };
        }
    }
    ResolvedBinary {
        path: PATH_FALLBACK.to_string(),
        cacheable: false,
    }
}

/// Existence probe usable from the WASM sandbox.
///
/// `std::fs` cannot see the worktree from inside the wasm32-wasip1 sandbox
/// (only the extension's own work directory is preopened), and
/// `Worktree::read_text_file` only reads UTF-8 text relative to the root, so
/// it cannot tell a missing binary from a present one. The extension already
/// holds the `process:exec` capability, so spawn the candidate with
/// `--version`: the host returns `Err` when the file cannot be spawned (not
/// found / not executable) and `Ok` — whatever the exit status — when it
/// exists and ran.
fn binary_exists(path: &str) -> bool {
    process::Command::new(path)
        .arg("--version")
        .env("RUST_LOG", "error")
        .env("NO_COLOR", "1")
        .output()
        .is_ok()
}

fn parse_graph_args(args: Vec<String>) -> (&'static str, String) {
    let mut parts = args
        .join(" ")
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();

    if parts.is_empty() {
        return (DEFAULT_GRAPH_FORMAT, String::new());
    }

    let format = match parts.first().map(|value| value.as_str()) {
        Some("ascii") => {
            parts.remove(0);
            "ascii"
        }
        Some("mermaid") => {
            parts.remove(0);
            "mermaid"
        }
        Some("dot") => {
            parts.remove(0);
            "dot"
        }
        Some("json") => {
            parts.remove(0);
            "json"
        }
        Some("plantuml") => {
            parts.remove(0);
            "plantuml"
        }
        _ => DEFAULT_GRAPH_FORMAT,
    };

    (format, parts.join(" "))
}

fn binary_path_from_settings(settings: Option<&serde_json::Value>) -> Option<String> {
    settings
        .and_then(serde_json::Value::as_object)
        .and_then(|config| config.get("binary_path"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn endpoint_from_settings(settings: Option<&serde_json::Value>) -> Option<String> {
    settings
        .and_then(serde_json::Value::as_object)
        .and_then(|config| config.get("endpoint"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn model_from_settings(settings: Option<&serde_json::Value>) -> Option<String> {
    settings
        .and_then(serde_json::Value::as_object)
        .and_then(|config| config.get("model"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn push_env_if_missing(env: &mut Vec<(String, String)>, key: &str, value: String) {
    if env.iter().any(|(existing, _)| existing == key) {
        return;
    }
    env.push((key.to_string(), value));
}

zed::register_extension!(SelfwareExtension);

#[cfg(test)]
mod tests {
    use super::*;

    fn locals() -> [String; 2] {
        local_build_candidates("/ws")
    }

    #[test]
    fn which_hit_wins_and_is_cached() {
        let r = pick_binary(Some("/usr/bin/selfware".into()), &locals(), |_| true);
        assert_eq!(r.path, "/usr/bin/selfware");
        assert!(r.cacheable);
    }

    #[test]
    fn missing_debug_build_falls_through_to_release() {
        let r = pick_binary(None, &locals(), |p| p == "/ws/target/release/selfware");
        assert_eq!(r.path, "/ws/target/release/selfware");
        assert!(r.cacheable);
    }

    #[test]
    fn existing_debug_build_is_preferred() {
        let r = pick_binary(None, &locals(), |_| true);
        assert_eq!(r.path, "/ws/target/debug/selfware");
        assert!(r.cacheable);
    }

    #[test]
    fn nothing_found_uses_uncached_path_fallback() {
        let r = pick_binary(None, &locals(), |_| false);
        assert_eq!(r.path, PATH_FALLBACK);
        assert!(!r.cacheable, "the bare PATH fallback must never be cached");
    }
}
