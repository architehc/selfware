use anyhow::{anyhow, Result};
use async_trait::async_trait;
use libloading::{Library, Symbol};
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::tools::Tool;

/// C-ABI interface for dynamically loaded tools.
#[derive(Debug)]
pub struct DynamicTool {
    library: Arc<Library>,
    name: String,
    description: String,
    schema: Value,
    _lib_path: PathBuf,
}

impl DynamicTool {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let lib_path = path.as_ref().to_path_buf();

        // Security: only allow loading from explicitly configured plugin directories
        let canonical = lib_path
            .canonicalize()
            .map_err(|e| anyhow!("Cannot resolve plugin path {:?}: {}", lib_path, e))?;

        // Reject paths outside the current working directory unless explicitly allowed
        let cwd = std::env::current_dir()
            .map_err(|e| anyhow!("Cannot determine current directory: {}", e))?;
        let plugin_dir = cwd.join(".selfware").join("plugins");
        if !canonical.starts_with(&plugin_dir) {
            anyhow::bail!(
                "Dynamic tool loading restricted to {:?}. Got: {:?}",
                plugin_dir,
                canonical
            );
        }

        unsafe {
            let library = Arc::new(Library::new(&lib_path)?);

            let get_name: Symbol<unsafe extern "C" fn() -> *const c_char> =
                library.get(b"get_name")?;
            let name_ptr = get_name();
            if name_ptr.is_null() {
                anyhow::bail!("get_name returned null pointer");
            }
            let name = CStr::from_ptr(name_ptr).to_string_lossy().into_owned();

            let get_description: Symbol<unsafe extern "C" fn() -> *const c_char> =
                library.get(b"get_description")?;
            let desc_ptr = get_description();
            if desc_ptr.is_null() {
                anyhow::bail!("get_description returned null pointer");
            }
            let description = CStr::from_ptr(desc_ptr).to_string_lossy().into_owned();

            let get_schema: Symbol<unsafe extern "C" fn() -> *const c_char> =
                library.get(b"get_schema")?;
            let schema_ptr = get_schema();
            if schema_ptr.is_null() {
                anyhow::bail!("get_schema returned null pointer");
            }
            let schema_str = CStr::from_ptr(schema_ptr).to_string_lossy();
            let schema: Value = serde_json::from_str(&schema_str)?;

            info!(
                "Successfully loaded dynamic tool '{}' from {:?}",
                name, lib_path
            );

            Ok(Self {
                library,
                name,
                description,
                schema,
                _lib_path: lib_path,
            })
        }
    }
}

#[async_trait]
impl Tool for DynamicTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> Value {
        self.schema.clone()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let args_str = serde_json::to_string(&args)?;
        let c_args = CString::new(args_str)?;
        let library = self.library.clone();

        tokio::task::spawn_blocking(move || unsafe {
            let execute_sym: Symbol<unsafe extern "C" fn(*const c_char) -> *mut c_char> = library
                .get(b"execute")
                .map_err(|e| anyhow!("Failed to find execute symbol: {}", e))?;

            let free_sym: Symbol<unsafe extern "C" fn(*mut c_char)> =
                library
                    .get(b"free_string")
                    .map_err(|e| anyhow!("Failed to find free_string symbol: {}", e))?;

            let result_ptr = execute_sym(c_args.as_ptr());
            if result_ptr.is_null() {
                return Err(anyhow!("Dynamic tool execution returned null pointer"));
            }

            let result_str = CStr::from_ptr(result_ptr).to_string_lossy().into_owned();
            free_sym(result_ptr);

            let result_value: Value = serde_json::from_str(&result_str)?;
            Ok(result_value)
        })
        .await?
    }
}

/// Manages hot-reloading of dynamic tools.
pub struct HotReloadManager {
    tools: Arc<RwLock<std::collections::HashMap<String, Arc<DynamicTool>>>>,
    tool_paths: std::collections::HashMap<String, PathBuf>,
}

impl Default for HotReloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HotReloadManager {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(std::collections::HashMap::new())),
            tool_paths: std::collections::HashMap::new(),
        }
    }

    pub async fn register(&mut self, path: impl AsRef<Path>) -> Result<Arc<DynamicTool>> {
        let tool = DynamicTool::load(path.as_ref())?;
        let name = tool.name().to_string();
        let arc_tool = Arc::new(tool);

        self.tool_paths
            .insert(name.clone(), path.as_ref().to_path_buf());
        self.tools
            .write()
            .await
            .insert(name.clone(), arc_tool.clone());

        Ok(arc_tool)
    }

    pub async fn reload(&mut self, name: &str) -> Result<Arc<DynamicTool>> {
        if let Some(path) = self.tool_paths.get(name).cloned() {
            info!("Hot-reloading tool '{}' from {:?}", name, path);
            let tool = DynamicTool::load(&path)?;
            let arc_tool = Arc::new(tool);
            self.tools
                .write()
                .await
                .insert(name.to_string(), arc_tool.clone());
            Ok(arc_tool)
        } else {
            Err(anyhow!("Tool '{}' not registered for hot-reloading", name))
        }
    }

    pub async fn get_tool(&self, name: &str) -> Option<Arc<DynamicTool>> {
        self.tools.read().await.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── HotReloadManager::new() ──────────────────────────────────────

    #[test]
    fn new_creates_empty_tools_map() {
        let mgr = HotReloadManager::new();
        // tool_paths is a plain HashMap -- verify it starts empty
        assert!(mgr.tool_paths.is_empty());
    }

    #[test]
    fn default_is_same_as_new() {
        let mgr = HotReloadManager::default();
        assert!(mgr.tool_paths.is_empty());
    }

    // ── get_tool() returns None for unknown names ────────────────────

    #[tokio::test]
    async fn get_tool_returns_none_for_unknown() {
        let mgr = HotReloadManager::new();
        assert!(mgr.get_tool("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn get_tool_returns_none_for_empty_string() {
        let mgr = HotReloadManager::new();
        assert!(mgr.get_tool("").await.is_none());
    }

    // ── reload() returns error for unregistered tools ────────────────

    #[tokio::test]
    async fn reload_errors_for_unregistered_tool() {
        let mut mgr = HotReloadManager::new();
        let result = mgr.reload("unknown_tool").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not registered"),
            "Expected 'not registered' in error, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn reload_errors_for_empty_name() {
        let mut mgr = HotReloadManager::new();
        let result = mgr.reload("").await;
        assert!(result.is_err());
    }

    // ── DynamicTool::load() path restriction ─────────────────────────
    //
    // We test the error path without needing a real .dylib.
    // The function should reject any path that does not reside under
    // `$CWD/.selfware/plugins/`.

    #[test]
    fn load_rejects_path_outside_plugin_dir() {
        // A path that definitely exists but is not under .selfware/plugins/
        let result = DynamicTool::load("/tmp/evil.dylib");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        // The error should mention the restriction or path resolution failure
        assert!(
            err_msg.contains("restricted") || err_msg.contains("Cannot resolve"),
            "Expected restriction or resolution error, got: {}",
            err_msg
        );
    }

    #[test]
    fn load_rejects_relative_path_outside_plugin_dir() {
        let result = DynamicTool::load("../../etc/passwd");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("restricted") || err_msg.contains("Cannot resolve"),
            "Expected restriction or resolution error, got: {}",
            err_msg
        );
    }

    #[test]
    fn load_rejects_absolute_system_path() {
        let result = DynamicTool::load("/usr/lib/libSystem.dylib");
        assert!(result.is_err());
    }

    #[test]
    fn load_rejects_nonexistent_path_in_plugin_dir() {
        // Even a path that *would* be in the plugins dir but doesn't exist
        // should fail at canonicalize.
        let result = DynamicTool::load(".selfware/plugins/nonexistent.dylib");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("Cannot resolve"),
            "Expected resolution error for missing file, got: {}",
            err_msg
        );
    }

    // ── Debug / Display helpers ──────────────────────────────────────
    //
    // DynamicTool does not derive Debug; verify we can still extract
    // meaningful error info from load failures.

    #[test]
    fn load_error_is_displayable() {
        let result = DynamicTool::load("/tmp/no_such_plugin.dylib");
        assert!(result.is_err());
        let err = result.unwrap_err();
        // anyhow errors implement Display; make sure it produces a non-empty string
        let msg = format!("{}", err);
        assert!(!msg.is_empty(), "Error Display should produce output");
        // Also check Debug works via anyhow's chain
        let debug_msg = format!("{:?}", err);
        assert!(!debug_msg.is_empty(), "Error Debug should produce output");
    }
}
