//! Minimal self-hosted LSP server for editor integrations.
//!
//! This server provides lightweight workspace navigation for Rust projects
//! using Selfware's local symbol index. It is intentionally small, but it
//! implements enough of the protocol for Zed to activate the extension and
//! offer symbol search, outline, go-to-definition, and hover.

use crate::cognitive::intelligence::{ProjectIntelligence, SearchResult, Symbol, SymbolKind};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info};
use url::Url;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INTERNAL_ERROR: i64 = -32603;

pub struct SelfwareLspServer {
    root: PathBuf,
    shutdown_requested: bool,
}

impl Default for SelfwareLspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfwareLspServer {
    pub fn new() -> Self {
        Self {
            root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            shutdown_requested: false,
        }
    }

    async fn handle_request(&mut self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        if request.jsonrpc != "2.0" {
            return request.id.map(|id| JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: INVALID_REQUEST,
                    message: "Unsupported jsonrpc version".to_string(),
                    data: None,
                }),
            });
        }

        let id = match request.id.clone() {
            Some(id) => id,
            None => {
                self.handle_notification(&request).await;
                return None;
            }
        };

        let response = match request.method.as_str() {
            "initialize" => self.initialize(request.params.as_ref()),
            "shutdown" => {
                self.shutdown_requested = true;
                Ok(Value::Null)
            }
            "textDocument/documentSymbol" => self.document_symbols(request.params.as_ref()),
            "workspace/symbol" => self.workspace_symbols(request.params.as_ref()),
            "textDocument/definition" => self.definition(request.params.as_ref()),
            "textDocument/hover" => self.hover(request.params.as_ref()),
            method => Err(JsonRpcError {
                code: METHOD_NOT_FOUND,
                message: format!("Method not found: {method}"),
                data: None,
            }),
        };

        match response {
            Ok(result) => Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(result),
                error: None,
            }),
            Err(error) => Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(error),
            }),
        }
    }

    async fn handle_notification(&mut self, request: &JsonRpcRequest) {
        match request.method.as_str() {
            "initialized" => {
                info!("LSP client initialized for {}", self.root.display());
            }
            "exit" => {
                self.shutdown_requested = true;
            }
            "textDocument/didOpen"
            | "textDocument/didChange"
            | "textDocument/didClose"
            | "workspace/didChangeConfiguration" => {}
            method => {
                debug!("Ignoring LSP notification: {}", method);
            }
        }
    }

    fn initialize(&mut self, params: Option<&Value>) -> std::result::Result<Value, JsonRpcError> {
        if let Some(root) = extract_root(params) {
            self.root = root;
        }

        Ok(serde_json::json!({
            "capabilities": {
                "textDocumentSync": 1,
                "documentSymbolProvider": true,
                "workspaceSymbolProvider": true,
                "definitionProvider": true,
                "hoverProvider": true
            },
            "serverInfo": {
                "name": "selfware",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    fn document_symbols(&self, params: Option<&Value>) -> std::result::Result<Value, JsonRpcError> {
        let Some(uri) = params
            .and_then(|p| p.get("textDocument"))
            .and_then(|doc| doc.get("uri"))
            .and_then(Value::as_str)
        else {
            return Err(invalid_params("Missing textDocument.uri"));
        };

        let path = uri_to_path(uri).ok_or_else(|| invalid_params("Invalid file URI"))?;
        let intelligence = self.load_intelligence()?;
        let symbols = intelligence
            .symbols()
            .read()
            .map_err(|_| internal_error("Failed to acquire symbol index"))?;

        let result = symbols
            .in_file(&path)
            .map(|items| items.iter().map(document_symbol_value).collect::<Vec<_>>())
            .unwrap_or_default();

        Ok(Value::Array(result))
    }

    fn workspace_symbols(
        &self,
        params: Option<&Value>,
    ) -> std::result::Result<Value, JsonRpcError> {
        let query = params
            .and_then(|p| p.get("query"))
            .and_then(Value::as_str)
            .unwrap_or("");

        let intelligence = self.load_intelligence()?;
        let results = intelligence
            .search(query)
            .into_iter()
            .filter_map(|result| match result {
                SearchResult::Symbol(symbol) => Some(workspace_symbol_value(&symbol)),
                _ => None,
            })
            .take(100)
            .collect::<Vec<_>>();

        Ok(Value::Array(results))
    }

    fn definition(&self, params: Option<&Value>) -> std::result::Result<Value, JsonRpcError> {
        let Some(uri) = params
            .and_then(|p| p.get("textDocument"))
            .and_then(|doc| doc.get("uri"))
            .and_then(Value::as_str)
        else {
            return Err(invalid_params("Missing textDocument.uri"));
        };

        let path = uri_to_path(uri).ok_or_else(|| invalid_params("Invalid file URI"))?;
        let position = params
            .and_then(|p| p.get("position"))
            .ok_or_else(|| invalid_params("Missing position"))?;
        let line = position.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
        let character = position
            .get("character")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read '{}'", path.display()))
            .map_err(|err| internal_error(&err.to_string()))?;
        let Some(token) = extract_token_at(&content, line, character) else {
            return Ok(Value::Null);
        };

        let intelligence = self.load_intelligence()?;
        let symbols = intelligence
            .symbols()
            .read()
            .map_err(|_| internal_error("Failed to acquire symbol index"))?;

        let Some(matches) = symbols.get(&token) else {
            return Ok(Value::Null);
        };

        let target = matches
            .iter()
            .find(|symbol| symbol.file == path)
            .or_else(|| matches.first());

        Ok(target.map(location_value).unwrap_or(Value::Null))
    }

    fn hover(&self, params: Option<&Value>) -> std::result::Result<Value, JsonRpcError> {
        let Some(uri) = params
            .and_then(|p| p.get("textDocument"))
            .and_then(|doc| doc.get("uri"))
            .and_then(Value::as_str)
        else {
            return Err(invalid_params("Missing textDocument.uri"));
        };

        let path = uri_to_path(uri).ok_or_else(|| invalid_params("Invalid file URI"))?;
        let position = params
            .and_then(|p| p.get("position"))
            .ok_or_else(|| invalid_params("Missing position"))?;
        let line = position.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
        let character = position
            .get("character")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read '{}'", path.display()))
            .map_err(|err| internal_error(&err.to_string()))?;
        let Some(token) = extract_token_at(&content, line, character) else {
            return Ok(Value::Null);
        };

        let intelligence = self.load_intelligence()?;
        let symbols = intelligence
            .symbols()
            .read()
            .map_err(|_| internal_error("Failed to acquire symbol index"))?;
        let Some(symbol) = symbols.get(&token).and_then(|items| {
            items
                .iter()
                .find(|item| item.file == path)
                .or_else(|| items.first())
        }) else {
            return Ok(Value::Null);
        };

        let body = format!(
            "```rust\n{}\n```\n{}:{}",
            symbol.signature,
            symbol.file.display(),
            symbol.line
        );

        Ok(serde_json::json!({
            "contents": {
                "kind": "markdown",
                "value": body
            },
            "range": symbol_range(symbol)
        }))
    }

    fn load_intelligence(&self) -> std::result::Result<ProjectIntelligence, JsonRpcError> {
        let mut intelligence = ProjectIntelligence::new(self.root.clone());
        intelligence
            .refresh()
            .map_err(|err| internal_error(&format!("Failed to index workspace: {}", err)))?;
        Ok(intelligence)
    }
}

pub async fn run_lsp_server() -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut server = SelfwareLspServer::new();

    loop {
        let Some(message) = read_message(&mut reader).await? else {
            break;
        };

        let request: JsonRpcRequest = match serde_json::from_str(&message) {
            Ok(request) => request,
            Err(error) => {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: PARSE_ERROR,
                        message: format!("Invalid JSON: {}", error),
                        data: None,
                    }),
                };
                write_message(&mut stdout, &serde_json::to_string(&response)?).await?;
                continue;
            }
        };

        if let Some(response) = server.handle_request(request).await {
            write_message(&mut stdout, &serde_json::to_string(&response)?).await?;
        }

        if server.shutdown_requested {
            break;
        }
    }

    Ok(())
}

async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>> {
    let mut content_length = None;

    loop {
        let mut header_line = String::new();
        let bytes_read = reader.read_line(&mut header_line).await?;
        if bytes_read == 0 {
            return Ok(None);
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("invalid Content-Length header")?,
            );
        }
    }

    let length = content_length.context("missing Content-Length header")?;
    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf).await?;
    Ok(Some(
        String::from_utf8(buf).context("message was not valid UTF-8")?,
    ))
}

async fn write_message<W: tokio::io::AsyncWrite + Unpin>(writer: &mut W, body: &str) -> Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

fn extract_root(params: Option<&Value>) -> Option<PathBuf> {
    params
        .and_then(|p| {
            p.get("workspaceFolders")
                .and_then(Value::as_array)
                .and_then(|folders| folders.first())
                .and_then(|folder| folder.get("uri"))
                .and_then(Value::as_str)
                .or_else(|| p.get("rootUri").and_then(Value::as_str))
        })
        .and_then(uri_to_path)
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

fn path_to_uri(path: &Path) -> Option<String> {
    Url::from_file_path(path).ok().map(|url| url.to_string())
}

fn extract_token_at(content: &str, line: usize, character: usize) -> Option<String> {
    let line_content = content.lines().nth(line)?;
    let chars = line_content.char_indices().collect::<Vec<_>>();

    if chars.is_empty() {
        return None;
    }

    let line_len = line_content.chars().count();
    let clamped = character.min(line_len.saturating_sub(1));
    let mut start = clamped;
    let mut end = clamped;
    let line_chars = line_content.chars().collect::<Vec<_>>();

    if !is_identifier_char(*line_chars.get(clamped)?) {
        if clamped > 0 && is_identifier_char(line_chars[clamped - 1]) {
            start = clamped - 1;
            end = clamped - 1;
        } else {
            return None;
        }
    }

    while start > 0 && is_identifier_char(line_chars[start - 1]) {
        start -= 1;
    }
    while end + 1 < line_chars.len() && is_identifier_char(line_chars[end + 1]) {
        end += 1;
    }

    Some(line_chars[start..=end].iter().collect())
}

fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn document_symbol_value(symbol: &Symbol) -> Value {
    serde_json::json!({
        "name": symbol.name,
        "detail": symbol.signature,
        "kind": symbol_kind_number(&symbol.kind),
        "range": symbol_range(symbol),
        "selectionRange": symbol_range(symbol),
        "children": []
    })
}

fn workspace_symbol_value(symbol: &Symbol) -> Value {
    serde_json::json!({
        "name": symbol.name,
        "kind": symbol_kind_number(&symbol.kind),
        "location": location_value(symbol),
        "containerName": symbol.parent
    })
}

fn location_value(symbol: &Symbol) -> Value {
    serde_json::json!({
        "uri": path_to_uri(&symbol.file).unwrap_or_default(),
        "range": symbol_range(symbol)
    })
}

fn symbol_range(symbol: &Symbol) -> Value {
    let line = symbol.line.saturating_sub(1) as u64;
    serde_json::json!({
        "start": { "line": line, "character": 0 },
        "end": { "line": line, "character": 200 }
    })
}

fn symbol_kind_number(kind: &SymbolKind) -> u32 {
    match kind {
        SymbolKind::Function => 12,
        SymbolKind::Struct => 23,
        SymbolKind::Enum => 10,
        SymbolKind::Trait => 11,
        SymbolKind::Impl => 2,
        SymbolKind::Const | SymbolKind::Static => 14,
        SymbolKind::Type => 26,
        SymbolKind::Macro => 25,
        SymbolKind::Module => 2,
    }
}

fn invalid_params(message: &str) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: message.to_string(),
        data: None,
    }
}

fn internal_error(message: &str) -> JsonRpcError {
    JsonRpcError {
        code: INTERNAL_ERROR,
        message: message.to_string(),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_token_at_cursor() {
        let source = "pub fn build_widget(widget: Widget) {}\n";
        assert_eq!(
            extract_token_at(source, 0, 8).as_deref(),
            Some("build_widget")
        );
        assert_eq!(extract_token_at(source, 0, 26).as_deref(), Some("widget"));
    }

    // Windows: `Url::to_file_path` parses `file:///tmp/example` as a UNC-style
    // path (`\tmp\example`), not the bare absolute `/tmp/example` we assert on.
    // There is no equivalent Windows literal to substitute for `/tmp` in this
    // assertion, so we keep the gate scoped to non-Windows.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn resolves_root_from_initialize_params() {
        let params = serde_json::json!({
            "workspaceFolders": [
                { "uri": "file:///tmp/example" }
            ]
        });
        assert_eq!(
            extract_root(Some(&params)),
            Some(PathBuf::from("/tmp/example"))
        );
    }

    #[test]
    fn maps_symbol_kinds_to_lsp_numbers() {
        assert_eq!(symbol_kind_number(&SymbolKind::Function), 12);
        assert_eq!(symbol_kind_number(&SymbolKind::Struct), 23);
        assert_eq!(symbol_kind_number(&SymbolKind::Trait), 11);
    }
}
