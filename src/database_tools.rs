//! Database & Schema Migration Tools
//!
//! Comprehensive database tooling including schema migration planning,
//! query optimization assistance, data pipeline orchestration,
//! and database refactoring capabilities.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

static MIGRATION_COUNTER: AtomicU64 = AtomicU64::new(1);
static QUERY_COUNTER: AtomicU64 = AtomicU64::new(1);
static PIPELINE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_migration_id() -> String {
    format!("mig-{}", MIGRATION_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_query_id() -> String {
    format!("qry-{}", QUERY_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_pipeline_id() -> String {
    format!("pipe-{}", PIPELINE_COUNTER.fetch_add(1, Ordering::SeqCst))
}

// ============================================================================
// Database Types
// ============================================================================

/// Supported database systems
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
    MongoDB,
    Redis,
    Cassandra,
    DynamoDB,
    ClickHouse,
    TimescaleDB,
    CockroachDB,
}

impl DatabaseType {
    /// Get database name
    pub fn name(&self) -> &'static str {
        match self {
            Self::PostgreSQL => "PostgreSQL",
            Self::MySQL => "MySQL",
            Self::SQLite => "SQLite",
            Self::MongoDB => "MongoDB",
            Self::Redis => "Redis",
            Self::Cassandra => "Cassandra",
            Self::DynamoDB => "DynamoDB",
            Self::ClickHouse => "ClickHouse",
            Self::TimescaleDB => "TimescaleDB",
            Self::CockroachDB => "CockroachDB",
        }
    }

    /// Check if SQL-based
    pub fn is_sql(&self) -> bool {
        matches!(
            self,
            Self::PostgreSQL
                | Self::MySQL
                | Self::SQLite
                | Self::ClickHouse
                | Self::TimescaleDB
                | Self::CockroachDB
        )
    }

    /// Check if supports transactions
    pub fn supports_transactions(&self) -> bool {
        matches!(
            self,
            Self::PostgreSQL | Self::MySQL | Self::SQLite | Self::MongoDB | Self::CockroachDB
        )
    }

    /// Get default port
    pub fn default_port(&self) -> u16 {
        match self {
            Self::PostgreSQL | Self::CockroachDB => 5432,
            Self::MySQL => 3306,
            Self::SQLite => 0,
            Self::MongoDB => 27017,
            Self::Redis => 6379,
            Self::Cassandra => 9042,
            Self::DynamoDB => 8000,
            Self::ClickHouse => 8123,
            Self::TimescaleDB => 5432,
        }
    }
}

/// ORM/Query builder types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ORMType {
    Diesel,
    SQLx,
    SeaORM,
    Prisma,
    TypeORM,
    Drizzle,
    Raw,
}

impl ORMType {
    /// Get ORM name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Diesel => "Diesel",
            Self::SQLx => "SQLx",
            Self::SeaORM => "SeaORM",
            Self::Prisma => "Prisma",
            Self::TypeORM => "TypeORM",
            Self::Drizzle => "Drizzle",
            Self::Raw => "Raw SQL",
        }
    }

    /// Get migration file extension
    pub fn migration_extension(&self) -> &'static str {
        match self {
            Self::Diesel => "sql",
            Self::SQLx => "sql",
            Self::SeaORM => "rs",
            Self::Prisma => "prisma",
            Self::TypeORM => "ts",
            Self::Drizzle => "ts",
            Self::Raw => "sql",
        }
    }
}

// ============================================================================
// Schema Types
// ============================================================================

/// Column data type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnType {
    Integer,
    BigInt,
    SmallInt,
    Serial,
    BigSerial,
    Float,
    Double,
    Decimal { precision: u8, scale: u8 },
    Boolean,
    Varchar(u32),
    Text,
    Char(u32),
    Uuid,
    Timestamp,
    TimestampTz,
    Date,
    Time,
    Interval,
    Json,
    Jsonb,
    Bytea,
    Array(Box<ColumnType>),
    Custom(String),
}

impl ColumnType {
    /// Get SQL representation for PostgreSQL
    pub fn to_postgres(&self) -> String {
        match self {
            Self::Integer => "INTEGER".to_string(),
            Self::BigInt => "BIGINT".to_string(),
            Self::SmallInt => "SMALLINT".to_string(),
            Self::Serial => "SERIAL".to_string(),
            Self::BigSerial => "BIGSERIAL".to_string(),
            Self::Float => "REAL".to_string(),
            Self::Double => "DOUBLE PRECISION".to_string(),
            Self::Decimal { precision, scale } => format!("DECIMAL({}, {})", precision, scale),
            Self::Boolean => "BOOLEAN".to_string(),
            Self::Varchar(len) => format!("VARCHAR({})", len),
            Self::Text => "TEXT".to_string(),
            Self::Char(len) => format!("CHAR({})", len),
            Self::Uuid => "UUID".to_string(),
            Self::Timestamp => "TIMESTAMP".to_string(),
            Self::TimestampTz => "TIMESTAMPTZ".to_string(),
            Self::Date => "DATE".to_string(),
            Self::Time => "TIME".to_string(),
            Self::Interval => "INTERVAL".to_string(),
            Self::Json => "JSON".to_string(),
            Self::Jsonb => "JSONB".to_string(),
            Self::Bytea => "BYTEA".to_string(),
            Self::Array(inner) => format!("{}[]", inner.to_postgres()),
            Self::Custom(s) => s.clone(),
        }
    }
}

/// Column definition
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub column_type: ColumnType,
    pub nullable: bool,
    pub default: Option<String>,
    pub primary_key: bool,
    pub unique: bool,
    pub references: Option<ForeignKey>,
    pub check: Option<String>,
}

impl Column {
    /// Create new column
    pub fn new(name: impl Into<String>, column_type: ColumnType) -> Self {
        Self {
            name: name.into(),
            column_type,
            nullable: true,
            default: None,
            primary_key: false,
            unique: false,
            references: None,
            check: None,
        }
    }

    /// Set not null
    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }

    /// Set default value
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Set as primary key
    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self.nullable = false;
        self
    }

    /// Set as unique
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// Add foreign key reference
    pub fn references(mut self, table: impl Into<String>, column: impl Into<String>) -> Self {
        self.references = Some(ForeignKey {
            table: table.into(),
            column: column.into(),
            on_delete: ReferentialAction::NoAction,
            on_update: ReferentialAction::NoAction,
        });
        self
    }

    /// Generate SQL for column definition
    pub fn to_sql(&self, db_type: DatabaseType) -> String {
        let mut sql = format!("{} {}", self.name, self.column_type.to_postgres());

        if !self.nullable {
            sql.push_str(" NOT NULL");
        }

        if let Some(ref default) = self.default {
            sql.push_str(&format!(" DEFAULT {}", default));
        }

        if self.primary_key && db_type.is_sql() {
            sql.push_str(" PRIMARY KEY");
        }

        if self.unique {
            sql.push_str(" UNIQUE");
        }

        if let Some(ref fk) = self.references {
            sql.push_str(&format!(" REFERENCES {}({})", fk.table, fk.column));
        }

        sql
    }
}

/// Foreign key reference
#[derive(Debug, Clone)]
pub struct ForeignKey {
    pub table: String,
    pub column: String,
    pub on_delete: ReferentialAction,
    pub on_update: ReferentialAction,
}

/// Referential action for foreign keys
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferentialAction {
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

impl ReferentialAction {
    /// Get SQL representation
    pub fn to_sql(&self) -> &'static str {
        match self {
            Self::NoAction => "NO ACTION",
            Self::Restrict => "RESTRICT",
            Self::Cascade => "CASCADE",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
        }
    }
}

/// Index definition
#[derive(Debug, Clone)]
pub struct Index {
    pub name: String,
    pub table: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub index_type: IndexType,
    pub where_clause: Option<String>,
}

/// Index types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexType {
    BTree,
    Hash,
    Gin,
    Gist,
    Brin,
}

impl Index {
    /// Create new index
    pub fn new(name: impl Into<String>, table: impl Into<String>, columns: Vec<String>) -> Self {
        Self {
            name: name.into(),
            table: table.into(),
            columns,
            unique: false,
            index_type: IndexType::BTree,
            where_clause: None,
        }
    }

    /// Set as unique
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    /// Set index type
    pub fn with_type(mut self, index_type: IndexType) -> Self {
        self.index_type = index_type;
        self
    }

    /// Add partial index condition
    pub fn with_where(mut self, condition: impl Into<String>) -> Self {
        self.where_clause = Some(condition.into());
        self
    }

    /// Generate CREATE INDEX SQL
    pub fn to_sql(&self) -> String {
        let unique = if self.unique { "UNIQUE " } else { "" };
        let using = match self.index_type {
            IndexType::BTree => "",
            IndexType::Hash => " USING HASH",
            IndexType::Gin => " USING GIN",
            IndexType::Gist => " USING GIST",
            IndexType::Brin => " USING BRIN",
        };

        let mut sql = format!(
            "CREATE {}INDEX {} ON {}{}({})",
            unique,
            self.name,
            self.table,
            using,
            self.columns.join(", ")
        );

        if let Some(ref where_clause) = self.where_clause {
            sql.push_str(&format!(" WHERE {}", where_clause));
        }

        sql
    }
}

/// Table definition
#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub schema: Option<String>,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    pub constraints: Vec<Constraint>,
}

/// Table constraint
#[derive(Debug, Clone)]
pub struct Constraint {
    pub name: String,
    pub constraint_type: ConstraintType,
}

/// Constraint types
#[derive(Debug, Clone)]
pub enum ConstraintType {
    PrimaryKey(Vec<String>),
    Unique(Vec<String>),
    ForeignKey {
        columns: Vec<String>,
        references_table: String,
        references_columns: Vec<String>,
        on_delete: ReferentialAction,
        on_update: ReferentialAction,
    },
    Check(String),
    Exclusion(String),
}

impl Table {
    /// Create new table
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: None,
            columns: Vec::new(),
            indexes: Vec::new(),
            constraints: Vec::new(),
        }
    }

    /// Set schema
    pub fn in_schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = Some(schema.into());
        self
    }

    /// Add column
    pub fn add_column(&mut self, column: Column) {
        self.columns.push(column);
    }

    /// Add index
    pub fn add_index(&mut self, index: Index) {
        self.indexes.push(index);
    }

    /// Add constraint
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Get full table name
    pub fn full_name(&self) -> String {
        match &self.schema {
            Some(schema) => format!("{}.{}", schema, self.name),
            None => self.name.clone(),
        }
    }

    /// Generate CREATE TABLE SQL
    pub fn to_sql(&self, db_type: DatabaseType) -> String {
        let mut sql = format!("CREATE TABLE {} (\n", self.full_name());

        let column_defs: Vec<String> = self
            .columns
            .iter()
            .map(|c| format!("  {}", c.to_sql(db_type)))
            .collect();

        sql.push_str(&column_defs.join(",\n"));
        sql.push_str("\n)");

        sql
    }
}

// ============================================================================
// Migration System
// ============================================================================

/// Migration status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStatus {
    Pending,
    Applied,
    Failed,
    RolledBack,
}

/// A database migration
#[derive(Debug, Clone)]
pub struct Migration {
    pub id: String,
    pub version: String,
    pub name: String,
    pub up_sql: String,
    pub down_sql: String,
    pub status: MigrationStatus,
    pub applied_at: Option<SystemTime>,
    pub checksum: String,
    pub execution_time: Option<Duration>,
}

impl Migration {
    /// Create new migration
    pub fn new(version: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: generate_migration_id(),
            version: version.into(),
            name: name.into(),
            up_sql: String::new(),
            down_sql: String::new(),
            status: MigrationStatus::Pending,
            applied_at: None,
            checksum: String::new(),
            execution_time: None,
        }
    }

    /// Set up migration SQL
    pub fn up(mut self, sql: impl Into<String>) -> Self {
        self.up_sql = sql.into();
        self.update_checksum();
        self
    }

    /// Set down migration SQL
    pub fn down(mut self, sql: impl Into<String>) -> Self {
        self.down_sql = sql.into();
        self
    }

    /// Update checksum
    fn update_checksum(&mut self) {
        // Simple checksum for demonstration
        self.checksum = format!("{:x}", self.up_sql.len() * 31 + self.down_sql.len());
    }

    /// Mark as applied
    pub fn mark_applied(&mut self, execution_time: Duration) {
        self.status = MigrationStatus::Applied;
        self.applied_at = Some(SystemTime::now());
        self.execution_time = Some(execution_time);
    }

    /// Mark as failed
    pub fn mark_failed(&mut self) {
        self.status = MigrationStatus::Failed;
    }

    /// Check if reversible
    pub fn is_reversible(&self) -> bool {
        !self.down_sql.is_empty()
    }
}

/// Migration planner for safe schema changes
#[derive(Debug)]
pub struct MigrationPlanner {
    migrations: Vec<Migration>,
    applied: Vec<String>,
    db_type: DatabaseType,
}

impl MigrationPlanner {
    /// Create new planner
    pub fn new(db_type: DatabaseType) -> Self {
        Self {
            migrations: Vec::new(),
            applied: Vec::new(),
            db_type,
        }
    }

    /// Add migration
    pub fn add_migration(&mut self, migration: Migration) {
        self.migrations.push(migration);
    }

    /// Mark migration as applied
    pub fn mark_applied(&mut self, version: &str) {
        self.applied.push(version.to_string());
        if let Some(m) = self.migrations.iter_mut().find(|m| m.version == version) {
            m.mark_applied(Duration::ZERO);
        }
    }

    /// Get pending migrations
    pub fn pending(&self) -> Vec<&Migration> {
        self.migrations
            .iter()
            .filter(|m| !self.applied.contains(&m.version))
            .collect()
    }

    /// Get applied migrations
    pub fn applied(&self) -> Vec<&Migration> {
        self.migrations
            .iter()
            .filter(|m| self.applied.contains(&m.version))
            .collect()
    }

    /// Generate migration plan
    pub fn plan(&self) -> MigrationPlan {
        let pending = self.pending();
        let steps: Vec<MigrationStep> = pending
            .iter()
            .map(|m| MigrationStep {
                migration_id: m.id.clone(),
                version: m.version.clone(),
                name: m.name.clone(),
                action: MigrationAction::Apply,
                sql: m.up_sql.clone(),
                estimated_duration: self.estimate_duration(&m.up_sql),
                risk_level: self.assess_risk(&m.up_sql),
                requires_lock: self.requires_lock(&m.up_sql),
            })
            .collect();

        let requires_downtime = steps.iter().any(|s| s.requires_lock);

        MigrationPlan {
            steps,
            total_estimated_duration: Duration::ZERO,
            requires_downtime,
        }
    }

    /// Estimate migration duration
    fn estimate_duration(&self, sql: &str) -> Duration {
        // Simple heuristic based on SQL complexity
        let complexity = sql.len() / 100 + 1;
        Duration::from_secs(complexity as u64)
    }

    /// Assess migration risk
    fn assess_risk(&self, sql: &str) -> RiskLevel {
        let sql_upper = sql.to_uppercase();

        if sql_upper.contains("DROP TABLE") || sql_upper.contains("TRUNCATE") {
            RiskLevel::Critical
        } else if sql_upper.contains("DROP COLUMN") || sql_upper.contains("ALTER TYPE") {
            RiskLevel::High
        } else if sql_upper.contains("ALTER TABLE") || sql_upper.contains("CREATE INDEX") {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }

    /// Check if migration requires table lock
    fn requires_lock(&self, sql: &str) -> bool {
        let sql_upper = sql.to_uppercase();
        sql_upper.contains("ALTER TABLE")
            || sql_upper.contains("CREATE INDEX")
            || sql_upper.contains("DROP")
    }

    /// Generate safe migration for adding column
    pub fn generate_add_column(&self, table: &str, column: &Column) -> Migration {
        let version = chrono_version();
        let name = format!("add_{}_{}", table, column.name);

        let up_sql = format!(
            "ALTER TABLE {} ADD COLUMN {};",
            table,
            column.to_sql(self.db_type)
        );

        let down_sql = format!("ALTER TABLE {} DROP COLUMN {};", table, column.name);

        Migration::new(version, name).up(up_sql).down(down_sql)
    }

    /// Generate safe migration for creating index
    pub fn generate_create_index(&self, index: &Index) -> Migration {
        let version = chrono_version();
        let name = format!("create_index_{}", index.name);

        // Use CONCURRENTLY for PostgreSQL to avoid locking
        let up_sql = if self.db_type == DatabaseType::PostgreSQL {
            format!(
                "CREATE {}INDEX CONCURRENTLY {} ON {}({})",
                if index.unique { "UNIQUE " } else { "" },
                index.name,
                index.table,
                index.columns.join(", ")
            )
        } else {
            index.to_sql()
        };

        let down_sql = format!("DROP INDEX {};", index.name);

        Migration::new(version, name).up(up_sql).down(down_sql)
    }
}

fn chrono_version() -> String {
    format!(
        "{}",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    )
}

/// Migration plan
#[derive(Debug, Clone)]
pub struct MigrationPlan {
    pub steps: Vec<MigrationStep>,
    pub total_estimated_duration: Duration,
    pub requires_downtime: bool,
}

/// A step in the migration plan
#[derive(Debug, Clone)]
pub struct MigrationStep {
    pub migration_id: String,
    pub version: String,
    pub name: String,
    pub action: MigrationAction,
    pub sql: String,
    pub estimated_duration: Duration,
    pub risk_level: RiskLevel,
    pub requires_lock: bool,
}

/// Migration action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationAction {
    Apply,
    Rollback,
    Skip,
}

/// Risk level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

// ============================================================================
// Query Optimization
// ============================================================================

/// Query plan node type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanNodeType {
    SeqScan,
    IndexScan,
    IndexOnlyScan,
    BitmapHeapScan,
    BitmapIndexScan,
    NestedLoop,
    HashJoin,
    MergeJoin,
    Sort,
    Aggregate,
    HashAggregate,
    GroupAggregate,
    Limit,
    Append,
    Result,
    Materialize,
    Unique,
    Gather,
    GatherMerge,
}

impl PlanNodeType {
    /// Check if potentially slow
    pub fn is_slow(&self) -> bool {
        matches!(self, Self::SeqScan | Self::NestedLoop | Self::Sort)
    }
}

/// Query execution plan node
#[derive(Debug, Clone)]
pub struct PlanNode {
    pub node_type: PlanNodeType,
    pub relation: Option<String>,
    pub alias: Option<String>,
    pub startup_cost: f64,
    pub total_cost: f64,
    pub rows: u64,
    pub width: u32,
    pub actual_time: Option<f64>,
    pub actual_rows: Option<u64>,
    pub loops: u32,
    pub filter: Option<String>,
    pub index_name: Option<String>,
    pub index_cond: Option<String>,
    pub children: Vec<PlanNode>,
}

impl PlanNode {
    /// Create new plan node
    pub fn new(node_type: PlanNodeType) -> Self {
        Self {
            node_type,
            relation: None,
            alias: None,
            startup_cost: 0.0,
            total_cost: 0.0,
            rows: 0,
            width: 0,
            actual_time: None,
            actual_rows: None,
            loops: 1,
            filter: None,
            index_name: None,
            index_cond: None,
            children: Vec::new(),
        }
    }

    /// Set relation
    pub fn with_relation(mut self, relation: impl Into<String>) -> Self {
        self.relation = Some(relation.into());
        self
    }

    /// Set costs
    pub fn with_costs(mut self, startup: f64, total: f64) -> Self {
        self.startup_cost = startup;
        self.total_cost = total;
        self
    }

    /// Set row estimates
    pub fn with_rows(mut self, rows: u64, width: u32) -> Self {
        self.rows = rows;
        self.width = width;
        self
    }

    /// Add child node
    pub fn add_child(&mut self, child: PlanNode) {
        self.children.push(child);
    }

    /// Find slow operations
    pub fn find_slow_operations(&self) -> Vec<&PlanNode> {
        let mut slow = Vec::new();

        if self.node_type.is_slow() && self.rows > 1000 {
            slow.push(self);
        }

        for child in &self.children {
            slow.extend(child.find_slow_operations());
        }

        slow
    }
}

/// Query analysis result
#[derive(Debug, Clone)]
pub struct QueryAnalysis {
    pub id: String,
    pub query: String,
    pub plan: Option<PlanNode>,
    pub estimated_cost: f64,
    pub actual_time: Option<Duration>,
    pub issues: Vec<QueryIssue>,
    pub suggestions: Vec<QuerySuggestion>,
}

/// Query issue
#[derive(Debug, Clone)]
pub struct QueryIssue {
    pub severity: IssueSeverity,
    pub category: IssueCategory,
    pub description: String,
    pub location: Option<String>,
}

/// Issue severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Issue category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueCategory {
    Performance,
    Correctness,
    Security,
    Style,
}

/// Query optimization suggestion
#[derive(Debug, Clone)]
pub struct QuerySuggestion {
    pub category: SuggestionCategory,
    pub description: String,
    pub suggested_change: Option<String>,
    pub estimated_improvement: Option<f64>,
}

/// Suggestion category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionCategory {
    AddIndex,
    RewriteQuery,
    UseExistingIndex,
    PartitionTable,
    Denormalize,
    AddMaterializedView,
    IncreaseWorkMem,
}

/// Query optimizer
#[derive(Debug)]
pub struct QueryOptimizer {
    _db_type: DatabaseType,
    table_stats: HashMap<String, TableStats>,
    index_info: HashMap<String, Vec<String>>,
}

/// Table statistics
#[derive(Debug, Clone)]
pub struct TableStats {
    pub table_name: String,
    pub row_count: u64,
    pub size_bytes: u64,
    pub last_analyzed: Option<SystemTime>,
    pub columns: HashMap<String, ColumnStats>,
}

/// Column statistics
#[derive(Debug, Clone)]
pub struct ColumnStats {
    pub distinct_values: u64,
    pub null_fraction: f32,
    pub avg_width: u32,
    pub most_common_values: Vec<String>,
}

impl QueryOptimizer {
    /// Create new optimizer
    pub fn new(db_type: DatabaseType) -> Self {
        Self {
            _db_type: db_type,
            table_stats: HashMap::new(),
            index_info: HashMap::new(),
        }
    }

    /// Add table statistics
    pub fn add_table_stats(&mut self, stats: TableStats) {
        self.table_stats.insert(stats.table_name.clone(), stats);
    }

    /// Add index info
    pub fn add_index(&mut self, table: impl Into<String>, index: impl Into<String>) {
        self.index_info
            .entry(table.into())
            .or_default()
            .push(index.into());
    }

    /// Analyze query
    pub fn analyze(&self, query: &str) -> QueryAnalysis {
        let mut issues = Vec::new();
        let mut suggestions = Vec::new();

        let query_upper = query.to_uppercase();

        // Check for SELECT *
        if query_upper.contains("SELECT *") {
            issues.push(QueryIssue {
                severity: IssueSeverity::Warning,
                category: IssueCategory::Performance,
                description: "SELECT * fetches all columns, consider selecting only needed columns"
                    .to_string(),
                location: Some("SELECT clause".to_string()),
            });
            suggestions.push(QuerySuggestion {
                category: SuggestionCategory::RewriteQuery,
                description: "Select only required columns".to_string(),
                suggested_change: None,
                estimated_improvement: Some(10.0),
            });
        }

        // Check for missing WHERE clause
        if (query_upper.contains("UPDATE") || query_upper.contains("DELETE"))
            && !query_upper.contains("WHERE")
        {
            issues.push(QueryIssue {
                severity: IssueSeverity::Critical,
                category: IssueCategory::Correctness,
                description: "UPDATE/DELETE without WHERE clause affects all rows".to_string(),
                location: None,
            });
        }

        // Check for LIKE with leading wildcard
        if query_upper.contains("LIKE '%") || query_upper.contains("LIKE \"%") {
            issues.push(QueryIssue {
                severity: IssueSeverity::Warning,
                category: IssueCategory::Performance,
                description: "LIKE with leading wildcard cannot use indexes".to_string(),
                location: Some("WHERE clause".to_string()),
            });
            suggestions.push(QuerySuggestion {
                category: SuggestionCategory::AddIndex,
                description: "Consider using full-text search or trigram index".to_string(),
                suggested_change: None,
                estimated_improvement: Some(50.0),
            });
        }

        // Check for functions on indexed columns
        if query_upper.contains("WHERE LOWER(") || query_upper.contains("WHERE UPPER(") {
            issues.push(QueryIssue {
                severity: IssueSeverity::Warning,
                category: IssueCategory::Performance,
                description: "Function on column prevents index usage".to_string(),
                location: Some("WHERE clause".to_string()),
            });
            suggestions.push(QuerySuggestion {
                category: SuggestionCategory::AddIndex,
                description: "Create functional index or use citext type".to_string(),
                suggested_change: Some("CREATE INDEX ON table (LOWER(column))".to_string()),
                estimated_improvement: Some(80.0),
            });
        }

        // Check for OR conditions
        if query_upper.contains(" OR ") && query_upper.contains("WHERE") {
            issues.push(QueryIssue {
                severity: IssueSeverity::Info,
                category: IssueCategory::Performance,
                description: "OR conditions may prevent optimal index usage".to_string(),
                location: Some("WHERE clause".to_string()),
            });
            suggestions.push(QuerySuggestion {
                category: SuggestionCategory::RewriteQuery,
                description: "Consider using UNION ALL instead of OR".to_string(),
                suggested_change: None,
                estimated_improvement: Some(30.0),
            });
        }

        QueryAnalysis {
            id: generate_query_id(),
            query: query.to_string(),
            plan: None,
            estimated_cost: 0.0,
            actual_time: None,
            issues,
            suggestions,
        }
    }

    /// Suggest indexes for query
    pub fn suggest_indexes(&self, query: &str) -> Vec<Index> {
        let mut suggestions = Vec::new();

        // Simple heuristic: find table and columns in WHERE clause
        let query_upper = query.to_uppercase();

        if let Some(where_pos) = query_upper.find("WHERE") {
            let where_clause = &query[where_pos..];

            // Extract potential column names (simplified)
            let words: Vec<&str> = where_clause.split_whitespace().collect();
            for word in words {
                let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                if !clean.is_empty()
                    && !["WHERE", "AND", "OR", "IN", "NOT", "NULL", "LIKE", "BETWEEN"]
                        .contains(&clean.to_uppercase().as_str())
                {
                    // This is a potential column name
                    if let Some(from_pos) = query_upper.find("FROM") {
                        let from_clause = &query[from_pos + 4..];
                        if let Some(table) = from_clause.split_whitespace().next() {
                            let table_clean =
                                table.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                            if !table_clean.is_empty() {
                                suggestions.push(Index::new(
                                    format!("idx_{}_{}", table_clean, clean.to_lowercase()),
                                    table_clean,
                                    vec![clean.to_lowercase()],
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        }

        suggestions
    }
}

// ============================================================================
// Data Pipeline
// ============================================================================

/// Data pipeline stage
#[derive(Debug, Clone)]
pub struct PipelineStage {
    pub name: String,
    pub stage_type: StageType,
    pub config: HashMap<String, String>,
    pub depends_on: Vec<String>,
    pub enabled: bool,
}

/// Stage type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageType {
    Extract,
    Transform,
    Load,
    Validate,
    Aggregate,
    Join,
    Filter,
    Deduplicate,
    Custom,
}

impl PipelineStage {
    /// Create new stage
    pub fn new(name: impl Into<String>, stage_type: StageType) -> Self {
        Self {
            name: name.into(),
            stage_type,
            config: HashMap::new(),
            depends_on: Vec::new(),
            enabled: true,
        }
    }

    /// Add configuration
    pub fn with_config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.insert(key.into(), value.into());
        self
    }

    /// Add dependency
    pub fn depends_on(mut self, stage: impl Into<String>) -> Self {
        self.depends_on.push(stage.into());
        self
    }

    /// Enable/disable
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Data pipeline
#[derive(Debug, Clone)]
pub struct DataPipeline {
    pub id: String,
    pub name: String,
    pub stages: Vec<PipelineStage>,
    pub schedule: Option<String>,
    pub created_at: SystemTime,
    pub last_run: Option<SystemTime>,
    pub status: PipelineStatus,
}

/// Pipeline status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStatus {
    Idle,
    Running,
    Succeeded,
    Failed,
    Paused,
}

impl DataPipeline {
    /// Create new pipeline
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: generate_pipeline_id(),
            name: name.into(),
            stages: Vec::new(),
            schedule: None,
            created_at: SystemTime::now(),
            last_run: None,
            status: PipelineStatus::Idle,
        }
    }

    /// Add stage
    pub fn add_stage(&mut self, stage: PipelineStage) {
        self.stages.push(stage);
    }

    /// Set schedule (cron expression)
    pub fn with_schedule(mut self, cron: impl Into<String>) -> Self {
        self.schedule = Some(cron.into());
        self
    }

    /// Get stages in execution order
    pub fn execution_order(&self) -> Vec<&PipelineStage> {
        let mut order = Vec::new();
        let mut remaining: Vec<_> = self.stages.iter().collect();
        let mut completed: std::collections::HashSet<&str> = std::collections::HashSet::new();

        while !remaining.is_empty() {
            let mut progress = false;
            remaining.retain(|stage| {
                let deps_met = stage
                    .depends_on
                    .iter()
                    .all(|d| completed.contains(d.as_str()));
                if deps_met {
                    order.push(*stage);
                    completed.insert(&stage.name);
                    progress = true;
                    false
                } else {
                    true
                }
            });

            if !progress && !remaining.is_empty() {
                // Circular dependency, add remaining
                order.append(&mut remaining);
            }
        }

        order
    }

    /// Validate pipeline
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.stages.is_empty() {
            errors.push("Pipeline has no stages".to_string());
        }

        let stage_names: std::collections::HashSet<_> =
            self.stages.iter().map(|s| &s.name).collect();

        for stage in &self.stages {
            for dep in &stage.depends_on {
                if !stage_names.contains(dep) {
                    errors.push(format!(
                        "Stage '{}' depends on unknown stage '{}'",
                        stage.name, dep
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Generate dbt-style YAML
    pub fn to_dbt_yaml(&self) -> String {
        let mut yaml = String::new();
        yaml.push_str("version: 2\n\n");
        yaml.push_str("models:\n");

        for stage in &self.stages {
            yaml.push_str(&format!("  - name: {}\n", stage.name));
            if !stage.depends_on.is_empty() {
                yaml.push_str("    depends_on:\n");
                for dep in &stage.depends_on {
                    yaml.push_str(&format!("      - ref('{}')\n", dep));
                }
            }
        }

        yaml
    }
}

// ============================================================================
// Database Refactoring
// ============================================================================

/// Refactoring operation
#[derive(Debug, Clone)]
pub enum RefactoringOperation {
    SplitTable {
        source_table: String,
        new_table: String,
        columns: Vec<String>,
        foreign_key: String,
    },
    MergeTable {
        source_tables: Vec<String>,
        target_table: String,
        join_columns: Vec<(String, String)>,
    },
    ExtractColumn {
        source_table: String,
        column: String,
        new_table: String,
        new_column: String,
    },
    RenameColumn {
        table: String,
        old_name: String,
        new_name: String,
    },
    ChangeColumnType {
        table: String,
        column: String,
        new_type: ColumnType,
    },
    AddNotNull {
        table: String,
        column: String,
        default_value: String,
    },
    Denormalize {
        source_table: String,
        target_table: String,
        columns: Vec<String>,
    },
}

/// Database refactoring planner
#[derive(Debug)]
pub struct RefactoringPlanner {
    _db_type: DatabaseType,
    operations: Vec<RefactoringOperation>,
}

impl RefactoringPlanner {
    /// Create new planner
    pub fn new(db_type: DatabaseType) -> Self {
        Self {
            _db_type: db_type,
            operations: Vec::new(),
        }
    }

    /// Add operation
    pub fn add_operation(&mut self, operation: RefactoringOperation) {
        self.operations.push(operation);
    }

    /// Generate migrations for all operations
    pub fn generate_migrations(&self) -> Vec<Migration> {
        self.operations
            .iter()
            .enumerate()
            .map(|(i, op)| self.generate_migration(op, i))
            .collect()
    }

    /// Generate migration for single operation
    fn generate_migration(&self, operation: &RefactoringOperation, index: usize) -> Migration {
        let version = format!("{}{:03}", chrono_version(), index);

        match operation {
            RefactoringOperation::SplitTable {
                source_table,
                new_table,
                columns,
                foreign_key,
            } => {
                let name = format!("split_{}_to_{}", source_table, new_table);
                let cols = columns.join(", ");

                let up_sql = format!(
                    "-- Create new table\n\
                     CREATE TABLE {} AS SELECT DISTINCT {} FROM {};\n\
                     ALTER TABLE {} ADD PRIMARY KEY ({});\n\
                     -- Add foreign key to source\n\
                     ALTER TABLE {} ADD COLUMN {}_id INTEGER REFERENCES {}({});\n\
                     -- Migrate data\n\
                     UPDATE {} SET {}_id = (SELECT id FROM {} WHERE {} = {}.{} LIMIT 1);\n\
                     -- Drop old columns\n\
                     ALTER TABLE {} DROP COLUMN {};",
                    new_table,
                    cols,
                    source_table,
                    new_table,
                    foreign_key,
                    source_table,
                    new_table,
                    new_table,
                    foreign_key,
                    source_table,
                    new_table,
                    new_table,
                    foreign_key,
                    source_table,
                    foreign_key,
                    source_table,
                    cols
                );

                Migration::new(version, name).up(up_sql)
            }
            RefactoringOperation::RenameColumn {
                table,
                old_name,
                new_name,
            } => {
                let name = format!("rename_{}_{}_to_{}", table, old_name, new_name);
                let up_sql = format!(
                    "ALTER TABLE {} RENAME COLUMN {} TO {};",
                    table, old_name, new_name
                );
                let down_sql = format!(
                    "ALTER TABLE {} RENAME COLUMN {} TO {};",
                    table, new_name, old_name
                );
                Migration::new(version, name).up(up_sql).down(down_sql)
            }
            RefactoringOperation::ChangeColumnType {
                table,
                column,
                new_type,
            } => {
                let name = format!("change_{}_{}_{}", table, column, new_type.to_postgres());
                let up_sql = format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE {} USING {}::{};",
                    table,
                    column,
                    new_type.to_postgres(),
                    column,
                    new_type.to_postgres()
                );
                Migration::new(version, name).up(up_sql)
            }
            RefactoringOperation::AddNotNull {
                table,
                column,
                default_value,
            } => {
                let name = format!("add_not_null_{}_{}", table, column);
                let up_sql = format!(
                    "-- Set default for existing nulls\n\
                     UPDATE {} SET {} = {} WHERE {} IS NULL;\n\
                     -- Add NOT NULL constraint\n\
                     ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;",
                    table, column, default_value, column, table, column
                );
                let down_sql = format!(
                    "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL;",
                    table, column
                );
                Migration::new(version, name).up(up_sql).down(down_sql)
            }
            _ => Migration::new(version, "placeholder").up("-- TODO"),
        }
    }

    /// Validate refactoring plan
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        for op in &self.operations {
            match op {
                RefactoringOperation::SplitTable { columns, .. } => {
                    if columns.is_empty() {
                        errors.push("SplitTable requires at least one column".to_string());
                    }
                }
                RefactoringOperation::RenameColumn {
                    old_name, new_name, ..
                } => {
                    if old_name == new_name {
                        errors.push("RenameColumn: old and new names are the same".to_string());
                    }
                }
                _ => {}
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_type() {
        assert_eq!(DatabaseType::PostgreSQL.name(), "PostgreSQL");
        assert!(DatabaseType::PostgreSQL.is_sql());
        assert!(!DatabaseType::MongoDB.is_sql());
        assert_eq!(DatabaseType::PostgreSQL.default_port(), 5432);
    }

    #[test]
    fn test_orm_type() {
        assert_eq!(ORMType::Diesel.name(), "Diesel");
        assert_eq!(ORMType::Prisma.migration_extension(), "prisma");
    }

    #[test]
    fn test_column_type_to_postgres() {
        assert_eq!(ColumnType::Integer.to_postgres(), "INTEGER");
        assert_eq!(ColumnType::Varchar(255).to_postgres(), "VARCHAR(255)");
        assert_eq!(
            ColumnType::Decimal {
                precision: 10,
                scale: 2
            }
            .to_postgres(),
            "DECIMAL(10, 2)"
        );
    }

    #[test]
    fn test_column_creation() {
        let column = Column::new("id", ColumnType::Serial)
            .primary_key()
            .not_null();

        assert_eq!(column.name, "id");
        assert!(column.primary_key);
        assert!(!column.nullable);
    }

    #[test]
    fn test_column_to_sql() {
        let column = Column::new("name", ColumnType::Varchar(100))
            .not_null()
            .with_default("'unknown'");

        let sql = column.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("VARCHAR(100)"));
        assert!(sql.contains("NOT NULL"));
        assert!(sql.contains("DEFAULT 'unknown'"));
    }

    #[test]
    fn test_column_with_reference() {
        let column = Column::new("user_id", ColumnType::Integer).references("users", "id");

        let sql = column.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("REFERENCES users(id)"));
    }

    #[test]
    fn test_index_creation() {
        let index = Index::new("idx_users_email", "users", vec!["email".to_string()])
            .unique()
            .with_where("deleted_at IS NULL");

        assert!(index.unique);
        assert!(index.where_clause.is_some());
    }

    #[test]
    fn test_index_to_sql() {
        let index = Index::new("idx_posts_user", "posts", vec!["user_id".to_string()])
            .with_type(IndexType::BTree);

        let sql = index.to_sql();
        assert!(sql.contains("CREATE INDEX"));
        assert!(sql.contains("idx_posts_user"));
        assert!(sql.contains("posts"));
    }

    #[test]
    fn test_table_creation() {
        let mut table = Table::new("users").in_schema("public");

        table.add_column(Column::new("id", ColumnType::Serial).primary_key());
        table.add_column(
            Column::new("email", ColumnType::Varchar(255))
                .not_null()
                .unique(),
        );

        assert_eq!(table.full_name(), "public.users");
        assert_eq!(table.columns.len(), 2);
    }

    #[test]
    fn test_table_to_sql() {
        let mut table = Table::new("users");
        table.add_column(Column::new("id", ColumnType::Serial).primary_key());
        table.add_column(Column::new("name", ColumnType::Text));

        let sql = table.to_sql(DatabaseType::PostgreSQL);
        assert!(sql.contains("CREATE TABLE users"));
        assert!(sql.contains("SERIAL"));
    }

    #[test]
    fn test_migration_creation() {
        let migration = Migration::new("20240101000000", "create_users")
            .up("CREATE TABLE users (id SERIAL PRIMARY KEY)")
            .down("DROP TABLE users");

        assert_eq!(migration.version, "20240101000000");
        assert!(migration.is_reversible());
        assert!(!migration.checksum.is_empty());
    }

    #[test]
    fn test_migration_status() {
        let mut migration = Migration::new("v1", "test");
        assert_eq!(migration.status, MigrationStatus::Pending);

        migration.mark_applied(Duration::from_secs(1));
        assert_eq!(migration.status, MigrationStatus::Applied);
    }

    #[test]
    fn test_migration_planner() {
        let mut planner = MigrationPlanner::new(DatabaseType::PostgreSQL);

        planner.add_migration(Migration::new("001", "first").up("SELECT 1"));
        planner.add_migration(Migration::new("002", "second").up("SELECT 2"));

        assert_eq!(planner.pending().len(), 2);

        planner.mark_applied("001");
        assert_eq!(planner.pending().len(), 1);
        assert_eq!(planner.applied().len(), 1);
    }

    #[test]
    fn test_migration_plan() {
        let mut planner = MigrationPlanner::new(DatabaseType::PostgreSQL);
        planner.add_migration(
            Migration::new("001", "test").up("ALTER TABLE users ADD COLUMN age INT"),
        );

        let plan = planner.plan();
        assert_eq!(plan.steps.len(), 1);
        assert!(plan.steps[0].requires_lock);
    }

    #[test]
    fn test_risk_assessment() {
        let planner = MigrationPlanner::new(DatabaseType::PostgreSQL);

        let low = planner.assess_risk("CREATE TABLE test (id INT)");
        assert_eq!(low, RiskLevel::Low);

        let high = planner.assess_risk("ALTER TABLE users DROP COLUMN email");
        assert_eq!(high, RiskLevel::High);

        let critical = planner.assess_risk("DROP TABLE users");
        assert_eq!(critical, RiskLevel::Critical);
    }

    #[test]
    fn test_generate_add_column() {
        let planner = MigrationPlanner::new(DatabaseType::PostgreSQL);
        let column = Column::new("age", ColumnType::Integer);

        let migration = planner.generate_add_column("users", &column);

        assert!(migration.up_sql.contains("ADD COLUMN"));
        assert!(migration.down_sql.contains("DROP COLUMN"));
    }

    #[test]
    fn test_generate_create_index() {
        let planner = MigrationPlanner::new(DatabaseType::PostgreSQL);
        let index = Index::new("idx_test", "users", vec!["email".to_string()]);

        let migration = planner.generate_create_index(&index);

        assert!(migration.up_sql.contains("CONCURRENTLY"));
    }

    #[test]
    fn test_query_analysis_select_star() {
        let optimizer = QueryOptimizer::new(DatabaseType::PostgreSQL);
        let analysis = optimizer.analyze("SELECT * FROM users");

        assert!(!analysis.issues.is_empty());
        assert!(analysis
            .issues
            .iter()
            .any(|i| i.description.contains("SELECT *")));
    }

    #[test]
    fn test_query_analysis_dangerous_update() {
        let optimizer = QueryOptimizer::new(DatabaseType::PostgreSQL);
        let analysis = optimizer.analyze("UPDATE users SET active = false");

        let critical = analysis
            .issues
            .iter()
            .find(|i| i.severity == IssueSeverity::Critical);
        assert!(critical.is_some());
    }

    #[test]
    fn test_query_analysis_like_wildcard() {
        let optimizer = QueryOptimizer::new(DatabaseType::PostgreSQL);
        let analysis = optimizer.analyze("SELECT * FROM users WHERE name LIKE '%john%'");

        assert!(analysis
            .issues
            .iter()
            .any(|i| i.description.contains("LIKE")));
        assert!(analysis
            .suggestions
            .iter()
            .any(|s| s.category == SuggestionCategory::AddIndex));
    }

    #[test]
    fn test_suggest_indexes() {
        let optimizer = QueryOptimizer::new(DatabaseType::PostgreSQL);
        let suggestions =
            optimizer.suggest_indexes("SELECT * FROM users WHERE email = 'test@example.com'");

        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_plan_node() {
        let mut node = PlanNode::new(PlanNodeType::SeqScan)
            .with_relation("users")
            .with_costs(0.0, 100.0)
            .with_rows(1000, 50);

        node.add_child(PlanNode::new(PlanNodeType::IndexScan));

        assert_eq!(node.children.len(), 1);
        assert!(node.node_type.is_slow());
    }

    #[test]
    fn test_find_slow_operations() {
        let mut root = PlanNode::new(PlanNodeType::Sort).with_rows(10000, 100);

        root.add_child(
            PlanNode::new(PlanNodeType::SeqScan)
                .with_relation("large_table")
                .with_rows(5000, 200),
        );

        let slow = root.find_slow_operations();
        assert_eq!(slow.len(), 2);
    }

    #[test]
    fn test_pipeline_stage() {
        let stage = PipelineStage::new("extract_users", StageType::Extract)
            .with_config("source", "postgresql://localhost/db")
            .depends_on("init");

        assert_eq!(stage.stage_type, StageType::Extract);
        assert!(!stage.depends_on.is_empty());
    }

    #[test]
    fn test_data_pipeline() {
        let mut pipeline = DataPipeline::new("etl_users");

        pipeline.add_stage(PipelineStage::new("extract", StageType::Extract));
        pipeline
            .add_stage(PipelineStage::new("transform", StageType::Transform).depends_on("extract"));
        pipeline.add_stage(PipelineStage::new("load", StageType::Load).depends_on("transform"));

        assert!(pipeline.validate().is_ok());
    }

    #[test]
    fn test_pipeline_execution_order() {
        let mut pipeline = DataPipeline::new("test");

        pipeline.add_stage(PipelineStage::new("c", StageType::Load).depends_on("b"));
        pipeline.add_stage(PipelineStage::new("a", StageType::Extract));
        pipeline.add_stage(PipelineStage::new("b", StageType::Transform).depends_on("a"));

        let order = pipeline.execution_order();
        let names: Vec<_> = order.iter().map(|s| &s.name).collect();

        let a_pos = names.iter().position(|n| *n == "a").unwrap();
        let b_pos = names.iter().position(|n| *n == "b").unwrap();
        let c_pos = names.iter().position(|n| *n == "c").unwrap();

        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn test_pipeline_validation_failure() {
        let mut pipeline = DataPipeline::new("test");
        pipeline
            .add_stage(PipelineStage::new("stage1", StageType::Extract).depends_on("nonexistent"));

        let result = pipeline.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_pipeline_to_dbt_yaml() {
        let mut pipeline = DataPipeline::new("test");
        pipeline.add_stage(PipelineStage::new("stg_users", StageType::Extract));
        pipeline.add_stage(
            PipelineStage::new("int_users", StageType::Transform).depends_on("stg_users"),
        );

        let yaml = pipeline.to_dbt_yaml();
        assert!(yaml.contains("version: 2"));
        assert!(yaml.contains("stg_users"));
    }

    #[test]
    fn test_refactoring_rename_column() {
        let mut planner = RefactoringPlanner::new(DatabaseType::PostgreSQL);
        planner.add_operation(RefactoringOperation::RenameColumn {
            table: "users".to_string(),
            old_name: "name".to_string(),
            new_name: "full_name".to_string(),
        });

        let migrations = planner.generate_migrations();
        assert_eq!(migrations.len(), 1);
        assert!(migrations[0].up_sql.contains("RENAME COLUMN"));
    }

    #[test]
    fn test_refactoring_add_not_null() {
        let mut planner = RefactoringPlanner::new(DatabaseType::PostgreSQL);
        planner.add_operation(RefactoringOperation::AddNotNull {
            table: "users".to_string(),
            column: "email".to_string(),
            default_value: "''".to_string(),
        });

        let migrations = planner.generate_migrations();
        assert!(migrations[0].up_sql.contains("UPDATE"));
        assert!(migrations[0].up_sql.contains("SET NOT NULL"));
    }

    #[test]
    fn test_refactoring_validation() {
        let mut planner = RefactoringPlanner::new(DatabaseType::PostgreSQL);
        planner.add_operation(RefactoringOperation::RenameColumn {
            table: "users".to_string(),
            old_name: "name".to_string(),
            new_name: "name".to_string(),
        });

        let result = planner.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_referential_action() {
        assert_eq!(ReferentialAction::Cascade.to_sql(), "CASCADE");
        assert_eq!(ReferentialAction::SetNull.to_sql(), "SET NULL");
    }

    #[test]
    fn test_database_supports_transactions() {
        assert!(DatabaseType::PostgreSQL.supports_transactions());
        assert!(DatabaseType::MySQL.supports_transactions());
        assert!(!DatabaseType::Redis.supports_transactions());
    }

    #[test]
    fn test_array_column_type() {
        let array_type = ColumnType::Array(Box::new(ColumnType::Integer));
        assert_eq!(array_type.to_postgres(), "INTEGER[]");
    }

    #[test]
    fn test_issue_severity_ordering() {
        assert!(IssueSeverity::Critical > IssueSeverity::Error);
        assert!(IssueSeverity::Error > IssueSeverity::Warning);
        assert!(IssueSeverity::Warning > IssueSeverity::Info);
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
    }
}
