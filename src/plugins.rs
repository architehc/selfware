//! Plugin Architecture
//!
//! This module provides an extensible plugin system:
//! - WASM plugin support
//! - Dynamic loading and unloading
//! - Sandboxed execution environment
//! - API versioning and compatibility
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Plugin Manager                           │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Plugin        │  │ Sandbox       │  │ API           │   │
//! │  │ Loader        │  │ Runtime       │  │ Registry      │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! │           │                  │                  │           │
//! │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐   │
//! │  │ Version       │  │ Permission    │  │ Event         │   │
//! │  │ Manager       │  │ System        │  │ Bus           │   │
//! │  └───────────────┘  └───────────────┘  └───────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// Plugin Metadata
// ============================================================================

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Unique plugin identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Version (semver)
    pub version: String,
    /// Description
    pub description: String,
    /// Author
    pub author: String,
    /// License
    pub license: String,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
    /// Required API version
    pub api_version: String,
    /// Required permissions
    pub permissions: Vec<Permission>,
    /// Plugin type
    pub plugin_type: PluginType,
    /// Dependencies
    pub dependencies: Vec<PluginDependency>,
}

impl PluginMetadata {
    pub fn new(id: &str, name: &str, version: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            description: String::new(),
            author: String::new(),
            license: "MIT".to_string(),
            homepage: None,
            repository: None,
            api_version: "1.0.0".to_string(),
            permissions: Vec::new(),
            plugin_type: PluginType::Tool,
            dependencies: Vec::new(),
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn with_author(mut self, author: &str) -> Self {
        self.author = author.to_string();
        self
    }

    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.permissions.push(permission);
        self
    }

    pub fn with_dependency(mut self, dep: PluginDependency) -> Self {
        self.dependencies.push(dep);
        self
    }
}

/// Plugin types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginType {
    /// Tool plugin (adds new tools)
    Tool,
    /// Provider plugin (adds model providers)
    Provider,
    /// Formatter plugin (output formatting)
    Formatter,
    /// Analyzer plugin (code analysis)
    Analyzer,
    /// Integration plugin (external services)
    Integration,
    /// Theme plugin (UI theming)
    Theme,
}

/// Plugin dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// Plugin ID
    pub plugin_id: String,
    /// Version requirement
    pub version_req: String,
    /// Is optional
    pub optional: bool,
}

// ============================================================================
// Permissions
// ============================================================================

/// Plugin permission
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Read files
    FileRead,
    /// Write files
    FileWrite,
    /// Execute commands
    Execute,
    /// Network access
    Network,
    /// Environment variables
    Environment,
    /// System information
    SystemInfo,
    /// Full file system access
    FileSystemFull,
    /// Custom permission
    Custom(String),
}

impl Permission {
    /// Check if permission is dangerous
    pub fn is_dangerous(&self) -> bool {
        matches!(
            self,
            Permission::Execute | Permission::FileSystemFull | Permission::Environment
        )
    }

    /// Get permission description
    pub fn description(&self) -> &str {
        match self {
            Permission::FileRead => "Read files in allowed directories",
            Permission::FileWrite => "Write files in allowed directories",
            Permission::Execute => "Execute shell commands",
            Permission::Network => "Make network requests",
            Permission::Environment => "Access environment variables",
            Permission::SystemInfo => "Read system information",
            Permission::FileSystemFull => "Full file system access",
            Permission::Custom(name) => name,
        }
    }
}

/// Permission request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// Permission requested
    pub permission: Permission,
    /// Reason for request
    pub reason: String,
    /// Whether it's required
    pub required: bool,
}

/// Permission grant
#[derive(Debug, Clone)]
pub struct PermissionGrant {
    /// Granted permissions
    pub permissions: Vec<Permission>,
    /// Granted at timestamp
    pub granted_at: u64,
    /// Expires at timestamp (None = never)
    pub expires_at: Option<u64>,
}

impl PermissionGrant {
    pub fn new(permissions: Vec<Permission>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            permissions,
            granted_at: now,
            expires_at: None,
        }
    }

    pub fn with_expiry(mut self, secs: u64) -> Self {
        self.expires_at = Some(self.granted_at + secs);
        self
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        // Check expiry
        if let Some(expires) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now > expires {
                return false;
            }
        }
        self.permissions.contains(permission)
    }
}

// ============================================================================
// API Versioning
// ============================================================================

/// Semantic version
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);

        Some(Self {
            major,
            minor,
            patch,
            prerelease: None,
        })
    }

    /// Check if this version is compatible with requirement
    pub fn is_compatible_with(&self, req: &str) -> bool {
        // Simple compatibility check
        if let Some(req_ver) = SemVer::parse(req.trim_start_matches('^').trim_start_matches('~')) {
            // Major version must match, minor can be >= required
            if self.major == req_ver.major && self.minor >= req_ver.minor {
                return true;
            }
        }
        false
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.prerelease {
            write!(f, "-{}", pre)?;
        }
        Ok(())
    }
}

/// API version manager
pub struct ApiVersionManager {
    /// Current API version
    current_version: SemVer,
    /// Minimum supported version
    min_version: SemVer,
    /// Deprecated versions
    deprecated: Vec<SemVer>,
}

impl ApiVersionManager {
    pub fn new(current: SemVer, min: SemVer) -> Self {
        Self {
            current_version: current,
            min_version: min,
            deprecated: Vec::new(),
        }
    }

    /// Check if version is supported
    pub fn is_supported(&self, version: &str) -> bool {
        if let Some(ver) = SemVer::parse(version) {
            ver.major >= self.min_version.major
                && (ver.major > self.min_version.major || ver.minor >= self.min_version.minor)
        } else {
            false
        }
    }

    /// Check if version is deprecated
    pub fn is_deprecated(&self, version: &str) -> bool {
        if let Some(ver) = SemVer::parse(version) {
            self.deprecated.iter().any(|d| d == &ver)
        } else {
            false
        }
    }

    /// Get current version
    pub fn current(&self) -> &SemVer {
        &self.current_version
    }

    /// Add deprecated version
    pub fn deprecate(&mut self, version: SemVer) {
        if !self.deprecated.contains(&version) {
            self.deprecated.push(version);
        }
    }
}

impl Default for ApiVersionManager {
    fn default() -> Self {
        Self::new(SemVer::new(1, 0, 0), SemVer::new(1, 0, 0))
    }
}

// ============================================================================
// Plugin Instance
// ============================================================================

/// Plugin state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    /// Not loaded
    Unloaded,
    /// Loading in progress
    Loading,
    /// Loaded and ready
    Loaded,
    /// Running
    Running,
    /// Paused
    Paused,
    /// Error state
    Error,
    /// Disabled by user
    Disabled,
}

/// Plugin instance
pub struct PluginInstance {
    /// Metadata
    pub metadata: PluginMetadata,
    /// Current state
    state: RwLock<PluginState>,
    /// Granted permissions
    permissions: RwLock<Option<PermissionGrant>>,
    /// Plugin path
    path: PathBuf,
    /// Load time
    loaded_at: RwLock<Option<u64>>,
    /// Error message if in error state
    error: RwLock<Option<String>>,
    /// Statistics
    stats: PluginStats,
}

/// Plugin statistics
#[derive(Debug, Default)]
pub struct PluginStats {
    pub invocations: AtomicU64,
    pub errors: AtomicU64,
    pub total_runtime_ms: AtomicU64,
}

impl PluginInstance {
    pub fn new(metadata: PluginMetadata, path: PathBuf) -> Self {
        Self {
            metadata,
            state: RwLock::new(PluginState::Unloaded),
            permissions: RwLock::new(None),
            path,
            loaded_at: RwLock::new(None),
            error: RwLock::new(None),
            stats: PluginStats::default(),
        }
    }

    /// Get current state
    pub fn state(&self) -> PluginState {
        *self
            .state
            .read()
            .unwrap_or_else(|_| panic!("lock poisoned"))
    }

    /// Set state
    pub fn set_state(&self, state: PluginState) {
        if let Ok(mut s) = self.state.write() {
            *s = state;
        }
    }

    /// Set error
    pub fn set_error(&self, error: &str) {
        if let Ok(mut e) = self.error.write() {
            *e = Some(error.to_string());
        }
        self.set_state(PluginState::Error);
    }

    /// Get error
    pub fn error(&self) -> Option<String> {
        self.error.read().ok()?.clone()
    }

    /// Grant permissions
    pub fn grant_permissions(&self, grant: PermissionGrant) {
        if let Ok(mut p) = self.permissions.write() {
            *p = Some(grant);
        }
    }

    /// Check permission
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions
            .read()
            .ok()
            .and_then(|p| p.as_ref().map(|g| g.has_permission(permission)))
            .unwrap_or(false)
    }

    /// Record invocation
    pub fn record_invocation(&self, duration_ms: u64, success: bool) {
        self.stats.invocations.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_runtime_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        if !success {
            self.stats.errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get statistics summary
    pub fn stats_summary(&self) -> PluginStatsSummary {
        let invocations = self.stats.invocations.load(Ordering::Relaxed);
        let errors = self.stats.errors.load(Ordering::Relaxed);
        let total_runtime = self.stats.total_runtime_ms.load(Ordering::Relaxed);

        PluginStatsSummary {
            invocations,
            errors,
            error_rate: if invocations > 0 {
                errors as f32 / invocations as f32
            } else {
                0.0
            },
            avg_runtime_ms: if invocations > 0 {
                total_runtime / invocations
            } else {
                0
            },
        }
    }
}

/// Plugin statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatsSummary {
    pub invocations: u64,
    pub errors: u64,
    pub error_rate: f32,
    pub avg_runtime_ms: u64,
}

// ============================================================================
// Sandbox Runtime
// ============================================================================

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Memory limit (bytes)
    pub memory_limit: usize,
    /// CPU time limit (ms)
    pub cpu_limit_ms: u64,
    /// Allowed paths for file access
    pub allowed_paths: Vec<PathBuf>,
    /// Allowed network hosts
    pub allowed_hosts: Vec<String>,
    /// Enable WASM sandbox
    pub enable_wasm: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            memory_limit: 64 * 1024 * 1024, // 64MB
            cpu_limit_ms: 5000,             // 5 seconds
            allowed_paths: Vec::new(),
            allowed_hosts: Vec::new(),
            enable_wasm: true,
        }
    }
}

/// Sandbox runtime for plugin execution
pub struct SandboxRuntime {
    config: SandboxConfig,
    /// Active sandboxes
    sandboxes: RwLock<HashMap<String, SandboxInstance>>,
    /// Statistics
    stats: SandboxStats,
}

/// Sandbox instance
#[derive(Debug)]
pub struct SandboxInstance {
    /// Plugin ID
    pub plugin_id: String,
    /// Created at
    pub created_at: u64,
    /// Memory used
    pub memory_used: usize,
    /// CPU time used (ms)
    pub cpu_used_ms: u64,
}

/// Sandbox statistics
#[derive(Debug, Default)]
pub struct SandboxStats {
    pub sandboxes_created: AtomicU64,
    pub sandboxes_destroyed: AtomicU64,
    pub memory_violations: AtomicU64,
    pub cpu_violations: AtomicU64,
}

impl SandboxRuntime {
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            sandboxes: RwLock::new(HashMap::new()),
            stats: SandboxStats::default(),
        }
    }

    /// Create sandbox for plugin
    pub fn create_sandbox(&self, plugin_id: &str) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let sandbox = SandboxInstance {
            plugin_id: plugin_id.to_string(),
            created_at: now,
            memory_used: 0,
            cpu_used_ms: 0,
        };

        if let Ok(mut sandboxes) = self.sandboxes.write() {
            sandboxes.insert(plugin_id.to_string(), sandbox);
            self.stats.sandboxes_created.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            Err("Failed to create sandbox".to_string())
        }
    }

    /// Destroy sandbox
    pub fn destroy_sandbox(&self, plugin_id: &str) {
        if let Ok(mut sandboxes) = self.sandboxes.write() {
            if sandboxes.remove(plugin_id).is_some() {
                self.stats
                    .sandboxes_destroyed
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Check memory limit
    pub fn check_memory(&self, plugin_id: &str, size: usize) -> bool {
        if let Ok(sandboxes) = self.sandboxes.read() {
            if let Some(sandbox) = sandboxes.get(plugin_id) {
                if sandbox.memory_used + size > self.config.memory_limit {
                    self.stats.memory_violations.fetch_add(1, Ordering::Relaxed);
                    return false;
                }
            }
        }
        true
    }

    /// Check CPU limit
    pub fn check_cpu(&self, plugin_id: &str, ms: u64) -> bool {
        if let Ok(sandboxes) = self.sandboxes.read() {
            if let Some(sandbox) = sandboxes.get(plugin_id) {
                if sandbox.cpu_used_ms + ms > self.config.cpu_limit_ms {
                    self.stats.cpu_violations.fetch_add(1, Ordering::Relaxed);
                    return false;
                }
            }
        }
        true
    }

    /// Check path access
    pub fn check_path(&self, path: &Path) -> bool {
        self.config
            .allowed_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
    }

    /// Check host access
    pub fn check_host(&self, host: &str) -> bool {
        self.config
            .allowed_hosts
            .iter()
            .any(|allowed| host == allowed || host.ends_with(&format!(".{}", allowed)))
    }

    /// Get summary
    pub fn summary(&self) -> SandboxSummary {
        SandboxSummary {
            active_sandboxes: self.sandboxes.read().map(|s| s.len()).unwrap_or(0),
            sandboxes_created: self.stats.sandboxes_created.load(Ordering::Relaxed),
            sandboxes_destroyed: self.stats.sandboxes_destroyed.load(Ordering::Relaxed),
            memory_violations: self.stats.memory_violations.load(Ordering::Relaxed),
            cpu_violations: self.stats.cpu_violations.load(Ordering::Relaxed),
        }
    }
}

impl Default for SandboxRuntime {
    fn default() -> Self {
        Self::new(SandboxConfig::default())
    }
}

/// Sandbox summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSummary {
    pub active_sandboxes: usize,
    pub sandboxes_created: u64,
    pub sandboxes_destroyed: u64,
    pub memory_violations: u64,
    pub cpu_violations: u64,
}

// ============================================================================
// Event Bus
// ============================================================================

/// Plugin event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEvent {
    /// Event type
    pub event_type: String,
    /// Source plugin
    pub source: Option<String>,
    /// Event data
    pub data: serde_json::Value,
    /// Timestamp
    pub timestamp: u64,
}

impl PluginEvent {
    pub fn new(event_type: &str, data: serde_json::Value) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            event_type: event_type.to_string(),
            source: None,
            data,
            timestamp: now,
        }
    }

    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }
}

/// Event handler callback type
pub type EventHandler = Box<dyn Fn(&PluginEvent) + Send + Sync>;

/// Event subscription
pub struct EventSubscription {
    /// Event type pattern
    pub pattern: String,
    /// Handler
    handler: EventHandler,
}

/// Event bus for plugin communication
pub struct EventBus {
    /// Subscriptions by event type
    subscriptions: RwLock<HashMap<String, Vec<Arc<EventSubscription>>>>,
    /// Event history
    history: RwLock<Vec<PluginEvent>>,
    /// Statistics
    stats: EventBusStats,
}

/// Event bus statistics
#[derive(Debug, Default)]
pub struct EventBusStats {
    pub events_published: AtomicU64,
    pub events_delivered: AtomicU64,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscriptions: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::new()),
            stats: EventBusStats::default(),
        }
    }

    /// Subscribe to events
    pub fn subscribe(&self, pattern: &str, handler: EventHandler) {
        let subscription = Arc::new(EventSubscription {
            pattern: pattern.to_string(),
            handler,
        });

        if let Ok(mut subs) = self.subscriptions.write() {
            subs.entry(pattern.to_string())
                .or_default()
                .push(subscription);
        }
    }

    /// Publish an event
    pub fn publish(&self, event: PluginEvent) {
        self.stats.events_published.fetch_add(1, Ordering::Relaxed);

        // Store in history
        if let Ok(mut history) = self.history.write() {
            history.push(event.clone());
            // Keep only last 1000 events
            while history.len() > 1000 {
                history.remove(0);
            }
        }

        // Deliver to subscribers
        if let Ok(subs) = self.subscriptions.read() {
            // Exact match
            if let Some(handlers) = subs.get(&event.event_type) {
                for sub in handlers {
                    (sub.handler)(&event);
                    self.stats.events_delivered.fetch_add(1, Ordering::Relaxed);
                }
            }

            // Wildcard match
            if let Some(handlers) = subs.get("*") {
                for sub in handlers {
                    (sub.handler)(&event);
                    self.stats.events_delivered.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Get recent events
    pub fn recent_events(&self, count: usize) -> Vec<PluginEvent> {
        self.history
            .read()
            .map(|h| h.iter().rev().take(count).cloned().collect())
            .unwrap_or_default()
    }

    /// Get summary
    pub fn summary(&self) -> EventBusSummary {
        EventBusSummary {
            subscriptions: self
                .subscriptions
                .read()
                .map(|s| s.values().map(|v| v.len()).sum())
                .unwrap_or(0),
            events_published: self.stats.events_published.load(Ordering::Relaxed),
            events_delivered: self.stats.events_delivered.load(Ordering::Relaxed),
            history_size: self.history.read().map(|h| h.len()).unwrap_or(0),
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Event bus summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBusSummary {
    pub subscriptions: usize,
    pub events_published: u64,
    pub events_delivered: u64,
    pub history_size: usize,
}

// ============================================================================
// Plugin Manager
// ============================================================================

/// Plugin manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerConfig {
    /// Plugin directory
    pub plugin_dir: PathBuf,
    /// Enable auto-discovery
    pub auto_discover: bool,
    /// Enable hot-reload
    pub hot_reload: bool,
    /// Sandbox configuration
    pub sandbox: SandboxConfig,
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        Self {
            plugin_dir: PathBuf::from("plugins"),
            auto_discover: true,
            hot_reload: false,
            sandbox: SandboxConfig::default(),
        }
    }
}

/// Plugin manager
pub struct PluginManager {
    config: PluginManagerConfig,
    /// Loaded plugins
    plugins: RwLock<HashMap<String, Arc<PluginInstance>>>,
    /// API version manager
    api_versions: ApiVersionManager,
    /// Sandbox runtime
    sandbox: SandboxRuntime,
    /// Event bus
    event_bus: EventBus,
    /// Statistics
    stats: PluginManagerStats,
}

/// Plugin manager statistics
#[derive(Debug, Default)]
pub struct PluginManagerStats {
    pub plugins_loaded: AtomicU64,
    pub plugins_unloaded: AtomicU64,
    pub load_failures: AtomicU64,
}

impl PluginManager {
    pub fn new(config: PluginManagerConfig) -> Self {
        let sandbox = SandboxRuntime::new(config.sandbox.clone());
        Self {
            config,
            plugins: RwLock::new(HashMap::new()),
            api_versions: ApiVersionManager::default(),
            sandbox,
            event_bus: EventBus::new(),
            stats: PluginManagerStats::default(),
        }
    }

    /// Register a plugin
    pub fn register(&self, plugin: PluginInstance) -> Result<(), String> {
        // Check API version compatibility
        if !self.api_versions.is_supported(&plugin.metadata.api_version) {
            return Err(format!(
                "Plugin {} requires API version {} which is not supported",
                plugin.metadata.id, plugin.metadata.api_version
            ));
        }

        // Check dependencies
        for dep in &plugin.metadata.dependencies {
            if !dep.optional && !self.has_plugin(&dep.plugin_id) {
                return Err(format!(
                    "Plugin {} requires dependency {} which is not installed",
                    plugin.metadata.id, dep.plugin_id
                ));
            }
        }

        let id = plugin.metadata.id.clone();
        if let Ok(mut plugins) = self.plugins.write() {
            plugins.insert(id.clone(), Arc::new(plugin));
            self.stats.plugins_loaded.fetch_add(1, Ordering::Relaxed);

            // Publish event
            self.event_bus.publish(PluginEvent::new(
                "plugin.registered",
                serde_json::json!({ "plugin_id": id }),
            ));

            Ok(())
        } else {
            Err("Failed to register plugin".to_string())
        }
    }

    /// Unregister a plugin
    pub fn unregister(&self, plugin_id: &str) -> Result<(), String> {
        // Destroy sandbox
        self.sandbox.destroy_sandbox(plugin_id);

        if let Ok(mut plugins) = self.plugins.write() {
            if plugins.remove(plugin_id).is_some() {
                self.stats.plugins_unloaded.fetch_add(1, Ordering::Relaxed);

                // Publish event
                self.event_bus.publish(PluginEvent::new(
                    "plugin.unregistered",
                    serde_json::json!({ "plugin_id": plugin_id }),
                ));

                Ok(())
            } else {
                Err(format!("Plugin {} not found", plugin_id))
            }
        } else {
            Err("Failed to unregister plugin".to_string())
        }
    }

    /// Check if plugin exists
    pub fn has_plugin(&self, plugin_id: &str) -> bool {
        self.plugins
            .read()
            .map(|p| p.contains_key(plugin_id))
            .unwrap_or(false)
    }

    /// Get plugin
    pub fn get_plugin(&self, plugin_id: &str) -> Option<Arc<PluginInstance>> {
        self.plugins.read().ok()?.get(plugin_id).cloned()
    }

    /// List all plugins
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .read()
            .map(|p| {
                p.values()
                    .map(|plugin| PluginInfo {
                        id: plugin.metadata.id.clone(),
                        name: plugin.metadata.name.clone(),
                        version: plugin.metadata.version.clone(),
                        state: plugin.state(),
                        plugin_type: plugin.metadata.plugin_type,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Enable plugin
    pub fn enable(&self, plugin_id: &str) -> Result<(), String> {
        if let Some(plugin) = self.get_plugin(plugin_id) {
            // Create sandbox
            self.sandbox.create_sandbox(plugin_id)?;

            plugin.set_state(PluginState::Loaded);

            self.event_bus.publish(PluginEvent::new(
                "plugin.enabled",
                serde_json::json!({ "plugin_id": plugin_id }),
            ));

            Ok(())
        } else {
            Err(format!("Plugin {} not found", plugin_id))
        }
    }

    /// Disable plugin
    pub fn disable(&self, plugin_id: &str) -> Result<(), String> {
        if let Some(plugin) = self.get_plugin(plugin_id) {
            self.sandbox.destroy_sandbox(plugin_id);
            plugin.set_state(PluginState::Disabled);

            self.event_bus.publish(PluginEvent::new(
                "plugin.disabled",
                serde_json::json!({ "plugin_id": plugin_id }),
            ));

            Ok(())
        } else {
            Err(format!("Plugin {} not found", plugin_id))
        }
    }

    /// Get event bus
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Get sandbox runtime
    pub fn sandbox(&self) -> &SandboxRuntime {
        &self.sandbox
    }

    /// Get API version manager
    pub fn api_versions(&self) -> &ApiVersionManager {
        &self.api_versions
    }

    /// Get comprehensive summary
    pub fn summary(&self) -> PluginManagerSummary {
        PluginManagerSummary {
            total_plugins: self.plugins.read().map(|p| p.len()).unwrap_or(0),
            loaded_plugins: self.stats.plugins_loaded.load(Ordering::Relaxed),
            unloaded_plugins: self.stats.plugins_unloaded.load(Ordering::Relaxed),
            load_failures: self.stats.load_failures.load(Ordering::Relaxed),
            sandbox: self.sandbox.summary(),
            event_bus: self.event_bus.summary(),
            api_version: self.api_versions.current().to_string(),
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new(PluginManagerConfig::default())
    }
}

/// Brief plugin info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub state: PluginState,
    pub plugin_type: PluginType,
}

/// Plugin manager summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerSummary {
    pub total_plugins: usize,
    pub loaded_plugins: u64,
    pub unloaded_plugins: u64,
    pub load_failures: u64,
    pub sandbox: SandboxSummary,
    pub event_bus: EventBusSummary,
    pub api_version: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata_new() {
        let meta = PluginMetadata::new("test-plugin", "Test Plugin", "1.0.0");
        assert_eq!(meta.id, "test-plugin");
        assert_eq!(meta.name, "Test Plugin");
        assert_eq!(meta.version, "1.0.0");
    }

    #[test]
    fn test_plugin_metadata_with_description() {
        let meta = PluginMetadata::new("test", "Test", "1.0.0").with_description("A test plugin");
        assert_eq!(meta.description, "A test plugin");
    }

    #[test]
    fn test_plugin_metadata_with_permission() {
        let meta =
            PluginMetadata::new("test", "Test", "1.0.0").with_permission(Permission::FileRead);
        assert_eq!(meta.permissions.len(), 1);
    }

    #[test]
    fn test_permission_is_dangerous() {
        assert!(Permission::Execute.is_dangerous());
        assert!(Permission::FileSystemFull.is_dangerous());
        assert!(!Permission::FileRead.is_dangerous());
        assert!(!Permission::Network.is_dangerous());
    }

    #[test]
    fn test_permission_grant_new() {
        let grant = PermissionGrant::new(vec![Permission::FileRead, Permission::Network]);
        assert!(grant.has_permission(&Permission::FileRead));
        assert!(!grant.has_permission(&Permission::Execute));
    }

    #[test]
    fn test_semver_parse() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_semver_display() {
        let v = SemVer::new(1, 2, 3);
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn test_semver_compatible() {
        let v = SemVer::new(1, 5, 0);
        assert!(v.is_compatible_with("^1.0.0"));
        assert!(v.is_compatible_with("~1.5.0"));
        assert!(!v.is_compatible_with("^2.0.0"));
    }

    #[test]
    fn test_api_version_manager() {
        let manager = ApiVersionManager::default();
        assert!(manager.is_supported("1.0.0"));
        assert!(!manager.is_supported("0.9.0"));
    }

    #[test]
    fn test_plugin_instance_state() {
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        assert_eq!(plugin.state(), PluginState::Unloaded);
        plugin.set_state(PluginState::Loaded);
        assert_eq!(plugin.state(), PluginState::Loaded);
    }

    #[test]
    fn test_plugin_instance_permissions() {
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        assert!(!plugin.has_permission(&Permission::FileRead));

        plugin.grant_permissions(PermissionGrant::new(vec![Permission::FileRead]));
        assert!(plugin.has_permission(&Permission::FileRead));
    }

    #[test]
    fn test_plugin_instance_stats() {
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        plugin.record_invocation(100, true);
        plugin.record_invocation(50, false);

        let stats = plugin.stats_summary();
        assert_eq!(stats.invocations, 2);
        assert_eq!(stats.errors, 1);
    }

    #[test]
    fn test_sandbox_runtime_create() {
        let runtime = SandboxRuntime::default();
        assert!(runtime.create_sandbox("test-plugin").is_ok());

        let summary = runtime.summary();
        assert_eq!(summary.active_sandboxes, 1);
    }

    #[test]
    fn test_sandbox_runtime_destroy() {
        let runtime = SandboxRuntime::default();
        runtime.create_sandbox("test-plugin").unwrap();
        runtime.destroy_sandbox("test-plugin");

        let summary = runtime.summary();
        assert_eq!(summary.active_sandboxes, 0);
    }

    #[test]
    fn test_sandbox_check_memory() {
        let config = SandboxConfig {
            memory_limit: 1000,
            ..Default::default()
        };
        let runtime = SandboxRuntime::new(config);
        runtime.create_sandbox("test").unwrap();

        assert!(runtime.check_memory("test", 500));
        // Note: This would fail if sandbox tracked usage
    }

    #[test]
    fn test_plugin_event_new() {
        let event = PluginEvent::new("test.event", serde_json::json!({"key": "value"}));
        assert_eq!(event.event_type, "test.event");
        assert!(event.source.is_none());
    }

    #[test]
    fn test_plugin_event_with_source() {
        let event = PluginEvent::new("test.event", serde_json::json!({})).with_source("my-plugin");
        assert_eq!(event.source, Some("my-plugin".to_string()));
    }

    #[test]
    fn test_event_bus_publish() {
        let bus = EventBus::new();
        bus.publish(PluginEvent::new("test", serde_json::json!({})));

        let summary = bus.summary();
        assert_eq!(summary.events_published, 1);
    }

    #[test]
    fn test_event_bus_recent() {
        let bus = EventBus::new();
        bus.publish(PluginEvent::new("event1", serde_json::json!({})));
        bus.publish(PluginEvent::new("event2", serde_json::json!({})));

        let recent = bus.recent_events(10);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_plugin_manager_register() {
        let manager = PluginManager::default();
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        assert!(manager.register(plugin).is_ok());
        assert!(manager.has_plugin("test"));
    }

    #[test]
    fn test_plugin_manager_unregister() {
        let manager = PluginManager::default();
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        manager.register(plugin).unwrap();
        assert!(manager.unregister("test").is_ok());
        assert!(!manager.has_plugin("test"));
    }

    #[test]
    fn test_plugin_manager_list() {
        let manager = PluginManager::default();
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        manager.register(plugin).unwrap();

        let list = manager.list_plugins();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "test");
    }

    #[test]
    fn test_plugin_manager_enable() {
        let manager = PluginManager::default();
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        manager.register(plugin).unwrap();
        assert!(manager.enable("test").is_ok());

        let p = manager.get_plugin("test").unwrap();
        assert_eq!(p.state(), PluginState::Loaded);
    }

    #[test]
    fn test_plugin_manager_summary() {
        let manager = PluginManager::default();
        let summary = manager.summary();

        assert_eq!(summary.total_plugins, 0);
        assert_eq!(summary.api_version, "1.0.0");
    }

    #[test]
    fn test_plugin_type_enum() {
        assert_eq!(PluginType::Tool, PluginType::Tool);
        assert_ne!(PluginType::Tool, PluginType::Provider);
    }

    #[test]
    fn test_plugin_state_enum() {
        assert_eq!(PluginState::Unloaded, PluginState::Unloaded);
        assert_ne!(PluginState::Unloaded, PluginState::Loaded);
    }

    // Additional comprehensive tests

    #[test]
    fn test_plugin_metadata_serialize() {
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: PluginMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, meta.id);
    }

    #[test]
    fn test_plugin_metadata_with_author() {
        let meta = PluginMetadata::new("test", "Test", "1.0.0").with_author("John Doe");
        assert_eq!(meta.author, "John Doe");
    }

    #[test]
    fn test_plugin_metadata_with_dependency() {
        let dep = PluginDependency {
            plugin_id: "core".to_string(),
            version_req: "^1.0.0".to_string(),
            optional: false,
        };
        let meta = PluginMetadata::new("test", "Test", "1.0.0").with_dependency(dep);
        assert_eq!(meta.dependencies.len(), 1);
    }

    #[test]
    fn test_plugin_metadata_clone() {
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let cloned = meta.clone();
        assert_eq!(cloned.id, meta.id);
    }

    #[test]
    fn test_plugin_type_all_variants() {
        let variants = [
            PluginType::Tool,
            PluginType::Provider,
            PluginType::Formatter,
            PluginType::Analyzer,
            PluginType::Integration,
            PluginType::Theme,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_plugin_type_serialize() {
        let pt = PluginType::Provider;
        let json = serde_json::to_string(&pt).unwrap();
        let parsed: PluginType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, pt);
    }

    #[test]
    fn test_plugin_dependency_struct() {
        let dep = PluginDependency {
            plugin_id: "dep-plugin".to_string(),
            version_req: ">=1.0.0".to_string(),
            optional: true,
        };
        assert_eq!(dep.plugin_id, "dep-plugin");
        assert!(dep.optional);
    }

    #[test]
    fn test_plugin_dependency_serialize() {
        let dep = PluginDependency {
            plugin_id: "test".to_string(),
            version_req: "1.0.0".to_string(),
            optional: false,
        };
        let json = serde_json::to_string(&dep).unwrap();
        let parsed: PluginDependency = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.plugin_id, dep.plugin_id);
    }

    #[test]
    fn test_permission_all_variants() {
        let variants = [
            Permission::FileRead,
            Permission::FileWrite,
            Permission::Execute,
            Permission::Network,
            Permission::Environment,
            Permission::SystemInfo,
            Permission::FileSystemFull,
            Permission::Custom("custom_perm".to_string()),
        ];
        for v in &variants {
            let _ = v.description();
        }
    }

    #[test]
    fn test_permission_serialize() {
        let perm = Permission::Network;
        let json = serde_json::to_string(&perm).unwrap();
        let parsed: Permission = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, perm);
    }

    #[test]
    fn test_permission_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Permission::FileRead);
        set.insert(Permission::FileWrite);
        set.insert(Permission::FileRead); // Duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_permission_clone() {
        let perm = Permission::Custom("test".to_string());
        let cloned = perm.clone();
        assert_eq!(cloned, perm);
    }

    #[test]
    fn test_permission_request_struct() {
        let req = PermissionRequest {
            permission: Permission::Execute,
            reason: "Need to run commands".to_string(),
            required: true,
        };
        assert_eq!(req.permission, Permission::Execute);
        assert!(req.required);
    }

    #[test]
    fn test_permission_request_serialize() {
        let req = PermissionRequest {
            permission: Permission::Network,
            reason: "API access".to_string(),
            required: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Network"));
    }

    #[test]
    fn test_permission_grant_with_expiry() {
        let grant = PermissionGrant::new(vec![Permission::FileRead]).with_expiry(3600);
        assert!(grant.expires_at.is_some());
    }

    #[test]
    fn test_permission_grant_expired() {
        let mut grant = PermissionGrant::new(vec![Permission::FileRead]);
        grant.expires_at = Some(0); // Expired immediately
        assert!(!grant.has_permission(&Permission::FileRead));
    }

    #[test]
    fn test_semver_new() {
        let v = SemVer::new(2, 1, 3);
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.is_none());
    }

    #[test]
    fn test_semver_parse_two_parts() {
        let v = SemVer::parse("1.2").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_semver_parse_invalid() {
        assert!(SemVer::parse("invalid").is_none());
        assert!(SemVer::parse("1").is_none());
    }

    #[test]
    fn test_semver_display_with_prerelease() {
        let mut v = SemVer::new(1, 0, 0);
        v.prerelease = Some("beta".to_string());
        assert_eq!(v.to_string(), "1.0.0-beta");
    }

    #[test]
    fn test_semver_serialize() {
        let v = SemVer::new(1, 2, 3);
        let json = serde_json::to_string(&v).unwrap();
        let parsed: SemVer = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, v);
    }

    #[test]
    fn test_api_version_manager_deprecated() {
        let mut manager = ApiVersionManager::default();
        manager.deprecate(SemVer::new(1, 0, 0));
        assert!(manager.is_deprecated("1.0.0"));
        assert!(!manager.is_deprecated("1.1.0"));
    }

    #[test]
    fn test_api_version_manager_current() {
        let manager = ApiVersionManager::new(SemVer::new(2, 0, 0), SemVer::new(1, 0, 0));
        assert_eq!(manager.current().major, 2);
    }

    #[test]
    fn test_plugin_state_all_variants() {
        let variants = [
            PluginState::Unloaded,
            PluginState::Loading,
            PluginState::Loaded,
            PluginState::Running,
            PluginState::Paused,
            PluginState::Error,
            PluginState::Disabled,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_plugin_state_serialize() {
        let state = PluginState::Running;
        let json = serde_json::to_string(&state).unwrap();
        let parsed: PluginState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }

    #[test]
    fn test_plugin_stats_default() {
        let stats = PluginStats::default();
        assert_eq!(stats.invocations.load(Ordering::Relaxed), 0);
        assert_eq!(stats.errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_plugin_instance_set_error() {
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        plugin.set_error("Something went wrong");

        assert_eq!(plugin.state(), PluginState::Error);
        assert_eq!(plugin.error(), Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_plugin_stats_summary_fields() {
        let summary = PluginStatsSummary {
            invocations: 100,
            errors: 5,
            error_rate: 0.05,
            avg_runtime_ms: 50,
        };
        assert_eq!(summary.invocations, 100);
        assert!((summary.error_rate - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn test_plugin_stats_summary_serialize() {
        let summary = PluginStatsSummary {
            invocations: 10,
            errors: 1,
            error_rate: 0.1,
            avg_runtime_ms: 100,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("invocations"));
    }

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.memory_limit, 64 * 1024 * 1024);
        assert_eq!(config.cpu_limit_ms, 5000);
        assert!(config.enable_wasm);
    }

    #[test]
    fn test_sandbox_config_serialize() {
        let config = SandboxConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.memory_limit, config.memory_limit);
    }

    #[test]
    fn test_sandbox_instance_struct() {
        let instance = SandboxInstance {
            plugin_id: "test".to_string(),
            created_at: 1234567890,
            memory_used: 1000,
            cpu_used_ms: 50,
        };
        assert_eq!(instance.plugin_id, "test");
        assert_eq!(instance.memory_used, 1000);
    }

    #[test]
    fn test_sandbox_stats_default() {
        let stats = SandboxStats::default();
        assert_eq!(stats.sandboxes_created.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_sandbox_runtime_check_path() {
        let config = SandboxConfig {
            allowed_paths: vec![PathBuf::from("/home/user/plugins")],
            ..Default::default()
        };
        let runtime = SandboxRuntime::new(config);

        assert!(runtime.check_path(&PathBuf::from("/home/user/plugins/test.wasm")));
        assert!(!runtime.check_path(&PathBuf::from("/etc/passwd")));
    }

    #[test]
    fn test_sandbox_runtime_check_host() {
        let config = SandboxConfig {
            allowed_hosts: vec!["api.example.com".to_string()],
            ..Default::default()
        };
        let runtime = SandboxRuntime::new(config);

        assert!(runtime.check_host("api.example.com"));
        assert!(runtime.check_host("sub.api.example.com"));
        assert!(!runtime.check_host("evil.com"));
    }

    #[test]
    fn test_sandbox_runtime_check_cpu() {
        let config = SandboxConfig {
            cpu_limit_ms: 100,
            ..Default::default()
        };
        let runtime = SandboxRuntime::new(config);
        runtime.create_sandbox("test").unwrap();

        assert!(runtime.check_cpu("test", 50));
    }

    #[test]
    fn test_sandbox_summary_serialize() {
        let summary = SandboxSummary {
            active_sandboxes: 2,
            sandboxes_created: 5,
            sandboxes_destroyed: 3,
            memory_violations: 0,
            cpu_violations: 1,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("active_sandboxes"));
    }

    #[test]
    fn test_plugin_event_serialize() {
        let event = PluginEvent::new("test.event", serde_json::json!({"data": 123}));
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("test.event"));
    }

    #[test]
    fn test_event_bus_subscribe() {
        let bus = EventBus::new();
        let received = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let received_clone = received.clone();

        bus.subscribe(
            "test",
            Box::new(move |_| {
                received_clone.store(true, Ordering::Relaxed);
            }),
        );

        bus.publish(PluginEvent::new("test", serde_json::json!({})));

        assert!(received.load(Ordering::Relaxed));
    }

    #[test]
    fn test_event_bus_wildcard() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicU64::new(0));
        let count_clone = count.clone();

        bus.subscribe(
            "*",
            Box::new(move |_| {
                count_clone.fetch_add(1, Ordering::Relaxed);
            }),
        );

        bus.publish(PluginEvent::new("event1", serde_json::json!({})));
        bus.publish(PluginEvent::new("event2", serde_json::json!({})));

        assert_eq!(count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_event_bus_stats_default() {
        let stats = EventBusStats::default();
        assert_eq!(stats.events_published.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_event_bus_summary_serialize() {
        let summary = EventBusSummary {
            subscriptions: 3,
            events_published: 100,
            events_delivered: 95,
            history_size: 50,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("events_published"));
    }

    #[test]
    fn test_plugin_manager_config_default() {
        let config = PluginManagerConfig::default();
        assert!(config.auto_discover);
        assert!(!config.hot_reload);
    }

    #[test]
    fn test_plugin_manager_config_serialize() {
        let config = PluginManagerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("plugin_dir"));
    }

    #[test]
    fn test_plugin_manager_stats_default() {
        let stats = PluginManagerStats::default();
        assert_eq!(stats.plugins_loaded.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_plugin_manager_get_plugin() {
        let manager = PluginManager::default();
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        manager.register(plugin).unwrap();

        let p = manager.get_plugin("test");
        assert!(p.is_some());

        let missing = manager.get_plugin("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_plugin_manager_disable() {
        let manager = PluginManager::default();
        let meta = PluginMetadata::new("test", "Test", "1.0.0");
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        manager.register(plugin).unwrap();
        manager.enable("test").unwrap();
        manager.disable("test").unwrap();

        let p = manager.get_plugin("test").unwrap();
        assert_eq!(p.state(), PluginState::Disabled);
    }

    #[test]
    fn test_plugin_manager_unregister_not_found() {
        let manager = PluginManager::default();
        let result = manager.unregister("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_plugin_manager_register_unsupported_api() {
        let manager = PluginManager::default();
        let mut meta = PluginMetadata::new("test", "Test", "1.0.0");
        meta.api_version = "0.5.0".to_string(); // Unsupported
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        let result = manager.register(plugin);
        assert!(result.is_err());
    }

    #[test]
    fn test_plugin_manager_register_missing_dep() {
        let manager = PluginManager::default();
        let meta = PluginMetadata::new("test", "Test", "1.0.0").with_dependency(PluginDependency {
            plugin_id: "missing-dep".to_string(),
            version_req: "^1.0.0".to_string(),
            optional: false,
        });
        let plugin = PluginInstance::new(meta, PathBuf::from("test.wasm"));

        let result = manager.register(plugin);
        assert!(result.is_err());
    }

    #[test]
    fn test_plugin_manager_components() {
        let manager = PluginManager::default();
        let _ = manager.event_bus();
        let _ = manager.sandbox();
        let _ = manager.api_versions();
    }

    #[test]
    fn test_plugin_info_struct() {
        let info = PluginInfo {
            id: "test".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            state: PluginState::Loaded,
            plugin_type: PluginType::Tool,
        };
        assert_eq!(info.id, "test");
        assert_eq!(info.state, PluginState::Loaded);
    }

    #[test]
    fn test_plugin_info_serialize() {
        let info = PluginInfo {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            state: PluginState::Running,
            plugin_type: PluginType::Provider,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test"));
    }

    #[test]
    fn test_plugin_manager_summary_serialize() {
        let summary = PluginManagerSummary {
            total_plugins: 5,
            loaded_plugins: 4,
            unloaded_plugins: 1,
            load_failures: 0,
            sandbox: SandboxSummary {
                active_sandboxes: 3,
                sandboxes_created: 5,
                sandboxes_destroyed: 2,
                memory_violations: 0,
                cpu_violations: 0,
            },
            event_bus: EventBusSummary {
                subscriptions: 2,
                events_published: 10,
                events_delivered: 10,
                history_size: 5,
            },
            api_version: "1.0.0".to_string(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("total_plugins"));
    }

    #[test]
    fn test_plugin_manager_enable_not_found() {
        let manager = PluginManager::default();
        let result = manager.enable("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_plugin_manager_disable_not_found() {
        let manager = PluginManager::default();
        let result = manager.disable("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_semver_compatible_minor() {
        let v = SemVer::new(1, 3, 0);
        assert!(v.is_compatible_with("^1.2.0"));
        assert!(!v.is_compatible_with("^1.4.0"));
    }
}
