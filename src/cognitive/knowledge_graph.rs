//! Codebase Knowledge Graph
//!
//! Entity relationships, cross-file references, semantic linking,
//! pattern recognition, and code smell detection.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Atomic counter for unique entity IDs
static ENTITY_COUNTER: AtomicU64 = AtomicU64::new(0);
static RELATION_COUNTER: AtomicU64 = AtomicU64::new(0);
static PATTERN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate unique entity ID
fn generate_entity_id() -> String {
    format!("entity-{}", ENTITY_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate unique relation ID
fn generate_relation_id() -> String {
    format!("rel-{}", RELATION_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate unique pattern ID
fn generate_pattern_id() -> String {
    format!("pattern-{}", PATTERN_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Entity type in the knowledge graph
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    /// A module/file
    Module,
    /// A function
    Function,
    /// A struct
    Struct,
    /// An enum
    Enum,
    /// A trait
    Trait,
    /// A constant
    Constant,
    /// A type alias
    TypeAlias,
    /// An implementation block
    Impl,
    /// A macro
    Macro,
    /// A method
    Method,
    /// A field
    Field,
    /// A variant
    Variant,
    /// A parameter
    Parameter,
    /// A local variable
    Variable,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityType::Module => write!(f, "Module"),
            EntityType::Function => write!(f, "Function"),
            EntityType::Struct => write!(f, "Struct"),
            EntityType::Enum => write!(f, "Enum"),
            EntityType::Trait => write!(f, "Trait"),
            EntityType::Constant => write!(f, "Constant"),
            EntityType::TypeAlias => write!(f, "TypeAlias"),
            EntityType::Impl => write!(f, "Impl"),
            EntityType::Macro => write!(f, "Macro"),
            EntityType::Method => write!(f, "Method"),
            EntityType::Field => write!(f, "Field"),
            EntityType::Variant => write!(f, "Variant"),
            EntityType::Parameter => write!(f, "Parameter"),
            EntityType::Variable => write!(f, "Variable"),
        }
    }
}

/// Relation type between entities
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationType {
    /// One entity calls another
    Calls,
    /// One entity is called by another
    CalledBy,
    /// One entity uses/references another
    Uses,
    /// One entity is used by another
    UsedBy,
    /// One entity extends/inherits another
    Extends,
    /// One entity is extended by another
    ExtendedBy,
    /// One entity implements another (trait implementation)
    Implements,
    /// One entity is implemented by another
    ImplementedBy,
    /// One entity contains another
    Contains,
    /// One entity is contained by another
    ContainedIn,
    /// One entity imports another
    Imports,
    /// One entity is imported by another
    ImportedBy,
    /// One entity depends on another
    DependsOn,
    /// One entity is a dependency of another
    DependencyOf,
    /// One entity is similar to another
    SimilarTo,
    /// One entity overrides another
    Overrides,
    /// One entity is overridden by another
    OverriddenBy,
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationType::Calls => write!(f, "calls"),
            RelationType::CalledBy => write!(f, "called_by"),
            RelationType::Uses => write!(f, "uses"),
            RelationType::UsedBy => write!(f, "used_by"),
            RelationType::Extends => write!(f, "extends"),
            RelationType::ExtendedBy => write!(f, "extended_by"),
            RelationType::Implements => write!(f, "implements"),
            RelationType::ImplementedBy => write!(f, "implemented_by"),
            RelationType::Contains => write!(f, "contains"),
            RelationType::ContainedIn => write!(f, "contained_in"),
            RelationType::Imports => write!(f, "imports"),
            RelationType::ImportedBy => write!(f, "imported_by"),
            RelationType::DependsOn => write!(f, "depends_on"),
            RelationType::DependencyOf => write!(f, "dependency_of"),
            RelationType::SimilarTo => write!(f, "similar_to"),
            RelationType::Overrides => write!(f, "overrides"),
            RelationType::OverriddenBy => write!(f, "overridden_by"),
        }
    }
}

impl RelationType {
    /// Get the inverse relation
    pub fn inverse(&self) -> Self {
        match self {
            RelationType::Calls => RelationType::CalledBy,
            RelationType::CalledBy => RelationType::Calls,
            RelationType::Uses => RelationType::UsedBy,
            RelationType::UsedBy => RelationType::Uses,
            RelationType::Extends => RelationType::ExtendedBy,
            RelationType::ExtendedBy => RelationType::Extends,
            RelationType::Implements => RelationType::ImplementedBy,
            RelationType::ImplementedBy => RelationType::Implements,
            RelationType::Contains => RelationType::ContainedIn,
            RelationType::ContainedIn => RelationType::Contains,
            RelationType::Imports => RelationType::ImportedBy,
            RelationType::ImportedBy => RelationType::Imports,
            RelationType::DependsOn => RelationType::DependencyOf,
            RelationType::DependencyOf => RelationType::DependsOn,
            RelationType::SimilarTo => RelationType::SimilarTo,
            RelationType::Overrides => RelationType::OverriddenBy,
            RelationType::OverriddenBy => RelationType::Overrides,
        }
    }
}

/// An entity in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier
    pub id: String,
    /// Entity name
    pub name: String,
    /// Entity type
    pub entity_type: EntityType,
    /// Fully qualified name
    pub qualified_name: String,
    /// Source file
    pub file: Option<PathBuf>,
    /// Line number
    pub line: Option<usize>,
    /// Column number
    pub column: Option<usize>,
    /// Visibility (public, private, etc.)
    pub visibility: Visibility,
    /// Documentation
    pub documentation: Option<String>,
    /// Signature (for functions/methods)
    pub signature: Option<String>,
    /// Semantic tags
    pub tags: Vec<String>,
    /// Custom attributes
    pub attributes: HashMap<String, String>,
}

impl Entity {
    /// Create a new entity
    pub fn new(name: impl Into<String>, entity_type: EntityType) -> Self {
        let name = name.into();
        Self {
            id: generate_entity_id(),
            qualified_name: name.clone(),
            name,
            entity_type,
            file: None,
            line: None,
            column: None,
            visibility: Visibility::Private,
            documentation: None,
            signature: None,
            tags: Vec::new(),
            attributes: HashMap::new(),
        }
    }

    /// Set qualified name
    pub fn with_qualified_name(mut self, qname: impl Into<String>) -> Self {
        self.qualified_name = qname.into();
        self
    }

    /// Set source location
    pub fn with_location(mut self, file: PathBuf, line: usize, column: usize) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    /// Set visibility
    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Set documentation
    pub fn with_documentation(mut self, doc: impl Into<String>) -> Self {
        self.documentation = Some(doc.into());
        self
    }

    /// Set signature
    pub fn with_signature(mut self, sig: impl Into<String>) -> Self {
        self.signature = Some(sig.into());
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add an attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// Visibility level
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    /// Public
    Public,
    /// Private
    Private,
    /// Crate-visible
    Crate,
    /// Pub(super)
    Super,
    /// Pub(in path)
    Restricted,
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Public => write!(f, "public"),
            Visibility::Private => write!(f, "private"),
            Visibility::Crate => write!(f, "crate"),
            Visibility::Super => write!(f, "super"),
            Visibility::Restricted => write!(f, "restricted"),
        }
    }
}

/// A relation between two entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// Unique identifier
    pub id: String,
    /// Source entity ID
    pub source_id: String,
    /// Target entity ID
    pub target_id: String,
    /// Relation type
    pub relation_type: RelationType,
    /// Relation strength (0.0-1.0)
    pub strength: f32,
    /// Source location where relation occurs
    pub location: Option<(PathBuf, usize)>,
    /// Additional context
    pub context: Option<String>,
}

impl Relation {
    /// Create a new relation
    pub fn new(
        source_id: impl Into<String>,
        target_id: impl Into<String>,
        relation_type: RelationType,
    ) -> Self {
        Self {
            id: generate_relation_id(),
            source_id: source_id.into(),
            target_id: target_id.into(),
            relation_type,
            strength: 1.0,
            location: None,
            context: None,
        }
    }

    /// Set strength
    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength.clamp(0.0, 1.0);
        self
    }

    /// Set location
    pub fn with_location(mut self, file: PathBuf, line: usize) -> Self {
        self.location = Some((file, line));
        self
    }

    /// Set context
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// A pattern detected in code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Unique identifier
    pub id: String,
    /// Pattern name
    pub name: String,
    /// Pattern type
    pub pattern_type: PatternType,
    /// Entities involved
    pub entities: Vec<String>,
    /// Confidence score (0.0-1.0)
    pub confidence: f32,
    /// Description
    pub description: String,
    /// Examples from codebase
    pub examples: Vec<PatternExample>,
    /// Creation timestamp for eviction ordering.
    #[serde(default)]
    pub created_at: u64,
}

impl Pattern {
    /// Create a new pattern
    pub fn new(name: impl Into<String>, pattern_type: PatternType) -> Self {
        Self {
            id: generate_pattern_id(),
            name: name.into(),
            pattern_type,
            entities: Vec::new(),
            confidence: 1.0,
            description: String::new(),
            examples: Vec::new(),
            created_at: KnowledgeGraph::now_secs(),
        }
    }


    /// Set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Add example
    pub fn with_example(mut self, example: PatternExample) -> Self {
        self.examples.push(example);
        self
    }
}

/// Pattern type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatternType {
    /// Design pattern (Singleton, Factory, etc.)
    DesignPattern,
    /// Architectural pattern (MVC, Repository, etc.)
    ArchitecturalPattern,
    /// Code idiom (Result handling, Iterator patterns)
    Idiom,
    /// Anti-pattern (code smell)
    AntiPattern,
    /// Custom pattern
    Custom,
}

impl std::fmt::Display for PatternType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternType::DesignPattern => write!(f, "Design Pattern"),
            PatternType::ArchitecturalPattern => write!(f, "Architectural Pattern"),
            PatternType::Idiom => write!(f, "Idiom"),
            PatternType::AntiPattern => write!(f, "Anti-Pattern"),
            PatternType::Custom => write!(f, "Custom"),
        }
    }
}

/// Example of a pattern in code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternExample {
    /// File path
    pub file: PathBuf,
    /// Start line
    pub start_line: usize,
    /// End line
    pub end_line: usize,
    /// Code snippet
    pub snippet: String,
}

impl PatternExample {
    /// Create a new pattern example
    pub fn new(
        file: PathBuf,
        start_line: usize,
        end_line: usize,
        snippet: impl Into<String>,
    ) -> Self {
        Self {
            file,
            start_line,
            end_line,
            snippet: snippet.into(),
        }
    }
}

/// Code smell type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeSmell {
    /// Large function/method
    LongMethod,
    /// Large class
    LargeClass,
    /// Too many parameters
    TooManyParameters,
    /// Deeply nested code
    DeeplyNested,
    /// Duplicated code
    DuplicatedCode,
    /// Dead code
    DeadCode,
    /// Unused import
    UnusedImport,
    /// Cyclic dependency
    CyclicDependency,
    /// Feature envy
    FeatureEnvy,
    /// Data clump
    DataClump,
    /// Primitive obsession
    PrimitiveObsession,
    /// God object
    GodObject,
    /// Shotgun surgery
    ShotgunSurgery,
    /// Inappropriate intimacy
    InappropriateIntimacy,
    /// Comments explaining code
    CommentedCode,
    /// Magic numbers
    MagicNumber,
    /// Hardcoded strings
    HardcodedString,
}

impl std::fmt::Display for CodeSmell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodeSmell::LongMethod => write!(f, "Long Method"),
            CodeSmell::LargeClass => write!(f, "Large Class"),
            CodeSmell::TooManyParameters => write!(f, "Too Many Parameters"),
            CodeSmell::DeeplyNested => write!(f, "Deeply Nested Code"),
            CodeSmell::DuplicatedCode => write!(f, "Duplicated Code"),
            CodeSmell::DeadCode => write!(f, "Dead Code"),
            CodeSmell::UnusedImport => write!(f, "Unused Import"),
            CodeSmell::CyclicDependency => write!(f, "Cyclic Dependency"),
            CodeSmell::FeatureEnvy => write!(f, "Feature Envy"),
            CodeSmell::DataClump => write!(f, "Data Clump"),
            CodeSmell::PrimitiveObsession => write!(f, "Primitive Obsession"),
            CodeSmell::GodObject => write!(f, "God Object"),
            CodeSmell::ShotgunSurgery => write!(f, "Shotgun Surgery"),
            CodeSmell::InappropriateIntimacy => write!(f, "Inappropriate Intimacy"),
            CodeSmell::CommentedCode => write!(f, "Commented Code"),
            CodeSmell::MagicNumber => write!(f, "Magic Number"),
            CodeSmell::HardcodedString => write!(f, "Hardcoded String"),
        }
    }
}

impl CodeSmell {
    /// Get severity of the code smell (1-5)
    pub fn severity(&self) -> u8 {
        match self {
            CodeSmell::GodObject => 5,
            CodeSmell::CyclicDependency => 5,
            CodeSmell::DuplicatedCode => 4,
            CodeSmell::LargeClass => 4,
            CodeSmell::LongMethod => 3,
            CodeSmell::TooManyParameters => 3,
            CodeSmell::DeeplyNested => 3,
            CodeSmell::FeatureEnvy => 3,
            CodeSmell::DeadCode => 2,
            CodeSmell::UnusedImport => 2,
            CodeSmell::MagicNumber => 2,
            CodeSmell::HardcodedString => 2,
            CodeSmell::DataClump => 3,
            CodeSmell::PrimitiveObsession => 3,
            CodeSmell::ShotgunSurgery => 4,
            CodeSmell::InappropriateIntimacy => 4,
            CodeSmell::CommentedCode => 1,
        }
    }

    /// Get suggested fix
    pub fn suggested_fix(&self) -> &'static str {
        match self {
            CodeSmell::LongMethod => "Extract smaller methods",
            CodeSmell::LargeClass => "Split into multiple classes",
            CodeSmell::TooManyParameters => "Use parameter object or builder",
            CodeSmell::DeeplyNested => "Flatten with early returns or extract methods",
            CodeSmell::DuplicatedCode => "Extract common code to shared function",
            CodeSmell::DeadCode => "Remove unused code",
            CodeSmell::UnusedImport => "Remove unused import",
            CodeSmell::CyclicDependency => "Restructure dependencies, use dependency inversion",
            CodeSmell::FeatureEnvy => "Move method to the class it uses most",
            CodeSmell::DataClump => "Extract data class",
            CodeSmell::PrimitiveObsession => "Create domain-specific types",
            CodeSmell::GodObject => "Split into smaller, focused classes",
            CodeSmell::ShotgunSurgery => "Consolidate related changes",
            CodeSmell::InappropriateIntimacy => "Use proper encapsulation",
            CodeSmell::CommentedCode => "Delete commented code, use version control",
            CodeSmell::MagicNumber => "Extract to named constant",
            CodeSmell::HardcodedString => "Extract to constant or config",
        }
    }
}

/// A detected code smell instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSmellInstance {
    /// Code smell type
    pub smell: CodeSmell,
    /// Entity with the smell
    pub entity_id: String,
    /// File path
    pub file: PathBuf,
    /// Line number
    pub line: usize,
    /// Description
    pub description: String,
    /// Severity (1-5)
    pub severity: u8,
    /// Suggested fix
    pub suggestion: String,
    /// Creation timestamp for eviction ordering.
    #[serde(default)]
    pub created_at: u64,
}

impl CodeSmellInstance {
    /// Create a new code smell instance
    pub fn new(smell: CodeSmell, entity_id: impl Into<String>, file: PathBuf, line: usize) -> Self {
        Self {
            smell: smell.clone(),
            entity_id: entity_id.into(),
            file,
            line,
            description: format!("{} detected", smell),
            severity: smell.severity(),
            suggestion: smell.suggested_fix().to_string(),
            created_at: KnowledgeGraph::now_secs(),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

/// Maximum number of entities before LRU eviction kicks in.
const MAX_GRAPH_ENTITIES: usize = 50_000;

/// Maximum number of patterns retained in the knowledge graph.
const MAX_PATTERNS: usize = 10_000;

/// Maximum number of code-smell instances retained in the knowledge graph.
const MAX_SMELLS: usize = 10_000;

/// The codebase knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    /// Entities by ID
    entities: HashMap<String, Entity>,
    /// Relations by ID
    relations: HashMap<String, Relation>,
    /// Patterns by ID
    patterns: HashMap<String, Pattern>,
    /// Code smells
    smells: Vec<CodeSmellInstance>,
    /// Entity index by name
    name_index: HashMap<String, HashSet<String>>,
    /// Entity index by type
    type_index: HashMap<EntityType, HashSet<String>>,
    /// Entity index by file
    file_index: HashMap<PathBuf, HashSet<String>>,
    /// Relations by source
    source_relations: HashMap<String, HashSet<String>>,
    /// Relations by target
    target_relations: HashMap<String, HashSet<String>>,
    /// Last-access timestamps for LRU eviction (entity ID -> epoch secs)
    access_times: HashMap<String, u64>,
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeGraph {
    /// Save the knowledge graph to a JSON file
    pub fn save_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load the knowledge graph from a JSON file
    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        if path.exists() {
            let json = std::fs::read_to_string(path)?;
            let graph = serde_json::from_str(&json)?;
            Ok(graph)
        } else {
            Ok(Self::new())
        }
    }

    /// Create a new knowledge graph
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relations: HashMap::new(),
            patterns: HashMap::new(),
            smells: Vec::new(),
            name_index: HashMap::new(),
            type_index: HashMap::new(),
            file_index: HashMap::new(),
            source_relations: HashMap::new(),
            target_relations: HashMap::new(),
            access_times: HashMap::new(),
        }
    }

    /// Get current epoch seconds for access tracking.
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Touch an entity to mark it as recently used.
    fn touch(&mut self, id: &str) {
        self.access_times.insert(id.to_string(), Self::now_secs());
    }

    /// Evict least-recently-used entities until we are under MAX_GRAPH_ENTITIES.
    fn evict_lru(&mut self) {
        if self.entities.len() <= MAX_GRAPH_ENTITIES {
            return;
        }

        let target = MAX_GRAPH_ENTITIES * 9 / 10; // Evict down to 90% capacity
        let to_remove = self.entities.len() - target;

        // Use partial sort (select_nth_unstable) which is O(N) instead of full
        // O(N log N) sort. This partitions so that entries[..to_remove] contains
        // the `to_remove` smallest (oldest) timestamps, which is all we need.
        let mut entries: Vec<_> = self
            .access_times
            .iter()
            .map(|(id, &ts)| (id.clone(), ts))
            .collect();

        if to_remove > 0 && to_remove <= entries.len() {
            entries.select_nth_unstable_by_key(to_remove - 1, |(_, ts)| *ts);
            for (id, _) in entries.into_iter().take(to_remove) {
                self.remove_entity_internal(&id);
            }
        }
    }

    /// Remove an entity and all associated relations and index entries.
    fn remove_entity_internal(&mut self, id: &str) {
        if let Some(entity) = self.entities.remove(id) {
            // Clean up name index
            if let Some(ids) = self.name_index.get_mut(&entity.name) {
                ids.remove(id);
                if ids.is_empty() {
                    self.name_index.remove(&entity.name);
                }
            }
            // Clean up type index
            if let Some(ids) = self.type_index.get_mut(&entity.entity_type) {
                ids.remove(id);
                if ids.is_empty() {
                    self.type_index.remove(&entity.entity_type);
                }
            }
            // Clean up file index
            if let Some(ref file) = entity.file {
                if let Some(ids) = self.file_index.get_mut(file) {
                    ids.remove(id);
                    if ids.is_empty() {
                        self.file_index.remove(file);
                    }
                }
            }
        }

        // Remove relations involving this entity
        let rel_ids_from: Vec<String> = self
            .source_relations
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let rel_ids_to: Vec<String> = self
            .target_relations
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .collect();

        for rel_id in rel_ids_from.iter().chain(rel_ids_to.iter()) {
            if let Some(rel) = self.relations.remove(rel_id) {
                if rel.source_id == id {
                    if let Some(ids) = self.target_relations.get_mut(&rel.target_id) {
                        ids.remove(rel_id);
                    }
                }
                if rel.target_id == id {
                    if let Some(ids) = self.source_relations.get_mut(&rel.source_id) {
                        ids.remove(rel_id);
                    }
                }
            }
        }

        self.access_times.remove(id);
    }

    /// Add an entity
    pub fn add_entity(&mut self, entity: Entity) -> String {
        let id = entity.id.clone();

        // Evict LRU entities if at capacity
        self.evict_lru();

        // Index by name
        self.name_index
            .entry(entity.name.clone())
            .or_default()
            .insert(id.clone());

        // Index by type
        self.type_index
            .entry(entity.entity_type.clone())
            .or_default()
            .insert(id.clone());

        // Index by file
        if let Some(ref file) = entity.file {
            self.file_index
                .entry(file.clone())
                .or_default()
                .insert(id.clone());
        }

        self.entities.insert(id.clone(), entity);
        self.touch(&id);
        id
    }

    /// Get an entity by ID
    pub fn get_entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Add a relation
    pub fn add_relation(&mut self, relation: Relation) -> String {
        let id = relation.id.clone();

        // Touch source and target entities (marks them as recently used)
        self.touch(&relation.source_id);
        self.touch(&relation.target_id);

        // Index by source
        self.source_relations
            .entry(relation.source_id.clone())
            .or_default()
            .insert(id.clone());

        // Index by target
        self.target_relations
            .entry(relation.target_id.clone())
            .or_default()
            .insert(id.clone());

        self.relations.insert(id.clone(), relation);
        id
    }

    /// Get a relation by ID
    pub fn get_relation(&self, id: &str) -> Option<&Relation> {
        self.relations.get(id)
    }

    /// Add a pattern, evicting the oldest patterns if the cap is exceeded.
    pub fn add_pattern(&mut self, pattern: Pattern) -> String {
        if self.patterns.len() >= MAX_PATTERNS {
            self.evict_oldest_patterns();
        }
        let id = pattern.id.clone();
        self.patterns.insert(id.clone(), pattern);
        id
    }

    /// Evict least-recently-created patterns down to 90% capacity.
    fn evict_oldest_patterns(&mut self) {
        let target = MAX_PATTERNS * 9 / 10;
        let to_remove = self.patterns.len().saturating_sub(target);
        if to_remove == 0 {
            return;
        }

        let mut entries: Vec<(String, u64)> = self
            .patterns
            .iter()
            .map(|(id, p)| (id.clone(), p.created_at))
            .collect();
        entries.select_nth_unstable_by_key(to_remove, |(_, ts)| *ts);

        for (id, _) in &entries[..to_remove] {
            self.patterns.remove(id);
        }
    }

    /// Get a pattern by ID
    pub fn get_pattern(&self, id: &str) -> Option<&Pattern> {
        self.patterns.get(id)
    }

    /// Add a code smell, deduplicating and capping the list.
    pub fn add_smell(&mut self, smell: CodeSmellInstance) {
        // Deduplicate by (file, smell type, line).
        if self
            .smells
            .iter()
            .any(|s| s.smell == smell.smell && s.file == smell.file && s.line == smell.line)
        {
            return;
        }

        if self.smells.len() >= MAX_SMELLS {
            let target = MAX_SMELLS * 9 / 10;
            let to_remove = self.smells.len().saturating_sub(target);
            self.smells.drain(0..to_remove);
        }

        self.smells.push(smell);
    }

    /// Get all code smells
    pub fn get_smells(&self) -> &[CodeSmellInstance] {
        &self.smells
    }

    /// Find entities by name
    pub fn find_by_name(&self, name: &str) -> Vec<&Entity> {
        self.name_index
            .get(name)
            .map(|ids| ids.iter().filter_map(|id| self.entities.get(id)).collect())
            .unwrap_or_default()
    }

    /// Find entities by type
    pub fn find_by_type(&self, entity_type: EntityType) -> Vec<&Entity> {
        self.type_index
            .get(&entity_type)
            .map(|ids| ids.iter().filter_map(|id| self.entities.get(id)).collect())
            .unwrap_or_default()
    }

    /// Find entities in a file
    pub fn find_in_file(&self, file: &Path) -> Vec<&Entity> {
        self.file_index
            .get(file)
            .map(|ids| ids.iter().filter_map(|id| self.entities.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all relations from an entity
    pub fn relations_from(&self, entity_id: &str) -> Vec<&Relation> {
        self.source_relations
            .get(entity_id)
            .map(|ids| ids.iter().filter_map(|id| self.relations.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all relations to an entity
    pub fn relations_to(&self, entity_id: &str) -> Vec<&Relation> {
        self.target_relations
            .get(entity_id)
            .map(|ids| ids.iter().filter_map(|id| self.relations.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get related entities
    pub fn get_related(
        &self,
        entity_id: &str,
        relation_type: Option<RelationType>,
    ) -> Vec<(&Entity, &Relation)> {
        let mut results = Vec::new();

        for rel in self.relations_from(entity_id) {
            if relation_type
                .clone()
                .is_none_or(|rt| rt == rel.relation_type)
            {
                if let Some(entity) = self.entities.get(&rel.target_id) {
                    results.push((entity, rel));
                }
            }
        }

        results
    }

    /// Get entities that reference this entity
    pub fn get_referencing(
        &self,
        entity_id: &str,
        relation_type: Option<RelationType>,
    ) -> Vec<(&Entity, &Relation)> {
        let mut results = Vec::new();

        for rel in self.relations_to(entity_id) {
            if relation_type
                .clone()
                .is_none_or(|rt| rt == rel.relation_type)
            {
                if let Some(entity) = self.entities.get(&rel.source_id) {
                    results.push((entity, rel));
                }
            }
        }

        results
    }

    /// Count entities by type
    pub fn entity_count_by_type(&self) -> HashMap<EntityType, usize> {
        self.type_index
            .iter()
            .map(|(t, ids)| (t.clone(), ids.len()))
            .collect()
    }

    /// Get total entity count
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Get total relation count
    pub fn relation_count(&self) -> usize {
        self.relations.len()
    }

    /// Get all patterns
    pub fn get_patterns(&self) -> Vec<&Pattern> {
        self.patterns.values().collect()
    }

    /// Find cyclic dependencies
    pub fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for entity_id in self.entities.keys() {
            if !visited.contains(entity_id) {
                self.find_cycles_dfs(
                    entity_id,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    /// DFS helper for cycle detection
    fn find_cycles_dfs(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        for rel in self.relations_from(node) {
            if rel.relation_type == RelationType::DependsOn
                || rel.relation_type == RelationType::Uses
            {
                let target = &rel.target_id;
                if !visited.contains(target) {
                    self.find_cycles_dfs(target, visited, rec_stack, path, cycles);
                } else if rec_stack.contains(target) {
                    // Found a cycle - target is guaranteed to be in path because:
                    // 1. rec_stack tracks nodes in the current DFS path
                    // 2. target was found in rec_stack, meaning it's an ancestor
                    // 3. path contains all ancestors from root to current node
                    // SAFETY: position() will always succeed since target is in rec_stack
                    // and rec_stack is always a subset of path during DFS
                    let cycle_start = path
                        .iter()
                        .position(|x| x == target)
                        .expect("target must be in path - this is guaranteed by DFS invariant");
                    cycles.push(path[cycle_start..].to_vec());
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }

    /// Export to DOT format
    pub fn to_dot(&self) -> String {
        let mut output = String::new();
        output.push_str("digraph KnowledgeGraph {\n");
        output.push_str("    rankdir=LR;\n");
        output.push_str("    node [shape=record];\n\n");

        // Nodes
        for entity in self.entities.values() {
            let color = match entity.entity_type {
                EntityType::Module => "lightblue",
                EntityType::Function | EntityType::Method => "lightgreen",
                EntityType::Struct | EntityType::Enum => "lightyellow",
                EntityType::Trait => "lightpink",
                _ => "white",
            };
            output.push_str(&format!(
                "    \"{}\" [label=\"{}: {}\" style=filled fillcolor={}];\n",
                entity.id, entity.entity_type, entity.name, color
            ));
        }

        output.push('\n');

        // Edges
        for rel in self.relations.values() {
            let style = match rel.relation_type {
                RelationType::Calls => "solid",
                RelationType::Uses => "dashed",
                RelationType::Implements => "bold",
                RelationType::Contains => "dotted",
                _ => "solid",
            };
            output.push_str(&format!(
                "    \"{}\" -> \"{}\" [label=\"{}\" style={}];\n",
                rel.source_id, rel.target_id, rel.relation_type, style
            ));
        }

        output.push_str("}\n");
        output
    }
}

/// Entity extractor for Rust code
#[derive(Debug)]
pub struct RustEntityExtractor {
    /// Current module path
    _module_path: Vec<String>,
}

impl Default for RustEntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl RustEntityExtractor {
    /// Create a new extractor
    pub fn new() -> Self {
        Self {
            _module_path: Vec::new(),
        }
    }

    /// Extract entities from Rust source code
    pub fn extract(&self, content: &str, file_path: &Path) -> Vec<Entity> {
        let mut entities = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            // Parse visibility
            let visibility = if trimmed.starts_with("pub ") {
                Visibility::Public
            } else if trimmed.starts_with("pub(crate)") {
                Visibility::Crate
            } else if trimmed.starts_with("pub(super)") {
                Visibility::Super
            } else {
                Visibility::Private
            };

            // Extract function
            if let Some(entity) =
                self.extract_function(trimmed, file_path, line_num + 1, visibility.clone())
            {
                entities.push(entity);
            }

            // Extract struct
            if let Some(entity) =
                self.extract_struct(trimmed, file_path, line_num + 1, visibility.clone())
            {
                entities.push(entity);
            }

            // Extract enum
            if let Some(entity) =
                self.extract_enum(trimmed, file_path, line_num + 1, visibility.clone())
            {
                entities.push(entity);
            }

            // Extract trait
            if let Some(entity) =
                self.extract_trait(trimmed, file_path, line_num + 1, visibility.clone())
            {
                entities.push(entity);
            }

            // Extract const
            if let Some(entity) = self.extract_const(trimmed, file_path, line_num + 1, visibility) {
                entities.push(entity);
            }
        }

        entities
    }

    /// Extract function
    fn extract_function(
        &self,
        line: &str,
        file: &Path,
        line_num: usize,
        visibility: Visibility,
    ) -> Option<Entity> {
        if !line.contains("fn ") {
            return None;
        }

        let fn_start = line.find("fn ")?;
        let rest = &line[fn_start + 3..];
        let name_end = rest.find(['<', '(', ' '])?;
        let name = &rest[..name_end];

        if name.is_empty() {
            return None;
        }

        Some(
            Entity::new(name, EntityType::Function)
                .with_location(file.to_path_buf(), line_num, fn_start + 1)
                .with_visibility(visibility)
                .with_signature(line.to_string()),
        )
    }

    /// Extract struct
    fn extract_struct(
        &self,
        line: &str,
        file: &Path,
        line_num: usize,
        visibility: Visibility,
    ) -> Option<Entity> {
        if !line.contains("struct ") {
            return None;
        }

        let struct_start = line.find("struct ")?;
        let rest = &line[struct_start + 7..];
        let name_end = rest.find(['<', '{', '(', ' ', ';'])?;
        let name = &rest[..name_end];

        if name.is_empty() {
            return None;
        }

        Some(
            Entity::new(name, EntityType::Struct)
                .with_location(file.to_path_buf(), line_num, struct_start + 1)
                .with_visibility(visibility),
        )
    }

    /// Extract enum
    fn extract_enum(
        &self,
        line: &str,
        file: &Path,
        line_num: usize,
        visibility: Visibility,
    ) -> Option<Entity> {
        if !line.contains("enum ") {
            return None;
        }

        let enum_start = line.find("enum ")?;
        let rest = &line[enum_start + 5..];
        let name_end = rest.find(['<', '{', ' '])?;
        let name = &rest[..name_end];

        if name.is_empty() {
            return None;
        }

        Some(
            Entity::new(name, EntityType::Enum)
                .with_location(file.to_path_buf(), line_num, enum_start + 1)
                .with_visibility(visibility),
        )
    }

    /// Extract trait
    fn extract_trait(
        &self,
        line: &str,
        file: &Path,
        line_num: usize,
        visibility: Visibility,
    ) -> Option<Entity> {
        if !line.contains("trait ") {
            return None;
        }

        let trait_start = line.find("trait ")?;
        let rest = &line[trait_start + 6..];
        let name_end = rest.find(['<', '{', ':', ' '])?;
        let name = &rest[..name_end];

        if name.is_empty() {
            return None;
        }

        Some(
            Entity::new(name, EntityType::Trait)
                .with_location(file.to_path_buf(), line_num, trait_start + 1)
                .with_visibility(visibility),
        )
    }

    /// Extract const
    fn extract_const(
        &self,
        line: &str,
        file: &Path,
        line_num: usize,
        visibility: Visibility,
    ) -> Option<Entity> {
        if !line.contains("const ") {
            return None;
        }

        let const_start = line.find("const ")?;
        let rest = &line[const_start + 6..];
        let name_end = rest.find(':')?;
        let name = &rest[..name_end].trim();

        if name.is_empty() {
            return None;
        }

        Some(
            Entity::new(*name, EntityType::Constant)
                .with_location(file.to_path_buf(), line_num, const_start + 1)
                .with_visibility(visibility),
        )
    }
}

/// Code smell detector
#[derive(Debug)]
pub struct SmellDetector {
    /// Max lines for a function before considered long
    max_function_lines: usize,
    /// Max lines for a file before considered large
    max_file_lines: usize,
    /// Max parameters before considered too many
    max_parameters: usize,
    /// Max nesting depth
    max_nesting: usize,
}

impl Default for SmellDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SmellDetector {
    /// Create new detector with defaults
    pub fn new() -> Self {
        Self {
            max_function_lines: 50,
            max_file_lines: 500,
            max_parameters: 5,
            max_nesting: 4,
        }
    }

    /// Set max function lines
    pub fn with_max_function_lines(mut self, max: usize) -> Self {
        self.max_function_lines = max;
        self
    }

    /// Set max file lines
    pub fn with_max_file_lines(mut self, max: usize) -> Self {
        self.max_file_lines = max;
        self
    }

    /// Set max parameters
    pub fn with_max_parameters(mut self, max: usize) -> Self {
        self.max_parameters = max;
        self
    }

    /// Set max nesting
    pub fn with_max_nesting(mut self, max: usize) -> Self {
        self.max_nesting = max;
        self
    }

    /// Detect smells in code
    pub fn detect(
        &self,
        content: &str,
        file_path: &Path,
        entity_id: &str,
    ) -> Vec<CodeSmellInstance> {
        let mut smells = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len();

        // Check file size
        if line_count > self.max_file_lines {
            smells.push(
                CodeSmellInstance::new(
                    CodeSmell::LargeClass,
                    entity_id,
                    file_path.to_path_buf(),
                    1,
                )
                .with_description(format!(
                    "File has {} lines (max: {})",
                    line_count, self.max_file_lines
                )),
            );
        }

        // Check for magic numbers
        for (line_num, line) in lines.iter().enumerate() {
            if self.has_magic_number(line) {
                smells.push(
                    CodeSmellInstance::new(
                        CodeSmell::MagicNumber,
                        entity_id,
                        file_path.to_path_buf(),
                        line_num + 1,
                    )
                    .with_description("Magic number detected"),
                );
            }
        }

        // Check for deeply nested code
        let mut max_depth = 0;
        let mut current_depth = 0;
        let mut deepest_line = 0;

        for (line_num, line) in lines.iter().enumerate() {
            current_depth += line.matches('{').count();
            current_depth = current_depth.saturating_sub(line.matches('}').count());

            if current_depth > max_depth {
                max_depth = current_depth;
                deepest_line = line_num + 1;
            }
        }

        if max_depth > self.max_nesting {
            smells.push(
                CodeSmellInstance::new(
                    CodeSmell::DeeplyNested,
                    entity_id,
                    file_path.to_path_buf(),
                    deepest_line,
                )
                .with_description(format!(
                    "Nesting depth: {} (max: {})",
                    max_depth, self.max_nesting
                )),
            );
        }

        // Check for commented code
        for (line_num, line) in lines.iter().enumerate() {
            if self.is_commented_code(line) {
                smells.push(
                    CodeSmellInstance::new(
                        CodeSmell::CommentedCode,
                        entity_id,
                        file_path.to_path_buf(),
                        line_num + 1,
                    )
                    .with_description("Commented out code detected"),
                );
            }
        }

        smells
    }

    /// Check for magic numbers in a line
    fn has_magic_number(&self, line: &str) -> bool {
        // Skip lines that are likely constants or typical patterns
        let trimmed = line.trim();
        if trimmed.starts_with("const ") || trimmed.starts_with("static ") {
            return false;
        }

        // Look for numeric literals that aren't 0, 1, 2
        let mut chars = trimmed.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_ascii_digit() {
                let mut num = String::new();
                num.push(c);
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() {
                        num.push(chars.next().expect("peek returned Some"));
                    } else {
                        break;
                    }
                }
                // Check if it's a magic number (not 0, 1, 2, or likely array index)
                if let Ok(n) = num.parse::<i32>() {
                    if n > 2 && n != 10 && n != 100 && n != 1000 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if line contains commented code
    fn is_commented_code(&self, line: &str) -> bool {
        let trimmed = line.trim();
        if !trimmed.starts_with("//") {
            return false;
        }

        let comment_content = trimmed.trim_start_matches('/').trim();

        // Heuristics for commented code
        comment_content.contains("fn ")
            || comment_content.contains("let ")
            || comment_content.contains("if ")
            || comment_content.ends_with(';')
            || comment_content.ends_with('{')
            || comment_content.contains("return ")
    }

    /// Detect function-specific smells
    pub fn detect_function_smells(
        &self,
        name: &str,
        param_count: usize,
        line_count: usize,
        file_path: &Path,
        start_line: usize,
        entity_id: &str,
    ) -> Vec<CodeSmellInstance> {
        let mut smells = Vec::new();

        // Too many parameters
        if param_count > self.max_parameters {
            smells.push(
                CodeSmellInstance::new(
                    CodeSmell::TooManyParameters,
                    entity_id,
                    file_path.to_path_buf(),
                    start_line,
                )
                .with_description(format!(
                    "Function '{}' has {} parameters (max: {})",
                    name, param_count, self.max_parameters
                )),
            );
        }

        // Long method
        if line_count > self.max_function_lines {
            smells.push(
                CodeSmellInstance::new(
                    CodeSmell::LongMethod,
                    entity_id,
                    file_path.to_path_buf(),
                    start_line,
                )
                .with_description(format!(
                    "Function '{}' has {} lines (max: {})",
                    name, line_count, self.max_function_lines
                )),
            );
        }

        smells
    }
}

/// Pattern recognizer
pub struct PatternRecognizer {
    /// Known pattern signatures
    pattern_signatures: Vec<(String, PatternType, Vec<String>)>,
}

impl Default for PatternRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternRecognizer {
    /// Create new recognizer with default patterns
    pub fn new() -> Self {
        let signatures = vec![
            // Singleton pattern
            (
                "Singleton".to_string(),
                PatternType::DesignPattern,
                vec![
                    "static".to_string(),
                    "instance".to_string(),
                    "get_instance".to_string(),
                ],
            ),
            // Builder pattern
            (
                "Builder".to_string(),
                PatternType::DesignPattern,
                vec![
                    "Builder".to_string(),
                    "build".to_string(),
                    "with_".to_string(),
                ],
            ),
            // Factory pattern
            (
                "Factory".to_string(),
                PatternType::DesignPattern,
                vec!["Factory".to_string(), "create".to_string()],
            ),
            // Repository pattern
            (
                "Repository".to_string(),
                PatternType::ArchitecturalPattern,
                vec![
                    "Repository".to_string(),
                    "find".to_string(),
                    "save".to_string(),
                ],
            ),
            // Observer pattern
            (
                "Observer".to_string(),
                PatternType::DesignPattern,
                vec![
                    "Observer".to_string(),
                    "subscribe".to_string(),
                    "notify".to_string(),
                ],
            ),
        ];

        Self {
            pattern_signatures: signatures,
        }
    }

    /// Add a pattern signature
    pub fn add_signature(
        &mut self,
        name: impl Into<String>,
        pattern_type: PatternType,
        keywords: Vec<String>,
    ) {
        self.pattern_signatures
            .push((name.into(), pattern_type, keywords));
    }

    /// Recognize patterns in code
    pub fn recognize(&self, content: &str) -> Vec<(String, PatternType, f32)> {
        let mut results = Vec::new();
        let lower_content = content.to_lowercase();

        for (name, pattern_type, keywords) in &self.pattern_signatures {
            let matches: Vec<_> = keywords
                .iter()
                .filter(|kw| lower_content.contains(&kw.to_lowercase()))
                .collect();

            if !matches.is_empty() {
                let confidence = matches.len() as f32 / keywords.len() as f32;
                if confidence >= 0.5 {
                    results.push((name.clone(), pattern_type.clone(), confidence));
                }
            }
        }

        results
    }
}

/// Semantic linker for cross-file references
pub struct SemanticLinker {
    /// Pre-compiled import regex patterns by language
    import_patterns: HashMap<String, Vec<regex::Regex>>,
}

impl std::fmt::Debug for SemanticLinker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let readable: HashMap<_, Vec<_>> = self
            .import_patterns
            .iter()
            .map(|(k, v)| (k.as_str(), v.iter().map(|r| r.as_str()).collect()))
            .collect();
        f.debug_struct("SemanticLinker")
            .field("import_patterns", &readable)
            .finish()
    }
}

impl Default for SemanticLinker {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticLinker {
    /// Compile a regex pattern. All patterns here are compile-time literals,
    /// so failure indicates a programming error.
    fn compile(pattern: &str) -> regex::Regex {
        regex::Regex::new(pattern).expect("invalid regex literal in SemanticLinker")
    }

    /// Create new linker with pre-compiled regex patterns
    pub fn new() -> Self {
        let mut patterns: HashMap<String, Vec<regex::Regex>> = HashMap::new();

        patterns.insert(
            "rust".to_string(),
            vec![
                Self::compile(r"use\s+(\w+(?:::\w+)*)"),
                Self::compile(r"crate::(\w+(?:::\w+)*)"),
                Self::compile(r"super::(\w+(?:::\w+)*)"),
            ],
        );

        patterns.insert(
            "javascript".to_string(),
            vec![
                Self::compile(r#"import\s+.*\s+from\s+['"](.+)['"]"#),
                Self::compile(r#"require\(['"](.+)['"]\)"#),
            ],
        );

        patterns.insert(
            "python".to_string(),
            vec![
                Self::compile(r"from\s+(\w+(?:\.\w+)*)\s+import"),
                Self::compile(r"import\s+(\w+(?:\.\w+)*)"),
            ],
        );

        Self {
            import_patterns: patterns,
        }
    }

    /// Extract imports from code using pre-compiled regex patterns
    pub fn extract_imports(&self, content: &str, language: &str) -> Vec<String> {
        let patterns = match self.import_patterns.get(language) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut imports = Vec::new();

        for re in patterns {
            for cap in re.captures_iter(content) {
                if let Some(m) = cap.get(1) {
                    imports.push(m.as_str().to_string());
                }
            }
        }

        imports
    }

    /// Create import relations
    pub fn link_imports(
        &self,
        source_entity_id: &str,
        imports: &[String],
        graph: &KnowledgeGraph,
    ) -> Vec<Relation> {
        let mut relations = Vec::new();

        for import in imports {
            // Find matching entities
            let name = import.split("::").last().unwrap_or(import);
            let targets = graph.find_by_name(name);

            for target in targets {
                relations.push(Relation::new(
                    source_entity_id,
                    &target.id,
                    RelationType::Imports,
                ));
            }
        }

        relations
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/knowledge_graph/knowledge_graph_test.rs"]
mod tests;
