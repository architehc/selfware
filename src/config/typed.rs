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

    // ---- Additional coverage tests ----

    #[test]
    fn test_display_float() {
        assert_eq!(ConfigValue::float(3.14).display(), "3.14");
    }

    #[test]
    fn test_display_boolean_true_false() {
        assert_eq!(ConfigValue::bool(true).display(), "true");
        assert_eq!(ConfigValue::bool(false).display(), "false");
    }

    #[test]
    fn test_display_array_items() {
        let v = ConfigValue::Array(vec![ConfigValue::int(1), ConfigValue::int(2)]);
        assert_eq!(v.display(), "[2 items]");
        assert_eq!(ConfigValue::Array(vec![]).display(), "[0 items]");
    }

    #[test]
    fn test_display_map_keys() {
        let mut m = HashMap::new();
        m.insert("a".to_string(), ConfigValue::int(1));
        m.insert("b".to_string(), ConfigValue::int(2));
        assert_eq!(ConfigValue::Map(m).display(), "{2 keys}");
    }

    #[test]
    fn test_display_duration_secs() {
        assert_eq!(ConfigValue::duration(120).display(), "120s");
    }

    #[test]
    fn test_display_path_value() {
        assert_eq!(ConfigValue::path("/home/user").display(), "/home/user");
    }

    #[test]
    fn test_display_null_value() {
        assert_eq!(ConfigValue::Null.display(), "null");
    }

    #[test]
    fn test_type_name_remaining_variants() {
        assert_eq!(ConfigValue::Array(vec![]).type_name(), "array");
        assert_eq!(ConfigValue::Map(HashMap::new()).type_name(), "map");
        assert_eq!(ConfigValue::duration(1).type_name(), "duration");
        assert_eq!(ConfigValue::path("/x").type_name(), "path");
        assert_eq!(ConfigValue::secret("s").type_name(), "secret");
    }

    #[test]
    fn test_as_str_returns_secret_content() {
        assert_eq!(ConfigValue::secret("tok").as_str(), Some("tok"));
    }

    #[test]
    fn test_as_str_returns_none_for_non_strings() {
        assert_eq!(ConfigValue::int(1).as_str(), None);
        assert_eq!(ConfigValue::bool(true).as_str(), None);
        assert_eq!(ConfigValue::Null.as_str(), None);
    }

    #[test]
    fn test_as_int_returns_none_for_non_int() {
        assert_eq!(ConfigValue::float(1.0).as_int(), None);
        assert_eq!(ConfigValue::string("1").as_int(), None);
    }

    #[test]
    fn test_as_float_from_float_variant() {
        assert_eq!(ConfigValue::float(2.718).as_float(), Some(2.718));
    }

    #[test]
    fn test_as_float_returns_none_for_non_numeric() {
        assert_eq!(ConfigValue::string("x").as_float(), None);
        assert_eq!(ConfigValue::bool(true).as_float(), None);
    }

    #[test]
    fn test_as_bool_returns_none_for_non_bool() {
        assert_eq!(ConfigValue::int(1).as_bool(), None);
        assert_eq!(ConfigValue::string("true").as_bool(), None);
    }

    #[test]
    fn test_as_path_from_string_variant() {
        let v = ConfigValue::string("/usr/local/bin");
        assert_eq!(v.as_path(), Some(Path::new("/usr/local/bin")));
    }

    #[test]
    fn test_as_path_returns_none_for_non_path() {
        assert_eq!(ConfigValue::int(42).as_path(), None);
        assert_eq!(ConfigValue::bool(true).as_path(), None);
    }

    #[test]
    fn test_as_duration_from_positive_int() {
        assert_eq!(
            ConfigValue::int(30).as_duration(),
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[test]
    fn test_as_duration_from_zero_int() {
        assert_eq!(
            ConfigValue::int(0).as_duration(),
            Some(std::time::Duration::from_secs(0))
        );
    }

    #[test]
    fn test_as_duration_from_negative_int_returns_none() {
        assert_eq!(ConfigValue::int(-5).as_duration(), None);
    }

    #[test]
    fn test_as_duration_returns_none_for_non_duration() {
        assert_eq!(ConfigValue::string("60").as_duration(), None);
        assert_eq!(ConfigValue::bool(true).as_duration(), None);
    }

    #[test]
    fn test_from_owned_string() {
        let v: ConfigValue = String::from("owned").into();
        assert_eq!(v.as_str(), Some("owned"));
    }

    #[test]
    fn test_from_i32() {
        let v: ConfigValue = 42i32.into();
        assert_eq!(v.as_int(), Some(42));
    }

    #[test]
    fn test_from_f64() {
        let v: ConfigValue = 3.14f64.into();
        assert_eq!(v.as_float(), Some(3.14));
    }

    #[test]
    fn test_from_pathbuf() {
        let v: ConfigValue = PathBuf::from("/tmp/test").into();
        assert_eq!(v.as_path(), Some(Path::new("/tmp/test")));
    }

    #[test]
    fn test_serde_roundtrip_all_variants() {
        let values = vec![
            ConfigValue::string("hello"),
            ConfigValue::int(42),
            ConfigValue::float(3.14),
            ConfigValue::bool(true),
            ConfigValue::Null,
            ConfigValue::path("/tmp"),
            ConfigValue::secret("s3cr3t"),
            ConfigValue::duration(60),
            ConfigValue::Array(vec![ConfigValue::int(1), ConfigValue::int(2)]),
        ];
        for original in &values {
            let json = serde_json::to_string(original).unwrap();
            let back: ConfigValue = serde_json::from_str(&json).unwrap();
            assert_eq!(original, &back);
        }
    }

    #[test]
    fn test_serde_roundtrip_map_variant() {
        let mut m = HashMap::new();
        m.insert("key".to_string(), ConfigValue::string("val"));
        let original = ConfigValue::Map(m);
        let json = serde_json::to_string(&original).unwrap();
        let back: ConfigValue = serde_json::from_str(&json).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn test_vt_matches_float_with_float() {
        assert!(ValueType::Float.matches(&ConfigValue::float(1.5)));
    }

    #[test]
    fn test_vt_matches_boolean_mismatch() {
        assert!(ValueType::Boolean.matches(&ConfigValue::bool(false)));
        assert!(!ValueType::Boolean.matches(&ConfigValue::int(0)));
    }

    #[test]
    fn test_vt_matches_array_inner() {
        let at = ValueType::Array(Box::new(ValueType::Integer));
        assert!(at.matches(&ConfigValue::Array(vec![ConfigValue::int(1)])));
        assert!(!at.matches(&ConfigValue::Array(vec![ConfigValue::string("x")])));
        assert!(at.matches(&ConfigValue::Array(vec![]))); // empty ok
    }

    #[test]
    fn test_vt_matches_map_variant() {
        assert!(ValueType::Map.matches(&ConfigValue::Map(HashMap::new())));
        assert!(!ValueType::Map.matches(&ConfigValue::int(1)));
    }

    #[test]
    fn test_vt_matches_duration_variants() {
        assert!(ValueType::Duration.matches(&ConfigValue::duration(5)));
        assert!(ValueType::Duration.matches(&ConfigValue::int(10)));
        assert!(!ValueType::Duration.matches(&ConfigValue::int(-1)));
        assert!(!ValueType::Duration.matches(&ConfigValue::string("5")));
    }

    #[test]
    fn test_vt_matches_path_with_string() {
        assert!(ValueType::Path.matches(&ConfigValue::path("/x")));
        assert!(ValueType::Path.matches(&ConfigValue::string("/x")));
    }

    #[test]
    fn test_vt_matches_secret_variants() {
        assert!(ValueType::Secret.matches(&ConfigValue::secret("k")));
        assert!(ValueType::Secret.matches(&ConfigValue::string("k")));
    }

    #[test]
    fn test_vt_matches_any_all() {
        assert!(ValueType::Any.matches(&ConfigValue::int(42)));
        assert!(ValueType::Any.matches(&ConfigValue::bool(true)));
        assert!(ValueType::Any.matches(&ConfigValue::Null));
        assert!(ValueType::Any.matches(&ConfigValue::Array(vec![])));
    }

    #[test]
    fn test_vt_null_valid_for_all_types() {
        for vt in &[
            ValueType::String,
            ValueType::Integer,
            ValueType::Float,
            ValueType::Boolean,
            ValueType::Duration,
            ValueType::Path,
            ValueType::Secret,
            ValueType::Map,
        ] {
            assert!(
                vt.matches(&ConfigValue::Null),
                "{:?} should accept Null",
                vt
            );
        }
    }

    #[test]
    fn test_vt_mismatches() {
        assert!(!ValueType::String.matches(&ConfigValue::int(42)));
        assert!(!ValueType::Integer.matches(&ConfigValue::string("x")));
        assert!(!ValueType::Integer.matches(&ConfigValue::bool(true)));
        assert!(!ValueType::Boolean.matches(&ConfigValue::string("true")));
        assert!(!ValueType::Secret.matches(&ConfigValue::int(1)));
        assert!(!ValueType::Path.matches(&ConfigValue::int(1)));
    }

    #[test]
    fn test_vt_display_remaining() {
        assert_eq!(format!("{}", ValueType::Float), "float");
        assert_eq!(format!("{}", ValueType::Boolean), "boolean");
        assert_eq!(format!("{}", ValueType::Map), "map");
        assert_eq!(format!("{}", ValueType::Duration), "duration");
        assert_eq!(format!("{}", ValueType::Path), "path");
        assert_eq!(format!("{}", ValueType::Secret), "secret");
        assert_eq!(format!("{}", ValueType::Any), "any");
    }

    #[test]
    fn test_vt_display_nested_array() {
        let nested = ValueType::Array(Box::new(ValueType::Array(Box::new(ValueType::Integer))));
        assert_eq!(format!("{}", nested), "array<array<integer>>");
    }

    #[test]
    fn test_constraint_min_float_boundary() {
        let c = Constraint::Min(1.5);
        assert!(c.validate("f", &ConfigValue::float(2.0)).is_ok());
        assert!(c.validate("f", &ConfigValue::float(1.5)).is_ok());
        assert!(c.validate("f", &ConfigValue::float(1.0)).is_err());
    }

    #[test]
    fn test_constraint_max_float_boundary() {
        let c = Constraint::Max(10.5);
        assert!(c.validate("f", &ConfigValue::float(10.5)).is_ok());
        assert!(c.validate("f", &ConfigValue::float(11.0)).is_err());
    }

    #[test]
    fn test_constraint_min_max_non_numeric_passes() {
        assert!(Constraint::Min(0.0)
            .validate("f", &ConfigValue::string("abc"))
            .is_ok());
        assert!(Constraint::Min(0.0)
            .validate("f", &ConfigValue::bool(true))
            .is_ok());
        assert!(Constraint::Max(100.0)
            .validate("f", &ConfigValue::string("abc"))
            .is_ok());
    }

    #[test]
    fn test_constraint_min_length_array() {
        let c = Constraint::MinLength(2);
        let ok = ConfigValue::Array(vec![
            ConfigValue::int(1),
            ConfigValue::int(2),
            ConfigValue::int(3),
        ]);
        assert!(c.validate("f", &ok).is_ok());
        let fail = ConfigValue::Array(vec![ConfigValue::int(1)]);
        assert!(c.validate("f", &fail).is_err());
    }

    #[test]
    fn test_constraint_max_length_array() {
        let c = Constraint::MaxLength(2);
        let ok = ConfigValue::Array(vec![ConfigValue::int(1), ConfigValue::int(2)]);
        assert!(c.validate("f", &ok).is_ok());
        let fail = ConfigValue::Array(vec![
            ConfigValue::int(1),
            ConfigValue::int(2),
            ConfigValue::int(3),
        ]);
        assert!(c.validate("f", &fail).is_err());
    }

    #[test]
    fn test_constraint_length_non_applicable_types() {
        assert!(Constraint::MinLength(1)
            .validate("f", &ConfigValue::int(42))
            .is_ok());
        assert!(Constraint::MaxLength(5)
            .validate("f", &ConfigValue::bool(true))
            .is_ok());
    }

    #[test]
    fn test_constraint_pattern_on_non_string_passes() {
        let c = Constraint::Pattern(r"^\d+$".to_string());
        assert!(c.validate("f", &ConfigValue::int(42)).is_ok());
    }

    #[test]
    fn test_constraint_pattern_on_secret() {
        let c = Constraint::Pattern(r"^sk-".to_string());
        assert!(c.validate("f", &ConfigValue::secret("sk-abc")).is_ok());
        assert!(c.validate("f", &ConfigValue::secret("wrong")).is_err());
    }

    #[test]
    fn test_constraint_pattern_invalid_regex() {
        let c = Constraint::Pattern(r"[invalid".to_string());
        assert!(c.validate("f", &ConfigValue::string("test")).is_err());
    }

    #[test]
    fn test_constraint_one_of_integers() {
        let c = Constraint::OneOf(vec![ConfigValue::int(1), ConfigValue::int(2)]);
        assert!(c.validate("f", &ConfigValue::int(2)).is_ok());
        assert!(c.validate("f", &ConfigValue::int(5)).is_err());
    }

    #[test]
    fn test_constraint_path_exists_ok() {
        assert!(Constraint::PathExists
            .validate("f", &ConfigValue::path("/tmp"))
            .is_ok());
    }

    #[test]
    fn test_constraint_path_exists_fail() {
        assert!(Constraint::PathExists
            .validate("f", &ConfigValue::path("/nonexistent/path/xyz"))
            .is_err());
    }

    #[test]
    fn test_constraint_path_exists_non_path_passes() {
        assert!(Constraint::PathExists
            .validate("f", &ConfigValue::int(42))
            .is_ok());
    }

    #[test]
    fn test_constraint_is_file_ok() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "data").unwrap();
        assert!(Constraint::IsFile
            .validate("f", &ConfigValue::path(tmp.path()))
            .is_ok());
    }

    #[test]
    fn test_constraint_is_file_on_directory_fails() {
        assert!(Constraint::IsFile
            .validate("f", &ConfigValue::path("/tmp"))
            .is_err());
    }

    #[test]
    fn test_constraint_is_file_non_path_passes() {
        assert!(Constraint::IsFile
            .validate("f", &ConfigValue::int(42))
            .is_ok());
    }

    #[test]
    fn test_constraint_is_directory_ok() {
        assert!(Constraint::IsDirectory
            .validate("f", &ConfigValue::path("/tmp"))
            .is_ok());
    }

    #[test]
    fn test_constraint_is_directory_on_file_fails() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "data").unwrap();
        assert!(Constraint::IsDirectory
            .validate("f", &ConfigValue::path(tmp.path()))
            .is_err());
    }

    #[test]
    fn test_constraint_is_directory_non_path_passes() {
        assert!(Constraint::IsDirectory
            .validate("f", &ConfigValue::int(42))
            .is_ok());
    }

    #[test]
    fn test_constraint_custom_passes() {
        let c = Constraint::Custom("must be valid".to_string());
        assert!(c.validate("f", &ConfigValue::string("x")).is_ok());
        assert!(c.validate("f", &ConfigValue::int(42)).is_ok());
    }

    #[test]
    fn test_constraint_description_remaining() {
        assert!(Constraint::MinLength(3)
            .description()
            .contains("min length"));
        assert!(Constraint::MaxLength(10)
            .description()
            .contains("max length"));
        assert!(Constraint::Pattern(r"\d+".to_string())
            .description()
            .contains("pattern"));
        assert!(Constraint::IsFile.description().contains("file"));
        assert!(Constraint::IsDirectory.description().contains("directory"));
        assert!(Constraint::Custom("rule".to_string())
            .description()
            .contains("rule"));
        assert!(Constraint::OneOf(vec![ConfigValue::string("a")])
            .description()
            .contains("one of"));
    }

    #[test]
    fn test_field_validate_type_mismatch_message() {
        let f = FieldSchema::new("name", "Name", ValueType::String);
        let err = f.validate(&ConfigValue::int(42)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("name"));
        assert!(msg.contains("string"));
        assert!(msg.contains("integer"));
    }

    #[test]
    fn test_field_validate_null_ok() {
        let f = FieldSchema::new("x", "X", ValueType::String);
        assert!(f.validate(&ConfigValue::Null).is_ok());
    }

    #[test]
    fn test_field_validate_multiple_constraints() {
        let f = FieldSchema::new("port", "Port", ValueType::Integer)
            .constrain(Constraint::Min(1.0))
            .constrain(Constraint::Max(65535.0));
        assert!(f.validate(&ConfigValue::int(8080)).is_ok());
        assert!(f.validate(&ConfigValue::int(0)).is_err());
        assert!(f.validate(&ConfigValue::int(70000)).is_err());
    }

    #[test]
    fn test_cli_help_no_flags_generates_from_name() {
        let f = FieldSchema::new("some_option", "An option", ValueType::String);
        let help = f.cli_help();
        assert!(help.contains("--some-option"));
        assert!(help.contains("An option"));
    }

    #[test]
    fn test_cli_help_only_short_flag() {
        let f = FieldSchema::new("verbose", "Verbose", ValueType::Boolean).short('v');
        let help = f.cli_help();
        assert!(help.contains("-v"));
    }

    #[test]
    fn test_cli_help_with_env_display() {
        let f = FieldSchema::new("t", "Timeout", ValueType::Integer)
            .cli("--timeout")
            .env("TIMEOUT");
        assert!(f.cli_help().contains("Env: TIMEOUT"));
    }

    #[test]
    fn test_cli_help_deprecated_display() {
        let f = FieldSchema::new("old", "Old", ValueType::String)
            .cli("--old")
            .deprecated("Use --new instead");
        let help = f.cli_help();
        assert!(help.contains("DEPRECATED"));
        assert!(help.contains("Use --new instead"));
    }

    #[test]
    fn test_schema_get_group_valid() {
        let s = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("h", "Host", ValueType::String))
            .field(FieldSchema::new("p", "Port", ValueType::Integer))
            .group("net", vec!["h", "p"]);
        let grp = s.get_group("net");
        assert_eq!(grp.len(), 2);
    }

    #[test]
    fn test_schema_get_group_nonexistent() {
        let s = ConfigSchema::new("t", "1.0");
        assert!(s.get_group("missing").is_empty());
    }

    #[test]
    fn test_schema_get_group_filters_missing_fields() {
        let s = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("h", "Host", ValueType::String))
            .group("net", vec!["h", "missing"]);
        assert_eq!(s.get_group("net").len(), 1);
    }

    #[test]
    fn test_schema_get_field_nonexistent() {
        assert!(ConfigSchema::new("t", "1.0").get_field("x").is_none());
    }

    #[test]
    fn test_schema_validate_deprecated_field_warning() {
        let s = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("old", "Old", ValueType::String).deprecated("Use new"));
        let mut c = HashMap::new();
        c.insert("old".to_string(), ConfigValue::string("v"));
        let errs = s.validate(&c).unwrap();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].severity, ErrorSeverity::Warning);
        assert!(errs[0].message.contains("deprecated"));
    }

    #[test]
    fn test_schema_validate_type_mismatch_error() {
        let s = ConfigSchema::new("t", "1.0").field(FieldSchema::new(
            "port",
            "Port",
            ValueType::Integer,
        ));
        let mut c = HashMap::new();
        c.insert("port".to_string(), ConfigValue::string("nan"));
        let errs = s.validate(&c).unwrap();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].severity, ErrorSeverity::Error);
    }

    #[test]
    fn test_schema_validate_required_null_counts_as_missing() {
        let s = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("n", "Name", ValueType::String).required());
        let mut c = HashMap::new();
        c.insert("n".to_string(), ConfigValue::Null);
        let errs = s.validate(&c).unwrap();
        assert!(errs.iter().any(|e| e.message.contains("required")));
    }

    #[test]
    fn test_schema_validate_constraint_failure_in_config() {
        let s = ConfigSchema::new("t", "1.0").field(
            FieldSchema::new("c", "Count", ValueType::Integer).constrain(Constraint::Min(0.0)),
        );
        let mut cfg = HashMap::new();
        cfg.insert("c".to_string(), ConfigValue::int(-5));
        let errs = s.validate(&cfg).unwrap();
        assert!(!errs.is_empty());
        assert_eq!(errs[0].severity, ErrorSeverity::Error);
    }

    #[test]
    fn test_schema_toml_template_with_groups() {
        let s = ConfigSchema::new("MyApp", "2.0")
            .field(
                FieldSchema::new("host", "Server hostname", ValueType::String)
                    .with_default(ConfigValue::string("localhost")),
            )
            .field(FieldSchema::new("port", "Port number", ValueType::Integer))
            .field(
                FieldSchema::new("debug", "Debug mode", ValueType::Boolean)
                    .with_default(ConfigValue::bool(false)),
            )
            .group("server", vec!["host", "port"]);
        let tmpl = s.toml_template();
        assert!(tmpl.contains("MyApp Configuration"));
        assert!(tmpl.contains("Version: 2.0"));
        assert!(tmpl.contains("[server]"));
        assert!(tmpl.contains("Server hostname"));
        assert!(tmpl.contains("Port number"));
        assert!(tmpl.contains("Debug mode"));
    }

    #[test]
    fn test_schema_toml_template_field_no_default() {
        let s = ConfigSchema::new("App", "1.0").field(FieldSchema::new(
            "key",
            "API Key",
            ValueType::String,
        ));
        assert!(s.toml_template().contains("# key = \n"));
    }

    #[test]
    fn test_schema_cli_help_excludes_non_cli_fields() {
        let s = ConfigSchema::new("t", "1.0").field(FieldSchema::new(
            "internal",
            "Internal",
            ValueType::String,
        ));
        let help = s.cli_help();
        assert!(!help.contains("internal"));
        assert!(help.contains("t v1.0"));
    }

    #[test]
    fn test_validation_error_display_warning_severity() {
        let e = ValidationError {
            field: "old_field".to_string(),
            message: "field is deprecated".to_string(),
            severity: ErrorSeverity::Warning,
        };
        let d = format!("{}", e);
        assert!(d.contains("warning"));
        assert!(d.contains("old_field"));
    }

    #[test]
    fn test_value_source_display_runtime() {
        assert_eq!(format!("{}", ValueSource::Runtime), "runtime");
    }

    #[test]
    fn test_value_source_display_file() {
        assert_eq!(
            format!("{}", ValueSource::File(PathBuf::from("/etc/app/c.toml"))),
            "file:/etc/app/c.toml"
        );
    }

    #[test]
    fn test_value_source_display_env() {
        assert_eq!(
            format!("{}", ValueSource::Environment("MY_VAR".to_string())),
            "env:MY_VAR"
        );
    }

    #[test]
    fn test_value_source_eq() {
        assert_eq!(ValueSource::Default, ValueSource::Default);
        assert_eq!(ValueSource::CliArg, ValueSource::CliArg);
        assert_eq!(ValueSource::Runtime, ValueSource::Runtime);
        assert_ne!(ValueSource::Default, ValueSource::Runtime);
        assert_eq!(
            ValueSource::File(PathBuf::from("/a")),
            ValueSource::File(PathBuf::from("/a"))
        );
        assert_ne!(
            ValueSource::File(PathBuf::from("/a")),
            ValueSource::File(PathBuf::from("/b"))
        );
    }

    #[test]
    fn test_store_set_validates_type_with_schema() {
        let schema = ConfigSchema::new("t", "1.0").field(FieldSchema::new(
            "port",
            "Port",
            ValueType::Integer,
        ));
        let mut store = ConfigStore::new().with_schema(schema);
        assert!(store
            .set("port", ConfigValue::string("nan"), ValueSource::Runtime)
            .is_err());
    }

    #[test]
    fn test_store_set_unknown_field_accepted() {
        let schema =
            ConfigSchema::new("t", "1.0").field(FieldSchema::new("known", "K", ValueType::String));
        let mut store = ConfigStore::new().with_schema(schema);
        assert!(store
            .set("unknown", ConfigValue::string("v"), ValueSource::Runtime)
            .is_ok());
    }

    #[test]
    fn test_store_with_schema_applies_defaults() {
        let schema = ConfigSchema::new("t", "1.0")
            .field(
                FieldSchema::new("a", "A", ValueType::Integer).with_default(ConfigValue::int(10)),
            )
            .field(
                FieldSchema::new("b", "B", ValueType::String)
                    .with_default(ConfigValue::string("hi")),
            )
            .field(FieldSchema::new("c", "C", ValueType::Boolean));
        let store = ConfigStore::new().with_schema(schema);
        assert_eq!(store.get_int("a"), Some(10));
        assert_eq!(store.get_string("b"), Some("hi"));
        assert!(store.get("c").is_none());
    }

    #[test]
    fn test_store_with_schema_keeps_existing_values() {
        let schema = ConfigSchema::new("t", "1.0").field(
            FieldSchema::new("k", "K", ValueType::Integer).with_default(ConfigValue::int(99)),
        );
        let mut store = ConfigStore::new();
        store
            .set("k", ConfigValue::int(42), ValueSource::Runtime)
            .unwrap();
        let store = store.with_schema(schema);
        assert_eq!(store.get_int("k"), Some(42));
    }

    #[test]
    fn test_store_validate_without_schema_empty() {
        assert!(ConfigStore::new().validate().unwrap().is_empty());
    }

    #[test]
    fn test_store_remove_returns_value() {
        let mut store = ConfigStore::new();
        store
            .set("k", ConfigValue::int(42), ValueSource::Runtime)
            .unwrap();
        assert_eq!(store.remove("k"), Some(ConfigValue::int(42)));
        assert!(store.source("k").is_none());
    }

    #[test]
    fn test_store_remove_nonexistent_returns_none() {
        assert!(ConfigStore::new().remove("x").is_none());
    }

    #[test]
    fn test_store_clear_removes_sources() {
        let mut store = ConfigStore::new();
        store
            .set("a", ConfigValue::int(1), ValueSource::Runtime)
            .unwrap();
        store
            .set("b", ConfigValue::int(2), ValueSource::CliArg)
            .unwrap();
        store.clear();
        assert!(store.is_empty());
        assert!(store.source("a").is_none());
        assert!(store.source("b").is_none());
    }

    #[test]
    fn test_store_typed_getters_return_none_when_missing() {
        let store = ConfigStore::new();
        assert_eq!(store.get_string("x"), None);
        assert_eq!(store.get_int("x"), None);
        assert_eq!(store.get_float("x"), None);
        assert_eq!(store.get_bool("x"), None);
        assert!(store.get_path("x").is_none());
        assert!(store.get_duration("x").is_none());
    }

    #[test]
    fn test_parse_env_value_string() {
        let s = ConfigStore::new();
        assert_eq!(
            s.parse_env_value("hello", &ValueType::String)
                .unwrap()
                .as_str(),
            Some("hello")
        );
    }

    #[test]
    fn test_parse_env_value_integer_ok() {
        let s = ConfigStore::new();
        assert_eq!(
            s.parse_env_value("42", &ValueType::Integer)
                .unwrap()
                .as_int(),
            Some(42)
        );
    }

    #[test]
    fn test_parse_env_value_integer_err() {
        assert!(ConfigStore::new()
            .parse_env_value("abc", &ValueType::Integer)
            .is_err());
    }

    #[test]
    fn test_parse_env_value_float_ok() {
        let s = ConfigStore::new();
        assert_eq!(
            s.parse_env_value("3.14", &ValueType::Float)
                .unwrap()
                .as_float(),
            Some(3.14)
        );
    }

    #[test]
    fn test_parse_env_value_float_err() {
        assert!(ConfigStore::new()
            .parse_env_value("xyz", &ValueType::Float)
            .is_err());
    }

    #[test]
    fn test_parse_env_value_boolean_all_cases() {
        let s = ConfigStore::new();
        for (input, expected) in &[
            ("true", true),
            ("1", true),
            ("yes", true),
            ("on", true),
            ("TRUE", true),
            ("Yes", true),
            ("ON", true),
            ("false", false),
            ("0", false),
            ("no", false),
            ("off", false),
            ("other", false),
        ] {
            let v = s.parse_env_value(input, &ValueType::Boolean).unwrap();
            assert_eq!(v.as_bool(), Some(*expected), "input={}", input);
        }
    }

    #[test]
    fn test_parse_env_value_path() {
        let s = ConfigStore::new();
        assert_eq!(
            s.parse_env_value("/usr/bin", &ValueType::Path)
                .unwrap()
                .as_path(),
            Some(Path::new("/usr/bin"))
        );
    }

    #[test]
    fn test_parse_env_value_secret() {
        let s = ConfigStore::new();
        let v = s.parse_env_value("tok", &ValueType::Secret).unwrap();
        assert!(matches!(v, ConfigValue::Secret(_)));
        assert_eq!(v.as_str(), Some("tok"));
    }

    #[test]
    fn test_parse_env_value_duration_ok() {
        let s = ConfigStore::new();
        assert_eq!(
            s.parse_env_value("30", &ValueType::Duration)
                .unwrap()
                .as_duration(),
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[test]
    fn test_parse_env_value_duration_err() {
        assert!(ConfigStore::new()
            .parse_env_value("abc", &ValueType::Duration)
            .is_err());
    }

    #[test]
    fn test_parse_env_value_fallback_types() {
        let s = ConfigStore::new();
        // Array, Map, Any types all fall back to String
        for vt in &[
            ValueType::Array(Box::new(ValueType::String)),
            ValueType::Map,
            ValueType::Any,
        ] {
            assert_eq!(s.parse_env_value("val", vt).unwrap().as_str(), Some("val"));
        }
    }

    #[test]
    fn test_store_load_env_sets_value() {
        let var = "SELFWARE_TYPED_TEST_LOAD_ENV_PORT_42";
        std::env::set_var(var, "42");
        let schema = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("port", "Port", ValueType::Integer).env(var));
        let mut store = ConfigStore::new().with_schema(schema);
        store.load_env().unwrap();
        assert_eq!(store.get_int("port"), Some(42));
        assert!(matches!(
            store.source("port"),
            Some(ValueSource::Environment(_))
        ));
        std::env::remove_var(var);
    }

    #[test]
    fn test_store_load_env_missing_var_no_effect() {
        let schema = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("p", "P", ValueType::Integer).env("SELFWARE_NOT_SET_XYZ"));
        let mut store = ConfigStore::new().with_schema(schema);
        store.load_env().unwrap();
        assert!(store.get("p").is_none());
    }

    #[test]
    fn test_store_load_env_no_schema_noop() {
        let mut store = ConfigStore::new();
        store.load_env().unwrap();
        assert!(store.is_empty());
    }

    #[derive(Debug)]
    struct CoverageWatcher {
        log: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
    }

    impl ConfigWatcher for CoverageWatcher {
        fn on_change(&self, field: &str, _old: Option<&ConfigValue>, new: &ConfigValue) {
            self.log
                .lock()
                .unwrap()
                .push((field.to_string(), new.display()));
        }
    }

    #[test]
    fn test_store_watcher_on_set() {
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let w = CoverageWatcher { log: log.clone() };
        let mut store = ConfigStore::new();
        store.watch(Box::new(w));
        store
            .set("k", ConfigValue::string("v"), ValueSource::Runtime)
            .unwrap();
        assert_eq!(log.lock().unwrap().len(), 1);
        assert_eq!(log.lock().unwrap()[0].0, "k");
    }

    #[test]
    fn test_store_watcher_on_multiple_sets() {
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let w = CoverageWatcher { log: log.clone() };
        let mut store = ConfigStore::new();
        store.watch(Box::new(w));
        store
            .set("k", ConfigValue::int(1), ValueSource::Runtime)
            .unwrap();
        store
            .set("k", ConfigValue::int(2), ValueSource::Runtime)
            .unwrap();
        assert_eq!(log.lock().unwrap().len(), 2);
    }

    #[test]
    fn test_wizard_current_step_returns_field() {
        let schema = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("n", "Name", ValueType::String).required())
            .field(FieldSchema::new("a", "Age", ValueType::Integer).required());
        let wiz = ConfigWizard::new().with_schema(schema);
        assert_eq!(wiz.current_step().unwrap().name, "n");
    }

    #[test]
    fn test_wizard_current_step_none_no_schema() {
        assert!(ConfigWizard::new().current_step().is_none());
    }

    #[test]
    fn test_wizard_progress_empty_is_100() {
        assert_eq!(ConfigWizard::new().progress(), 100.0);
    }

    #[test]
    fn test_wizard_set_current_validates() {
        let schema = ConfigSchema::new("t", "1.0").field(
            FieldSchema::new("c", "Count", ValueType::Integer)
                .required()
                .constrain(Constraint::Min(0.0)),
        );
        let mut wiz = ConfigWizard::new().with_schema(schema);
        assert!(wiz.set_current(ConfigValue::int(-1)).is_err());
        assert_eq!(wiz.step_index(), 0);
    }

    #[test]
    fn test_wizard_set_current_no_steps() {
        let mut wiz = ConfigWizard::new().with_schema(ConfigSchema::new("t", "1.0"));
        assert!(wiz.set_current(ConfigValue::string("x")).unwrap());
    }

    #[test]
    fn test_wizard_skip_optional_no_default_ok() {
        let schema =
            ConfigSchema::new("t", "1.0").field(FieldSchema::new("opt", "Opt", ValueType::String));
        let mut wiz = ConfigWizard::new().with_schema(schema);
        assert!(wiz.skip().unwrap());
    }

    #[test]
    fn test_wizard_skip_required_fails() {
        let schema = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("r", "R", ValueType::String).required());
        let mut wiz = ConfigWizard::new().with_schema(schema);
        assert!(wiz.skip().is_err());
    }

    #[test]
    fn test_wizard_skip_no_step_returns_true() {
        assert!(ConfigWizard::new().skip().unwrap());
    }

    #[test]
    fn test_wizard_full_flow_with_apply() {
        let schema = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("name", "N", ValueType::String).required())
            .field(FieldSchema::new("port", "P", ValueType::Integer).required())
            .field(FieldSchema::new("dbg", "D", ValueType::Boolean));

        let mut wiz = ConfigWizard::new().with_schema(schema.clone());
        assert_eq!(wiz.total_steps(), 3);
        assert!(!wiz.is_complete());

        assert!(!wiz.set_current(ConfigValue::string("app")).unwrap());
        assert!(!wiz.set_current(ConfigValue::int(8080)).unwrap());
        assert!(wiz.set_current(ConfigValue::bool(true)).unwrap());
        assert!(wiz.is_complete());

        let mut store = ConfigStore::new().with_schema(schema);
        wiz.apply_to(&mut store).unwrap();
        assert_eq!(store.get_string("name"), Some("app"));
        assert_eq!(store.get_int("port"), Some(8080));
        assert_eq!(store.get_bool("dbg"), Some(true));
    }

    #[test]
    fn test_hot_reload_nonexistent_file() {
        let mut h = HotReloadHandler::new(PathBuf::from("/nonexistent/file.toml"));
        assert!(!h.has_changed());
    }

    #[test]
    fn test_hot_reload_default_interval_5s() {
        let h = HotReloadHandler::new(PathBuf::from("/tmp/c.toml"));
        assert_eq!(h.interval(), std::time::Duration::from_secs(5));
    }

    #[test]
    fn test_hot_reload_detects_then_no_change() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "init").unwrap();
        let mut h = HotReloadHandler::new(tmp.path().to_path_buf());
        assert!(h.has_changed());
        assert!(!h.has_changed());
    }

    #[test]
    fn test_wizard_step_filtering() {
        let schema = ConfigSchema::new("t", "1.0")
            .field(FieldSchema::new("req", "Req", ValueType::String).required())
            .field(FieldSchema::new("nodef", "NoDef", ValueType::String))
            .field(
                FieldSchema::new("hasdef", "HasDef", ValueType::String)
                    .with_default(ConfigValue::string("d")),
            );
        let wiz = ConfigWizard::new().with_schema(schema);
        assert_eq!(wiz.total_steps(), 2); // req + nodef
    }

    #[test]
    fn test_config_value_clone_eq() {
        let vals = vec![
            ConfigValue::string("t"),
            ConfigValue::int(1),
            ConfigValue::float(1.0),
            ConfigValue::bool(true),
            ConfigValue::Null,
            ConfigValue::path("/x"),
            ConfigValue::secret("s"),
            ConfigValue::duration(1),
            ConfigValue::Array(vec![ConfigValue::int(1)]),
        ];
        for v in &vals {
            assert_eq!(v, &v.clone());
        }
    }

    #[test]
    fn test_config_value_ne() {
        assert_ne!(ConfigValue::int(1), ConfigValue::int(2));
        assert_ne!(ConfigValue::string("a"), ConfigValue::string("b"));
        assert_ne!(ConfigValue::string("42"), ConfigValue::int(42));
    }

    #[test]
    fn test_field_schema_serde() {
        let f = FieldSchema::new("t", "Timeout", ValueType::Integer)
            .with_default(ConfigValue::int(30))
            .required()
            .env("T")
            .cli("--t")
            .short('t')
            .constrain(Constraint::Min(1.0))
            .constrain(Constraint::Max(3600.0))
            .secret()
            .deprecated("Use new_t");
        let json = serde_json::to_string(&f).unwrap();
        let d: FieldSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(d.name, "t");
        assert!(d.required);
        assert!(d.secret);
        assert_eq!(d.env_var, Some("T".to_string()));
        assert_eq!(d.cli_flag, Some("--t".to_string()));
        assert_eq!(d.cli_short, Some('t'));
        assert_eq!(d.constraints.len(), 2);
        assert!(d.deprecated.is_some());
    }

    #[test]
    fn test_config_schema_serde() {
        let s = ConfigSchema::new("app", "1.0")
            .field(FieldSchema::new("h", "H", ValueType::String))
            .field(FieldSchema::new("p", "P", ValueType::Integer))
            .group("srv", vec!["h", "p"]);
        let json = serde_json::to_string(&s).unwrap();
        let d: ConfigSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(d.name, "app");
        assert_eq!(d.version, "1.0");
        assert_eq!(d.fields.len(), 2);
        assert!(d.groups.contains_key("srv"));
    }

    #[test]
    fn test_error_severity_eq_clone() {
        assert_eq!(ErrorSeverity::Error, ErrorSeverity::Error);
        assert_eq!(ErrorSeverity::Warning, ErrorSeverity::Warning);
        assert_ne!(ErrorSeverity::Error, ErrorSeverity::Warning);
        let s = ErrorSeverity::Error;
        let c = s;
        assert_eq!(s, c);
    }

    #[test]
    fn test_config_schema_default_empty() {
        let s = ConfigSchema::default();
        assert!(s.name.is_empty());
        assert!(s.version.is_empty());
        assert!(s.fields.is_empty());
        assert!(s.groups.is_empty());
    }

    #[test]
    fn test_value_type_serde() {
        let types = vec![
            ValueType::String,
            ValueType::Integer,
            ValueType::Float,
            ValueType::Boolean,
            ValueType::Map,
            ValueType::Duration,
            ValueType::Path,
            ValueType::Secret,
            ValueType::Any,
            ValueType::Array(Box::new(ValueType::Integer)),
            ValueType::Array(Box::new(ValueType::Array(Box::new(ValueType::String)))),
        ];
        for vt in &types {
            let json = serde_json::to_string(vt).unwrap();
            let d: ValueType = serde_json::from_str(&json).unwrap();
            assert_eq!(vt, &d);
        }
    }

    #[test]
    fn test_constraint_serde() {
        let cs = vec![
            Constraint::Min(0.0),
            Constraint::Max(100.0),
            Constraint::MinLength(1),
            Constraint::MaxLength(255),
            Constraint::Pattern(r"^\w+$".to_string()),
            Constraint::OneOf(vec![ConfigValue::string("a"), ConfigValue::int(1)]),
            Constraint::PathExists,
            Constraint::IsFile,
            Constraint::IsDirectory,
            Constraint::Custom("rule".to_string()),
        ];
        for c in &cs {
            let json = serde_json::to_string(c).unwrap();
            let _d: Constraint = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_validation_error_serde() {
        let e = ValidationError {
            field: "p".to_string(),
            message: "bad".to_string(),
            severity: ErrorSeverity::Error,
        };
        let json = serde_json::to_string(&e).unwrap();
        let d: ValidationError = serde_json::from_str(&json).unwrap();
        assert_eq!(d.field, "p");
        assert_eq!(d.message, "bad");
        assert_eq!(d.severity, ErrorSeverity::Error);
    }

    #[test]
    fn test_as_path_from_path_variant() {
        assert_eq!(
            ConfigValue::path("/usr/local").as_path(),
            Some(Path::new("/usr/local"))
        );
    }

    #[test]
    fn test_complex_schema_validation() {
        let s = ConfigSchema::new("cx", "1.0")
            .field(
                FieldSchema::new("name", "N", ValueType::String)
                    .required()
                    .constrain(Constraint::MinLength(1))
                    .constrain(Constraint::MaxLength(50))
                    .constrain(Constraint::Pattern(r"^[a-zA-Z]".to_string())),
            )
            .field(
                FieldSchema::new("port", "P", ValueType::Integer)
                    .required()
                    .constrain(Constraint::Min(1.0))
                    .constrain(Constraint::Max(65535.0)),
            )
            .field(
                FieldSchema::new("mode", "M", ValueType::String).constrain(Constraint::OneOf(
                    vec![ConfigValue::string("dev"), ConfigValue::string("prod")],
                )),
            );

        let mut ok = HashMap::new();
        ok.insert("name".to_string(), ConfigValue::string("myapp"));
        ok.insert("port".to_string(), ConfigValue::int(8080));
        ok.insert("mode".to_string(), ConfigValue::string("dev"));
        assert!(s.validate(&ok).unwrap().is_empty());

        let mut bad = HashMap::new();
        bad.insert("mode".to_string(), ConfigValue::string("staging"));
        let errs = s.validate(&bad).unwrap();
        assert!(errs.len() >= 3);
    }

    #[test]
    fn test_store_set_overwrites_source() {
        let mut store = ConfigStore::new();
        store
            .set("k", ConfigValue::int(1), ValueSource::Default)
            .unwrap();
        assert_eq!(store.source("k"), Some(&ValueSource::Default));
        store
            .set("k", ConfigValue::int(2), ValueSource::CliArg)
            .unwrap();
        assert_eq!(store.source("k"), Some(&ValueSource::CliArg));
        assert_eq!(store.get_int("k"), Some(2));
    }

    #[test]
    fn test_config_value_debug_format() {
        let v = ConfigValue::string("test");
        let d = format!("{:?}", v);
        assert!(d.contains("String"));
        assert!(d.contains("test"));
    }
}
