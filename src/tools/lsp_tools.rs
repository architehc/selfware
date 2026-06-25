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
         Returns a list of diagnostic messages with severity and line numbers. \
         Requires a language server to be installed."
    }

    fn schema(&self) -> Value {
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

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            file: String,
        }
        let args: Args = serde_json::from_value(args)?;
        validate_lsp_file(&args.file, self.handle.safety_config.as_ref())?;
        let client = self.handle.get().await?;

        let diags = client.diagnostics(&args.file).await?;

        let errors = diags.iter().filter(|d| d.severity == "error").count();
        let warnings = diags.iter().filter(|d| d.severity == "warning").count();

        Ok(json!({
            "status": "ok",
            "count": diags.len(),
            "errors": errors,
            "warnings": warnings,
            "diagnostics": diags
        }))
    }
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
mod tests {
    use super::*;

    #[test]
    fn test_goto_definition_tool_metadata() {
        let (goto, _refs, _syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
        assert_eq!(goto.name(), "lsp_goto_definition");
        assert!(!goto.description().is_empty());

        let schema = goto.schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&json!("file")));
        assert!(required.contains(&json!("line")));
        assert!(required.contains(&json!("column")));
    }

    #[test]
    fn test_find_references_tool_metadata() {
        let (_goto, refs, _syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
        assert_eq!(refs.name(), "lsp_find_references");
        assert!(!refs.description().is_empty());
    }

    #[test]
    fn test_document_symbols_tool_metadata() {
        let (_goto, _refs, syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
        assert_eq!(syms.name(), "lsp_document_symbols");

        let schema = syms.schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&json!("file")));
    }

    #[test]
    fn test_hover_tool_metadata() {
        let (_goto, _refs, _syms, hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
        assert_eq!(hover.name(), "lsp_hover");
        assert!(!hover.description().is_empty());
    }

    #[test]
    fn test_all_tools_share_handle() {
        let (goto, refs, syms, hover) = create_lsp_tools(PathBuf::from("/tmp/test"), None);
        // They all share the same Arc handle.
        assert!(Arc::ptr_eq(&goto.handle, &refs.handle));
        assert!(Arc::ptr_eq(&refs.handle, &syms.handle));
        assert!(Arc::ptr_eq(&syms.handle, &hover.handle));
    }

    #[test]
    fn test_diagnostics_tool_metadata() {
        let (diag, _ws, _impl) = create_extra_lsp_tools(PathBuf::from("/tmp/test"), None);
        assert_eq!(diag.name(), "lsp_diagnostics");
        assert!(!diag.description().is_empty());

        let schema = diag.schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&json!("file")));
    }

    #[test]
    fn test_workspace_symbols_tool_metadata() {
        let (_diag, ws, _impl) = create_extra_lsp_tools(PathBuf::from("/tmp/test"), None);
        assert_eq!(ws.name(), "lsp_workspace_symbols");
        assert!(!ws.description().is_empty());

        let schema = ws.schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&json!("query")));
    }

    #[test]
    fn test_goto_implementation_tool_metadata() {
        let (_diag, _ws, imp) = create_extra_lsp_tools(PathBuf::from("/tmp/test"), None);
        assert_eq!(imp.name(), "lsp_goto_implementation");
        assert!(!imp.description().is_empty());

        let schema = imp.schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&json!("file")));
        assert!(required.contains(&json!("line")));
        assert!(required.contains(&json!("column")));
    }

    fn default_test_safety_config() -> SafetyConfig {
        SafetyConfig::default()
    }

    #[test]
    fn test_validate_lsp_file_rejects_etc_passwd() {
        let config = default_test_safety_config();
        let result = validate_lsp_file("/etc/passwd", Some(&config));
        assert!(
            result.is_err(),
            "/etc/passwd should be rejected by path validation"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("outside")
                || err.contains("traversal")
                || err.contains("system")
                || err.contains("allowed"),
            "Expected security error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_lsp_file_allows_workspace_file() {
        let cwd = std::env::current_dir().unwrap();
        let config = default_test_safety_config();
        let file = cwd.join("src/tools/lsp_tools.rs");
        let result = validate_lsp_file(file.to_str().unwrap(), Some(&config));
        assert!(
            result.is_ok(),
            "Workspace file should be allowed, got error: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_goto_definition_rejects_etc_passwd() {
        let (goto, _, _, _) = create_lsp_tools(
            PathBuf::from("/tmp/test"),
            Some(default_test_safety_config()),
        );
        let result = goto
            .execute(json!({"file": "/etc/passwd", "line": 0, "column": 0}))
            .await;
        assert!(result.is_err(), "goto_definition should reject /etc/passwd");
    }

    #[tokio::test]
    async fn test_find_references_rejects_etc_passwd() {
        let (_, refs, _, _) = create_lsp_tools(
            PathBuf::from("/tmp/test"),
            Some(default_test_safety_config()),
        );
        let result = refs
            .execute(json!({"file": "/etc/passwd", "line": 0, "column": 0}))
            .await;
        assert!(result.is_err(), "find_references should reject /etc/passwd");
    }

    #[tokio::test]
    async fn test_document_symbols_rejects_etc_passwd() {
        let (_, _, syms, _) = create_lsp_tools(
            PathBuf::from("/tmp/test"),
            Some(default_test_safety_config()),
        );
        let result = syms.execute(json!({"file": "/etc/passwd"})).await;
        assert!(
            result.is_err(),
            "document_symbols should reject /etc/passwd"
        );
    }

    #[tokio::test]
    async fn test_hover_rejects_etc_passwd() {
        let (_, _, _, hover) = create_lsp_tools(
            PathBuf::from("/tmp/test"),
            Some(default_test_safety_config()),
        );
        let result = hover
            .execute(json!({"file": "/etc/passwd", "line": 0, "column": 0}))
            .await;
        assert!(result.is_err(), "hover should reject /etc/passwd");
    }

    #[tokio::test]
    async fn test_diagnostics_rejects_etc_passwd() {
        let (diag, _, _) = create_extra_lsp_tools(
            PathBuf::from("/tmp/test"),
            Some(default_test_safety_config()),
        );
        let result = diag.execute(json!({"file": "/etc/passwd"})).await;
        assert!(result.is_err(), "diagnostics should reject /etc/passwd");
    }

    #[tokio::test]
    async fn test_goto_implementation_rejects_etc_passwd() {
        let (_, _, imp) = create_extra_lsp_tools(
            PathBuf::from("/tmp/test"),
            Some(default_test_safety_config()),
        );
        let result = imp
            .execute(json!({"file": "/etc/passwd", "line": 0, "column": 0}))
            .await;
        assert!(
            result.is_err(),
            "goto_implementation should reject /etc/passwd"
        );
    }
}
