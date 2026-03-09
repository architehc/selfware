//! Selfware ZED Extension
//!
//! Provides AI-powered coding assistance inside the ZED editor by starting
//! `selfware lsp` as a language server process. Communication happens over
//! the LSP protocol (JSON-RPC over stdio).
//!
//! # Capabilities
//! - Language server integration for Rust, Python, TypeScript, JavaScript, Go
//! - Inline completions and suggestions
//! - Code actions: fix, refactor, explain, generate tests
//! - Context menu items: "Ask Selfware", "Explain Code", "Generate Tests"

use zed_extension_api::{self as zed, serde_json, settings::LspSettings, LanguageServerId, Result};

/// Main extension struct holding runtime state.
struct SelfwareExtension {
    /// Cached path to the selfware binary (resolved on first use).
    cached_binary_path: Option<String>,
}

impl SelfwareExtension {
    /// Create a new extension instance.
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    /// Locate the `selfware` binary on the system PATH or in common install
    /// locations. Returns the path string or an error if not found.
    fn find_selfware_binary(&mut self) -> std::result::Result<String, String> {
        // Return cached path if we already resolved it.
        if let Some(ref path) = self.cached_binary_path {
            return Ok(path.clone());
        }

        // Try common locations in order of preference.
        let candidates = [
            // Cargo install default
            "selfware",
            // Explicit paths users might have
            "/usr/local/bin/selfware",
            "/opt/homebrew/bin/selfware",
        ];

        for candidate in &candidates {
            // For bare command names, rely on PATH resolution at spawn time.
            // For absolute paths, we can check existence but WASM doesn't have
            // std::fs — so we just store the first candidate and let the LSP
            // client report a spawn failure if it doesn't exist.
            self.cached_binary_path = Some(candidate.to_string());
            return Ok(candidate.to_string());
        }

        Err("Could not find the selfware binary. Install it with: cargo install selfware".into())
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
        let binary = self.find_selfware_binary().map_err(|e| e.to_string())?;

        // Read user settings to pass as environment variables.
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.settings);

        let mut env = Vec::new();

        if let Some(ref settings) = settings {
            if let Some(endpoint) = settings.get("endpoint").and_then(|v| v.as_str()) {
                env.push(("SELFWARE_ENDPOINT".to_string(), endpoint.to_string()));
            }
            if let Some(model) = settings.get("model").and_then(|v| v.as_str()) {
                env.push(("SELFWARE_MODEL".to_string(), model.to_string()));
            }
        }

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
            .unwrap_or("http://localhost:8000/v1")
            .to_string();

        let model = settings
            .as_ref()
            .and_then(|s| s.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("Qwen/Qwen3-Coder-Next-FP8")
            .to_string();

        let auto_suggest = settings
            .as_ref()
            .and_then(|s| s.get("auto_suggest"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let inline_completions = settings
            .as_ref()
            .and_then(|s| s.get("inline_completions"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(Some(serde_json::json!({
            "endpoint": endpoint,
            "model": model,
            "auto_suggest": auto_suggest,
            "inline_completions": inline_completions,
            "capabilities": {
                "code_actions": [
                    {
                        "id": "selfware.fix",
                        "title": "Fix with Selfware",
                        "kind": "quickfix"
                    },
                    {
                        "id": "selfware.refactor",
                        "title": "Refactor with Selfware",
                        "kind": "refactor"
                    },
                    {
                        "id": "selfware.explain",
                        "title": "Explain Code",
                        "kind": "source"
                    },
                    {
                        "id": "selfware.generate_tests",
                        "title": "Generate Tests",
                        "kind": "source"
                    }
                ],
                "context_menu": [
                    {
                        "id": "selfware.ask",
                        "label": "Ask Selfware"
                    },
                    {
                        "id": "selfware.explain",
                        "label": "Explain Code"
                    },
                    {
                        "id": "selfware.generate_tests",
                        "label": "Generate Tests"
                    }
                ]
            }
        })))
    }
}

zed::register_extension!(SelfwareExtension);
