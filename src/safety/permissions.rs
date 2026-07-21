//! Permission grants for tool execution.
//!
//! Allows users to pre-authorize specific tools/patterns with optional
//! time-based expiry, reducing confirmation prompts for trusted operations.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// A pre-authorized permission grant for tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrant {
    /// Tool name pattern (exact match or glob, e.g., "file_*").
    pub tool_pattern: String,
    /// Resource pattern the grant applies to (e.g., "./src/**").
    #[serde(default)]
    pub resource_pattern: Option<String>,
    /// When this grant expires. None means it persists until removed.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// Human-readable reason for the grant.
    #[serde(default)]
    pub reason: Option<String>,
}

impl PermissionGrant {
    /// Create a permanent grant for a tool pattern.
    pub fn permanent(tool_pattern: &str) -> Self {
        Self {
            tool_pattern: tool_pattern.to_string(),
            resource_pattern: None,
            expires_at: None,
            reason: None,
        }
    }

    /// Create a grant that expires after `duration` from now.
    pub fn temporary(tool_pattern: &str, duration: Duration) -> Self {
        Self {
            tool_pattern: tool_pattern.to_string(),
            resource_pattern: None,
            expires_at: Some(Utc::now() + duration),
            reason: None,
        }
    }

    /// Create a session-scoped grant (expires in 24 hours).
    pub fn session(tool_pattern: &str) -> Self {
        Self::temporary(tool_pattern, Duration::hours(24))
    }

    /// Add a resource pattern constraint.
    pub fn with_resource(mut self, pattern: &str) -> Self {
        self.resource_pattern = Some(pattern.to_string());
        self
    }

    /// Add a reason for the grant.
    pub fn with_reason(mut self, reason: &str) -> Self {
        self.reason = Some(reason.to_string());
        self
    }

    /// Check if this grant has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }

    /// Check if this grant matches a tool name.
    pub fn matches_tool(&self, tool_name: &str) -> bool {
        if self.is_expired() {
            return false;
        }
        pattern_matches(&self.tool_pattern, tool_name)
    }

    /// Check if this grant matches a tool name and resource path.
    pub fn matches(&self, tool_name: &str, resource_path: Option<&str>) -> bool {
        if !self.matches_tool(tool_name) {
            return false;
        }

        // If the grant has a resource pattern, the path must match
        if let Some(ref res_pattern) = self.resource_pattern {
            if let Some(path) = resource_path {
                return pattern_matches(res_pattern, path);
            }
            return false; // Grant requires a resource but none provided
        }

        true // No resource constraint
    }
}

/// Simple glob-like pattern matching (supports `*` wildcard).
fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }

    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }

    pattern == value
}

/// Store for managing permission grants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionStore {
    grants: Vec<PermissionGrant>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self { grants: Vec::new() }
    }

    /// Create a store from configuration grants.
    pub fn from_config(grants: &[PermissionGrant]) -> Self {
        Self {
            grants: grants.to_vec(),
        }
    }

    /// Add a new grant.
    pub fn add(&mut self, grant: PermissionGrant) {
        info!(
            "Permission granted: {} (expires: {:?})",
            grant.tool_pattern, grant.expires_at
        );
        self.grants.push(grant);
    }

    /// Check if the given tool+resource is authorized by any grant.
    pub fn is_authorized(&self, tool_name: &str, resource_path: Option<&str>) -> bool {
        self.grants
            .iter()
            .any(|g| g.matches(tool_name, resource_path))
    }

    /// Remove expired grants.
    pub fn cleanup_expired(&mut self) -> usize {
        let before = self.grants.len();
        self.grants.retain(|g| !g.is_expired());
        let removed = before - self.grants.len();
        if removed > 0 {
            debug!("Cleaned up {} expired permission grant(s)", removed);
        }
        removed
    }

    /// Number of active (non-expired) grants.
    pub fn active_count(&self) -> usize {
        self.grants.iter().filter(|g| !g.is_expired()).count()
    }

    /// Clear all grants.
    pub fn clear(&mut self) {
        self.grants.clear();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/permissions/permissions_test.rs"]
mod tests;
