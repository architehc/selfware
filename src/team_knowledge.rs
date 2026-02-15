//! Team Knowledge Sharing
//!
//! Provides shared memory and knowledge management across teams:
//! - Pattern libraries: shared code patterns
//! - Decision records: team decisions and rationale
//! - Onboarding context: documentation for new team members
//! - Expertise mapping: who knows what

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global counter for unique IDs
static PATTERN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static DECISION_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static CONTEXT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static EXPERT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_pattern_id() -> String {
    format!(
        "pat_{}_{:x}",
        PATTERN_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_decision_id() -> String {
    format!(
        "dec_{}_{:x}",
        DECISION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_context_id() -> String {
    format!(
        "ctx_{}_{:x}",
        CONTEXT_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_expert_id() -> String {
    format!(
        "exp_{}_{:x}",
        EXPERT_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================================
// Pattern Library
// ============================================================================

/// Pattern category
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PatternCategory {
    /// Creational design patterns
    Creational,
    /// Structural design patterns
    Structural,
    /// Behavioral design patterns
    Behavioral,
    /// Error handling patterns
    ErrorHandling,
    /// Concurrency patterns
    Concurrency,
    /// API design patterns
    ApiDesign,
    /// Testing patterns
    Testing,
    /// Custom category
    Custom(String),
}

impl PatternCategory {
    pub fn as_str(&self) -> &str {
        match self {
            PatternCategory::Creational => "creational",
            PatternCategory::Structural => "structural",
            PatternCategory::Behavioral => "behavioral",
            PatternCategory::ErrorHandling => "error_handling",
            PatternCategory::Concurrency => "concurrency",
            PatternCategory::ApiDesign => "api_design",
            PatternCategory::Testing => "testing",
            PatternCategory::Custom(s) => s.as_str(),
        }
    }
}

/// Programming language for a pattern
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Java,
    Other(String),
}

impl Language {
    pub fn as_str(&self) -> &str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::Go => "go",
            Language::Java => "java",
            Language::Other(s) => s.as_str(),
        }
    }
}

/// A shared code pattern
#[derive(Debug, Clone)]
pub struct CodePattern {
    /// Unique pattern ID
    pub id: String,
    /// Pattern name
    pub name: String,
    /// Pattern description
    pub description: String,
    /// Category
    pub category: PatternCategory,
    /// Programming language
    pub language: Language,
    /// Code template
    pub template: String,
    /// Example usage
    pub example: Option<String>,
    /// When to use this pattern
    pub when_to_use: Vec<String>,
    /// When NOT to use this pattern
    pub when_not_to_use: Vec<String>,
    /// Related patterns
    pub related_patterns: Vec<String>,
    /// Tags for search
    pub tags: HashSet<String>,
    /// Creator
    pub created_by: String,
    /// Creation timestamp
    pub created_at: u64,
    /// Last updated
    pub updated_at: u64,
    /// Usage count
    pub usage_count: u64,
    /// Rating (0-5)
    pub rating: f32,
    /// Number of ratings
    pub rating_count: u32,
}

impl CodePattern {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        category: PatternCategory,
        language: Language,
        template: impl Into<String>,
    ) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_pattern_id(),
            name: name.into(),
            description: description.into(),
            category,
            language,
            template: template.into(),
            example: None,
            when_to_use: Vec::new(),
            when_not_to_use: Vec::new(),
            related_patterns: Vec::new(),
            tags: HashSet::new(),
            created_by: String::new(),
            created_at: now,
            updated_at: now,
            usage_count: 0,
            rating: 0.0,
            rating_count: 0,
        }
    }

    /// Builder: set example
    pub fn with_example(mut self, example: impl Into<String>) -> Self {
        self.example = Some(example.into());
        self
    }

    /// Builder: add when to use
    pub fn when_to_use(mut self, reason: impl Into<String>) -> Self {
        self.when_to_use.push(reason.into());
        self
    }

    /// Builder: add when not to use
    pub fn when_not_to_use(mut self, reason: impl Into<String>) -> Self {
        self.when_not_to_use.push(reason.into());
        self
    }

    /// Builder: add related pattern
    pub fn related(mut self, pattern_name: impl Into<String>) -> Self {
        self.related_patterns.push(pattern_name.into());
        self
    }

    /// Builder: add tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Builder: set creator
    pub fn created_by(mut self, creator: impl Into<String>) -> Self {
        self.created_by = creator.into();
        self
    }

    /// Record usage
    pub fn record_usage(&mut self) {
        self.usage_count += 1;
        self.updated_at = current_timestamp();
    }

    /// Add rating
    pub fn add_rating(&mut self, rating: f32) {
        let clamped = rating.clamp(0.0, 5.0);
        let total = self.rating * self.rating_count as f32 + clamped;
        self.rating_count += 1;
        self.rating = total / self.rating_count as f32;
        self.updated_at = current_timestamp();
    }

    /// Check if pattern matches search query
    pub fn matches_query(&self, query: &str) -> bool {
        let query_lower = query.to_lowercase();
        self.name.to_lowercase().contains(&query_lower)
            || self.description.to_lowercase().contains(&query_lower)
            || self
                .tags
                .iter()
                .any(|t| t.to_lowercase().contains(&query_lower))
    }
}

/// Pattern library for a team
#[derive(Debug)]
pub struct PatternLibrary {
    /// All patterns
    patterns: HashMap<String, CodePattern>,
    /// Index by category
    by_category: HashMap<PatternCategory, HashSet<String>>,
    /// Index by language
    by_language: HashMap<Language, HashSet<String>>,
    /// Index by tag
    by_tag: HashMap<String, HashSet<String>>,
}

impl Default for PatternLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternLibrary {
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
            by_category: HashMap::new(),
            by_language: HashMap::new(),
            by_tag: HashMap::new(),
        }
    }

    /// Add a pattern
    pub fn add(&mut self, pattern: CodePattern) -> String {
        let id = pattern.id.clone();

        // Index by category
        self.by_category
            .entry(pattern.category.clone())
            .or_default()
            .insert(id.clone());

        // Index by language
        self.by_language
            .entry(pattern.language.clone())
            .or_default()
            .insert(id.clone());

        // Index by tags
        for tag in &pattern.tags {
            self.by_tag
                .entry(tag.clone())
                .or_default()
                .insert(id.clone());
        }

        self.patterns.insert(id.clone(), pattern);
        id
    }

    /// Get a pattern
    pub fn get(&self, id: &str) -> Option<&CodePattern> {
        self.patterns.get(id)
    }

    /// Get a pattern mutably
    pub fn get_mut(&mut self, id: &str) -> Option<&mut CodePattern> {
        self.patterns.get_mut(id)
    }

    /// Remove a pattern
    pub fn remove(&mut self, id: &str) -> Option<CodePattern> {
        if let Some(pattern) = self.patterns.remove(id) {
            // Remove from indices
            if let Some(set) = self.by_category.get_mut(&pattern.category) {
                set.remove(id);
            }
            if let Some(set) = self.by_language.get_mut(&pattern.language) {
                set.remove(id);
            }
            for tag in &pattern.tags {
                if let Some(set) = self.by_tag.get_mut(tag) {
                    set.remove(id);
                }
            }
            Some(pattern)
        } else {
            None
        }
    }

    /// Get patterns by category
    pub fn by_category(&self, category: &PatternCategory) -> Vec<&CodePattern> {
        self.by_category
            .get(category)
            .map(|ids| ids.iter().filter_map(|id| self.patterns.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get patterns by language
    pub fn by_language(&self, language: &Language) -> Vec<&CodePattern> {
        self.by_language
            .get(language)
            .map(|ids| ids.iter().filter_map(|id| self.patterns.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get patterns by tag
    pub fn by_tag(&self, tag: &str) -> Vec<&CodePattern> {
        self.by_tag
            .get(tag)
            .map(|ids| ids.iter().filter_map(|id| self.patterns.get(id)).collect())
            .unwrap_or_default()
    }

    /// Search patterns
    pub fn search(&self, query: &str) -> Vec<&CodePattern> {
        self.patterns
            .values()
            .filter(|p| p.matches_query(query))
            .collect()
    }

    /// Get most used patterns
    pub fn most_used(&self, limit: usize) -> Vec<&CodePattern> {
        let mut patterns: Vec<_> = self.patterns.values().collect();
        patterns.sort_by(|a, b| b.usage_count.cmp(&a.usage_count));
        patterns.truncate(limit);
        patterns
    }

    /// Get highest rated patterns
    pub fn highest_rated(&self, limit: usize) -> Vec<&CodePattern> {
        let mut patterns: Vec<_> = self
            .patterns
            .values()
            .filter(|p| p.rating_count > 0)
            .collect();
        patterns.sort_by(|a, b| {
            b.rating
                .partial_cmp(&a.rating)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        patterns.truncate(limit);
        patterns
    }

    /// Get all patterns
    pub fn all(&self) -> Vec<&CodePattern> {
        self.patterns.values().collect()
    }

    /// Get pattern count
    pub fn count(&self) -> usize {
        self.patterns.len()
    }
}

// ============================================================================
// Decision Records
// ============================================================================

/// Decision status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionStatus {
    /// Proposed, under discussion
    Proposed,
    /// Accepted and implemented
    Accepted,
    /// Deprecated, replaced by another decision
    Deprecated,
    /// Superseded by another decision
    Superseded(String),
    /// Rejected
    Rejected,
}

impl DecisionStatus {
    pub fn as_str(&self) -> &str {
        match self {
            DecisionStatus::Proposed => "proposed",
            DecisionStatus::Accepted => "accepted",
            DecisionStatus::Deprecated => "deprecated",
            DecisionStatus::Superseded(_) => "superseded",
            DecisionStatus::Rejected => "rejected",
        }
    }
}

/// A team decision record (similar to ADR)
#[derive(Debug, Clone)]
pub struct DecisionRecord {
    /// Unique decision ID
    pub id: String,
    /// Decision number (for ordering)
    pub number: u32,
    /// Decision title
    pub title: String,
    /// Status
    pub status: DecisionStatus,
    /// Context: what is the issue?
    pub context: String,
    /// Decision: what was decided
    pub decision: String,
    /// Consequences: what are the results
    pub consequences: Vec<String>,
    /// Alternatives considered
    pub alternatives: Vec<DecisionAlternative>,
    /// Stakeholders involved
    pub stakeholders: Vec<String>,
    /// Related decisions
    pub related_decisions: Vec<String>,
    /// Tags
    pub tags: HashSet<String>,
    /// Creation timestamp
    pub created_at: u64,
    /// Last updated
    pub updated_at: u64,
    /// Created by
    pub created_by: String,
    /// Discussion links
    pub discussion_links: Vec<String>,
}

/// An alternative considered for a decision
#[derive(Debug, Clone)]
pub struct DecisionAlternative {
    /// Alternative title
    pub title: String,
    /// Description
    pub description: String,
    /// Pros
    pub pros: Vec<String>,
    /// Cons
    pub cons: Vec<String>,
    /// Why it was not chosen
    pub rejection_reason: Option<String>,
}

impl DecisionAlternative {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            pros: Vec::new(),
            cons: Vec::new(),
            rejection_reason: None,
        }
    }

    /// Add a pro
    pub fn pro(mut self, pro: impl Into<String>) -> Self {
        self.pros.push(pro.into());
        self
    }

    /// Add a con
    pub fn con(mut self, con: impl Into<String>) -> Self {
        self.cons.push(con.into());
        self
    }

    /// Set rejection reason
    pub fn rejected_because(mut self, reason: impl Into<String>) -> Self {
        self.rejection_reason = Some(reason.into());
        self
    }
}

impl DecisionRecord {
    pub fn new(
        number: u32,
        title: impl Into<String>,
        context: impl Into<String>,
        decision: impl Into<String>,
    ) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_decision_id(),
            number,
            title: title.into(),
            status: DecisionStatus::Proposed,
            context: context.into(),
            decision: decision.into(),
            consequences: Vec::new(),
            alternatives: Vec::new(),
            stakeholders: Vec::new(),
            related_decisions: Vec::new(),
            tags: HashSet::new(),
            created_at: now,
            updated_at: now,
            created_by: String::new(),
            discussion_links: Vec::new(),
        }
    }

    /// Builder: set status
    pub fn with_status(mut self, status: DecisionStatus) -> Self {
        self.status = status;
        self
    }

    /// Builder: add consequence
    pub fn consequence(mut self, consequence: impl Into<String>) -> Self {
        self.consequences.push(consequence.into());
        self
    }

    /// Builder: add alternative
    pub fn alternative(mut self, alt: DecisionAlternative) -> Self {
        self.alternatives.push(alt);
        self
    }

    /// Builder: add stakeholder
    pub fn stakeholder(mut self, stakeholder: impl Into<String>) -> Self {
        self.stakeholders.push(stakeholder.into());
        self
    }

    /// Builder: add related decision
    pub fn related(mut self, decision_id: impl Into<String>) -> Self {
        self.related_decisions.push(decision_id.into());
        self
    }

    /// Builder: add tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Builder: set creator
    pub fn created_by(mut self, creator: impl Into<String>) -> Self {
        self.created_by = creator.into();
        self
    }

    /// Builder: add discussion link
    pub fn discussion_link(mut self, link: impl Into<String>) -> Self {
        self.discussion_links.push(link.into());
        self
    }

    /// Accept the decision
    pub fn accept(&mut self) {
        self.status = DecisionStatus::Accepted;
        self.updated_at = current_timestamp();
    }

    /// Reject the decision
    pub fn reject(&mut self) {
        self.status = DecisionStatus::Rejected;
        self.updated_at = current_timestamp();
    }

    /// Deprecate the decision
    pub fn deprecate(&mut self) {
        self.status = DecisionStatus::Deprecated;
        self.updated_at = current_timestamp();
    }

    /// Supersede with another decision
    pub fn supersede_with(&mut self, new_decision_id: impl Into<String>) {
        self.status = DecisionStatus::Superseded(new_decision_id.into());
        self.updated_at = current_timestamp();
    }

    /// Check if decision is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, DecisionStatus::Accepted)
    }

    /// Check if decision matches search query
    pub fn matches_query(&self, query: &str) -> bool {
        let query_lower = query.to_lowercase();
        self.title.to_lowercase().contains(&query_lower)
            || self.context.to_lowercase().contains(&query_lower)
            || self.decision.to_lowercase().contains(&query_lower)
            || self
                .tags
                .iter()
                .any(|t| t.to_lowercase().contains(&query_lower))
    }
}

/// Decision record manager
#[derive(Debug, Default)]
pub struct DecisionManager {
    /// All decisions
    decisions: HashMap<String, DecisionRecord>,
    /// Index by number
    by_number: HashMap<u32, String>,
    /// Next decision number
    next_number: u32,
}

impl DecisionManager {
    pub fn new() -> Self {
        Self {
            decisions: HashMap::new(),
            by_number: HashMap::new(),
            next_number: 1,
        }
    }

    /// Get next decision number
    pub fn next_number(&self) -> u32 {
        self.next_number
    }

    /// Add a decision
    pub fn add(&mut self, decision: DecisionRecord) -> String {
        let id = decision.id.clone();
        let number = decision.number;

        self.by_number.insert(number, id.clone());
        if number >= self.next_number {
            self.next_number = number + 1;
        }

        self.decisions.insert(id.clone(), decision);
        id
    }

    /// Get a decision by ID
    pub fn get(&self, id: &str) -> Option<&DecisionRecord> {
        self.decisions.get(id)
    }

    /// Get a decision by number
    pub fn get_by_number(&self, number: u32) -> Option<&DecisionRecord> {
        self.by_number
            .get(&number)
            .and_then(|id| self.decisions.get(id))
    }

    /// Get a decision mutably
    pub fn get_mut(&mut self, id: &str) -> Option<&mut DecisionRecord> {
        self.decisions.get_mut(id)
    }

    /// Get active decisions
    pub fn active(&self) -> Vec<&DecisionRecord> {
        self.decisions.values().filter(|d| d.is_active()).collect()
    }

    /// Get all decisions sorted by number
    pub fn all_sorted(&self) -> Vec<&DecisionRecord> {
        let mut decisions: Vec<_> = self.decisions.values().collect();
        decisions.sort_by_key(|d| d.number);
        decisions
    }

    /// Search decisions
    pub fn search(&self, query: &str) -> Vec<&DecisionRecord> {
        self.decisions
            .values()
            .filter(|d| d.matches_query(query))
            .collect()
    }

    /// Get decisions by status
    pub fn by_status(&self, status: &DecisionStatus) -> Vec<&DecisionRecord> {
        self.decisions
            .values()
            .filter(|d| &d.status == status)
            .collect()
    }

    /// Get recent decisions
    pub fn recent(&self, limit: usize) -> Vec<&DecisionRecord> {
        let mut decisions: Vec<_> = self.decisions.values().collect();
        decisions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        decisions.truncate(limit);
        decisions
    }

    /// Get decision count
    pub fn count(&self) -> usize {
        self.decisions.len()
    }
}

// ============================================================================
// Onboarding Context
// ============================================================================

/// Onboarding context type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContextType {
    /// Project overview
    ProjectOverview,
    /// Architecture documentation
    Architecture,
    /// Development setup
    DevSetup,
    /// Coding standards
    CodingStandards,
    /// Testing practices
    Testing,
    /// Deployment procedures
    Deployment,
    /// Team processes
    Processes,
    /// FAQ
    Faq,
    /// Custom type
    Custom(String),
}

impl ContextType {
    pub fn as_str(&self) -> &str {
        match self {
            ContextType::ProjectOverview => "project_overview",
            ContextType::Architecture => "architecture",
            ContextType::DevSetup => "dev_setup",
            ContextType::CodingStandards => "coding_standards",
            ContextType::Testing => "testing",
            ContextType::Deployment => "deployment",
            ContextType::Processes => "processes",
            ContextType::Faq => "faq",
            ContextType::Custom(s) => s.as_str(),
        }
    }
}

/// Onboarding context section
#[derive(Debug, Clone)]
pub struct OnboardingContext {
    /// Unique context ID
    pub id: String,
    /// Section title
    pub title: String,
    /// Context type
    pub context_type: ContextType,
    /// Content (markdown)
    pub content: String,
    /// Order within type
    pub order: u32,
    /// Prerequisites (context IDs that should be read first)
    pub prerequisites: Vec<String>,
    /// Estimated read time in minutes
    pub estimated_minutes: u32,
    /// Tags
    pub tags: HashSet<String>,
    /// Last updated
    pub updated_at: u64,
    /// Updated by
    pub updated_by: String,
    /// Is this essential for all new team members?
    pub is_essential: bool,
}

impl OnboardingContext {
    pub fn new(
        title: impl Into<String>,
        context_type: ContextType,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: generate_context_id(),
            title: title.into(),
            context_type,
            content: content.into(),
            order: 0,
            prerequisites: Vec::new(),
            estimated_minutes: 5,
            tags: HashSet::new(),
            updated_at: current_timestamp(),
            updated_by: String::new(),
            is_essential: false,
        }
    }

    /// Builder: set order
    pub fn with_order(mut self, order: u32) -> Self {
        self.order = order;
        self
    }

    /// Builder: add prerequisite
    pub fn prerequisite(mut self, context_id: impl Into<String>) -> Self {
        self.prerequisites.push(context_id.into());
        self
    }

    /// Builder: set estimated read time
    pub fn estimated_minutes(mut self, minutes: u32) -> Self {
        self.estimated_minutes = minutes;
        self
    }

    /// Builder: add tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Builder: set as essential
    pub fn essential(mut self) -> Self {
        self.is_essential = true;
        self
    }

    /// Builder: set updated by
    pub fn updated_by(mut self, user: impl Into<String>) -> Self {
        self.updated_by = user.into();
        self
    }

    /// Update content
    pub fn update_content(&mut self, content: impl Into<String>, updated_by: impl Into<String>) {
        self.content = content.into();
        self.updated_by = updated_by.into();
        self.updated_at = current_timestamp();
    }
}

/// Onboarding manager
#[derive(Debug, Default)]
pub struct OnboardingManager {
    /// All context sections
    contexts: HashMap<String, OnboardingContext>,
    /// Index by type
    by_type: HashMap<ContextType, Vec<String>>,
}

impl OnboardingManager {
    pub fn new() -> Self {
        Self {
            contexts: HashMap::new(),
            by_type: HashMap::new(),
        }
    }

    /// Add a context section
    pub fn add(&mut self, context: OnboardingContext) -> String {
        let id = context.id.clone();
        let context_type = context.context_type.clone();

        self.by_type
            .entry(context_type)
            .or_default()
            .push(id.clone());

        self.contexts.insert(id.clone(), context);
        id
    }

    /// Get a context section
    pub fn get(&self, id: &str) -> Option<&OnboardingContext> {
        self.contexts.get(id)
    }

    /// Get a context section mutably
    pub fn get_mut(&mut self, id: &str) -> Option<&mut OnboardingContext> {
        self.contexts.get_mut(id)
    }

    /// Get contexts by type, sorted by order
    pub fn by_type(&self, context_type: &ContextType) -> Vec<&OnboardingContext> {
        let mut contexts: Vec<_> = self
            .by_type
            .get(context_type)
            .map(|ids| ids.iter().filter_map(|id| self.contexts.get(id)).collect())
            .unwrap_or_default();
        contexts.sort_by_key(|c| c.order);
        contexts
    }

    /// Get essential contexts
    pub fn essential(&self) -> Vec<&OnboardingContext> {
        self.contexts.values().filter(|c| c.is_essential).collect()
    }

    /// Get onboarding path (ordered list for new members)
    pub fn onboarding_path(&self) -> Vec<&OnboardingContext> {
        let mut essential: Vec<_> = self.essential();
        essential.sort_by_key(|c| c.order);
        essential
    }

    /// Get total estimated read time
    pub fn total_read_time(&self) -> u32 {
        self.contexts.values().map(|c| c.estimated_minutes).sum()
    }

    /// Get essential read time
    pub fn essential_read_time(&self) -> u32 {
        self.essential().iter().map(|c| c.estimated_minutes).sum()
    }

    /// Get context count
    pub fn count(&self) -> usize {
        self.contexts.len()
    }
}

// ============================================================================
// Expertise Mapping
// ============================================================================

/// Expertise level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExpertiseLevel {
    /// Beginner: learning the topic
    Beginner,
    /// Intermediate: can work independently
    Intermediate,
    /// Advanced: can mentor others
    Advanced,
    /// Expert: deep knowledge, go-to person
    Expert,
}

impl ExpertiseLevel {
    pub fn as_str(&self) -> &str {
        match self {
            ExpertiseLevel::Beginner => "beginner",
            ExpertiseLevel::Intermediate => "intermediate",
            ExpertiseLevel::Advanced => "advanced",
            ExpertiseLevel::Expert => "expert",
        }
    }

    pub fn score(&self) -> u32 {
        match self {
            ExpertiseLevel::Beginner => 1,
            ExpertiseLevel::Intermediate => 2,
            ExpertiseLevel::Advanced => 3,
            ExpertiseLevel::Expert => 4,
        }
    }
}

/// Expertise entry for a team member
#[derive(Debug, Clone)]
pub struct ExpertiseEntry {
    /// Topic/skill
    pub topic: String,
    /// Expertise level
    pub level: ExpertiseLevel,
    /// Description of experience
    pub description: Option<String>,
    /// Years of experience
    pub years: Option<f32>,
    /// Last updated
    pub updated_at: u64,
}

impl ExpertiseEntry {
    pub fn new(topic: impl Into<String>, level: ExpertiseLevel) -> Self {
        Self {
            topic: topic.into(),
            level,
            description: None,
            years: None,
            updated_at: current_timestamp(),
        }
    }

    /// Builder: set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Builder: set years
    pub fn with_years(mut self, years: f32) -> Self {
        self.years = Some(years);
        self
    }
}

/// A team member's expertise profile
#[derive(Debug, Clone)]
pub struct ExpertProfile {
    /// Unique profile ID
    pub id: String,
    /// Team member name
    pub name: String,
    /// Email
    pub email: Option<String>,
    /// Role/title
    pub role: Option<String>,
    /// Expertise entries
    pub expertise: Vec<ExpertiseEntry>,
    /// Available for mentoring
    pub available_for_mentoring: bool,
    /// Preferred contact method
    pub preferred_contact: Option<String>,
    /// Timezone
    pub timezone: Option<String>,
    /// Last updated
    pub updated_at: u64,
}

impl ExpertProfile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: generate_expert_id(),
            name: name.into(),
            email: None,
            role: None,
            expertise: Vec::new(),
            available_for_mentoring: false,
            preferred_contact: None,
            timezone: None,
            updated_at: current_timestamp(),
        }
    }

    /// Builder: set email
    pub fn email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Builder: set role
    pub fn role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    /// Builder: add expertise
    pub fn expertise(mut self, entry: ExpertiseEntry) -> Self {
        self.expertise.push(entry);
        self
    }

    /// Builder: available for mentoring
    pub fn mentoring(mut self) -> Self {
        self.available_for_mentoring = true;
        self
    }

    /// Builder: set preferred contact
    pub fn preferred_contact(mut self, contact: impl Into<String>) -> Self {
        self.preferred_contact = Some(contact.into());
        self
    }

    /// Builder: set timezone
    pub fn timezone(mut self, tz: impl Into<String>) -> Self {
        self.timezone = Some(tz.into());
        self
    }

    /// Get expertise for a topic
    pub fn expertise_for(&self, topic: &str) -> Option<&ExpertiseEntry> {
        let topic_lower = topic.to_lowercase();
        self.expertise
            .iter()
            .find(|e| e.topic.to_lowercase() == topic_lower)
    }

    /// Get highest expertise level
    pub fn highest_level(&self) -> Option<ExpertiseLevel> {
        self.expertise.iter().map(|e| e.level).max()
    }

    /// Get expert topics (advanced or expert level)
    pub fn expert_topics(&self) -> Vec<&str> {
        self.expertise
            .iter()
            .filter(|e| e.level >= ExpertiseLevel::Advanced)
            .map(|e| e.topic.as_str())
            .collect()
    }

    /// Calculate total expertise score
    pub fn total_score(&self) -> u32 {
        self.expertise.iter().map(|e| e.level.score()).sum()
    }
}

/// Expertise mapper for a team
#[derive(Debug, Default)]
pub struct ExpertiseMapper {
    /// All profiles
    profiles: HashMap<String, ExpertProfile>,
    /// Index by topic
    by_topic: HashMap<String, Vec<String>>,
    /// Index by name (lowercase)
    by_name: HashMap<String, String>,
}

impl ExpertiseMapper {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
            by_topic: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    /// Add a profile
    pub fn add(&mut self, profile: ExpertProfile) -> String {
        let id = profile.id.clone();

        // Index by name
        self.by_name.insert(profile.name.to_lowercase(), id.clone());

        // Index by topics
        for entry in &profile.expertise {
            self.by_topic
                .entry(entry.topic.to_lowercase())
                .or_default()
                .push(id.clone());
        }

        self.profiles.insert(id.clone(), profile);
        id
    }

    /// Get a profile by ID
    pub fn get(&self, id: &str) -> Option<&ExpertProfile> {
        self.profiles.get(id)
    }

    /// Get a profile by name
    pub fn get_by_name(&self, name: &str) -> Option<&ExpertProfile> {
        self.by_name
            .get(&name.to_lowercase())
            .and_then(|id| self.profiles.get(id))
    }

    /// Get a profile mutably
    pub fn get_mut(&mut self, id: &str) -> Option<&mut ExpertProfile> {
        self.profiles.get_mut(id)
    }

    /// Find experts for a topic
    pub fn find_experts(&self, topic: &str) -> Vec<(&ExpertProfile, &ExpertiseEntry)> {
        let topic_lower = topic.to_lowercase();

        let mut experts: Vec<_> = self
            .profiles
            .values()
            .filter_map(|p| {
                p.expertise
                    .iter()
                    .find(|e| e.topic.to_lowercase() == topic_lower)
                    .map(|e| (p, e))
            })
            .collect();

        // Sort by expertise level (highest first)
        experts.sort_by(|a, b| b.1.level.cmp(&a.1.level));
        experts
    }

    /// Find mentors for a topic
    pub fn find_mentors(&self, topic: &str) -> Vec<&ExpertProfile> {
        self.find_experts(topic)
            .into_iter()
            .filter(|(p, e)| p.available_for_mentoring && e.level >= ExpertiseLevel::Advanced)
            .map(|(p, _)| p)
            .collect()
    }

    /// Get all topics
    pub fn all_topics(&self) -> Vec<&str> {
        self.by_topic.keys().map(|s| s.as_str()).collect()
    }

    /// Get coverage for a topic (number of experts)
    pub fn topic_coverage(&self, topic: &str) -> usize {
        self.by_topic
            .get(&topic.to_lowercase())
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Find gaps (topics with no experts)
    pub fn find_gaps<'a>(&self, required_topics: &[&'a str]) -> Vec<&'a str> {
        required_topics
            .iter()
            .filter(|t| self.topic_coverage(t) == 0)
            .copied()
            .collect()
    }

    /// Get expertise matrix
    pub fn expertise_matrix(&self) -> HashMap<String, HashMap<String, ExpertiseLevel>> {
        let mut matrix: HashMap<String, HashMap<String, ExpertiseLevel>> = HashMap::new();

        for profile in self.profiles.values() {
            let mut topics = HashMap::new();
            for entry in &profile.expertise {
                topics.insert(entry.topic.clone(), entry.level);
            }
            matrix.insert(profile.name.clone(), topics);
        }

        matrix
    }

    /// Get profile count
    pub fn count(&self) -> usize {
        self.profiles.len()
    }
}

// ============================================================================
// Team Knowledge Hub
// ============================================================================

/// Unified team knowledge hub
#[derive(Debug, Default)]
pub struct TeamKnowledgeHub {
    /// Pattern library
    pub patterns: PatternLibrary,
    /// Decision records
    pub decisions: DecisionManager,
    /// Onboarding context
    pub onboarding: OnboardingManager,
    /// Expertise mapping
    pub expertise: ExpertiseMapper,
}

impl TeamKnowledgeHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Search across all knowledge types
    #[allow(mismatched_lifetime_syntaxes)]
    pub fn search(&self, query: &str) -> TeamSearchResults {
        TeamSearchResults {
            patterns: self.patterns.search(query),
            decisions: self.decisions.search(query),
            onboarding: Vec::new(), // Add search to onboarding if needed
            experts: Vec::new(),    // Add expert search if needed
        }
    }

    /// Get statistics
    pub fn stats(&self) -> TeamKnowledgeStats {
        TeamKnowledgeStats {
            pattern_count: self.patterns.count(),
            decision_count: self.decisions.count(),
            onboarding_count: self.onboarding.count(),
            expert_count: self.expertise.count(),
            active_decisions: self.decisions.active().len(),
            essential_onboarding_count: self.onboarding.essential().len(),
            total_read_time_minutes: self.onboarding.total_read_time(),
        }
    }
}

/// Search results across all knowledge types
pub struct TeamSearchResults<'a> {
    pub patterns: Vec<&'a CodePattern>,
    pub decisions: Vec<&'a DecisionRecord>,
    pub onboarding: Vec<&'a OnboardingContext>,
    pub experts: Vec<&'a ExpertProfile>,
}

/// Team knowledge statistics
#[derive(Debug)]
pub struct TeamKnowledgeStats {
    pub pattern_count: usize,
    pub decision_count: usize,
    pub onboarding_count: usize,
    pub expert_count: usize,
    pub active_decisions: usize,
    pub essential_onboarding_count: usize,
    pub total_read_time_minutes: u32,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Pattern Library Tests

    #[test]
    fn test_code_pattern_creation() {
        let pattern = CodePattern::new(
            "Builder Pattern",
            "Create complex objects step by step",
            PatternCategory::Creational,
            Language::Rust,
            "struct Builder { ... }",
        )
        .with_example("let obj = Builder::new().field(value).build();")
        .when_to_use("Many optional parameters")
        .when_not_to_use("Simple object construction")
        .tag("design-pattern")
        .created_by("alice");

        assert!(!pattern.id.is_empty());
        assert_eq!(pattern.name, "Builder Pattern");
        assert_eq!(pattern.category, PatternCategory::Creational);
        assert_eq!(pattern.language, Language::Rust);
        assert!(pattern.example.is_some());
        assert_eq!(pattern.when_to_use.len(), 1);
        assert_eq!(pattern.when_not_to_use.len(), 1);
        assert!(pattern.tags.contains("design-pattern"));
        assert_eq!(pattern.created_by, "alice");
    }

    #[test]
    fn test_pattern_usage_tracking() {
        let mut pattern = CodePattern::new(
            "Singleton",
            "Single instance",
            PatternCategory::Creational,
            Language::Rust,
            "static INSTANCE: ...",
        );

        assert_eq!(pattern.usage_count, 0);
        pattern.record_usage();
        assert_eq!(pattern.usage_count, 1);
        pattern.record_usage();
        assert_eq!(pattern.usage_count, 2);
    }

    #[test]
    fn test_pattern_rating() {
        let mut pattern = CodePattern::new(
            "Test Pattern",
            "Test",
            PatternCategory::Testing,
            Language::Rust,
            "...",
        );

        assert_eq!(pattern.rating, 0.0);
        assert_eq!(pattern.rating_count, 0);

        pattern.add_rating(4.0);
        assert_eq!(pattern.rating, 4.0);
        assert_eq!(pattern.rating_count, 1);

        pattern.add_rating(5.0);
        assert_eq!(pattern.rating, 4.5);
        assert_eq!(pattern.rating_count, 2);

        // Test clamping
        pattern.add_rating(10.0); // Should be clamped to 5.0
        assert!(pattern.rating <= 5.0);
    }

    #[test]
    fn test_pattern_library_add_and_get() {
        let mut library = PatternLibrary::new();

        let pattern = CodePattern::new(
            "Factory Pattern",
            "Create objects without specifying exact class",
            PatternCategory::Creational,
            Language::Rust,
            "fn create() -> Box<dyn Trait>",
        );

        let id = library.add(pattern);
        assert_eq!(library.count(), 1);

        let retrieved = library.get(&id).unwrap();
        assert_eq!(retrieved.name, "Factory Pattern");
    }

    #[test]
    fn test_pattern_library_by_category() {
        let mut library = PatternLibrary::new();

        library.add(CodePattern::new(
            "Builder",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        ));
        library.add(CodePattern::new(
            "Singleton",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        ));
        library.add(CodePattern::new(
            "Observer",
            "...",
            PatternCategory::Behavioral,
            Language::Rust,
            "...",
        ));

        let creational = library.by_category(&PatternCategory::Creational);
        assert_eq!(creational.len(), 2);

        let behavioral = library.by_category(&PatternCategory::Behavioral);
        assert_eq!(behavioral.len(), 1);
    }

    #[test]
    fn test_pattern_library_by_language() {
        let mut library = PatternLibrary::new();

        library.add(CodePattern::new(
            "Rust Pattern",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        ));
        library.add(CodePattern::new(
            "Python Pattern",
            "...",
            PatternCategory::Creational,
            Language::Python,
            "...",
        ));

        let rust = library.by_language(&Language::Rust);
        assert_eq!(rust.len(), 1);
        assert_eq!(rust[0].name, "Rust Pattern");
    }

    #[test]
    fn test_pattern_library_search() {
        let mut library = PatternLibrary::new();

        library.add(
            CodePattern::new(
                "Error Handler",
                "Handle errors gracefully",
                PatternCategory::ErrorHandling,
                Language::Rust,
                "...",
            )
            .tag("error"),
        );
        library.add(CodePattern::new(
            "Builder",
            "Build objects",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        ));

        let results = library.search("error");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Error Handler");
    }

    #[test]
    fn test_pattern_library_most_used() {
        let mut library = PatternLibrary::new();

        let id1 = library.add(CodePattern::new(
            "Pattern 1",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        ));
        let id2 = library.add(CodePattern::new(
            "Pattern 2",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        ));

        // Record usage
        library.get_mut(&id1).unwrap().record_usage();
        library.get_mut(&id2).unwrap().record_usage();
        library.get_mut(&id2).unwrap().record_usage();
        library.get_mut(&id2).unwrap().record_usage();

        let most_used = library.most_used(1);
        assert_eq!(most_used.len(), 1);
        assert_eq!(most_used[0].name, "Pattern 2");
    }

    // Decision Record Tests

    #[test]
    fn test_decision_record_creation() {
        let decision = DecisionRecord::new(
            1,
            "Use Rust for Backend",
            "Need a systems language",
            "We will use Rust",
        )
        .consequence("Type safety")
        .consequence("Learning curve")
        .stakeholder("alice")
        .tag("language")
        .created_by("bob");

        assert!(!decision.id.is_empty());
        assert_eq!(decision.number, 1);
        assert_eq!(decision.title, "Use Rust for Backend");
        assert_eq!(decision.consequences.len(), 2);
        assert_eq!(decision.stakeholders.len(), 1);
        assert!(decision.tags.contains("language"));
    }

    #[test]
    fn test_decision_alternative() {
        let alt = DecisionAlternative::new("Use Go", "Go is also a good choice")
            .pro("Simpler")
            .con("Less type safety")
            .rejected_because("Team expertise in Rust");

        assert_eq!(alt.title, "Use Go");
        assert_eq!(alt.pros.len(), 1);
        assert_eq!(alt.cons.len(), 1);
        assert!(alt.rejection_reason.is_some());
    }

    #[test]
    fn test_decision_status_transitions() {
        let mut decision = DecisionRecord::new(1, "Test", "Context", "Decision");

        assert_eq!(decision.status, DecisionStatus::Proposed);
        assert!(!decision.is_active());

        decision.accept();
        assert_eq!(decision.status, DecisionStatus::Accepted);
        assert!(decision.is_active());

        decision.deprecate();
        assert_eq!(decision.status, DecisionStatus::Deprecated);
        assert!(!decision.is_active());
    }

    #[test]
    fn test_decision_supersede() {
        let mut decision = DecisionRecord::new(1, "Old Decision", "...", "...");
        decision.accept();

        decision.supersede_with("new-decision-id");
        assert!(matches!(decision.status, DecisionStatus::Superseded(_)));
        assert!(!decision.is_active());
    }

    #[test]
    fn test_decision_manager_add_and_get() {
        let mut manager = DecisionManager::new();

        let decision = DecisionRecord::new(1, "Decision 1", "Context", "Decision");
        let id = manager.add(decision);

        assert_eq!(manager.count(), 1);
        assert!(manager.get(&id).is_some());
        assert!(manager.get_by_number(1).is_some());
    }

    #[test]
    fn test_decision_manager_auto_number() {
        let mut manager = DecisionManager::new();

        assert_eq!(manager.next_number(), 1);

        manager.add(DecisionRecord::new(1, "D1", "...", "..."));
        assert_eq!(manager.next_number(), 2);

        manager.add(DecisionRecord::new(5, "D5", "...", "..."));
        assert_eq!(manager.next_number(), 6);
    }

    #[test]
    fn test_decision_manager_active() {
        let mut manager = DecisionManager::new();

        let id1 = manager
            .add(DecisionRecord::new(1, "D1", "...", "...").with_status(DecisionStatus::Accepted));
        let _id2 = manager.add(DecisionRecord::new(2, "D2", "...", "..."));

        let active = manager.active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id1);
    }

    // Onboarding Tests

    #[test]
    fn test_onboarding_context_creation() {
        let context = OnboardingContext::new(
            "Getting Started",
            ContextType::DevSetup,
            "# Setup\n\nRun `cargo build`",
        )
        .with_order(1)
        .estimated_minutes(10)
        .tag("setup")
        .essential();

        assert!(!context.id.is_empty());
        assert_eq!(context.title, "Getting Started");
        assert_eq!(context.context_type, ContextType::DevSetup);
        assert_eq!(context.order, 1);
        assert_eq!(context.estimated_minutes, 10);
        assert!(context.is_essential);
    }

    #[test]
    fn test_onboarding_manager_add_and_get() {
        let mut manager = OnboardingManager::new();

        let context = OnboardingContext::new(
            "Architecture",
            ContextType::Architecture,
            "# Architecture\n...",
        );

        let id = manager.add(context);
        assert_eq!(manager.count(), 1);

        let retrieved = manager.get(&id).unwrap();
        assert_eq!(retrieved.title, "Architecture");
    }

    #[test]
    fn test_onboarding_manager_by_type() {
        let mut manager = OnboardingManager::new();

        manager.add(OnboardingContext::new("Setup 1", ContextType::DevSetup, "...").with_order(2));
        manager.add(OnboardingContext::new("Setup 2", ContextType::DevSetup, "...").with_order(1));
        manager.add(OnboardingContext::new(
            "Architecture",
            ContextType::Architecture,
            "...",
        ));

        let setup = manager.by_type(&ContextType::DevSetup);
        assert_eq!(setup.len(), 2);
        assert_eq!(setup[0].title, "Setup 2"); // Sorted by order
        assert_eq!(setup[1].title, "Setup 1");
    }

    #[test]
    fn test_onboarding_read_time() {
        let mut manager = OnboardingManager::new();

        manager.add(
            OnboardingContext::new("Doc 1", ContextType::ProjectOverview, "...")
                .estimated_minutes(5)
                .essential(),
        );
        manager.add(
            OnboardingContext::new("Doc 2", ContextType::Architecture, "...")
                .estimated_minutes(10)
                .essential(),
        );
        manager.add(
            OnboardingContext::new("Doc 3", ContextType::DevSetup, "...").estimated_minutes(15),
        );

        assert_eq!(manager.total_read_time(), 30);
        assert_eq!(manager.essential_read_time(), 15);
    }

    // Expertise Tests

    #[test]
    fn test_expertise_entry() {
        let entry = ExpertiseEntry::new("Rust", ExpertiseLevel::Expert)
            .with_description("5 years of Rust experience")
            .with_years(5.0);

        assert_eq!(entry.topic, "Rust");
        assert_eq!(entry.level, ExpertiseLevel::Expert);
        assert_eq!(entry.years, Some(5.0));
    }

    #[test]
    fn test_expertise_level_ordering() {
        assert!(ExpertiseLevel::Expert > ExpertiseLevel::Advanced);
        assert!(ExpertiseLevel::Advanced > ExpertiseLevel::Intermediate);
        assert!(ExpertiseLevel::Intermediate > ExpertiseLevel::Beginner);
    }

    #[test]
    fn test_expertise_level_score() {
        assert_eq!(ExpertiseLevel::Beginner.score(), 1);
        assert_eq!(ExpertiseLevel::Intermediate.score(), 2);
        assert_eq!(ExpertiseLevel::Advanced.score(), 3);
        assert_eq!(ExpertiseLevel::Expert.score(), 4);
    }

    #[test]
    fn test_expert_profile_creation() {
        let profile = ExpertProfile::new("Alice")
            .email("alice@example.com")
            .role("Senior Engineer")
            .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert))
            .expertise(ExpertiseEntry::new("Python", ExpertiseLevel::Intermediate))
            .mentoring()
            .timezone("UTC");

        assert!(!profile.id.is_empty());
        assert_eq!(profile.name, "Alice");
        assert_eq!(profile.expertise.len(), 2);
        assert!(profile.available_for_mentoring);
    }

    #[test]
    fn test_expert_profile_expertise_for() {
        let profile = ExpertProfile::new("Bob")
            .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert))
            .expertise(ExpertiseEntry::new("Go", ExpertiseLevel::Beginner));

        let rust = profile.expertise_for("rust").unwrap();
        assert_eq!(rust.level, ExpertiseLevel::Expert);

        let go = profile.expertise_for("GO").unwrap(); // Case insensitive
        assert_eq!(go.level, ExpertiseLevel::Beginner);

        assert!(profile.expertise_for("Python").is_none());
    }

    #[test]
    fn test_expert_profile_expert_topics() {
        let profile = ExpertProfile::new("Carol")
            .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert))
            .expertise(ExpertiseEntry::new("Python", ExpertiseLevel::Advanced))
            .expertise(ExpertiseEntry::new("Go", ExpertiseLevel::Intermediate));

        let expert_topics = profile.expert_topics();
        assert_eq!(expert_topics.len(), 2);
        assert!(expert_topics.contains(&"Rust"));
        assert!(expert_topics.contains(&"Python"));
    }

    #[test]
    fn test_expert_profile_total_score() {
        let profile = ExpertProfile::new("Dave")
            .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert)) // 4
            .expertise(ExpertiseEntry::new("Go", ExpertiseLevel::Intermediate)); // 2

        assert_eq!(profile.total_score(), 6);
    }

    #[test]
    fn test_expertise_mapper_add_and_get() {
        let mut mapper = ExpertiseMapper::new();

        let profile = ExpertProfile::new("Alice")
            .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert));

        let id = mapper.add(profile);
        assert_eq!(mapper.count(), 1);

        let retrieved = mapper.get(&id).unwrap();
        assert_eq!(retrieved.name, "Alice");

        let by_name = mapper.get_by_name("alice").unwrap();
        assert_eq!(by_name.name, "Alice");
    }

    #[test]
    fn test_expertise_mapper_find_experts() {
        let mut mapper = ExpertiseMapper::new();

        mapper.add(
            ExpertProfile::new("Alice")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert)),
        );
        mapper.add(
            ExpertProfile::new("Bob")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Intermediate)),
        );
        mapper.add(
            ExpertProfile::new("Carol")
                .expertise(ExpertiseEntry::new("Python", ExpertiseLevel::Expert)),
        );

        let rust_experts = mapper.find_experts("rust");
        assert_eq!(rust_experts.len(), 2);
        assert_eq!(rust_experts[0].0.name, "Alice"); // Expert first
        assert_eq!(rust_experts[1].0.name, "Bob"); // Intermediate second
    }

    #[test]
    fn test_expertise_mapper_find_mentors() {
        let mut mapper = ExpertiseMapper::new();

        mapper.add(
            ExpertProfile::new("Alice")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert))
                .mentoring(),
        );
        mapper.add(
            ExpertProfile::new("Bob")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Advanced)), // Not mentoring
        );
        mapper.add(
            ExpertProfile::new("Carol")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Intermediate))
                .mentoring(), // Mentoring but not advanced
        );

        let mentors = mapper.find_mentors("rust");
        assert_eq!(mentors.len(), 1);
        assert_eq!(mentors[0].name, "Alice");
    }

    #[test]
    fn test_expertise_mapper_find_gaps() {
        let mut mapper = ExpertiseMapper::new();

        mapper.add(
            ExpertProfile::new("Alice")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert)),
        );

        let required = vec!["rust", "python", "go"];
        let gaps = mapper.find_gaps(&required);

        assert_eq!(gaps.len(), 2);
        assert!(gaps.contains(&"python"));
        assert!(gaps.contains(&"go"));
    }

    #[test]
    fn test_expertise_mapper_coverage() {
        let mut mapper = ExpertiseMapper::new();

        mapper.add(
            ExpertProfile::new("Alice")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert)),
        );
        mapper.add(
            ExpertProfile::new("Bob")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Intermediate)),
        );

        assert_eq!(mapper.topic_coverage("rust"), 2);
        assert_eq!(mapper.topic_coverage("python"), 0);
    }

    // Team Knowledge Hub Tests

    #[test]
    fn test_team_knowledge_hub_creation() {
        let hub = TeamKnowledgeHub::new();

        let stats = hub.stats();
        assert_eq!(stats.pattern_count, 0);
        assert_eq!(stats.decision_count, 0);
        assert_eq!(stats.onboarding_count, 0);
        assert_eq!(stats.expert_count, 0);
    }

    #[test]
    fn test_team_knowledge_hub_stats() {
        let mut hub = TeamKnowledgeHub::new();

        // Add some data
        hub.patterns.add(CodePattern::new(
            "Pattern",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        ));

        hub.decisions.add(
            DecisionRecord::new(1, "Decision", "...", "...").with_status(DecisionStatus::Accepted),
        );

        hub.onboarding.add(
            OnboardingContext::new("Doc", ContextType::ProjectOverview, "...")
                .essential()
                .estimated_minutes(10),
        );

        hub.expertise.add(ExpertProfile::new("Alice"));

        let stats = hub.stats();
        assert_eq!(stats.pattern_count, 1);
        assert_eq!(stats.decision_count, 1);
        assert_eq!(stats.onboarding_count, 1);
        assert_eq!(stats.expert_count, 1);
        assert_eq!(stats.active_decisions, 1);
        assert_eq!(stats.essential_onboarding_count, 1);
        assert_eq!(stats.total_read_time_minutes, 10);
    }

    #[test]
    fn test_team_knowledge_hub_search() {
        let mut hub = TeamKnowledgeHub::new();

        hub.patterns.add(
            CodePattern::new(
                "Error Handler",
                "Handle errors",
                PatternCategory::ErrorHandling,
                Language::Rust,
                "...",
            )
            .tag("error"),
        );

        hub.decisions
            .add(DecisionRecord::new(1, "Error Strategy", "...", "..."));

        let results = hub.search("error");
        assert_eq!(results.patterns.len(), 1);
        assert_eq!(results.decisions.len(), 1);
    }

    #[test]
    fn test_unique_pattern_ids() {
        let p1 = CodePattern::new(
            "P1",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        );
        let p2 = CodePattern::new(
            "P2",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        );

        assert_ne!(p1.id, p2.id);
    }

    #[test]
    fn test_unique_decision_ids() {
        let d1 = DecisionRecord::new(1, "D1", "...", "...");
        let d2 = DecisionRecord::new(2, "D2", "...", "...");

        assert_ne!(d1.id, d2.id);
    }

    #[test]
    fn test_unique_context_ids() {
        let c1 = OnboardingContext::new("C1", ContextType::DevSetup, "...");
        let c2 = OnboardingContext::new("C2", ContextType::DevSetup, "...");

        assert_ne!(c1.id, c2.id);
    }

    #[test]
    fn test_unique_expert_ids() {
        let e1 = ExpertProfile::new("Alice");
        let e2 = ExpertProfile::new("Bob");

        assert_ne!(e1.id, e2.id);
    }

    #[test]
    fn test_pattern_category_as_str() {
        assert_eq!(PatternCategory::Creational.as_str(), "creational");
        assert_eq!(PatternCategory::Behavioral.as_str(), "behavioral");
        assert_eq!(
            PatternCategory::Custom("custom".to_string()).as_str(),
            "custom"
        );
    }

    #[test]
    fn test_language_as_str() {
        assert_eq!(Language::Rust.as_str(), "rust");
        assert_eq!(Language::Python.as_str(), "python");
        assert_eq!(Language::Other("cpp".to_string()).as_str(), "cpp");
    }

    #[test]
    fn test_context_type_as_str() {
        assert_eq!(ContextType::DevSetup.as_str(), "dev_setup");
        assert_eq!(ContextType::Architecture.as_str(), "architecture");
        assert_eq!(ContextType::Custom("custom".to_string()).as_str(), "custom");
    }

    #[test]
    fn test_decision_status_as_str() {
        assert_eq!(DecisionStatus::Proposed.as_str(), "proposed");
        assert_eq!(DecisionStatus::Accepted.as_str(), "accepted");
        assert_eq!(
            DecisionStatus::Superseded("id".to_string()).as_str(),
            "superseded"
        );
    }

    #[test]
    fn test_expertise_level_as_str() {
        assert_eq!(ExpertiseLevel::Beginner.as_str(), "beginner");
        assert_eq!(ExpertiseLevel::Expert.as_str(), "expert");
    }

    #[test]
    fn test_pattern_matches_query() {
        let pattern = CodePattern::new(
            "Error Handler",
            "Handle runtime errors",
            PatternCategory::ErrorHandling,
            Language::Rust,
            "...",
        )
        .tag("error-handling");

        assert!(pattern.matches_query("error"));
        assert!(pattern.matches_query("ERROR")); // Case insensitive
        assert!(pattern.matches_query("runtime"));
        assert!(pattern.matches_query("handling"));
        assert!(!pattern.matches_query("testing"));
    }

    #[test]
    fn test_decision_matches_query() {
        let decision = DecisionRecord::new(
            1,
            "Use PostgreSQL",
            "Need a relational database",
            "We will use PostgreSQL",
        )
        .tag("database");

        assert!(decision.matches_query("postgres"));
        assert!(decision.matches_query("RELATIONAL")); // Case insensitive
        assert!(decision.matches_query("database"));
        assert!(!decision.matches_query("mongodb"));
    }

    #[test]
    fn test_onboarding_update_content() {
        let mut context = OnboardingContext::new("Setup", ContextType::DevSetup, "Old content");

        context.update_content("New content", "alice");

        assert_eq!(context.content, "New content");
        assert_eq!(context.updated_by, "alice");
    }

    #[test]
    fn test_pattern_library_remove() {
        let mut library = PatternLibrary::new();

        let pattern = CodePattern::new(
            "Pattern",
            "...",
            PatternCategory::Creational,
            Language::Rust,
            "...",
        )
        .tag("test");

        let id = library.add(pattern);
        assert_eq!(library.count(), 1);

        let removed = library.remove(&id);
        assert!(removed.is_some());
        assert_eq!(library.count(), 0);
        assert!(library.get(&id).is_none());
    }

    #[test]
    fn test_decision_manager_search() {
        let mut manager = DecisionManager::new();

        manager.add(DecisionRecord::new(1, "Use REST API", "...", "..."));
        manager.add(DecisionRecord::new(2, "Use GraphQL", "...", "..."));

        let results = manager.search("REST");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Use REST API");
    }

    #[test]
    fn test_decision_manager_by_status() {
        let mut manager = DecisionManager::new();

        manager
            .add(DecisionRecord::new(1, "D1", "...", "...").with_status(DecisionStatus::Accepted));
        manager.add(DecisionRecord::new(2, "D2", "...", "..."));

        let proposed = manager.by_status(&DecisionStatus::Proposed);
        assert_eq!(proposed.len(), 1);

        let accepted = manager.by_status(&DecisionStatus::Accepted);
        assert_eq!(accepted.len(), 1);
    }

    #[test]
    fn test_expertise_matrix() {
        let mut mapper = ExpertiseMapper::new();

        mapper.add(
            ExpertProfile::new("Alice")
                .expertise(ExpertiseEntry::new("Rust", ExpertiseLevel::Expert))
                .expertise(ExpertiseEntry::new("Python", ExpertiseLevel::Intermediate)),
        );

        let matrix = mapper.expertise_matrix();
        assert!(matrix.contains_key("Alice"));
        assert_eq!(
            matrix.get("Alice").unwrap().get("Rust"),
            Some(&ExpertiseLevel::Expert)
        );
    }
}
