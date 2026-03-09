#![allow(dead_code, unused_imports, unused_variables)]
//! LSP tool wrappers for the agent's tool registry.
//!
//! Provides `LspGotoDefinitionTool`, `LspFindReferencesTool`,
//! `LspDocumentSymbolsTool`, and `LspHoverTool`, each backed by a shared
//! [`LspClient`](crate::lsp::LspClient) that lazily starts language servers.

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::info;

use super::Tool;
use crate::lsp::LspClient;

/// Shared, lazily-initialized LSP client.
///
/// All four LSP tools hold an `Arc` to the same `LspClientHandle`, which
/// ensures only one set of language servers is started per session.
pub struct LspClientHandle {
    client: OnceCell<LspClient>,
    project_root: PathBuf,
}

impl LspClientHandle {
    /// Create a new handle. The actual `LspClient` is created on first use.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            client: OnceCell::new(),
            project_root,
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
) -> (
    LspGotoDefinitionTool,
    LspFindReferencesTool,
    LspDocumentSymbolsTool,
    LspHoverTool,
) {
    let handle = Arc::new(LspClientHandle::new(project_root));
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goto_definition_tool_metadata() {
        let (goto, _refs, _syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"));
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
        let (_goto, refs, _syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"));
        assert_eq!(refs.name(), "lsp_find_references");
        assert!(!refs.description().is_empty());
    }

    #[test]
    fn test_document_symbols_tool_metadata() {
        let (_goto, _refs, syms, _hover) = create_lsp_tools(PathBuf::from("/tmp/test"));
        assert_eq!(syms.name(), "lsp_document_symbols");

        let schema = syms.schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&json!("file")));
    }

    #[test]
    fn test_hover_tool_metadata() {
        let (_goto, _refs, _syms, hover) = create_lsp_tools(PathBuf::from("/tmp/test"));
        assert_eq!(hover.name(), "lsp_hover");
        assert!(!hover.description().is_empty());
    }

    #[test]
    fn test_all_tools_share_handle() {
        let (goto, refs, syms, hover) = create_lsp_tools(PathBuf::from("/tmp/test"));
        // They all share the same Arc handle.
        assert!(Arc::ptr_eq(&goto.handle, &refs.handle));
        assert!(Arc::ptr_eq(&refs.handle, &syms.handle));
        assert!(Arc::ptr_eq(&syms.handle, &hover.handle));
    }
}
