use anyhow::{anyhow, Result};
use async_trait::async_trait;
use libloading::{Library, Symbol};
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::tools::Tool;

/// C-ABI interface for dynamically loaded tools.
pub struct DynamicTool {
    library: Arc<Library>,
    name: String,
    description: String,
    schema: Value,
    lib_path: PathBuf,
}

impl DynamicTool {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let lib_path = path.as_ref().to_path_buf();
        unsafe {
            let library = Arc::new(Library::new(&lib_path)?);

            let get_name: Symbol<unsafe extern "C" fn() -> *const c_char> = library.get(b"get_name")?;
            let name_ptr = get_name();
            let name = CStr::from_ptr(name_ptr).to_string_lossy().into_owned();

            let get_description: Symbol<unsafe extern "C" fn() -> *const c_char> = library.get(b"get_description")?;
            let desc_ptr = get_description();
            let description = CStr::from_ptr(desc_ptr).to_string_lossy().into_owned();

            let get_schema: Symbol<unsafe extern "C" fn() -> *const c_char> = library.get(b"get_schema")?;
            let schema_ptr = get_schema();
            let schema_str = CStr::from_ptr(schema_ptr).to_string_lossy();
            let schema: Value = serde_json::from_str(&schema_str)?;

            info!("Successfully loaded dynamic tool '{}' from {:?}", name, lib_path);

            Ok(Self {
                library,
                name,
                description,
                schema,
                lib_path,
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
            let execute_sym: Symbol<unsafe extern "C" fn(*const c_char) -> *mut c_char> = 
                library.get(b"execute").map_err(|e| anyhow!("Failed to find execute symbol: {}", e))?;
                
            let free_sym: Symbol<unsafe extern "C" fn(*mut c_char)> = 
                library.get(b"free_string").map_err(|e| anyhow!("Failed to find free_string symbol: {}", e))?;

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
        
        self.tool_paths.insert(name.clone(), path.as_ref().to_path_buf());
        self.tools.write().await.insert(name.clone(), arc_tool.clone());
        
        Ok(arc_tool)
    }

    pub async fn reload(&mut self, name: &str) -> Result<Arc<DynamicTool>> {
        if let Some(path) = self.tool_paths.get(name).cloned() {
            info!("Hot-reloading tool '{}' from {:?}", name, path);
            let tool = DynamicTool::load(&path)?;
            let arc_tool = Arc::new(tool);
            self.tools.write().await.insert(name.to_string(), arc_tool.clone());
            Ok(arc_tool)
        } else {
            Err(anyhow!("Tool '{}' not registered for hot-reloading", name))
        }
    }
    
    pub async fn get_tool(&self, name: &str) -> Option<Arc<DynamicTool>> {
        self.tools.read().await.get(name).cloned()
    }
}
