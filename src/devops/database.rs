//! Database Query Tool
//!
//! Database tooling capabilities:
//! - SQL and NoSQL query support
//! - Schema introspection
//! - Migration generation
//! - Query history and analysis

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Database type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
    MongoDB,
    Redis,
    DynamoDB,
    Cassandra,
}

impl DatabaseType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PostgreSQL => "postgresql",
            Self::MySQL => "mysql",
            Self::SQLite => "sqlite",
            Self::MongoDB => "mongodb",
            Self::Redis => "redis",
            Self::DynamoDB => "dynamodb",
            Self::Cassandra => "cassandra",
        }
    }

    pub fn is_sql(&self) -> bool {
        matches!(self, Self::PostgreSQL | Self::MySQL | Self::SQLite)
    }

    pub fn is_nosql(&self) -> bool {
        !self.is_sql()
    }

    pub fn default_port(&self) -> u16 {
        match self {
            Self::PostgreSQL => 5432,
            Self::MySQL => 3306,
            Self::SQLite => 0,
            Self::MongoDB => 27017,
            Self::Redis => 6379,
            Self::DynamoDB => 8000,
            Self::Cassandra => 9042,
        }
    }
}

/// Database connection configuration
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Connection name
    pub name: String,
    /// Database type
    pub db_type: DatabaseType,
    /// Host
    pub host: String,
    /// Port
    pub port: u16,
    /// Database name
    pub database: String,
    /// Username
    pub username: Option<String>,
    /// Password (would be stored securely in practice)
    pub password: Option<String>,
    /// Additional options
    pub options: HashMap<String, String>,
    /// SSL enabled
    pub ssl_enabled: bool,
}

impl ConnectionConfig {
    pub fn new(name: &str, db_type: DatabaseType, host: &str, database: &str) -> Self {
        Self {
            name: name.to_string(),
            db_type,
            host: host.to_string(),
            port: db_type.default_port(),
            database: database.to_string(),
            username: None,
            password: None,
            options: HashMap::new(),
            ssl_enabled: false,
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_credentials(mut self, username: &str, password: &str) -> Self {
        self.username = Some(username.to_string());
        self.password = Some(password.to_string());
        self
    }

    pub fn with_ssl(mut self) -> Self {
        self.ssl_enabled = true;
        self
    }

    pub fn with_option(mut self, key: &str, value: &str) -> Self {
        self.options.insert(key.to_string(), value.to_string());
        self
    }

    /// Build connection string
    pub fn connection_string(&self) -> String {
        match self.db_type {
            DatabaseType::PostgreSQL => {
                let auth = self
                    .username
                    .as_ref()
                    .map(|u| format!("{}:{}@", u, self.password.as_deref().unwrap_or("")))
                    .unwrap_or_default();
                format!(
                    "postgresql://{}{}:{}/{}",
                    auth, self.host, self.port, self.database
                )
            }
            DatabaseType::MySQL => {
                let auth = self
                    .username
                    .as_ref()
                    .map(|u| format!("{}:{}@", u, self.password.as_deref().unwrap_or("")))
                    .unwrap_or_default();
                format!(
                    "mysql://{}{}:{}/{}",
                    auth, self.host, self.port, self.database
                )
            }
            DatabaseType::SQLite => {
                format!("sqlite://{}", self.database)
            }
            DatabaseType::MongoDB => {
                let auth = self
                    .username
                    .as_ref()
                    .map(|u| format!("{}:{}@", u, self.password.as_deref().unwrap_or("")))
                    .unwrap_or_default();
                format!(
                    "mongodb://{}{}:{}/{}",
                    auth, self.host, self.port, self.database
                )
            }
            DatabaseType::Redis => {
                format!("redis://{}:{}", self.host, self.port)
            }
            _ => format!(
                "{}://{}:{}/{}",
                self.db_type.as_str(),
                self.host,
                self.port,
                self.database
            ),
        }
    }
}

/// SQL data type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlDataType {
    Integer,
    BigInt,
    SmallInt,
    Decimal(u8, u8),
    Float,
    Double,
    Boolean,
    Char(u32),
    Varchar(u32),
    Text,
    Blob,
    Date,
    Time,
    Timestamp,
    Json,
    Uuid,
    Array(Box<SqlDataType>),
    Custom(String),
}

impl SqlDataType {
    pub fn as_str(&self) -> String {
        match self {
            Self::Integer => "INTEGER".to_string(),
            Self::BigInt => "BIGINT".to_string(),
            Self::SmallInt => "SMALLINT".to_string(),
            Self::Decimal(p, s) => format!("DECIMAL({}, {})", p, s),
            Self::Float => "FLOAT".to_string(),
            Self::Double => "DOUBLE PRECISION".to_string(),
            Self::Boolean => "BOOLEAN".to_string(),
            Self::Char(n) => format!("CHAR({})", n),
            Self::Varchar(n) => format!("VARCHAR({})", n),
            Self::Text => "TEXT".to_string(),
            Self::Blob => "BLOB".to_string(),
            Self::Date => "DATE".to_string(),
            Self::Time => "TIME".to_string(),
            Self::Timestamp => "TIMESTAMP".to_string(),
            Self::Json => "JSON".to_string(),
            Self::Uuid => "UUID".to_string(),
            Self::Array(inner) => format!("{}[]", inner.as_str()),
            Self::Custom(s) => s.clone(),
        }
    }

    /// Parse from SQL type string
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        let upper = s.to_uppercase();
        match upper.as_str() {
            "INTEGER" | "INT" | "INT4" => Self::Integer,
            "BIGINT" | "INT8" => Self::BigInt,
            "SMALLINT" | "INT2" => Self::SmallInt,
            "FLOAT" | "REAL" => Self::Float,
            "DOUBLE PRECISION" | "FLOAT8" => Self::Double,
            "BOOLEAN" | "BOOL" => Self::Boolean,
            "TEXT" => Self::Text,
            "BLOB" | "BYTEA" => Self::Blob,
            "DATE" => Self::Date,
            "TIME" => Self::Time,
            "TIMESTAMP" | "TIMESTAMPTZ" => Self::Timestamp,
            "JSON" | "JSONB" => Self::Json,
            "UUID" => Self::Uuid,
            _ => {
                // Check for VARCHAR(n)
                if upper.starts_with("VARCHAR") || upper.starts_with("CHARACTER VARYING") {
                    if let Some(n) = Self::extract_length(&upper) {
                        return Self::Varchar(n);
                    }
                }
                // Check for CHAR(n)
                if upper.starts_with("CHAR") || upper.starts_with("CHARACTER") {
                    if let Some(n) = Self::extract_length(&upper) {
                        return Self::Char(n);
                    }
                }
                Self::Custom(s.to_string())
            }
        }
    }

    fn extract_length(s: &str) -> Option<u32> {
        let start = s.find('(')?;
        let end = s.find(')')?;
        s[start + 1..end].parse().ok()
    }
}

/// Column definition
#[derive(Debug, Clone)]
pub struct ColumnDef {
    /// Column name
    pub name: String,
    /// Data type
    pub data_type: SqlDataType,
    /// Is nullable?
    pub nullable: bool,
    /// Is primary key?
    pub primary_key: bool,
    /// Is unique?
    pub unique: bool,
    /// Default value
    pub default: Option<String>,
    /// Foreign key reference (table.column)
    pub foreign_key: Option<String>,
    /// Comment
    pub comment: Option<String>,
}

impl ColumnDef {
    pub fn new(name: &str, data_type: SqlDataType) -> Self {
        Self {
            name: name.to_string(),
            data_type,
            nullable: true,
            primary_key: false,
            unique: false,
            default: None,
            foreign_key: None,
            comment: None,
        }
    }

    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }

    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self.nullable = false;
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    pub fn default(mut self, value: &str) -> Self {
        self.default = Some(value.to_string());
        self
    }

    pub fn references(mut self, table_column: &str) -> Self {
        self.foreign_key = Some(table_column.to_string());
        self
    }

    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = Some(comment.to_string());
        self
    }

    /// Generate SQL for column definition
    pub fn to_sql(&self) -> String {
        let mut parts = vec![self.name.clone(), self.data_type.as_str()];

        if self.primary_key {
            parts.push("PRIMARY KEY".to_string());
        }
        if !self.nullable && !self.primary_key {
            parts.push("NOT NULL".to_string());
        }
        if self.unique && !self.primary_key {
            parts.push("UNIQUE".to_string());
        }
        if let Some(ref default) = self.default {
            parts.push(format!("DEFAULT {}", default));
        }
        if let Some(ref fk) = self.foreign_key {
            parts.push(format!("REFERENCES {}", fk));
        }

        parts.join(" ")
    }
}

/// Index definition
#[derive(Debug, Clone)]
pub struct IndexDef {
    /// Index name
    pub name: String,
    /// Table name
    pub table: String,
    /// Columns
    pub columns: Vec<String>,
    /// Is unique?
    pub unique: bool,
    /// Index type (btree, hash, etc.)
    pub index_type: Option<String>,
}

impl IndexDef {
    pub fn new(name: &str, table: &str, columns: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            table: table.to_string(),
            columns,
            unique: false,
            index_type: None,
        }
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    pub fn with_type(mut self, index_type: &str) -> Self {
        self.index_type = Some(index_type.to_string());
        self
    }

    /// Generate CREATE INDEX SQL
    pub fn to_sql(&self) -> String {
        let unique = if self.unique { "UNIQUE " } else { "" };
        let using = self
            .index_type
            .as_ref()
            .map(|t| format!(" USING {}", t))
            .unwrap_or_default();
        format!(
            "CREATE {}INDEX {} ON {}{} ({})",
            unique,
            self.name,
            self.table,
            using,
            self.columns.join(", ")
        )
    }
}

/// Table schema
#[derive(Debug, Clone)]
pub struct TableSchema {
    /// Table name
    pub name: String,
    /// Schema name
    pub schema: Option<String>,
    /// Columns
    pub columns: Vec<ColumnDef>,
    /// Indexes
    pub indexes: Vec<IndexDef>,
    /// Primary key columns
    pub primary_key: Vec<String>,
    /// Comment
    pub comment: Option<String>,
}

impl TableSchema {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            schema: None,
            columns: Vec::new(),
            indexes: Vec::new(),
            primary_key: Vec::new(),
            comment: None,
        }
    }

    pub fn with_schema(mut self, schema: &str) -> Self {
        self.schema = Some(schema.to_string());
        self
    }

    pub fn add_column(mut self, column: ColumnDef) -> Self {
        if column.primary_key {
            self.primary_key.push(column.name.clone());
        }
        self.columns.push(column);
        self
    }

    pub fn add_index(mut self, index: IndexDef) -> Self {
        self.indexes.push(index);
        self
    }

    pub fn with_comment(mut self, comment: &str) -> Self {
        self.comment = Some(comment.to_string());
        self
    }

    /// Full table name with schema
    pub fn full_name(&self) -> String {
        match &self.schema {
            Some(s) => format!("{}.{}", s, self.name),
            None => self.name.clone(),
        }
    }

    /// Get column by name
    pub fn get_column(&self, name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Generate CREATE TABLE SQL
    pub fn to_create_sql(&self) -> String {
        let columns: Vec<String> = self
            .columns
            .iter()
            .map(|c| format!("    {}", c.to_sql()))
            .collect();

        format!(
            "CREATE TABLE {} (\n{}\n)",
            self.full_name(),
            columns.join(",\n")
        )
    }

    /// Generate DROP TABLE SQL
    pub fn to_drop_sql(&self) -> String {
        format!("DROP TABLE IF EXISTS {}", self.full_name())
    }
}

/// Database schema (collection of tables)
#[derive(Debug, Clone)]
pub struct DatabaseSchema {
    /// Database name
    pub name: String,
    /// Tables
    pub tables: HashMap<String, TableSchema>,
    /// Database type
    pub db_type: DatabaseType,
}

impl DatabaseSchema {
    pub fn new(name: &str, db_type: DatabaseType) -> Self {
        Self {
            name: name.to_string(),
            tables: HashMap::new(),
            db_type,
        }
    }

    pub fn add_table(&mut self, table: TableSchema) {
        self.tables.insert(table.name.clone(), table);
    }

    pub fn get_table(&self, name: &str) -> Option<&TableSchema> {
        self.tables.get(name)
    }

    pub fn table_names(&self) -> Vec<&String> {
        self.tables.keys().collect()
    }
}

/// Query type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    Select,
    Insert,
    Update,
    Delete,
    CreateTable,
    AlterTable,
    DropTable,
    CreateIndex,
    DropIndex,
    Other,
}

impl QueryType {
    pub fn from_sql(sql: &str) -> Self {
        let upper = sql.trim().to_uppercase();
        if upper.starts_with("SELECT") {
            Self::Select
        } else if upper.starts_with("INSERT") {
            Self::Insert
        } else if upper.starts_with("UPDATE") {
            Self::Update
        } else if upper.starts_with("DELETE") {
            Self::Delete
        } else if upper.starts_with("CREATE TABLE") {
            Self::CreateTable
        } else if upper.starts_with("ALTER TABLE") {
            Self::AlterTable
        } else if upper.starts_with("DROP TABLE") {
            Self::DropTable
        } else if upper.starts_with("CREATE INDEX") || upper.starts_with("CREATE UNIQUE INDEX") {
            Self::CreateIndex
        } else if upper.starts_with("DROP INDEX") {
            Self::DropIndex
        } else {
            Self::Other
        }
    }

    pub fn is_read(&self) -> bool {
        matches!(self, Self::Select)
    }

    pub fn is_write(&self) -> bool {
        matches!(self, Self::Insert | Self::Update | Self::Delete)
    }

    pub fn is_ddl(&self) -> bool {
        matches!(
            self,
            Self::CreateTable
                | Self::AlterTable
                | Self::DropTable
                | Self::CreateIndex
                | Self::DropIndex
        )
    }
}

/// Query execution result
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Query that was executed
    pub query: String,
    /// Query type
    pub query_type: QueryType,
    /// Rows affected (for write operations)
    pub rows_affected: Option<u64>,
    /// Result rows (for select operations)
    pub rows: Vec<HashMap<String, serde_json::Value>>,
    /// Column names (for select operations)
    pub columns: Vec<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Success?
    pub success: bool,
    /// Error message
    pub error: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl QueryResult {
    pub fn success(query: &str, query_type: QueryType) -> Self {
        Self {
            query: query.to_string(),
            query_type,
            rows_affected: None,
            rows: Vec::new(),
            columns: Vec::new(),
            execution_time_ms: 0,
            success: true,
            error: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn error(query: &str, error: &str) -> Self {
        Self {
            query: query.to_string(),
            query_type: QueryType::from_sql(query),
            rows_affected: None,
            rows: Vec::new(),
            columns: Vec::new(),
            execution_time_ms: 0,
            success: false,
            error: Some(error.to_string()),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn with_rows_affected(mut self, count: u64) -> Self {
        self.rows_affected = Some(count);
        self
    }

    pub fn with_rows(
        mut self,
        columns: Vec<String>,
        rows: Vec<HashMap<String, serde_json::Value>>,
    ) -> Self {
        self.columns = columns;
        self.rows = rows;
        self
    }

    pub fn with_execution_time(mut self, ms: u64) -> Self {
        self.execution_time_ms = ms;
        self
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

/// Migration operation
#[derive(Debug, Clone)]
pub enum MigrationOp {
    CreateTable(TableSchema),
    DropTable(String),
    AddColumn(String, ColumnDef),
    DropColumn(String, String),
    AlterColumn(String, String, ColumnDef),
    AddIndex(IndexDef),
    DropIndex(String, String),
    RenameTable(String, String),
    RenameColumn(String, String, String),
    AddForeignKey(String, String, String, String),
    DropForeignKey(String, String),
    Custom(String),
}

impl MigrationOp {
    /// Generate SQL for this operation
    pub fn to_sql(&self, db_type: DatabaseType) -> String {
        match self {
            Self::CreateTable(schema) => schema.to_create_sql(),
            Self::DropTable(name) => format!("DROP TABLE IF EXISTS {}", name),
            Self::AddColumn(table, column) => {
                format!("ALTER TABLE {} ADD COLUMN {}", table, column.to_sql())
            }
            Self::DropColumn(table, column) => {
                format!("ALTER TABLE {} DROP COLUMN {}", table, column)
            }
            Self::AlterColumn(table, column, new_def) => match db_type {
                DatabaseType::PostgreSQL => {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                        table,
                        column,
                        new_def.data_type.as_str()
                    )
                }
                DatabaseType::MySQL => {
                    format!("ALTER TABLE {} MODIFY COLUMN {}", table, new_def.to_sql())
                }
                _ => format!("ALTER TABLE {} ALTER COLUMN {}", table, new_def.to_sql()),
            },
            Self::AddIndex(index) => index.to_sql(),
            Self::DropIndex(table, name) => match db_type {
                DatabaseType::MySQL => format!("DROP INDEX {} ON {}", name, table),
                _ => format!("DROP INDEX IF EXISTS {}", name),
            },
            Self::RenameTable(old, new) => {
                format!("ALTER TABLE {} RENAME TO {}", old, new)
            }
            Self::RenameColumn(table, old, new) => {
                format!("ALTER TABLE {} RENAME COLUMN {} TO {}", table, old, new)
            }
            Self::AddForeignKey(table, column, ref_table, ref_column) => {
                format!(
                    "ALTER TABLE {} ADD CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {}({})",
                    table, table, column, column, ref_table, ref_column
                )
            }
            Self::DropForeignKey(table, constraint) => {
                format!("ALTER TABLE {} DROP CONSTRAINT {}", table, constraint)
            }
            Self::Custom(sql) => sql.clone(),
        }
    }
}

/// Database migration
#[derive(Debug, Clone)]
pub struct Migration {
    /// Migration version/id
    pub version: String,
    /// Description
    pub description: String,
    /// Up operations
    pub up_ops: Vec<MigrationOp>,
    /// Down operations (rollback)
    pub down_ops: Vec<MigrationOp>,
    /// Created timestamp
    pub created_at: u64,
}

impl Migration {
    pub fn new(version: &str, description: &str) -> Self {
        Self {
            version: version.to_string(),
            description: description.to_string(),
            up_ops: Vec::new(),
            down_ops: Vec::new(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn add_up(mut self, op: MigrationOp) -> Self {
        self.up_ops.push(op);
        self
    }

    pub fn add_down(mut self, op: MigrationOp) -> Self {
        self.down_ops.push(op);
        self
    }

    /// Generate up migration SQL
    pub fn up_sql(&self, db_type: DatabaseType) -> Vec<String> {
        self.up_ops.iter().map(|op| op.to_sql(db_type)).collect()
    }

    /// Generate down migration SQL
    pub fn down_sql(&self, db_type: DatabaseType) -> Vec<String> {
        self.down_ops.iter().map(|op| op.to_sql(db_type)).collect()
    }
}

/// Migration generator
pub struct MigrationGenerator {
    /// Current schema
    current_schema: Option<DatabaseSchema>,
    /// Target schema
    target_schema: Option<DatabaseSchema>,
}

impl MigrationGenerator {
    pub fn new() -> Self {
        Self {
            current_schema: None,
            target_schema: None,
        }
    }

    pub fn set_current(&mut self, schema: DatabaseSchema) {
        self.current_schema = Some(schema);
    }

    pub fn set_target(&mut self, schema: DatabaseSchema) {
        self.target_schema = Some(schema);
    }

    /// Generate migration from current to target
    pub fn generate(&self, version: &str, description: &str) -> Option<Migration> {
        let current = self.current_schema.as_ref()?;
        let target = self.target_schema.as_ref()?;

        let mut migration = Migration::new(version, description);

        // Find new tables
        for (name, table) in &target.tables {
            if !current.tables.contains_key(name) {
                migration
                    .up_ops
                    .push(MigrationOp::CreateTable(table.clone()));
                migration
                    .down_ops
                    .push(MigrationOp::DropTable(name.clone()));
            }
        }

        // Find dropped tables
        for name in current.tables.keys() {
            if !target.tables.contains_key(name) {
                if let Some(table) = current.tables.get(name) {
                    migration.up_ops.push(MigrationOp::DropTable(name.clone()));
                    migration
                        .down_ops
                        .push(MigrationOp::CreateTable(table.clone()));
                }
            }
        }

        // Find column changes in existing tables
        for (name, target_table) in &target.tables {
            if let Some(current_table) = current.tables.get(name) {
                // New columns
                for col in &target_table.columns {
                    if current_table.get_column(&col.name).is_none() {
                        migration
                            .up_ops
                            .push(MigrationOp::AddColumn(name.clone(), col.clone()));
                        migration
                            .down_ops
                            .push(MigrationOp::DropColumn(name.clone(), col.name.clone()));
                    }
                }

                // Dropped columns
                for col in &current_table.columns {
                    if target_table.get_column(&col.name).is_none() {
                        migration
                            .up_ops
                            .push(MigrationOp::DropColumn(name.clone(), col.name.clone()));
                        migration
                            .down_ops
                            .push(MigrationOp::AddColumn(name.clone(), col.clone()));
                    }
                }
            }
        }

        Some(migration)
    }
}

impl Default for MigrationGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Query analyzer for optimization suggestions
pub struct QueryAnalyzer {
    /// Analysis results
    analyses: Vec<QueryAnalysis>,
}

/// Query analysis result
#[derive(Debug, Clone)]
pub struct QueryAnalysis {
    /// Query analyzed
    pub query: String,
    /// Suggestions
    pub suggestions: Vec<String>,
    /// Warnings
    pub warnings: Vec<String>,
    /// Estimated complexity
    pub complexity: QueryComplexity,
    /// Tables accessed
    pub tables_accessed: Vec<String>,
}

/// Query complexity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryComplexity {
    Simple,
    Moderate,
    Complex,
    VeryComplex,
}

impl QueryAnalyzer {
    pub fn new() -> Self {
        Self {
            analyses: Vec::new(),
        }
    }

    /// Analyze a SQL query
    pub fn analyze(&mut self, query: &str) -> QueryAnalysis {
        let mut suggestions = Vec::new();
        let mut warnings = Vec::new();
        let mut tables = Vec::new();

        let upper = query.to_uppercase();

        // Check for SELECT *
        if upper.contains("SELECT *") {
            suggestions.push("Consider selecting specific columns instead of SELECT *".to_string());
        }

        // Check for missing WHERE on UPDATE/DELETE
        if (upper.starts_with("UPDATE") || upper.starts_with("DELETE")) && !upper.contains("WHERE")
        {
            warnings.push("UPDATE/DELETE without WHERE clause affects all rows!".to_string());
        }

        // Check for LIKE with leading wildcard
        if upper.contains("LIKE '%") {
            suggestions.push("LIKE with leading wildcard prevents index usage".to_string());
        }

        // Check for OR in WHERE
        if upper.contains(" OR ") {
            suggestions
                .push("Consider using UNION instead of OR for better index usage".to_string());
        }

        // Check for implicit conversion
        if upper.contains("= '") && upper.contains("INTEGER") {
            suggestions.push("Check for implicit type conversions".to_string());
        }

        // Check for subqueries
        let subquery_count = upper.matches("SELECT").count();
        if subquery_count > 1 {
            suggestions
                .push("Consider rewriting subqueries as JOINs for better performance".to_string());
        }

        // Check for NOT IN
        if upper.contains("NOT IN") {
            suggestions
                .push("Consider using NOT EXISTS instead of NOT IN for NULL safety".to_string());
        }

        // Extract table names (simplified)
        if upper.contains("FROM ") {
            if let Some(start) = upper.find("FROM ") {
                let after_from = &upper[start + 5..];
                let table = after_from
                    .split_whitespace()
                    .next()
                    .map(|s| s.trim_matches(',').to_string());
                if let Some(t) = table {
                    tables.push(t);
                }
            }
        }

        // Calculate complexity
        let complexity = if subquery_count > 2 || upper.matches("JOIN").count() > 3 {
            QueryComplexity::VeryComplex
        } else if subquery_count > 1 || upper.matches("JOIN").count() > 1 {
            QueryComplexity::Complex
        } else if upper.contains("JOIN") || upper.contains("GROUP BY") {
            QueryComplexity::Moderate
        } else {
            QueryComplexity::Simple
        };

        let analysis = QueryAnalysis {
            query: query.to_string(),
            suggestions,
            warnings,
            complexity,
            tables_accessed: tables,
        };

        self.analyses.push(analysis.clone());
        analysis
    }

    /// Get all analyses
    pub fn get_analyses(&self) -> &[QueryAnalysis] {
        &self.analyses
    }
}

impl Default for QueryAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Main database tool
pub struct DatabaseTool {
    /// Connection configurations
    connections: RwLock<HashMap<String, ConnectionConfig>>,
    /// Query history
    history: RwLock<Vec<QueryResult>>,
    /// Active connection name
    active_connection: RwLock<Option<String>>,
    /// Schemas
    schemas: RwLock<HashMap<String, DatabaseSchema>>,
    /// Query analyzer
    analyzer: RwLock<QueryAnalyzer>,
    /// Maximum history size
    max_history: usize,
}

impl DatabaseTool {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::new()),
            active_connection: RwLock::new(None),
            schemas: RwLock::new(HashMap::new()),
            analyzer: RwLock::new(QueryAnalyzer::new()),
            max_history: 1000,
        }
    }

    /// Add a connection configuration
    pub fn add_connection(&self, config: ConnectionConfig) {
        let name = config.name.clone();
        if let Ok(mut conns) = self.connections.write() {
            conns.insert(name.clone(), config);
            // Set as active if first
            if let Ok(mut active) = self.active_connection.write() {
                if active.is_none() {
                    *active = Some(name);
                }
            }
        }
    }

    /// Set active connection
    pub fn set_active(&self, name: &str) -> bool {
        if let Ok(conns) = self.connections.read() {
            if conns.contains_key(name) {
                if let Ok(mut active) = self.active_connection.write() {
                    *active = Some(name.to_string());
                    return true;
                }
            }
        }
        false
    }

    /// Get active connection config
    pub fn get_active_connection(&self) -> Option<ConnectionConfig> {
        if let Ok(active) = self.active_connection.read() {
            if let Some(name) = active.as_ref() {
                if let Ok(conns) = self.connections.read() {
                    return conns.get(name).cloned();
                }
            }
        }
        None
    }

    /// Store schema
    pub fn store_schema(&self, name: &str, schema: DatabaseSchema) {
        if let Ok(mut schemas) = self.schemas.write() {
            schemas.insert(name.to_string(), schema);
        }
    }

    /// Get schema
    pub fn get_schema(&self, name: &str) -> Option<DatabaseSchema> {
        self.schemas.read().ok()?.get(name).cloned()
    }

    /// Record query result
    pub fn record_query(&self, result: QueryResult) {
        if let Ok(mut history) = self.history.write() {
            history.push(result);
            if history.len() > self.max_history {
                history.drain(0..self.max_history / 2);
            }
        }
    }

    /// Get query history
    pub fn get_history(&self) -> Vec<QueryResult> {
        self.history.read().map(|h| h.clone()).unwrap_or_default()
    }

    /// Analyze a query
    pub fn analyze_query(&self, query: &str) -> Option<QueryAnalysis> {
        self.analyzer.write().ok().map(|mut a| a.analyze(query))
    }

    /// Get statistics
    pub fn get_stats(&self) -> DatabaseStats {
        let history = self.history.read().map(|h| h.clone()).unwrap_or_default();
        let total = history.len();
        let successful = history.iter().filter(|r| r.success).count();
        let reads = history.iter().filter(|r| r.query_type.is_read()).count();
        let writes = history.iter().filter(|r| r.query_type.is_write()).count();

        let avg_time: f64 = if total > 0 {
            history
                .iter()
                .map(|r| r.execution_time_ms as f64)
                .sum::<f64>()
                / total as f64
        } else {
            0.0
        };

        DatabaseStats {
            total_queries: total,
            successful_queries: successful,
            failed_queries: total - successful,
            read_queries: reads,
            write_queries: writes,
            avg_execution_time_ms: avg_time,
            connections_count: self.connections.read().map(|c| c.len()).unwrap_or(0),
        }
    }
}

impl Default for DatabaseTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_queries: usize,
    pub successful_queries: usize,
    pub failed_queries: usize,
    pub read_queries: usize,
    pub write_queries: usize,
    pub avg_execution_time_ms: f64,
    pub connections_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_type_as_str() {
        assert_eq!(DatabaseType::PostgreSQL.as_str(), "postgresql");
        assert_eq!(DatabaseType::MySQL.as_str(), "mysql");
    }

    #[test]
    fn test_database_type_is_sql() {
        assert!(DatabaseType::PostgreSQL.is_sql());
        assert!(DatabaseType::MySQL.is_sql());
        assert!(!DatabaseType::MongoDB.is_sql());
    }

    #[test]
    fn test_database_type_default_port() {
        assert_eq!(DatabaseType::PostgreSQL.default_port(), 5432);
        assert_eq!(DatabaseType::MySQL.default_port(), 3306);
    }

    #[test]
    fn test_connection_config_new() {
        let config = ConnectionConfig::new("test", DatabaseType::PostgreSQL, "localhost", "mydb");
        assert_eq!(config.name, "test");
        assert_eq!(config.port, 5432);
    }

    #[test]
    fn test_connection_config_connection_string() {
        let config = ConnectionConfig::new("test", DatabaseType::PostgreSQL, "localhost", "mydb")
            .with_credentials("user", "pass");
        let conn_str = config.connection_string();
        assert!(conn_str.contains("postgresql://"));
        assert!(conn_str.contains("user:pass@"));
    }

    #[test]
    fn test_sql_data_type_as_str() {
        assert_eq!(SqlDataType::Integer.as_str(), "INTEGER");
        assert_eq!(SqlDataType::Varchar(255).as_str(), "VARCHAR(255)");
    }

    #[test]
    fn test_sql_data_type_from_str() {
        assert_eq!(SqlDataType::from_str("INTEGER"), SqlDataType::Integer);
        assert_eq!(
            SqlDataType::from_str("VARCHAR(100)"),
            SqlDataType::Varchar(100)
        );
    }

    #[test]
    fn test_column_def_new() {
        let col = ColumnDef::new("id", SqlDataType::Integer);
        assert_eq!(col.name, "id");
        assert!(col.nullable);
    }

    #[test]
    fn test_column_def_to_sql() {
        let col = ColumnDef::new("id", SqlDataType::Integer).primary_key();
        let sql = col.to_sql();
        assert!(sql.contains("INTEGER"));
        assert!(sql.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_index_def_to_sql() {
        let idx = IndexDef::new("idx_users_email", "users", vec!["email".to_string()]).unique();
        let sql = idx.to_sql();
        assert!(sql.contains("UNIQUE INDEX"));
        assert!(sql.contains("idx_users_email"));
    }

    #[test]
    fn test_table_schema_new() {
        let table = TableSchema::new("users");
        assert_eq!(table.name, "users");
    }

    #[test]
    fn test_table_schema_to_create_sql() {
        let table = TableSchema::new("users")
            .add_column(ColumnDef::new("id", SqlDataType::Integer).primary_key())
            .add_column(ColumnDef::new("name", SqlDataType::Varchar(255)).not_null());
        let sql = table.to_create_sql();
        assert!(sql.contains("CREATE TABLE users"));
        assert!(sql.contains("id INTEGER PRIMARY KEY"));
    }

    #[test]
    fn test_query_type_from_sql() {
        assert_eq!(
            QueryType::from_sql("SELECT * FROM users"),
            QueryType::Select
        );
        assert_eq!(QueryType::from_sql("INSERT INTO users"), QueryType::Insert);
        assert_eq!(QueryType::from_sql("UPDATE users SET"), QueryType::Update);
    }

    #[test]
    fn test_query_type_classifications() {
        assert!(QueryType::Select.is_read());
        assert!(!QueryType::Select.is_write());
        assert!(QueryType::Insert.is_write());
        assert!(QueryType::CreateTable.is_ddl());
    }

    #[test]
    fn test_query_result_success() {
        let result = QueryResult::success("SELECT 1", QueryType::Select);
        assert!(result.success);
    }

    #[test]
    fn test_query_result_error() {
        let result = QueryResult::error("SELECT", "syntax error");
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_migration_new() {
        let migration = Migration::new("001", "Create users table");
        assert_eq!(migration.version, "001");
    }

    #[test]
    fn test_migration_sql() {
        let table = TableSchema::new("users")
            .add_column(ColumnDef::new("id", SqlDataType::Integer).primary_key());
        let migration = Migration::new("001", "Create users")
            .add_up(MigrationOp::CreateTable(table))
            .add_down(MigrationOp::DropTable("users".to_string()));

        let up = migration.up_sql(DatabaseType::PostgreSQL);
        assert!(!up.is_empty());
        assert!(up[0].contains("CREATE TABLE"));
    }

    #[test]
    fn test_migration_generator() {
        let mut gen = MigrationGenerator::new();
        let current = DatabaseSchema::new("test", DatabaseType::PostgreSQL);
        let mut target = DatabaseSchema::new("test", DatabaseType::PostgreSQL);
        target.add_table(TableSchema::new("users"));

        gen.set_current(current);
        gen.set_target(target);

        let migration = gen.generate("001", "Add users table");
        assert!(migration.is_some());
    }

    #[test]
    fn test_query_analyzer_select_star() {
        let mut analyzer = QueryAnalyzer::new();
        let analysis = analyzer.analyze("SELECT * FROM users");
        assert!(!analysis.suggestions.is_empty());
    }

    #[test]
    fn test_query_analyzer_update_no_where() {
        let mut analyzer = QueryAnalyzer::new();
        let analysis = analyzer.analyze("UPDATE users SET active = 1");
        assert!(!analysis.warnings.is_empty());
    }

    #[test]
    fn test_query_complexity() {
        let mut analyzer = QueryAnalyzer::new();

        let simple = analyzer.analyze("SELECT id FROM users");
        assert_eq!(simple.complexity, QueryComplexity::Simple);

        let complex = analyzer.analyze("SELECT * FROM users u JOIN orders o ON u.id = o.user_id JOIN items i ON o.id = i.order_id");
        assert!(matches!(
            complex.complexity,
            QueryComplexity::Complex | QueryComplexity::VeryComplex
        ));
    }

    #[test]
    fn test_database_tool_new() {
        let tool = DatabaseTool::new();
        let stats = tool.get_stats();
        assert_eq!(stats.total_queries, 0);
    }

    #[test]
    fn test_database_tool_add_connection() {
        let tool = DatabaseTool::new();
        let config = ConnectionConfig::new("test", DatabaseType::PostgreSQL, "localhost", "mydb");
        tool.add_connection(config);

        let active = tool.get_active_connection();
        assert!(active.is_some());
    }

    #[test]
    fn test_database_tool_record_query() {
        let tool = DatabaseTool::new();
        let result = QueryResult::success("SELECT 1", QueryType::Select);
        tool.record_query(result);

        let history = tool.get_history();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_database_tool_analyze() {
        let tool = DatabaseTool::new();
        let analysis = tool.analyze_query("SELECT * FROM users");
        assert!(analysis.is_some());
    }

    #[test]
    fn test_database_schema() {
        let mut schema = DatabaseSchema::new("mydb", DatabaseType::PostgreSQL);
        schema.add_table(TableSchema::new("users"));
        assert_eq!(schema.table_names().len(), 1);
    }

    #[test]
    fn test_migration_op_to_sql() {
        let op = MigrationOp::AddColumn(
            "users".to_string(),
            ColumnDef::new("email", SqlDataType::Varchar(255)).not_null(),
        );
        let sql = op.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("ALTER TABLE users ADD COLUMN"));
    }

    #[test]
    fn test_query_result_with_rows() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), serde_json::json!(1));
        let result = QueryResult::success("SELECT id FROM users", QueryType::Select)
            .with_rows(vec!["id".to_string()], vec![row]);
        assert_eq!(result.row_count(), 1);
    }

    #[test]
    fn test_database_stats() {
        let stats = DatabaseStats {
            total_queries: 100,
            successful_queries: 95,
            failed_queries: 5,
            read_queries: 80,
            write_queries: 15,
            avg_execution_time_ms: 50.0,
            connections_count: 2,
        };
        assert_eq!(
            stats.total_queries,
            stats.successful_queries + stats.failed_queries
        );
    }

    #[test]
    fn test_database_type_all_variants() {
        let variants = [
            DatabaseType::PostgreSQL,
            DatabaseType::MySQL,
            DatabaseType::SQLite,
            DatabaseType::MongoDB,
            DatabaseType::Redis,
            DatabaseType::DynamoDB,
            DatabaseType::Cassandra,
        ];

        for variant in variants {
            let _ = variant.as_str();
            let _ = variant.default_port();
        }
    }

    #[test]
    fn test_database_type_is_nosql() {
        assert!(DatabaseType::MongoDB.is_nosql());
        assert!(DatabaseType::Redis.is_nosql());
        assert!(DatabaseType::DynamoDB.is_nosql());
        assert!(DatabaseType::Cassandra.is_nosql());
        assert!(!DatabaseType::PostgreSQL.is_nosql());
    }

    #[test]
    fn test_database_type_default_ports() {
        assert_eq!(DatabaseType::SQLite.default_port(), 0);
        assert_eq!(DatabaseType::MongoDB.default_port(), 27017);
        assert_eq!(DatabaseType::Redis.default_port(), 6379);
        assert_eq!(DatabaseType::DynamoDB.default_port(), 8000);
        assert_eq!(DatabaseType::Cassandra.default_port(), 9042);
    }

    #[test]
    fn test_connection_config_with_port() {
        let config = ConnectionConfig::new("test", DatabaseType::PostgreSQL, "localhost", "mydb")
            .with_port(5433);
        assert_eq!(config.port, 5433);
    }

    #[test]
    fn test_connection_config_with_ssl() {
        let config =
            ConnectionConfig::new("test", DatabaseType::PostgreSQL, "localhost", "mydb").with_ssl();
        assert!(config.ssl_enabled);
    }

    #[test]
    fn test_connection_config_with_option() {
        let config = ConnectionConfig::new("test", DatabaseType::PostgreSQL, "localhost", "mydb")
            .with_option("pool_size", "10");
        assert_eq!(config.options.get("pool_size"), Some(&"10".to_string()));
    }

    #[test]
    fn test_connection_config_mysql_connection_string() {
        let config = ConnectionConfig::new("test", DatabaseType::MySQL, "localhost", "mydb")
            .with_credentials("user", "pass");
        let conn_str = config.connection_string();
        assert!(conn_str.starts_with("mysql://"));
    }

    #[test]
    fn test_connection_config_sqlite_connection_string() {
        let config = ConnectionConfig::new("test", DatabaseType::SQLite, "", "test.db");
        let conn_str = config.connection_string();
        assert!(conn_str.starts_with("sqlite://"));
    }

    #[test]
    fn test_connection_config_mongodb_connection_string() {
        let config = ConnectionConfig::new("test", DatabaseType::MongoDB, "localhost", "mydb");
        let conn_str = config.connection_string();
        assert!(conn_str.starts_with("mongodb://"));
    }

    #[test]
    fn test_connection_config_redis_connection_string() {
        let config = ConnectionConfig::new("test", DatabaseType::Redis, "localhost", "0");
        let conn_str = config.connection_string();
        assert!(conn_str.starts_with("redis://"));
    }

    #[test]
    fn test_connection_config_dynamodb_connection_string() {
        let config = ConnectionConfig::new("test", DatabaseType::DynamoDB, "localhost", "mydb");
        let conn_str = config.connection_string();
        assert!(conn_str.contains("dynamodb://"));
    }

    #[test]
    fn test_sql_data_type_all_variants() {
        let types = vec![
            SqlDataType::Integer,
            SqlDataType::BigInt,
            SqlDataType::SmallInt,
            SqlDataType::Decimal(10, 2),
            SqlDataType::Float,
            SqlDataType::Double,
            SqlDataType::Boolean,
            SqlDataType::Char(10),
            SqlDataType::Varchar(255),
            SqlDataType::Text,
            SqlDataType::Blob,
            SqlDataType::Date,
            SqlDataType::Time,
            SqlDataType::Timestamp,
            SqlDataType::Json,
            SqlDataType::Uuid,
            SqlDataType::Array(Box::new(SqlDataType::Integer)),
            SqlDataType::Custom("GEOMETRY".to_string()),
        ];

        for t in types {
            let _ = t.as_str();
        }
    }

    #[test]
    fn test_sql_data_type_from_str_variations() {
        assert_eq!(SqlDataType::from_str("INT"), SqlDataType::Integer);
        assert_eq!(SqlDataType::from_str("INT4"), SqlDataType::Integer);
        assert_eq!(SqlDataType::from_str("INT8"), SqlDataType::BigInt);
        assert_eq!(SqlDataType::from_str("INT2"), SqlDataType::SmallInt);
        assert_eq!(SqlDataType::from_str("REAL"), SqlDataType::Float);
        assert_eq!(SqlDataType::from_str("FLOAT8"), SqlDataType::Double);
        assert_eq!(SqlDataType::from_str("BOOL"), SqlDataType::Boolean);
        assert_eq!(SqlDataType::from_str("BYTEA"), SqlDataType::Blob);
        assert_eq!(SqlDataType::from_str("TIMESTAMPTZ"), SqlDataType::Timestamp);
        assert_eq!(SqlDataType::from_str("JSONB"), SqlDataType::Json);
    }

    #[test]
    fn test_sql_data_type_char_from_str() {
        if let SqlDataType::Char(n) = SqlDataType::from_str("CHAR(50)") {
            assert_eq!(n, 50);
        } else {
            panic!("Expected Char type");
        }
    }

    #[test]
    fn test_sql_data_type_array() {
        let arr = SqlDataType::Array(Box::new(SqlDataType::Text));
        assert_eq!(arr.as_str(), "TEXT[]");
    }

    #[test]
    fn test_column_def_builder_methods() {
        let col = ColumnDef::new("email", SqlDataType::Varchar(255))
            .not_null()
            .unique()
            .default("''")
            .references("users(id)")
            .comment("User email address");

        assert!(!col.nullable);
        assert!(col.unique);
        assert_eq!(col.default, Some("''".to_string()));
        assert_eq!(col.foreign_key, Some("users(id)".to_string()));
        assert_eq!(col.comment, Some("User email address".to_string()));
    }

    #[test]
    fn test_column_def_to_sql_with_default() {
        let col = ColumnDef::new("created_at", SqlDataType::Timestamp)
            .not_null()
            .default("NOW()");
        let sql = col.to_sql();
        assert!(sql.contains("DEFAULT NOW()"));
    }

    #[test]
    fn test_column_def_to_sql_with_foreign_key() {
        let col = ColumnDef::new("user_id", SqlDataType::Integer).references("users(id)");
        let sql = col.to_sql();
        assert!(sql.contains("REFERENCES users(id)"));
    }

    #[test]
    fn test_index_def_with_type() {
        let idx = IndexDef::new("idx_hash", "users", vec!["data".to_string()]).with_type("HASH");
        let sql = idx.to_sql();
        assert!(sql.contains("USING HASH"));
    }

    #[test]
    fn test_table_schema_with_schema() {
        let table = TableSchema::new("users").with_schema("public");
        assert_eq!(table.full_name(), "public.users");
    }

    #[test]
    fn test_table_schema_add_index() {
        let idx = IndexDef::new("idx_email", "users", vec!["email".to_string()]);
        let table = TableSchema::new("users").add_index(idx);
        assert_eq!(table.indexes.len(), 1);
    }

    #[test]
    fn test_table_schema_with_comment() {
        let table = TableSchema::new("users").with_comment("User accounts table");
        assert_eq!(table.comment, Some("User accounts table".to_string()));
    }

    #[test]
    fn test_table_schema_get_column() {
        let table = TableSchema::new("users")
            .add_column(ColumnDef::new("id", SqlDataType::Integer))
            .add_column(ColumnDef::new("name", SqlDataType::Text));

        assert!(table.get_column("id").is_some());
        assert!(table.get_column("nonexistent").is_none());
    }

    #[test]
    fn test_table_schema_to_drop_sql() {
        let table = TableSchema::new("users");
        let sql = table.to_drop_sql();
        assert_eq!(sql, "DROP TABLE IF EXISTS users");
    }

    #[test]
    fn test_database_schema_get_table() {
        let mut schema = DatabaseSchema::new("mydb", DatabaseType::PostgreSQL);
        schema.add_table(TableSchema::new("users"));

        assert!(schema.get_table("users").is_some());
        assert!(schema.get_table("orders").is_none());
    }

    #[test]
    fn test_query_type_from_sql_all_variants() {
        assert_eq!(QueryType::from_sql("DELETE FROM users"), QueryType::Delete);
        assert_eq!(
            QueryType::from_sql("CREATE TABLE users"),
            QueryType::CreateTable
        );
        assert_eq!(
            QueryType::from_sql("ALTER TABLE users"),
            QueryType::AlterTable
        );
        assert_eq!(
            QueryType::from_sql("DROP TABLE users"),
            QueryType::DropTable
        );
        assert_eq!(
            QueryType::from_sql("CREATE INDEX idx"),
            QueryType::CreateIndex
        );
        assert_eq!(
            QueryType::from_sql("CREATE UNIQUE INDEX idx"),
            QueryType::CreateIndex
        );
        assert_eq!(QueryType::from_sql("DROP INDEX idx"), QueryType::DropIndex);
        assert_eq!(
            QueryType::from_sql("TRUNCATE TABLE users"),
            QueryType::Other
        );
    }

    #[test]
    fn test_query_result_with_rows_affected() {
        let result =
            QueryResult::success("INSERT INTO users", QueryType::Insert).with_rows_affected(5);
        assert_eq!(result.rows_affected, Some(5));
    }

    #[test]
    fn test_query_result_with_execution_time() {
        let result = QueryResult::success("SELECT 1", QueryType::Select).with_execution_time(50);
        assert_eq!(result.execution_time_ms, 50);
    }

    #[test]
    fn test_migration_op_drop_column() {
        let op = MigrationOp::DropColumn("users".to_string(), "old_column".to_string());
        let sql = op.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("DROP COLUMN old_column"));
    }

    #[test]
    fn test_migration_op_alter_column_mysql() {
        let op = MigrationOp::AlterColumn(
            "users".to_string(),
            "name".to_string(),
            ColumnDef::new("name", SqlDataType::Varchar(500)),
        );
        let sql = op.to_sql(DatabaseType::MySQL);
        assert!(sql.contains("MODIFY COLUMN"));
    }

    #[test]
    fn test_migration_op_alter_column_postgresql() {
        let op = MigrationOp::AlterColumn(
            "users".to_string(),
            "name".to_string(),
            ColumnDef::new("name", SqlDataType::Varchar(500)),
        );
        let sql = op.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("ALTER COLUMN"));
        assert!(sql.contains("TYPE"));
    }

    #[test]
    fn test_migration_op_drop_index_mysql() {
        let op = MigrationOp::DropIndex("users".to_string(), "idx_email".to_string());
        let sql = op.to_sql(DatabaseType::MySQL);
        assert!(sql.contains("DROP INDEX idx_email ON users"));
    }

    #[test]
    fn test_migration_op_rename_table() {
        let op = MigrationOp::RenameTable("old_name".to_string(), "new_name".to_string());
        let sql = op.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("RENAME TO new_name"));
    }

    #[test]
    fn test_migration_op_rename_column() {
        let op = MigrationOp::RenameColumn(
            "users".to_string(),
            "old_col".to_string(),
            "new_col".to_string(),
        );
        let sql = op.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("RENAME COLUMN old_col TO new_col"));
    }

    #[test]
    fn test_migration_op_add_foreign_key() {
        let op = MigrationOp::AddForeignKey(
            "orders".to_string(),
            "user_id".to_string(),
            "users".to_string(),
            "id".to_string(),
        );
        let sql = op.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("FOREIGN KEY"));
        assert!(sql.contains("REFERENCES users(id)"));
    }

    #[test]
    fn test_migration_op_drop_foreign_key() {
        let op = MigrationOp::DropForeignKey("orders".to_string(), "fk_user".to_string());
        let sql = op.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("DROP CONSTRAINT fk_user"));
    }

    #[test]
    fn test_migration_op_custom() {
        let op = MigrationOp::Custom("VACUUM ANALYZE users".to_string());
        let sql = op.to_sql(DatabaseType::PostgreSQL);
        assert_eq!(sql, "VACUUM ANALYZE users");
    }

    #[test]
    fn test_migration_down_sql() {
        let migration =
            Migration::new("001", "Test").add_down(MigrationOp::DropTable("users".to_string()));
        let down = migration.down_sql(DatabaseType::PostgreSQL);
        assert!(!down.is_empty());
    }

    #[test]
    fn test_migration_generator_default() {
        let gen = MigrationGenerator::default();
        assert!(gen.generate("001", "test").is_none());
    }

    #[test]
    fn test_migration_generator_column_changes() {
        let mut gen = MigrationGenerator::new();

        let current_table = TableSchema::new("users")
            .add_column(ColumnDef::new("id", SqlDataType::Integer))
            .add_column(ColumnDef::new("old_col", SqlDataType::Text));
        let mut current = DatabaseSchema::new("test", DatabaseType::PostgreSQL);
        current.add_table(current_table);

        let target_table = TableSchema::new("users")
            .add_column(ColumnDef::new("id", SqlDataType::Integer))
            .add_column(ColumnDef::new("new_col", SqlDataType::Text));
        let mut target = DatabaseSchema::new("test", DatabaseType::PostgreSQL);
        target.add_table(target_table);

        gen.set_current(current);
        gen.set_target(target);

        let migration = gen.generate("001", "Modify columns").unwrap();
        assert!(!migration.up_ops.is_empty());
    }

    #[test]
    fn test_query_analyzer_like_wildcard() {
        let mut analyzer = QueryAnalyzer::new();
        let analysis = analyzer.analyze("SELECT * FROM users WHERE name LIKE '%john'");
        assert!(analysis.suggestions.iter().any(|s| s.contains("LIKE")));
    }

    #[test]
    fn test_query_analyzer_or_clause() {
        let mut analyzer = QueryAnalyzer::new();
        let analysis = analyzer.analyze("SELECT * FROM users WHERE status = 1 OR status = 2");
        assert!(analysis.suggestions.iter().any(|s| s.contains("OR")));
    }

    #[test]
    fn test_query_analyzer_not_in() {
        let mut analyzer = QueryAnalyzer::new();
        let analysis = analyzer.analyze("SELECT * FROM users WHERE id NOT IN (1, 2, 3)");
        assert!(analysis.suggestions.iter().any(|s| s.contains("NOT IN")));
    }

    #[test]
    fn test_query_analyzer_get_analyses() {
        let mut analyzer = QueryAnalyzer::new();
        analyzer.analyze("SELECT 1");
        analyzer.analyze("SELECT 2");
        assert_eq!(analyzer.get_analyses().len(), 2);
    }

    #[test]
    fn test_query_analyzer_default() {
        let analyzer = QueryAnalyzer::default();
        assert!(analyzer.get_analyses().is_empty());
    }

    #[test]
    fn test_database_tool_set_active() {
        let tool = DatabaseTool::new();
        tool.add_connection(ConnectionConfig::new(
            "test1",
            DatabaseType::PostgreSQL,
            "localhost",
            "db1",
        ));
        tool.add_connection(ConnectionConfig::new(
            "test2",
            DatabaseType::PostgreSQL,
            "localhost",
            "db2",
        ));

        assert!(tool.set_active("test2"));
        assert!(!tool.set_active("nonexistent"));
    }

    #[test]
    fn test_database_tool_store_get_schema() {
        let tool = DatabaseTool::new();
        let schema = DatabaseSchema::new("mydb", DatabaseType::PostgreSQL);
        tool.store_schema("mydb", schema);

        assert!(tool.get_schema("mydb").is_some());
        assert!(tool.get_schema("other").is_none());
    }

    #[test]
    fn test_database_tool_default() {
        let tool = DatabaseTool::default();
        assert!(tool.get_history().is_empty());
    }

    #[test]
    fn test_database_tool_stats_with_queries() {
        let tool = DatabaseTool::new();
        tool.record_query(QueryResult::success("SELECT 1", QueryType::Select));
        tool.record_query(QueryResult::success("INSERT INTO t", QueryType::Insert));
        tool.record_query(QueryResult::error("BAD SQL", "error"));

        let stats = tool.get_stats();
        assert_eq!(stats.total_queries, 3);
        assert_eq!(stats.successful_queries, 2);
        assert_eq!(stats.failed_queries, 1);
        assert_eq!(stats.read_queries, 1);
        assert_eq!(stats.write_queries, 1);
    }

    #[test]
    fn test_database_stats_clone() {
        let stats = DatabaseStats {
            total_queries: 10,
            successful_queries: 8,
            failed_queries: 2,
            read_queries: 5,
            write_queries: 3,
            avg_execution_time_ms: 25.5,
            connections_count: 1,
        };

        let cloned = stats.clone();
        assert_eq!(stats.total_queries, cloned.total_queries);
    }

    #[test]
    fn test_database_stats_debug() {
        let stats = DatabaseStats {
            total_queries: 10,
            successful_queries: 8,
            failed_queries: 2,
            read_queries: 5,
            write_queries: 3,
            avg_execution_time_ms: 25.5,
            connections_count: 1,
        };

        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("DatabaseStats"));
    }

    #[test]
    fn test_connection_config_clone() {
        let config = ConnectionConfig::new("test", DatabaseType::PostgreSQL, "localhost", "mydb")
            .with_credentials("user", "pass");
        let cloned = config.clone();
        assert_eq!(config.name, cloned.name);
        assert_eq!(config.username, cloned.username);
    }

    #[test]
    fn test_column_def_clone() {
        let col = ColumnDef::new("id", SqlDataType::Integer).primary_key();
        let cloned = col.clone();
        assert_eq!(col.name, cloned.name);
        assert_eq!(col.primary_key, cloned.primary_key);
    }

    #[test]
    fn test_table_schema_clone() {
        let table =
            TableSchema::new("users").add_column(ColumnDef::new("id", SqlDataType::Integer));
        let cloned = table.clone();
        assert_eq!(table.name, cloned.name);
    }

    #[test]
    fn test_query_result_clone() {
        let result = QueryResult::success("SELECT 1", QueryType::Select);
        let cloned = result.clone();
        assert_eq!(result.query, cloned.query);
    }

    #[test]
    fn test_query_analysis_clone() {
        let analysis = QueryAnalysis {
            query: "SELECT 1".to_string(),
            suggestions: vec!["suggestion".to_string()],
            warnings: vec![],
            complexity: QueryComplexity::Simple,
            tables_accessed: vec!["table".to_string()],
        };

        let cloned = analysis.clone();
        assert_eq!(analysis.query, cloned.query);
    }
}
