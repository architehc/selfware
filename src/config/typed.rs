//! Typed Configuration System
//!
//! Type-safe configuration with:
//! - Schema validation
//! - CLI flag generation
//! - Hot reload watching
//! - Configuration wizard support

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Configuration value types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum ConfigValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<ConfigValue>),
    Map(HashMap<String, ConfigValue>),
    Duration(std::time::Duration),
    Path(PathBuf),
    Secret(String), // Will be redacted in logs
    #[default]
    Null,
}

impl ConfigValue {
    /// Create a string value
    pub fn string(s: impl Into<String>) -> Self {
        ConfigValue::String(s.into())
    }

    /// Create an integer value
    pub fn int(i: i64) -> Self {
        ConfigValue::Integer(i)
    }

    /// Create a float value
    pub fn float(f: f64) -> Self {
        ConfigValue::Float(f)
    }

    /// Create a boolean value
    pub fn bool(b: bool) -> Self {
        ConfigValue::Boolean(b)
    }

    /// Create a path value
    pub fn path(p: impl Into<PathBuf>) -> Self {
        ConfigValue::Path(p.into())
    }

    /// Create a secret value
    pub fn secret(s: impl Into<String>) -> Self {
        ConfigValue::Secret(s.into())
    }

    /// Create a duration value
    pub fn duration(secs: u64) -> Self {
        ConfigValue::Duration(std::time::Duration::from_secs(secs))
    }

    /// Try to get as string
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ConfigValue::String(s) | ConfigValue::Secret(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as integer
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ConfigValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Try to get as float
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ConfigValue::Float(f) => Some(*f),
            ConfigValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to get as boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as path
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            ConfigValue::Path(p) => Some(p),
            ConfigValue::String(s) => Some(Path::new(s)),
            _ => None,
        }
    }

    /// Try to get as duration
    pub fn as_duration(&self) -> Option<std::time::Duration> {
        match self {
            ConfigValue::Duration(d) => Some(*d),
            ConfigValue::Integer(i) if *i >= 0 => Some(std::time::Duration::from_secs(*i as u64)),
            _ => None,
        }
    }

    /// Get type name
    pub fn type_name(&self) -> &'static str {
        match self {
            ConfigValue::String(_) => "string",
            ConfigValue::Integer(_) => "integer",
            ConfigValue::Float(_) => "float",
            ConfigValue::Boolean(_) => "boolean",
            ConfigValue::Array(_) => "array",
            ConfigValue::Map(_) => "map",
            ConfigValue::Duration(_) => "duration",
            ConfigValue::Path(_) => "path",
            ConfigValue::Secret(_) => "secret",
            ConfigValue::Null => "null",
        }
    }

    /// Is null
    pub fn is_null(&self) -> bool {
        matches!(self, ConfigValue::Null)
    }

    /// Display value (secrets are redacted)
    pub fn display(&self) -> String {
        match self {
            ConfigValue::String(s) => format!("\"{}\"", s),
            ConfigValue::Integer(i) => i.to_string(),
            ConfigValue::Float(f) => f.to_string(),
            ConfigValue::Boolean(b) => b.to_string(),
            ConfigValue::Array(a) => format!("[{} items]", a.len()),
            ConfigValue::Map(m) => format!("{{{} keys}}", m.len()),
            ConfigValue::Duration(d) => format!("{}s", d.as_secs()),
            ConfigValue::Path(p) => p.display().to_string(),
            ConfigValue::Secret(_) => "***REDACTED***".to_string(),
            ConfigValue::Null => "null".to_string(),
        }
    }
}

impl From<String> for ConfigValue {
    fn from(s: String) -> Self {
        ConfigValue::String(s)
    }
}

impl From<&str> for ConfigValue {
    fn from(s: &str) -> Self {
        ConfigValue::String(s.to_string())
    }
}

impl From<i64> for ConfigValue {
    fn from(i: i64) -> Self {
        ConfigValue::Integer(i)
    }
}

impl From<i32> for ConfigValue {
    fn from(i: i32) -> Self {
        ConfigValue::Integer(i as i64)
    }
}

impl From<f64> for ConfigValue {
    fn from(f: f64) -> Self {
        ConfigValue::Float(f)
    }
}

impl From<bool> for ConfigValue {
    fn from(b: bool) -> Self {
        ConfigValue::Boolean(b)
    }
}

impl From<PathBuf> for ConfigValue {
    fn from(p: PathBuf) -> Self {
        ConfigValue::Path(p)
    }
}

/// Configuration field metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    /// Field name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Expected type
    pub value_type: ValueType,
    /// Default value
    pub default: Option<ConfigValue>,
    /// Is required
    pub required: bool,
    /// Environment variable name
    pub env_var: Option<String>,
    /// CLI flag (e.g., "--timeout")
    pub cli_flag: Option<String>,
    /// CLI short flag (e.g., "-t")
    pub cli_short: Option<char>,
    /// Validation constraints
    pub constraints: Vec<Constraint>,
    /// Is secret (should be redacted in logs)
    pub secret: bool,
    /// Deprecated message (if deprecated)
    pub deprecated: Option<String>,
}

impl FieldSchema {
    /// Create a new field schema
    pub fn new(name: &str, description: &str, value_type: ValueType) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            value_type,
            default: None,
            required: false,
            env_var: None,
            cli_flag: None,
            cli_short: None,
            constraints: Vec::new(),
            secret: false,
            deprecated: None,
        }
    }

    /// Set default value
    pub fn with_default(mut self, value: ConfigValue) -> Self {
        self.default = Some(value);
        self
    }

    /// Mark as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set environment variable
    pub fn env(mut self, var: &str) -> Self {
        self.env_var = Some(var.to_string());
        self
    }

    /// Set CLI flag
    pub fn cli(mut self, flag: &str) -> Self {
        self.cli_flag = Some(flag.to_string());
        self
    }

    /// Set CLI short flag
    pub fn short(mut self, c: char) -> Self {
        self.cli_short = Some(c);
        self
    }

    /// Add constraint
    pub fn constrain(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Mark as secret
    pub fn secret(mut self) -> Self {
        self.secret = true;
        self
    }

    /// Mark as deprecated
    pub fn deprecated(mut self, message: &str) -> Self {
        self.deprecated = Some(message.to_string());
        self
    }

    /// Validate a value against this schema
    pub fn validate(&self, value: &ConfigValue) -> Result<()> {
        // Check type
        if !self.value_type.matches(value) {
            return Err(anyhow!(
                "Field '{}': expected {}, got {}",
                self.name,
                self.value_type,
                value.type_name()
            ));
        }

        // Check constraints
        for constraint in &self.constraints {
            constraint.validate(&self.name, value)?;
        }

        Ok(())
    }

    /// Generate CLI help text
    pub fn cli_help(&self) -> String {
        let mut parts = Vec::new();

        if let Some(short) = self.cli_short {
            parts.push(format!("-{}", short));
        }
        if let Some(flag) = &self.cli_flag {
            parts.push(flag.clone());
        }

        let flags = if parts.is_empty() {
            format!("--{}", self.name.replace('_', "-"))
        } else {
            parts.join(", ")
        };

        let mut help = format!("{}\n    {}", flags, self.description);

        if let Some(default) = &self.default {
            help.push_str(&format!("\n    Default: {}", default.display()));
        }

        if let Some(env) = &self.env_var {
            help.push_str(&format!("\n    Env: {}", env));
        }

        if let Some(deprecated) = &self.deprecated {
            help.push_str(&format!("\n    DEPRECATED: {}", deprecated));
        }

        help
    }
}

/// Value types for schema
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueType {
    String,
    Integer,
    Float,
    Boolean,
    Array(Box<ValueType>),
    Map,
    Duration,
    Path,
    Secret,
    Any,
}

impl ValueType {
    /// Check if a value matches this type
    pub fn matches(&self, value: &ConfigValue) -> bool {
        match (self, value) {
            (ValueType::Any, _) => true,
            (ValueType::String, ConfigValue::String(_)) => true,
            (ValueType::Integer, ConfigValue::Integer(_)) => true,
            (ValueType::Float, ConfigValue::Float(_)) => true,
            (ValueType::Float, ConfigValue::Integer(_)) => true, // Int is valid as float
            (ValueType::Boolean, ConfigValue::Boolean(_)) => true,
            (ValueType::Array(inner), ConfigValue::Array(arr)) => {
                arr.iter().all(|v| inner.matches(v))
            }
            (ValueType::Map, ConfigValue::Map(_)) => true,
            (ValueType::Duration, ConfigValue::Duration(_)) => true,
            (ValueType::Duration, ConfigValue::Integer(i)) if *i >= 0 => true,
            (ValueType::Path, ConfigValue::Path(_)) => true,
            (ValueType::Path, ConfigValue::String(_)) => true, // String is valid as path
            (ValueType::Secret, ConfigValue::Secret(_)) => true,
            (ValueType::Secret, ConfigValue::String(_)) => true, // String is valid as secret
            (_, ConfigValue::Null) => true, // Null is valid for optional fields
            _ => false,
        }
    }
}

impl std::fmt::Display for ValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueType::String => write!(f, "string"),
            ValueType::Integer => write!(f, "integer"),
            ValueType::Float => write!(f, "float"),
            ValueType::Boolean => write!(f, "boolean"),
            ValueType::Array(inner) => write!(f, "array<{}>", inner),
            ValueType::Map => write!(f, "map"),
            ValueType::Duration => write!(f, "duration"),
            ValueType::Path => write!(f, "path"),
            ValueType::Secret => write!(f, "secret"),
            ValueType::Any => write!(f, "any"),
        }
    }
}

/// Validation constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Constraint {
    /// Minimum value (for numbers)
    Min(f64),
    /// Maximum value (for numbers)
    Max(f64),
    /// Minimum length (for strings/arrays)
    MinLength(usize),
    /// Maximum length (for strings/arrays)
    MaxLength(usize),
    /// Regex pattern (for strings)
    Pattern(String),
    /// Allowed values
    OneOf(Vec<ConfigValue>),
    /// Path must exist
    PathExists,
    /// Path must be a file
    IsFile,
    /// Path must be a directory
    IsDirectory,
    /// Custom validation message
    Custom(String),
}

impl Constraint {
    /// Validate a value against this constraint
    pub fn validate(&self, field: &str, value: &ConfigValue) -> Result<()> {
        match self {
            Constraint::Min(min) => {
                if let Some(v) = value.as_float() {
                    if v < *min {
                        return Err(anyhow!(
                            "Field '{}': value {} is less than minimum {}",
                            field,
                            v,
                            min
                        ));
                    }
                }
            }
            Constraint::Max(max) => {
                if let Some(v) = value.as_float() {
                    if v > *max {
                        return Err(anyhow!(
                            "Field '{}': value {} is greater than maximum {}",
                            field,
                            v,
                            max
                        ));
                    }
                }
            }
            Constraint::MinLength(min) => {
                let len = match value {
                    ConfigValue::String(s) => s.len(),
                    ConfigValue::Array(a) => a.len(),
                    _ => return Ok(()),
                };
                if len < *min {
                    return Err(anyhow!(
                        "Field '{}': length {} is less than minimum {}",
                        field,
                        len,
                        min
                    ));
                }
            }
            Constraint::MaxLength(max) => {
                let len = match value {
                    ConfigValue::String(s) => s.len(),
                    ConfigValue::Array(a) => a.len(),
                    _ => return Ok(()),
                };
                if len > *max {
                    return Err(anyhow!(
                        "Field '{}': length {} is greater than maximum {}",
                        field,
                        len,
                        max
                    ));
                }
            }
            Constraint::Pattern(pattern) => {
                if let Some(s) = value.as_str() {
                    let re = regex::Regex::new(pattern)?;
                    if !re.is_match(s) {
                        return Err(anyhow!(
                            "Field '{}': value '{}' does not match pattern '{}'",
                            field,
                            s,
                            pattern
                        ));
                    }
                }
            }
            Constraint::OneOf(allowed) => {
                if !allowed.contains(value) {
                    return Err(anyhow!(
                        "Field '{}': value {} is not one of allowed values",
                        field,
                        value.display()
                    ));
                }
            }
            Constraint::PathExists => {
                if let Some(p) = value.as_path() {
                    if !p.exists() {
                        return Err(anyhow!(
                            "Field '{}': path '{}' does not exist",
                            field,
                            p.display()
                        ));
                    }
                }
            }
            Constraint::IsFile => {
                if let Some(p) = value.as_path() {
                    if !p.is_file() {
                        return Err(anyhow!(
                            "Field '{}': '{}' is not a file",
                            field,
                            p.display()
                        ));
                    }
                }
            }
            Constraint::IsDirectory => {
                if let Some(p) = value.as_path() {
                    if !p.is_dir() {
                        return Err(anyhow!(
                            "Field '{}': '{}' is not a directory",
                            field,
                            p.display()
                        ));
                    }
                }
            }
            Constraint::Custom(msg) => {
                // Custom validation is a marker - actual validation is done externally
                let _ = msg;
            }
        }
        Ok(())
    }

    /// Description of the constraint
    pub fn description(&self) -> String {
        match self {
            Constraint::Min(v) => format!("minimum: {}", v),
            Constraint::Max(v) => format!("maximum: {}", v),
            Constraint::MinLength(v) => format!("min length: {}", v),
            Constraint::MaxLength(v) => format!("max length: {}", v),
            Constraint::Pattern(p) => format!("pattern: {}", p),
            Constraint::OneOf(v) => format!(
                "one of: {:?}",
                v.iter().map(|x| x.display()).collect::<Vec<_>>()
            ),
            Constraint::PathExists => "path must exist".to_string(),
            Constraint::IsFile => "must be a file".to_string(),
            Constraint::IsDirectory => "must be a directory".to_string(),
            Constraint::Custom(msg) => msg.clone(),
        }
    }
}

/// Configuration schema
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigSchema {
    /// Schema name
    pub name: String,
    /// Schema version
    pub version: String,
    /// Field schemas
    pub fields: Vec<FieldSchema>,
    /// Groups of related fields
    pub groups: HashMap<String, Vec<String>>,
}

impl ConfigSchema {
    /// Create a new schema
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            fields: Vec::new(),
            groups: HashMap::new(),
        }
    }

    /// Add a field
    pub fn field(mut self, field: FieldSchema) -> Self {
        self.fields.push(field);
        self
    }

    /// Add a group
    pub fn group(mut self, name: &str, fields: Vec<&str>) -> Self {
        self.groups.insert(
            name.to_string(),
            fields.into_iter().map(String::from).collect(),
        );
        self
    }

    /// Get field by name
    pub fn get_field(&self, name: &str) -> Option<&FieldSchema> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get fields in a group
    pub fn get_group(&self, name: &str) -> Vec<&FieldSchema> {
        self.groups
            .get(name)
            .map(|names| names.iter().filter_map(|n| self.get_field(n)).collect())
            .unwrap_or_default()
    }

    /// Validate a config map against this schema
    pub fn validate(&self, config: &HashMap<String, ConfigValue>) -> Result<Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Check required fields
        for field in &self.fields {
            if field.required {
                match config.get(&field.name) {
                    None | Some(ConfigValue::Null) => {
                        errors.push(ValidationError {
                            field: field.name.clone(),
                            message: "required field is missing".to_string(),
                            severity: ErrorSeverity::Error,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Validate each field
        for (name, value) in config {
            if let Some(field) = self.get_field(name) {
                if let Err(e) = field.validate(value) {
                    errors.push(ValidationError {
                        field: name.clone(),
                        message: e.to_string(),
                        severity: ErrorSeverity::Error,
                    });
                }

                // Check for deprecated
                if let Some(ref reason) = field.deprecated {
                    errors.push(ValidationError {
                        field: name.clone(),
                        message: format!("field is deprecated: {}", reason),
                        severity: ErrorSeverity::Warning,
                    });
                }
            } else {
                // Unknown field
                errors.push(ValidationError {
                    field: name.clone(),
                    message: "unknown configuration field".to_string(),
                    severity: ErrorSeverity::Warning,
                });
            }
        }

        Ok(errors)
    }

    /// Generate CLI help
    pub fn cli_help(&self) -> String {
        let mut help = format!("{} v{}\n\nOptions:\n", self.name, self.version);

        for field in &self.fields {
            if field.cli_flag.is_some() || field.cli_short.is_some() {
                help.push_str(&format!("\n{}\n", field.cli_help()));
            }
        }

        help
    }

    /// Generate TOML template
    pub fn toml_template(&self) -> String {
        let mut template = format!(
            "# {} Configuration\n# Version: {}\n\n",
            self.name, self.version
        );

        // Group by groups first
        let mut used_fields: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for (group_name, field_names) in &self.groups {
            template.push_str(&format!("[{}]\n", group_name));
            for field_name in field_names {
                if let Some(field) = self.get_field(field_name) {
                    template.push_str(&format!("# {}\n", field.description));
                    if let Some(default) = &field.default {
                        template.push_str(&format!("# {} = {}\n\n", field.name, default.display()));
                    } else {
                        template.push_str(&format!("# {} = \n\n", field.name));
                    }
                    used_fields.insert(field_name.as_str());
                }
            }
            template.push('\n');
        }

        // Ungrouped fields
        for field in &self.fields {
            if !used_fields.contains(field.name.as_str()) {
                template.push_str(&format!("# {}\n", field.description));
                if let Some(default) = &field.default {
                    template.push_str(&format!("# {} = {}\n\n", field.name, default.display()));
                } else {
                    template.push_str(&format!("# {} = \n\n", field.name));
                }
            }
        }

        template
    }
}

/// Validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Field name
    pub field: String,
    /// Error message
    pub message: String,
    /// Severity
    pub severity: ErrorSeverity,
}

/// Error severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Warning,
    Error,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.severity {
            ErrorSeverity::Warning => "warning",
            ErrorSeverity::Error => "error",
        };
        write!(f, "{}: {}: {}", prefix, self.field, self.message)
    }
}

/// Configuration store
#[derive(Debug, Default)]
pub struct ConfigStore {
    /// Schema
    schema: Option<ConfigSchema>,
    /// Current values
    values: HashMap<String, ConfigValue>,
    /// Source of each value
    sources: HashMap<String, ValueSource>,
    /// Watchers for hot reload
    watchers: Vec<Box<dyn ConfigWatcher>>,
}

/// Source of a configuration value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSource {
    Default,
    File(PathBuf),
    Environment(String),
    CliArg,
    Runtime,
}

impl std::fmt::Display for ValueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueSource::Default => write!(f, "default"),
            ValueSource::File(p) => write!(f, "file:{}", p.display()),
            ValueSource::Environment(e) => write!(f, "env:{}", e),
            ValueSource::CliArg => write!(f, "cli"),
            ValueSource::Runtime => write!(f, "runtime"),
        }
    }
}

/// Trait for config change watchers
pub trait ConfigWatcher: std::fmt::Debug + Send + Sync {
    /// Called when a config value changes
    fn on_change(&self, field: &str, old: Option<&ConfigValue>, new: &ConfigValue);
}

impl ConfigStore {
    /// Create a new config store
    pub fn new() -> Self {
        Self::default()
    }

    /// Set schema
    pub fn with_schema(mut self, schema: ConfigSchema) -> Self {
        // Apply defaults from schema
        for field in &schema.fields {
            if let Some(default) = &field.default {
                if !self.values.contains_key(&field.name) {
                    self.values.insert(field.name.clone(), default.clone());
                    self.sources
                        .insert(field.name.clone(), ValueSource::Default);
                }
            }
        }
        self.schema = Some(schema);
        self
    }

    /// Get a value
    pub fn get(&self, key: &str) -> Option<&ConfigValue> {
        self.values.get(key)
    }

    /// Get value as specific type
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_int())
    }

    pub fn get_float(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.as_float())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }

    pub fn get_path(&self, key: &str) -> Option<&Path> {
        self.get(key).and_then(|v| v.as_path())
    }

    pub fn get_duration(&self, key: &str) -> Option<std::time::Duration> {
        self.get(key).and_then(|v| v.as_duration())
    }

    /// Set a value
    pub fn set(&mut self, key: &str, value: ConfigValue, source: ValueSource) -> Result<()> {
        // Validate if schema exists
        if let Some(schema) = &self.schema {
            if let Some(field) = schema.get_field(key) {
                field.validate(&value)?;
            }
        }

        // Notify watchers
        let old = self.values.get(key);
        for watcher in &self.watchers {
            watcher.on_change(key, old, &value);
        }

        self.values.insert(key.to_string(), value);
        self.sources.insert(key.to_string(), source);
        Ok(())
    }

    /// Get source of a value
    pub fn source(&self, key: &str) -> Option<&ValueSource> {
        self.sources.get(key)
    }

    /// Load from environment variables
    pub fn load_env(&mut self) -> Result<()> {
        // Collect values first to avoid borrow issues
        let to_set: Vec<(String, ConfigValue, String)> = if let Some(schema) = &self.schema {
            schema
                .fields
                .iter()
                .filter_map(|field| {
                    field.env_var.as_ref().and_then(|env_var| {
                        std::env::var(env_var).ok().and_then(|value| {
                            self.parse_env_value(&value, &field.value_type).ok().map(
                                |config_value| (field.name.clone(), config_value, env_var.clone()),
                            )
                        })
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        // Now apply them
        for (name, value, env_var) in to_set {
            self.set(&name, value, ValueSource::Environment(env_var))?;
        }
        Ok(())
    }

    /// Parse environment variable value to config value
    fn parse_env_value(&self, value: &str, value_type: &ValueType) -> Result<ConfigValue> {
        match value_type {
            ValueType::String => Ok(ConfigValue::String(value.to_string())),
            ValueType::Integer => {
                let i: i64 = value.parse()?;
                Ok(ConfigValue::Integer(i))
            }
            ValueType::Float => {
                let f: f64 = value.parse()?;
                Ok(ConfigValue::Float(f))
            }
            ValueType::Boolean => {
                let b = matches!(value.to_lowercase().as_str(), "true" | "1" | "yes" | "on");
                Ok(ConfigValue::Boolean(b))
            }
            ValueType::Path => Ok(ConfigValue::Path(PathBuf::from(value))),
            ValueType::Secret => Ok(ConfigValue::Secret(value.to_string())),
            ValueType::Duration => {
                // Parse as seconds
                let secs: u64 = value.parse()?;
                Ok(ConfigValue::Duration(std::time::Duration::from_secs(secs)))
            }
            _ => Ok(ConfigValue::String(value.to_string())),
        }
    }

    /// Add a watcher
    pub fn watch(&mut self, watcher: Box<dyn ConfigWatcher>) {
        self.watchers.push(watcher);
    }

    /// Validate all values
    pub fn validate(&self) -> Result<Vec<ValidationError>> {
        if let Some(schema) = &self.schema {
            schema.validate(&self.values)
        } else {
            Ok(Vec::new())
        }
    }

    /// Get all values as map
    pub fn all(&self) -> &HashMap<String, ConfigValue> {
        &self.values
    }

    /// Number of values
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Remove a value
    pub fn remove(&mut self, key: &str) -> Option<ConfigValue> {
        self.sources.remove(key);
        self.values.remove(key)
    }

    /// Clear all values
    pub fn clear(&mut self) {
        self.values.clear();
        self.sources.clear();
    }
}

/// Configuration wizard for interactive setup
#[derive(Debug, Default)]
pub struct ConfigWizard {
    /// Schema to use
    schema: Option<ConfigSchema>,
    /// Current step
    current_step: usize,
    /// Collected values
    values: HashMap<String, ConfigValue>,
    /// Steps (field names)
    steps: Vec<String>,
}

impl ConfigWizard {
    /// Create new wizard
    pub fn new() -> Self {
        Self::default()
    }

    /// Set schema
    pub fn with_schema(mut self, schema: ConfigSchema) -> Self {
        self.steps = schema
            .fields
            .iter()
            .filter(|f| f.required || f.default.is_none())
            .map(|f| f.name.clone())
            .collect();
        self.schema = Some(schema);
        self
    }

    /// Get current step
    pub fn current_step(&self) -> Option<&FieldSchema> {
        self.steps
            .get(self.current_step)
            .and_then(|name| self.schema.as_ref().and_then(|s| s.get_field(name)))
    }

    /// Get current step index
    pub fn step_index(&self) -> usize {
        self.current_step
    }

    /// Get total steps
    pub fn total_steps(&self) -> usize {
        self.steps.len()
    }

    /// Progress percentage
    pub fn progress(&self) -> f32 {
        if self.steps.is_empty() {
            return 100.0;
        }
        (self.current_step as f32 / self.steps.len() as f32) * 100.0
    }

    /// Set value for current step and advance
    pub fn set_current(&mut self, value: ConfigValue) -> Result<bool> {
        if let Some(field) = self.current_step() {
            // Validate
            field.validate(&value)?;

            // Store
            let name = field.name.clone();
            self.values.insert(name, value);

            // Advance
            self.current_step += 1;

            // Check if done
            Ok(self.current_step >= self.steps.len())
        } else {
            Ok(true)
        }
    }

    /// Go back one step
    pub fn back(&mut self) -> bool {
        if self.current_step > 0 {
            self.current_step -= 1;
            true
        } else {
            false
        }
    }

    /// Skip current step (if optional)
    pub fn skip(&mut self) -> Result<bool> {
        if let Some(field) = self.current_step() {
            if field.required {
                return Err(anyhow!("Cannot skip required field: {}", field.name));
            }

            // Use default if available
            if let Some(default) = &field.default {
                self.values.insert(field.name.clone(), default.clone());
            }

            self.current_step += 1;
            Ok(self.current_step >= self.steps.len())
        } else {
            Ok(true)
        }
    }

    /// Is complete
    pub fn is_complete(&self) -> bool {
        self.current_step >= self.steps.len()
    }

    /// Get collected values
    pub fn values(&self) -> &HashMap<String, ConfigValue> {
        &self.values
    }

    /// Apply values to a config store
    pub fn apply_to(&self, store: &mut ConfigStore) -> Result<()> {
        for (key, value) in &self.values {
            store.set(key, value.clone(), ValueSource::Runtime)?;
        }
        Ok(())
    }

    /// Reset wizard
    pub fn reset(&mut self) {
        self.current_step = 0;
        self.values.clear();
    }
}

/// Hot reload configuration handler
#[derive(Debug)]
pub struct HotReloadHandler {
    /// Config file path
    file_path: PathBuf,
    /// Last modified time
    last_modified: Option<std::time::SystemTime>,
    /// Check interval
    interval: std::time::Duration,
    /// Is enabled
    enabled: bool,
}

impl HotReloadHandler {
    /// Create new handler
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            last_modified: None,
            interval: std::time::Duration::from_secs(5),
            enabled: true,
        }
    }

    /// Set check interval
    pub fn with_interval(mut self, interval: std::time::Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Enable/disable
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if file has changed
    pub fn has_changed(&mut self) -> bool {
        if !self.enabled {
            return false;
        }

        let modified = std::fs::metadata(&self.file_path)
            .ok()
            .and_then(|m| m.modified().ok());

        if modified != self.last_modified {
            self.last_modified = modified;
            return modified.is_some();
        }

        false
    }

    /// Get check interval
    pub fn interval(&self) -> std::time::Duration {
        self.interval
    }

    /// Get file path
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_value_constructors() {
        assert!(matches!(
            ConfigValue::string("test"),
            ConfigValue::String(_)
        ));
        assert!(matches!(ConfigValue::int(42), ConfigValue::Integer(42)));
        assert!(matches!(ConfigValue::float(3.15), ConfigValue::Float(_)));
        assert!(matches!(
            ConfigValue::bool(true),
            ConfigValue::Boolean(true)
        ));
        assert!(matches!(ConfigValue::path("/tmp"), ConfigValue::Path(_)));
        assert!(matches!(ConfigValue::secret("key"), ConfigValue::Secret(_)));
        assert!(matches!(
            ConfigValue::duration(60),
            ConfigValue::Duration(_)
        ));
    }

    #[test]
    fn test_config_value_as_methods() {
        let s = ConfigValue::string("test");
        assert_eq!(s.as_str(), Some("test"));
        assert_eq!(s.as_int(), None);

        let i = ConfigValue::int(42);
        assert_eq!(i.as_int(), Some(42));
        assert_eq!(i.as_float(), Some(42.0));

        let b = ConfigValue::bool(true);
        assert_eq!(b.as_bool(), Some(true));

        let p = ConfigValue::path("/tmp");
        assert!(p.as_path().is_some());

        let d = ConfigValue::duration(60);
        assert!(d.as_duration().is_some());
    }

    #[test]
    fn test_config_value_type_name() {
        assert_eq!(ConfigValue::string("").type_name(), "string");
        assert_eq!(ConfigValue::int(0).type_name(), "integer");
        assert_eq!(ConfigValue::float(0.0).type_name(), "float");
        assert_eq!(ConfigValue::bool(true).type_name(), "boolean");
        assert_eq!(ConfigValue::Null.type_name(), "null");
    }

    #[test]
    fn test_config_value_is_null() {
        assert!(ConfigValue::Null.is_null());
        assert!(!ConfigValue::int(0).is_null());
    }

    #[test]
    fn test_config_value_display() {
        assert!(ConfigValue::string("test").display().contains("test"));
        assert_eq!(ConfigValue::int(42).display(), "42");
        assert!(ConfigValue::secret("key").display().contains("REDACTED"));
    }

    #[test]
    fn test_config_value_default() {
        let v: ConfigValue = Default::default();
        assert!(v.is_null());
    }

    #[test]
    fn test_config_value_from() {
        let s: ConfigValue = "test".into();
        assert!(matches!(s, ConfigValue::String(_)));

        let i: ConfigValue = 42i64.into();
        assert!(matches!(i, ConfigValue::Integer(42)));

        let b: ConfigValue = true.into();
        assert!(matches!(b, ConfigValue::Boolean(true)));
    }

    #[test]
    fn test_field_schema_creation() {
        let field = FieldSchema::new("timeout", "Request timeout", ValueType::Integer);
        assert_eq!(field.name, "timeout");
        assert!(!field.required);
    }

    #[test]
    fn test_field_schema_builder() {
        let field = FieldSchema::new("api_key", "API key", ValueType::Secret)
            .with_default(ConfigValue::string(""))
            .required()
            .env("API_KEY")
            .cli("--api-key")
            .short('k')
            .secret();

        assert!(field.required);
        assert!(field.secret);
        assert_eq!(field.env_var, Some("API_KEY".to_string()));
        assert_eq!(field.cli_short, Some('k'));
    }

    #[test]
    fn test_field_schema_validate() {
        let field = FieldSchema::new("count", "Count", ValueType::Integer)
            .constrain(Constraint::Min(0.0))
            .constrain(Constraint::Max(100.0));

        assert!(field.validate(&ConfigValue::int(50)).is_ok());
        assert!(field.validate(&ConfigValue::int(-1)).is_err());
        assert!(field.validate(&ConfigValue::int(101)).is_err());
    }

    #[test]
    fn test_field_schema_validate_type() {
        let field = FieldSchema::new("name", "Name", ValueType::String);
        assert!(field.validate(&ConfigValue::string("test")).is_ok());
        assert!(field.validate(&ConfigValue::int(42)).is_err());
    }

    #[test]
    fn test_field_schema_deprecated() {
        let field =
            FieldSchema::new("old", "Old field", ValueType::String).deprecated("Use 'new' instead");

        assert!(field.deprecated.is_some());
    }

    #[test]
    fn test_field_schema_cli_help() {
        let field = FieldSchema::new("timeout", "Request timeout in seconds", ValueType::Integer)
            .cli("--timeout")
            .short('t')
            .with_default(ConfigValue::int(30))
            .env("TIMEOUT");

        let help = field.cli_help();
        assert!(help.contains("-t"));
        assert!(help.contains("--timeout"));
        assert!(help.contains("30"));
    }

    #[test]
    fn test_value_type_matches() {
        assert!(ValueType::String.matches(&ConfigValue::string("test")));
        assert!(ValueType::Integer.matches(&ConfigValue::int(42)));
        assert!(ValueType::Float.matches(&ConfigValue::int(42))); // Int is valid as float
        assert!(ValueType::Path.matches(&ConfigValue::string("/tmp"))); // String is valid as path
        assert!(ValueType::Any.matches(&ConfigValue::string("anything")));
        assert!(ValueType::String.matches(&ConfigValue::Null)); // Null is valid for optional
    }

    #[test]
    fn test_value_type_display() {
        assert_eq!(format!("{}", ValueType::String), "string");
        assert_eq!(format!("{}", ValueType::Integer), "integer");
        assert_eq!(
            format!("{}", ValueType::Array(Box::new(ValueType::String))),
            "array<string>"
        );
    }

    #[test]
    fn test_constraint_min_max() {
        let min = Constraint::Min(0.0);
        assert!(min.validate("field", &ConfigValue::int(10)).is_ok());
        assert!(min.validate("field", &ConfigValue::int(-1)).is_err());

        let max = Constraint::Max(100.0);
        assert!(max.validate("field", &ConfigValue::int(50)).is_ok());
        assert!(max.validate("field", &ConfigValue::int(150)).is_err());
    }

    #[test]
    fn test_constraint_length() {
        let min_len = Constraint::MinLength(3);
        assert!(min_len
            .validate("field", &ConfigValue::string("hello"))
            .is_ok());
        assert!(min_len
            .validate("field", &ConfigValue::string("hi"))
            .is_err());

        let max_len = Constraint::MaxLength(5);
        assert!(max_len
            .validate("field", &ConfigValue::string("hello"))
            .is_ok());
        assert!(max_len
            .validate("field", &ConfigValue::string("hello world"))
            .is_err());
    }

    #[test]
    fn test_constraint_pattern() {
        let pattern = Constraint::Pattern(r"^\d+$".to_string());
        assert!(pattern
            .validate("field", &ConfigValue::string("123"))
            .is_ok());
        assert!(pattern
            .validate("field", &ConfigValue::string("abc"))
            .is_err());
    }

    #[test]
    fn test_constraint_one_of() {
        let one_of = Constraint::OneOf(vec![ConfigValue::string("a"), ConfigValue::string("b")]);
        assert!(one_of.validate("field", &ConfigValue::string("a")).is_ok());
        assert!(one_of.validate("field", &ConfigValue::string("c")).is_err());
    }

    #[test]
    fn test_constraint_description() {
        assert!(Constraint::Min(0.0).description().contains("minimum"));
        assert!(Constraint::Max(100.0).description().contains("maximum"));
        assert!(Constraint::PathExists.description().contains("exist"));
    }

    #[test]
    fn test_config_schema_creation() {
        let schema = ConfigSchema::new("test-app", "1.0.0")
            .field(FieldSchema::new("timeout", "Timeout", ValueType::Integer))
            .field(FieldSchema::new("host", "Host", ValueType::String))
            .group("network", vec!["timeout", "host"]);

        assert_eq!(schema.name, "test-app");
        assert_eq!(schema.fields.len(), 2);
        assert!(schema.get_field("timeout").is_some());
    }

    #[test]
    fn test_config_schema_validate() {
        let schema = ConfigSchema::new("test", "1.0")
            .field(FieldSchema::new("name", "Name", ValueType::String).required());

        let mut config = HashMap::new();
        let errors = schema.validate(&config).unwrap();
        assert!(!errors.is_empty()); // Missing required field

        config.insert("name".to_string(), ConfigValue::string("test"));
        let errors = schema.validate(&config).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_config_schema_unknown_field() {
        let schema = ConfigSchema::new("test", "1.0");
        let mut config = HashMap::new();
        config.insert("unknown".to_string(), ConfigValue::string("value"));

        let errors = schema.validate(&config).unwrap();
        assert!(!errors.is_empty());
        assert_eq!(errors[0].severity, ErrorSeverity::Warning);
    }

    #[test]
    fn test_config_schema_cli_help() {
        let schema = ConfigSchema::new("test", "1.0").field(
            FieldSchema::new("verbose", "Enable verbose", ValueType::Boolean)
                .cli("--verbose")
                .short('v'),
        );

        let help = schema.cli_help();
        assert!(help.contains("--verbose"));
        assert!(help.contains("-v"));
    }

    #[test]
    fn test_config_schema_toml_template() {
        let schema = ConfigSchema::new("test", "1.0").field(
            FieldSchema::new("timeout", "Timeout in seconds", ValueType::Integer)
                .with_default(ConfigValue::int(30)),
        );

        let template = schema.toml_template();
        assert!(template.contains("timeout"));
        assert!(template.contains("30"));
    }

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError {
            field: "timeout".to_string(),
            message: "must be positive".to_string(),
            severity: ErrorSeverity::Error,
        };
        let display = format!("{}", err);
        assert!(display.contains("error"));
        assert!(display.contains("timeout"));
    }

    #[test]
    fn test_config_store_new() {
        let store = ConfigStore::new();
        assert!(store.is_empty());
    }

    #[test]
    fn test_config_store_set_get() {
        let mut store = ConfigStore::new();
        store
            .set("key", ConfigValue::string("value"), ValueSource::Runtime)
            .unwrap();

        assert_eq!(store.get_string("key"), Some("value"));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_config_store_typed_getters() {
        let mut store = ConfigStore::new();
        store
            .set("str", ConfigValue::string("hello"), ValueSource::Runtime)
            .unwrap();
        store
            .set("int", ConfigValue::int(42), ValueSource::Runtime)
            .unwrap();
        store
            .set("float", ConfigValue::float(3.15), ValueSource::Runtime)
            .unwrap();
        store
            .set("bool", ConfigValue::bool(true), ValueSource::Runtime)
            .unwrap();
        store
            .set("path", ConfigValue::path("/tmp"), ValueSource::Runtime)
            .unwrap();
        store
            .set("dur", ConfigValue::duration(60), ValueSource::Runtime)
            .unwrap();

        assert_eq!(store.get_string("str"), Some("hello"));
        assert_eq!(store.get_int("int"), Some(42));
        assert!(store.get_float("float").is_some());
        assert_eq!(store.get_bool("bool"), Some(true));
        assert!(store.get_path("path").is_some());
        assert!(store.get_duration("dur").is_some());
    }

    #[test]
    fn test_config_store_with_schema() {
        let schema = ConfigSchema::new("test", "1.0").field(
            FieldSchema::new("timeout", "Timeout", ValueType::Integer)
                .with_default(ConfigValue::int(30)),
        );

        let store = ConfigStore::new().with_schema(schema);
        assert_eq!(store.get_int("timeout"), Some(30));
        assert_eq!(store.source("timeout"), Some(&ValueSource::Default));
    }

    #[test]
    fn test_config_store_validation() {
        let schema = ConfigSchema::new("test", "1.0").field(
            FieldSchema::new("count", "Count", ValueType::Integer).constrain(Constraint::Min(0.0)),
        );

        let mut store = ConfigStore::new().with_schema(schema);
        assert!(store
            .set("count", ConfigValue::int(10), ValueSource::Runtime)
            .is_ok());
        assert!(store
            .set("count", ConfigValue::int(-1), ValueSource::Runtime)
            .is_err());
    }

    #[test]
    fn test_config_store_source() {
        let mut store = ConfigStore::new();
        store
            .set(
                "key",
                ConfigValue::string("val"),
                ValueSource::Environment("KEY".to_string()),
            )
            .unwrap();

        let source = store.source("key");
        assert!(matches!(source, Some(ValueSource::Environment(_))));
    }

    #[test]
    fn test_config_store_remove_clear() {
        let mut store = ConfigStore::new();
        store
            .set("a", ConfigValue::int(1), ValueSource::Runtime)
            .unwrap();
        store
            .set("b", ConfigValue::int(2), ValueSource::Runtime)
            .unwrap();

        store.remove("a");
        assert!(store.get("a").is_none());
        assert!(store.get("b").is_some());

        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn test_value_source_display() {
        assert_eq!(format!("{}", ValueSource::Default), "default");
        assert!(format!("{}", ValueSource::File(PathBuf::from("/etc/config"))).contains("file:"));
        assert!(format!("{}", ValueSource::Environment("KEY".to_string())).contains("env:"));
        assert_eq!(format!("{}", ValueSource::CliArg), "cli");
    }

    #[test]
    fn test_config_wizard_new() {
        let wizard = ConfigWizard::new();
        assert!(wizard.is_complete()); // No steps
    }

    #[test]
    fn test_config_wizard_with_schema() {
        let schema = ConfigSchema::new("test", "1.0")
            .field(FieldSchema::new("name", "Name", ValueType::String).required())
            .field(
                FieldSchema::new("optional", "Optional", ValueType::String)
                    .with_default(ConfigValue::string("default")),
            );

        let wizard = ConfigWizard::new().with_schema(schema);
        assert_eq!(wizard.total_steps(), 1); // Only required/no-default
    }

    #[test]
    fn test_config_wizard_progress() {
        let schema = ConfigSchema::new("test", "1.0")
            .field(FieldSchema::new("a", "A", ValueType::String).required())
            .field(FieldSchema::new("b", "B", ValueType::String).required());

        let mut wizard = ConfigWizard::new().with_schema(schema);
        assert_eq!(wizard.progress(), 0.0);

        wizard.set_current(ConfigValue::string("value")).unwrap();
        assert_eq!(wizard.progress(), 50.0);

        wizard.set_current(ConfigValue::string("value")).unwrap();
        assert!(wizard.is_complete());
    }

    #[test]
    fn test_config_wizard_back() {
        let schema = ConfigSchema::new("test", "1.0")
            .field(FieldSchema::new("a", "A", ValueType::String).required())
            .field(FieldSchema::new("b", "B", ValueType::String).required());

        let mut wizard = ConfigWizard::new().with_schema(schema);
        wizard.set_current(ConfigValue::string("value")).unwrap();
        assert_eq!(wizard.step_index(), 1);

        assert!(wizard.back());
        assert_eq!(wizard.step_index(), 0);

        assert!(!wizard.back()); // Can't go before start
    }

    #[test]
    fn test_config_wizard_skip() {
        let schema = ConfigSchema::new("test", "1.0").field(FieldSchema::new(
            "optional",
            "Optional",
            ValueType::String,
        ));

        let mut wizard = ConfigWizard::new().with_schema(schema);
        assert!(wizard.skip().is_ok());
        assert!(wizard.is_complete());
    }

    #[test]
    fn test_config_wizard_skip_required_fails() {
        let schema = ConfigSchema::new("test", "1.0")
            .field(FieldSchema::new("required", "Required", ValueType::String).required());

        let mut wizard = ConfigWizard::new().with_schema(schema);
        assert!(wizard.skip().is_err());
    }

    #[test]
    fn test_config_wizard_reset() {
        let schema = ConfigSchema::new("test", "1.0")
            .field(FieldSchema::new("a", "A", ValueType::String).required());

        let mut wizard = ConfigWizard::new().with_schema(schema);
        wizard.set_current(ConfigValue::string("value")).unwrap();
        wizard.reset();

        assert_eq!(wizard.step_index(), 0);
        assert!(wizard.values().is_empty());
    }

    #[test]
    fn test_config_wizard_apply_to() {
        let schema = ConfigSchema::new("test", "1.0")
            .field(FieldSchema::new("name", "Name", ValueType::String).required());

        let mut wizard = ConfigWizard::new().with_schema(schema.clone());
        wizard.set_current(ConfigValue::string("test")).unwrap();

        let mut store = ConfigStore::new().with_schema(schema);
        wizard.apply_to(&mut store).unwrap();

        assert_eq!(store.get_string("name"), Some("test"));
    }

    #[test]
    fn test_hot_reload_handler_new() {
        let handler = HotReloadHandler::new(PathBuf::from("/etc/config.toml"));
        assert!(handler.is_enabled());
        assert_eq!(handler.file_path(), Path::new("/etc/config.toml"));
    }

    #[test]
    fn test_hot_reload_handler_interval() {
        let handler = HotReloadHandler::new(PathBuf::from("/etc/config"))
            .with_interval(std::time::Duration::from_secs(10));
        assert_eq!(handler.interval(), std::time::Duration::from_secs(10));
    }

    #[test]
    fn test_hot_reload_handler_enable_disable() {
        let mut handler = HotReloadHandler::new(PathBuf::from("/etc/config"));
        assert!(handler.is_enabled());

        handler.set_enabled(false);
        assert!(!handler.is_enabled());
        assert!(!handler.has_changed()); // Disabled, so no change
    }

    #[test]
    fn test_hot_reload_handler_has_changed() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp = NamedTempFile::new().unwrap();
        writeln!(temp, "test").unwrap();

        let mut handler = HotReloadHandler::new(temp.path().to_path_buf());

        // First check should detect change (no previous time)
        assert!(handler.has_changed());

        // Second check without modification should not detect change
        assert!(!handler.has_changed());
    }

    #[test]
    fn test_config_store_validate_all() {
        let schema = ConfigSchema::new("test", "1.0")
            .field(FieldSchema::new("required", "Required", ValueType::String).required());

        let store = ConfigStore::new().with_schema(schema);
        let errors = store.validate().unwrap();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_config_store_all() {
        let mut store = ConfigStore::new();
        store
            .set("a", ConfigValue::int(1), ValueSource::Runtime)
            .unwrap();
        store
            .set("b", ConfigValue::int(2), ValueSource::Runtime)
            .unwrap();

        let all = store.all();
        assert_eq!(all.len(), 2);
    }
}
