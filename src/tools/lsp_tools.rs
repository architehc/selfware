//! LSP tool wrappers for the agent's tool registry.
//!
//! Provides `LspGotoDefinitionTool`, `LspFindReferencesTool`,
//! `LspDocumentSymbolsTool`, and `LspHoverTool`, each backed by a shared
//! [`LspClient`] that lazily starts language servers.

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::OnceCell;

use super::Tool;
use crate::config::SafetyConfig;
use crate::lsp::LspClient;
use crate::tools::file::{resolve_safety_config, validate_tool_path};

/// Shared input schema for the LSP tools that take `file`, `line`, `column`.
fn position_schema() -> Value {
    json!({
        "type": "object",
        "required": ["file", "line", "column"],
        "properties": {
            "file": {
                "type": "string",
                "description": "Path to the source file"
            },
            "line": {
                "type": "integer",
                "description": "Zero-based line number"
            },
            "column": {
                "type": "integer",
                "description": "Zero-based column number (character offset)"
            }
        }
    })
}

/// Shared input schema for the LSP tools that take only `file`.
fn file_schema() -> Value {
    json!({
        "type": "object",
        "required": ["file"],
        "properties": {
            "file": {
                "type": "string",
                "description": "Path to the source file"
            }
        }
    })
}

/// Shared, lazily-initialized LSP client.
///
/// All four LSP tools hold an `Arc` to the same `LspClientHandle`, which
/// ensures only one set of language servers is started per session.
pub struct LspClientHandle {
    client: OnceCell<LspClient>,
    project_root: PathBuf,
    safety_config: Option<SafetyConfig>,
}

impl LspClientHandle {
    /// Create a new handle. The actual `LspClient` is created on first use.
    pub fn new(project_root: PathBuf, safety_config: Option<SafetyConfig>) -> Self {
        Self {
            client: OnceCell::new(),
            project_root,
            safety_config,
        }
    }

    /// Get or initialize the LSP client.
    async fn get(&self) -> Result<&LspClient> {
        self.client
            .get_or_try_init(|| async {
                let client = LspClient::new(&self.project_root);
                client.initialize(&self.project_root).await?;
                Ok(client)
            })
            .await
    }

    /// Explicitly shut down all language servers managed by this handle.
    ///
    /// After this call, the handle should not be used again (create a new
    /// one if needed).
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(client) = self.client.get() {
            client.shutdown().await?;
        }
        Ok(())
    }
}

/// Ensure language-server subprocesses are cleaned up when the handle is
/// dropped, so they don't leak for the agent's entire lifetime.
impl Drop for LspClientHandle {
    fn drop(&mut self) {
        if let Some(client) = self.client.get() {
            // Drop runs outside an async context, so we can't call the
            // async `shutdown()` directly.  Try to use the current tokio
            // runtime if we're inside one; otherwise create a temporary
            // one to perform cleanup.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                // We're inside a tokio runtime — use block_in_place to
                // avoid a nested-runtime panic.
                let _ = tokio::task::block_in_place(|| handle.block_on(client.shutdown()));
            } else {
                // No runtime available — create a temporary one to do
                // the cleanup.  This is safe because we're not inside
                // any runtime.
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(_) => return,
                };
                let _ = rt.block_on(client.shutdown());
            }
        }
    }
}

/// Create all four LSP tools sharing a single client handle.
///
/// Call this from `ToolRegistry::new()` to register the tools.
pub fn create_lsp_tools(
    project_root: PathBuf,
    safety_config: Option<SafetyConfig>,
) -> (
    LspGotoDefinitionTool,
    LspFindReferencesTool,
    LspDocumentSymbolsTool,
    LspHoverTool,
) {
    let handle = Arc::new(LspClientHandle::new(project_root, safety_config));
    (
        LspGotoDefinitionTool {
            handle: Arc::clone(&handle),
        },
        LspFindReferencesTool {
            handle: Arc::clone(&handle),
        },
        LspDocumentSymbolsTool {
            handle: Arc::clone(&handle),
        },
        LspHoverTool { handle },
    )
}

/// Validate that an LSP tool's `file` argument is safe to access.
fn validate_lsp_file(path: &str, safety_config: Option<&SafetyConfig>) -> Result<()> {
    let safety = resolve_safety_config(safety_config);
    validate_tool_path(path, &safety)
}

// ---------------------------------------------------------------------------
// LspGotoDefinitionTool
// ---------------------------------------------------------------------------

/// Navigate to the definition of a symbol at a given file/line/column.
pub struct LspGotoDefinitionTool {
    handle: Arc<LspClientHandle>,
}

#[async_trait]
impl Tool for LspGotoDefinitionTool {
    fn name(&self) -> &str {
        "lsp_goto_definition"
    }

    fn description(&self) -> &str {
        "Go to the definition of a symbol. Provide the file path and cursor position (line, column). \
         Returns the file path and location where the symbol is defined. Requires a language server \
         (rust-analyzer, pyright, typescript-language-server, gopls) to be installed."
    }

    fn schema(&self) -> Value {
        position_schema()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            file: String,
            line: u32,
            column: u32,
        }
        let args: Args = serde_json::from_value(args)?;
        validate_lsp_file(&args.file, self.handle.safety_config.as_ref())?;
        let client = self.handle.get().await?;

        // Ensure the file is open in the server.
        let content = tokio::fs::read_to_string(&args.file)
            .await
            .unwrap_or_default();
        client.did_open(&args.file, &content).await?;

        let locations = client
            .goto_definition(&args.file, args.line, args.column)
            .await?;

        if locations.is_empty() {
            Ok(json!({
                "status": "not_found",
                "message": "No definition found at the given position"
            }))
        } else {
            Ok(json!({
                "status": "ok",
                "definitions": locations
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// LspFindReferencesTool
// ---------------------------------------------------------------------------

/// Find all references to a symbol at a given file/line/column.
pub struct LspFindReferencesTool {
    handle: Arc<LspClientHandle>,
}

#[async_trait]
impl Tool for LspFindReferencesTool {
    fn name(&self) -> &str {
        "lsp_find_references"
    }

    fn description(&self) -> &str {
        "Find all references to a symbol. Provide the file path and cursor position (line, column). \
         Returns all locations where the symbol is used. Requires a language server to be installed."
    }

    fn schema(&self) -> Value {
        position_schema()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            file: String,
            line: u32,
            column: u32,
        }
        let args: Args = serde_json::from_value(args)?;
        validate_lsp_file(&args.file, self.handle.safety_config.as_ref())?;
        let client = self.handle.get().await?;

        let content = tokio::fs::read_to_string(&args.file)
            .await
            .unwrap_or_default();
        client.did_open(&args.file, &content).await?;

        let locations = client
            .find_references(&args.file, args.line, args.column)
            .await?;

        Ok(json!({
            "status": "ok",
            "count": locations.len(),
            "references": locations
        }))
    }
}

// ---------------------------------------------------------------------------
// LspDocumentSymbolsTool
// ---------------------------------------------------------------------------

/// List all symbols (functions, structs, methods, etc.) in a document.
pub struct LspDocumentSymbolsTool {
    handle: Arc<LspClientHandle>,
}

#[async_trait]
impl Tool for LspDocumentSymbolsTool {
    fn name(&self) -> &str {
        "lsp_document_symbols"
    }

    fn description(&self) -> &str {
        "List all symbols in a source file — functions, structs, classes, methods, constants, etc. \
         Returns name, kind, and position for each symbol. Requires a language server to be installed."
    }

    fn schema(&self) -> Value {
        file_schema()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            file: String,
        }
        let args: Args = serde_json::from_value(args)?;
        validate_lsp_file(&args.file, self.handle.safety_config.as_ref())?;
        let client = self.handle.get().await?;

        let content = tokio::fs::read_to_string(&args.file)
            .await
            .unwrap_or_default();
        client.did_open(&args.file, &content).await?;

        let symbols = client.document_symbols(&args.file).await?;

        Ok(json!({
            "status": "ok",
            "count": symbols.len(),
            "symbols": symbols
        }))
    }
}

// ---------------------------------------------------------------------------
// LspHoverTool
// ---------------------------------------------------------------------------

/// Get hover information (type signature, documentation) for a symbol.
pub struct LspHoverTool {
    handle: Arc<LspClientHandle>,
}

#[async_trait]
impl Tool for LspHoverTool {
    fn name(&self) -> &str {
        "lsp_hover"
    }

    fn description(&self) -> &str {
        "Get hover information for a symbol — type signatures, documentation, and other details. \
         Provide the file path and cursor position. Requires a language server to be installed."
    }

    fn schema(&self) -> Value {
        position_schema()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            file: String,
            line: u32,
            column: u32,
        }
        let args: Args = serde_json::from_value(args)?;
        validate_lsp_file(&args.file, self.handle.safety_config.as_ref())?;
        let client = self.handle.get().await?;

        let content = tokio::fs::read_to_string(&args.file)
            .await
            .unwrap_or_default();
        client.did_open(&args.file, &content).await?;

        let info = client.hover(&args.file, args.line, args.column).await?;

        match info {
            Some(text) => Ok(json!({
                "status": "ok",
                "hover": text
            })),
            None => Ok(json!({
                "status": "not_found",
                "message": "No hover information available at the given position"
            })),
        }
    }
}

/// Create extra LSP tools (diagnostics, workspace symbols, goto implementation)
/// sharing a single client handle.
pub fn create_extra_lsp_tools(
    project_root: PathBuf,
    safety_config: Option<SafetyConfig>,
) -> (
    LspDiagnosticsTool,
    LspWorkspaceSymbolsTool,
    LspGotoImplementationTool,
) {
    let handle = Arc::new(LspClientHandle::new(project_root, safety_config));
    (
        LspDiagnosticsTool {
            handle: Arc::clone(&handle),
        },
        LspWorkspaceSymbolsTool {
            handle: Arc::clone(&handle),
        },
        LspGotoImplementationTool { handle },
    )
}

// ---------------------------------------------------------------------------
// LspDiagnosticsTool
// ---------------------------------------------------------------------------

/// Get diagnostics (errors, warnings) for a file from the language server.
pub struct LspDiagnosticsTool {
    handle: Arc<LspClientHandle>,
}

#[async_trait]
impl Tool for LspDiagnosticsTool {
    fn name(&self) -> &str {
        "lsp_diagnostics"
    }

    fn description(&self) -> &str {
        "Get diagnostics (errors, warnings, infos, hints) for a source file. \
         Opens the file in the language server and waits briefly for it to publish diagnostics, \
         then returns them with severity and line numbers. When the server publishes nothing \
         within the wait window, reports 'unavailable' (no server data) instead of confirming \
         the file is clean. Requires a language server to be installed."
    }

    fn schema(&self) -> Value {
        file_schema()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            file: String,
        }
        let args: Args = serde_json::from_value(args)?;
        validate_lsp_file(&args.file, self.handle.safety_config.as_ref())?;
        let client = self.handle.get().await?;

        // Diagnostics are push-based: the server only sends them as
        // `textDocument/publishDiagnostics` notifications AFTER a `didOpen`.
        // Previously this read the (usually empty) passive store without
        // opening the file, so it reported `status: ok, 0 errors` for files
        // the server had never analyzed — a false "clean" signal.
        let content = tokio::fs::read_to_string(&args.file)
            .await
            .unwrap_or_default();
        client.did_open(&args.file, &content).await?;

        // Wait for the server to publish, polling the store. A background
        // reader task dispatches notifications, so no request needs to be in
        // flight for the store to fill.
        const DIAG_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
        const DIAG_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
        let deadline = std::time::Instant::now() + DIAG_WAIT_TIMEOUT;
        let diags = loop {
            let diags = client.diagnostics(&args.file).await?;
            if !diags.is_empty() || std::time::Instant::now() >= deadline {
                break diags;
            }
            tokio::time::sleep(DIAG_POLL_INTERVAL).await;
        };
        let _ = client.did_close(&args.file).await;

        Ok(diagnostics_response(&args.file, &diags, DIAG_WAIT_TIMEOUT))
    }
}

/// Build the `lsp_diagnostics` response from the diagnostics collected after
/// the didOpen wait window. An empty store after the window means "no server
/// data" — NOT "file is clean": through the client API an empty publish is
/// indistinguishable from no publish at all, so report `unavailable`
/// honestly instead of a false `ok`.
fn diagnostics_response(
    file: &str,
    diags: &[crate::lsp::client::Diagnostic],
    wait_timeout: std::time::Duration,
) -> Value {
    if diags.is_empty() {
        return json!({
            "status": "unavailable",
            "message": format!(
                "diagnostics unavailable: the language server published no diagnostics for '{}' within {}s of opening it. \
                 The file may be clean, or the server may still be initializing or may not support diagnostics for it — \
                 do NOT treat this as confirmation that the file is error-free.",
                file,
                wait_timeout.as_secs()
            )
        });
    }

    let errors = diags.iter().filter(|d| d.severity == "error").count();
    let warnings = diags.iter().filter(|d| d.severity == "warning").count();

    json!({
        "status": "ok",
        "count": diags.len(),
        "errors": errors,
        "warnings": warnings,
        "diagnostics": diags
    })
}

// ---------------------------------------------------------------------------
// LspWorkspaceSymbolsTool
// ---------------------------------------------------------------------------

/// Search for symbols across the entire workspace.
pub struct LspWorkspaceSymbolsTool {
    handle: Arc<LspClientHandle>,
}

#[async_trait]
impl Tool for LspWorkspaceSymbolsTool {
    fn name(&self) -> &str {
        "lsp_workspace_symbols"
    }

    fn description(&self) -> &str {
        "Search for symbols (functions, structs, classes, etc.) across the entire workspace. \
         Provide a query string to filter results. Requires a language server to be installed."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Symbol name or partial name to search for"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            query: String,
        }
        let args: Args = serde_json::from_value(args)?;
        let client = self.handle.get().await?;

        let symbols = client.workspace_symbol(&args.query).await?;

        if symbols.is_empty() {
            Ok(json!({
                "status": "not_found",
                "message": "No workspace symbols matched the query"
            }))
        } else {
            Ok(json!({
                "status": "ok",
                "count": symbols.len(),
                "symbols": symbols
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// LspGotoImplementationTool
// ---------------------------------------------------------------------------

/// Navigate to the implementation of a symbol.
pub struct LspGotoImplementationTool {
    handle: Arc<LspClientHandle>,
}

#[async_trait]
impl Tool for LspGotoImplementationTool {
    fn name(&self) -> &str {
        "lsp_goto_implementation"
    }

    fn description(&self) -> &str {
        "Go to the implementation of a symbol. Provide the file path and cursor position \
         (line, column). Returns the file path and location where the symbol is implemented. \
         Requires a language server to be installed."
    }

    fn schema(&self) -> Value {
        position_schema()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            file: String,
            line: u32,
            column: u32,
        }
        let args: Args = serde_json::from_value(args)?;
        validate_lsp_file(&args.file, self.handle.safety_config.as_ref())?;
        let client = self.handle.get().await?;

        let locations = client
            .goto_implementation(&args.file, args.line, args.column)
            .await?;

        if locations.is_empty() {
            Ok(json!({
                "status": "not_found",
                "message": "No implementation found at the given position"
            }))
        } else {
            Ok(json!({
                "status": "ok",
                "implementations": locations
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "../../tests/unit/tools/lsp_tools/lsp_tools_test.rs"]
mod tests;
