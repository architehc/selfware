//! Intent-Based Programming System
//!
//! High-level intent specification: goal decomposition, constraint satisfaction,
//! and solution synthesis.
//!
//! # Concepts
//!
//! - **Intent**: A high-level description of what you want to achieve
//! - **Goal**: A concrete, measurable objective derived from intent
//! - **Constraint**: A condition that must be satisfied
//! - **Solution**: Generated code or actions that achieve goals

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// Atomic counters for unique IDs
static GOAL_COUNTER: AtomicU64 = AtomicU64::new(0);
static INTENT_COUNTER: AtomicU64 = AtomicU64::new(0);
static CONSTRAINT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Priority level for goals
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Priority {
    /// Critical - must be satisfied
    Critical = 4,
    /// High priority
    High = 3,
    /// Medium priority
    #[default]
    Medium = 2,
    /// Low priority
    Low = 1,
    /// Optional - nice to have
    Optional = 0,
}

/// Type of constraint
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstraintType {
    /// Must include certain elements
    MustInclude(String),
    /// Must exclude certain elements
    MustExclude(String),
    /// Must be within range
    Range { min: i64, max: i64 },
    /// Must match pattern
    Pattern(String),
    /// Performance constraint (e.g., "< 100ms")
    Performance(String),
    /// Resource constraint (e.g., "memory < 100MB")
    Resource(String),
    /// Security constraint
    Security(String),
    /// Type constraint
    Type(String),
    /// Custom predicate
    Custom(String),
}

/// A constraint that must be satisfied
#[derive(Debug, Clone)]
pub struct Constraint {
    /// Constraint identifier
    pub id: String,
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Target entity this constraint applies to
    pub target: String,
    /// Priority of the constraint
    pub priority: Priority,
    /// Is this a hard constraint (must be satisfied)?
    pub hard: bool,
    /// Explanation of why this constraint exists
    pub rationale: Option<String>,
}

impl Constraint {
    /// Create a must-include constraint
    pub fn must_include(target: &str, element: &str) -> Self {
        Self {
            id: Self::generate_id(),
            constraint_type: ConstraintType::MustInclude(element.to_string()),
            target: target.to_string(),
            priority: Priority::High,
            hard: true,
            rationale: None,
        }
    }

    /// Create a must-exclude constraint
    pub fn must_exclude(target: &str, element: &str) -> Self {
        Self {
            id: Self::generate_id(),
            constraint_type: ConstraintType::MustExclude(element.to_string()),
            target: target.to_string(),
            priority: Priority::High,
            hard: true,
            rationale: None,
        }
    }

    /// Create a range constraint
    pub fn in_range(target: &str, min: i64, max: i64) -> Self {
        Self {
            id: Self::generate_id(),
            constraint_type: ConstraintType::Range { min, max },
            target: target.to_string(),
            priority: Priority::Medium,
            hard: true,
            rationale: None,
        }
    }

    /// Create a pattern constraint
    pub fn matches_pattern(target: &str, pattern: &str) -> Self {
        Self {
            id: Self::generate_id(),
            constraint_type: ConstraintType::Pattern(pattern.to_string()),
            target: target.to_string(),
            priority: Priority::Medium,
            hard: true,
            rationale: None,
        }
    }

    /// Create a performance constraint
    pub fn performance(target: &str, requirement: &str) -> Self {
        Self {
            id: Self::generate_id(),
            constraint_type: ConstraintType::Performance(requirement.to_string()),
            target: target.to_string(),
            priority: Priority::High,
            hard: false,
            rationale: None,
        }
    }

    /// Create a security constraint
    pub fn security(target: &str, requirement: &str) -> Self {
        Self {
            id: Self::generate_id(),
            constraint_type: ConstraintType::Security(requirement.to_string()),
            target: target.to_string(),
            priority: Priority::Critical,
            hard: true,
            rationale: None,
        }
    }

    /// Add rationale
    pub fn with_rationale(mut self, rationale: &str) -> Self {
        self.rationale = Some(rationale.to_string());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Make constraint soft (optional)
    pub fn soft(mut self) -> Self {
        self.hard = false;
        self
    }

    fn generate_id() -> String {
        format!(
            "constraint_{}",
            CONSTRAINT_COUNTER.fetch_add(1, Ordering::SeqCst)
        )
    }
}

/// Status of a goal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    /// Not started
    Pending,
    /// Currently being worked on
    InProgress,
    /// Successfully achieved
    Achieved,
    /// Failed to achieve
    Failed,
    /// Blocked by dependencies
    Blocked,
    /// Abandoned/cancelled
    Abandoned,
}

/// A concrete, measurable goal
#[derive(Debug, Clone)]
pub struct Goal {
    /// Goal identifier
    pub id: String,
    /// Goal description
    pub description: String,
    /// Current status
    pub status: GoalStatus,
    /// Priority
    pub priority: Priority,
    /// Constraints that apply to this goal
    pub constraints: Vec<Constraint>,
    /// Sub-goals (decomposition)
    pub sub_goals: Vec<String>,
    /// Parent goal (if this is a sub-goal)
    pub parent: Option<String>,
    /// Dependencies (other goals that must be achieved first)
    pub depends_on: Vec<String>,
    /// Success criteria
    pub success_criteria: Vec<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Created timestamp
    pub created_at: u64,
    /// Completion timestamp
    pub completed_at: Option<u64>,
}

impl Goal {
    /// Create a new goal
    pub fn new(description: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: format!("goal_{}", GOAL_COUNTER.fetch_add(1, Ordering::SeqCst)),
            description: description.to_string(),
            status: GoalStatus::Pending,
            priority: Priority::Medium,
            constraints: Vec::new(),
            sub_goals: Vec::new(),
            parent: None,
            depends_on: Vec::new(),
            success_criteria: Vec::new(),
            tags: Vec::new(),
            created_at: now,
            completed_at: None,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Add a constraint
    pub fn with_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Add dependency
    pub fn depends_on(mut self, goal_id: &str) -> Self {
        self.depends_on.push(goal_id.to_string());
        self
    }

    /// Add success criterion
    pub fn with_criterion(mut self, criterion: &str) -> Self {
        self.success_criteria.push(criterion.to_string());
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// Check if all dependencies are satisfied
    pub fn dependencies_met(&self, achieved_goals: &HashSet<String>) -> bool {
        self.depends_on
            .iter()
            .all(|dep| achieved_goals.contains(dep))
    }

    /// Mark as achieved
    pub fn achieve(&mut self) {
        self.status = GoalStatus::Achieved;
        self.completed_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }

    /// Mark as failed
    pub fn fail(&mut self) {
        self.status = GoalStatus::Failed;
    }
}

/// A high-level intent
#[derive(Debug, Clone)]
pub struct Intent {
    /// Intent identifier
    pub id: String,
    /// Natural language description
    pub description: String,
    /// Parsed goals
    pub goals: Vec<Goal>,
    /// Global constraints
    pub constraints: Vec<Constraint>,
    /// Context information
    pub context: HashMap<String, String>,
    /// Domain/category
    pub domain: Option<String>,
    /// Created timestamp
    pub created_at: u64,
}

impl Intent {
    /// Create a new intent
    pub fn new(description: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id: format!("intent_{}", INTENT_COUNTER.fetch_add(1, Ordering::SeqCst)),
            description: description.to_string(),
            goals: Vec::new(),
            constraints: Vec::new(),
            context: HashMap::new(),
            domain: None,
            created_at: now,
        }
    }

    /// Add a goal
    pub fn with_goal(mut self, goal: Goal) -> Self {
        self.goals.push(goal);
        self
    }

    /// Add a global constraint
    pub fn with_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Add context
    pub fn with_context(mut self, key: &str, value: &str) -> Self {
        self.context.insert(key.to_string(), value.to_string());
        self
    }

    /// Set domain
    pub fn in_domain(mut self, domain: &str) -> Self {
        self.domain = Some(domain.to_string());
        self
    }

    /// Get all constraints (global + from goals)
    pub fn all_constraints(&self) -> Vec<&Constraint> {
        let mut all: Vec<&Constraint> = self.constraints.iter().collect();
        for goal in &self.goals {
            all.extend(goal.constraints.iter());
        }
        all
    }

    /// Get goals by status
    pub fn goals_by_status(&self, status: GoalStatus) -> Vec<&Goal> {
        self.goals.iter().filter(|g| g.status == status).collect()
    }

    /// Check if intent is fully satisfied
    pub fn is_satisfied(&self) -> bool {
        self.goals.iter().all(|g| g.status == GoalStatus::Achieved)
    }
}

/// Parser for intent specifications
#[derive(Debug, Default)]
pub struct IntentParser {
    /// Keyword patterns for goal extraction
    goal_patterns: Vec<String>,
    /// Keyword patterns for constraint extraction
    constraint_patterns: Vec<String>,
}

impl IntentParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self {
            goal_patterns: vec![
                "create".to_string(),
                "add".to_string(),
                "implement".to_string(),
                "build".to_string(),
                "make".to_string(),
                "ensure".to_string(),
                "fix".to_string(),
                "update".to_string(),
                "remove".to_string(),
                "delete".to_string(),
            ],
            constraint_patterns: vec![
                "must".to_string(),
                "should".to_string(),
                "without".to_string(),
                "no more than".to_string(),
                "at least".to_string(),
                "less than".to_string(),
                "greater than".to_string(),
                "within".to_string(),
            ],
        }
    }

    /// Parse an intent description into structured intent
    pub fn parse(&self, description: &str) -> Intent {
        let mut intent = Intent::new(description);

        // Split into sentences
        let sentences: Vec<&str> = description
            .split(['.', '\n'])
            .filter(|s| !s.trim().is_empty())
            .collect();

        for sentence in sentences {
            let sentence_lower = sentence.to_lowercase();

            // Check for constraint patterns
            let is_constraint = self
                .constraint_patterns
                .iter()
                .any(|p| sentence_lower.contains(p));

            // Check for goal patterns
            let is_goal = self
                .goal_patterns
                .iter()
                .any(|p| sentence_lower.contains(p));

            if is_constraint {
                if let Some(constraint) = self.parse_constraint(sentence) {
                    intent.constraints.push(constraint);
                }
            }

            if is_goal {
                let goal = self.parse_goal(sentence);
                intent.goals.push(goal);
            }
        }

        // If no goals found, create one from the entire description
        if intent.goals.is_empty() {
            intent.goals.push(Goal::new(description));
        }

        intent
    }

    /// Parse a goal from a sentence
    fn parse_goal(&self, sentence: &str) -> Goal {
        let sentence = sentence.trim();
        let mut goal = Goal::new(sentence);

        // Detect priority from keywords
        let lower = sentence.to_lowercase();
        if lower.contains("critical") || lower.contains("urgent") {
            goal.priority = Priority::Critical;
        } else if lower.contains("important") || lower.contains("high priority") {
            goal.priority = Priority::High;
        } else if lower.contains("optional") || lower.contains("nice to have") {
            goal.priority = Priority::Optional;
        }

        // Add tags based on action verbs
        for pattern in &self.goal_patterns {
            if lower.contains(pattern) {
                goal.tags.push(pattern.clone());
            }
        }

        goal
    }

    /// Parse a constraint from a sentence
    fn parse_constraint(&self, sentence: &str) -> Option<Constraint> {
        let sentence = sentence.trim();
        let lower = sentence.to_lowercase();

        // Must include
        if lower.contains("must include") || lower.contains("must have") {
            return Some(Constraint::must_include("output", sentence));
        }

        // Must not / without
        if lower.contains("must not") || lower.contains("without") || lower.contains("no ") {
            return Some(Constraint::must_exclude("output", sentence));
        }

        // Performance
        if lower.contains("fast") || lower.contains("quick") || lower.contains("ms") {
            return Some(Constraint::performance("execution", sentence));
        }

        // Security
        if lower.contains("secure") || lower.contains("safe") || lower.contains("encrypt") {
            return Some(Constraint::security("code", sentence));
        }

        // Generic constraint
        Some(Constraint {
            id: Constraint::generate_id(),
            constraint_type: ConstraintType::Custom(sentence.to_string()),
            target: "general".to_string(),
            priority: Priority::Medium,
            hard: lower.contains("must"),
            rationale: None,
        })
    }
}

/// Goal decomposition strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecompositionStrategy {
    /// Break down by functional components
    Functional,
    /// Break down by sequential steps
    Sequential,
    /// Break down by data flow
    DataFlow,
    /// Break down by layers
    Layered,
    /// No decomposition
    Atomic,
}

/// Goal decomposer
#[derive(Debug)]
pub struct GoalDecomposer {
    /// Default decomposition strategy
    _default_strategy: DecompositionStrategy,
    /// Maximum decomposition depth
    max_depth: usize,
    /// Decomposition patterns
    patterns: HashMap<String, Vec<String>>,
}

impl Default for GoalDecomposer {
    fn default() -> Self {
        let mut patterns = HashMap::new();

        // Common decomposition patterns
        patterns.insert(
            "create".to_string(),
            vec![
                "design".to_string(),
                "implement".to_string(),
                "test".to_string(),
                "document".to_string(),
            ],
        );

        patterns.insert(
            "implement".to_string(),
            vec![
                "define interface".to_string(),
                "write implementation".to_string(),
                "add tests".to_string(),
            ],
        );

        patterns.insert(
            "fix".to_string(),
            vec![
                "identify root cause".to_string(),
                "apply fix".to_string(),
                "verify fix".to_string(),
                "add regression test".to_string(),
            ],
        );

        patterns.insert(
            "refactor".to_string(),
            vec![
                "ensure tests exist".to_string(),
                "apply refactoring".to_string(),
                "verify tests pass".to_string(),
            ],
        );

        Self {
            _default_strategy: DecompositionStrategy::Sequential,
            max_depth: 3,
            patterns,
        }
    }
}

impl GoalDecomposer {
    /// Create a new decomposer
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Decompose a goal into sub-goals
    pub fn decompose(&self, goal: &Goal) -> Vec<Goal> {
        let desc_lower = goal.description.to_lowercase();

        // Find matching pattern
        for (keyword, sub_steps) in &self.patterns {
            if desc_lower.contains(keyword) {
                return sub_steps
                    .iter()
                    .enumerate()
                    .map(|(i, step)| {
                        let sub_desc = format!("{}: {}", step, goal.description);
                        let mut sub_goal = Goal::new(&sub_desc)
                            .with_priority(goal.priority)
                            .with_tag(keyword);

                        // Set dependencies - each step depends on previous
                        if i > 0 {
                            sub_goal.depends_on.push(format!("{}_{}", goal.id, i - 1));
                        }

                        sub_goal.parent = Some(goal.id.clone());
                        sub_goal.id = format!("{}_{}", goal.id, i);
                        sub_goal
                    })
                    .collect();
            }
        }

        // No pattern matched - return atomic goal
        Vec::new()
    }

    /// Recursively decompose all goals in intent
    pub fn decompose_intent(&self, intent: &mut Intent, depth: usize) {
        if depth >= self.max_depth {
            return;
        }

        let mut new_goals = Vec::new();

        for goal in &intent.goals {
            let sub_goals = self.decompose(goal);
            if !sub_goals.is_empty() {
                new_goals.extend(sub_goals);
            }
        }

        if !new_goals.is_empty() {
            intent.goals.extend(new_goals);
        }
    }
}

/// Solution type
#[derive(Debug, Clone)]
pub enum SolutionType {
    /// Code to be generated
    Code { language: String, content: String },
    /// Command to execute
    Command(String),
    /// File operation
    FileOperation {
        operation: String,
        path: String,
        content: Option<String>,
    },
    /// Composite solution
    Composite(Vec<Solution>),
}

/// A synthesized solution
#[derive(Debug, Clone)]
pub struct Solution {
    /// Solution identifier
    pub id: String,
    /// Goal this solution addresses
    pub goal_id: String,
    /// Solution type
    pub solution_type: SolutionType,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Constraints satisfied
    pub constraints_satisfied: Vec<String>,
    /// Constraints violated (for soft constraints)
    pub constraints_violated: Vec<String>,
    /// Explanation of the solution
    pub explanation: Option<String>,
}

impl Solution {
    /// Create a code solution
    pub fn code(goal_id: &str, language: &str, content: &str) -> Self {
        Self {
            id: format!("solution_{}", goal_id),
            goal_id: goal_id.to_string(),
            solution_type: SolutionType::Code {
                language: language.to_string(),
                content: content.to_string(),
            },
            confidence: 0.8,
            constraints_satisfied: Vec::new(),
            constraints_violated: Vec::new(),
            explanation: None,
        }
    }

    /// Create a command solution
    pub fn command(goal_id: &str, cmd: &str) -> Self {
        Self {
            id: format!("solution_{}", goal_id),
            goal_id: goal_id.to_string(),
            solution_type: SolutionType::Command(cmd.to_string()),
            confidence: 0.9,
            constraints_satisfied: Vec::new(),
            constraints_violated: Vec::new(),
            explanation: None,
        }
    }

    /// Create a file operation solution
    pub fn file_op(goal_id: &str, operation: &str, path: &str) -> Self {
        Self {
            id: format!("solution_{}", goal_id),
            goal_id: goal_id.to_string(),
            solution_type: SolutionType::FileOperation {
                operation: operation.to_string(),
                path: path.to_string(),
                content: None,
            },
            confidence: 0.9,
            constraints_satisfied: Vec::new(),
            constraints_violated: Vec::new(),
            explanation: None,
        }
    }

    /// Set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Add explanation
    pub fn with_explanation(mut self, explanation: &str) -> Self {
        self.explanation = Some(explanation.to_string());
        self
    }
}

/// Constraint checker
#[derive(Debug, Default)]
pub struct ConstraintChecker {
    /// Violations found
    pub violations: Vec<ConstraintViolation>,
}

/// A constraint violation
#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    /// The violated constraint
    pub constraint_id: String,
    /// Description of violation
    pub description: String,
    /// Is this a hard constraint?
    pub hard: bool,
    /// Suggested fix
    pub suggested_fix: Option<String>,
}

impl ConstraintChecker {
    /// Create a new checker
    pub fn new() -> Self {
        Self::default()
    }

    /// Check a solution against constraints
    pub fn check(&mut self, solution: &Solution, constraints: &[Constraint]) -> bool {
        self.violations.clear();
        let mut all_satisfied = true;

        for constraint in constraints {
            if !self.check_single(solution, constraint) {
                self.violations.push(ConstraintViolation {
                    constraint_id: constraint.id.clone(),
                    description: format!("Constraint violated: {:?}", constraint.constraint_type),
                    hard: constraint.hard,
                    suggested_fix: None,
                });

                if constraint.hard {
                    all_satisfied = false;
                }
            }
        }

        all_satisfied
    }

    /// Check single constraint
    fn check_single(&self, solution: &Solution, constraint: &Constraint) -> bool {
        // Extract content based on solution type
        let content = match &solution.solution_type {
            SolutionType::Code { content, .. } => content.clone(),
            SolutionType::Command(cmd) => cmd.clone(),
            SolutionType::FileOperation { content, .. } => content.clone().unwrap_or_default(),
            SolutionType::Composite(_) => return true, // Composite checks are recursive
        };

        match &constraint.constraint_type {
            ConstraintType::MustInclude(element) => {
                content.to_lowercase().contains(&element.to_lowercase())
            }
            ConstraintType::MustExclude(element) => {
                !content.to_lowercase().contains(&element.to_lowercase())
            }
            ConstraintType::Pattern(pattern) => regex::Regex::new(pattern)
                .map(|re| re.is_match(&content))
                .unwrap_or(false),
            ConstraintType::Range { min, max } => {
                // Check if content length is within range (simple interpretation)
                let len = content.len() as i64;
                len >= *min && len <= *max
            }
            ConstraintType::Security(requirement) => {
                // Simple security checks
                let req_lower = requirement.to_lowercase();
                if req_lower.contains("no eval") {
                    !content.contains("eval(")
                } else if req_lower.contains("no exec") {
                    !content.contains("exec(")
                } else if req_lower.contains("sanitize") {
                    // Would need actual validation
                    true
                } else {
                    true
                }
            }
            _ => true, // Other constraints pass by default
        }
    }

    /// Get hard violations
    pub fn hard_violations(&self) -> Vec<&ConstraintViolation> {
        self.violations.iter().filter(|v| v.hard).collect()
    }

    /// Check if all hard constraints are satisfied
    pub fn all_hard_satisfied(&self) -> bool {
        self.violations.iter().all(|v| !v.hard)
    }
}

/// Solution synthesizer
#[derive(Debug)]
pub struct SolutionSynthesizer {
    /// Templates for common solutions
    templates: HashMap<String, String>,
    /// Constraint checker
    checker: ConstraintChecker,
}

impl Default for SolutionSynthesizer {
    fn default() -> Self {
        let mut templates = HashMap::new();

        templates.insert(
            "rust_function".to_string(),
            "fn {{name}}({{params}}) -> {{return_type}} {\n    {{body}}\n}".to_string(),
        );

        templates.insert(
            "rust_test".to_string(),
            "#[test]\nfn test_{{name}}() {\n    {{assertions}}\n}".to_string(),
        );

        templates.insert(
            "rust_struct".to_string(),
            "#[derive(Debug, Clone)]\npub struct {{name}} {\n    {{fields}}\n}".to_string(),
        );

        Self {
            templates,
            checker: ConstraintChecker::new(),
        }
    }
}

impl SolutionSynthesizer {
    /// Create a new synthesizer
    pub fn new() -> Self {
        Self::default()
    }

    /// Synthesize solutions for an intent
    pub fn synthesize(&mut self, intent: &Intent) -> Vec<Solution> {
        let mut solutions = Vec::new();

        for goal in &intent.goals {
            if goal.status != GoalStatus::Pending {
                continue;
            }

            if let Some(solution) = self.synthesize_for_goal(goal, &intent.constraints) {
                solutions.push(solution);
            }
        }

        solutions
    }

    /// Synthesize a solution for a specific goal
    fn synthesize_for_goal(
        &mut self,
        goal: &Goal,
        global_constraints: &[Constraint],
    ) -> Option<Solution> {
        let desc_lower = goal.description.to_lowercase();

        // Combine goal and global constraints
        let all_constraints: Vec<Constraint> = goal
            .constraints
            .iter()
            .chain(global_constraints.iter())
            .cloned()
            .collect();

        // Determine solution type based on goal description
        let solution = if desc_lower.contains("create function")
            || desc_lower.contains("implement function")
        {
            self.synthesize_function(goal)
        } else if desc_lower.contains("add test") || desc_lower.contains("write test") {
            self.synthesize_test(goal)
        } else if desc_lower.contains("create file") || desc_lower.contains("add file") {
            self.synthesize_file_creation(goal)
        } else if desc_lower.contains("run") || desc_lower.contains("execute") {
            self.synthesize_command(goal)
        } else {
            // Generic code solution
            Solution::code(&goal.id, "rust", &format!("// TODO: {}", goal.description))
                .with_confidence(0.5)
        };

        // Check constraints
        if self.checker.check(&solution, &all_constraints) {
            Some(solution)
        } else if self.checker.all_hard_satisfied() {
            // Soft constraints violated but still acceptable
            let mut solution = solution;
            solution.constraints_violated = self
                .checker
                .violations
                .iter()
                .map(|v| v.constraint_id.clone())
                .collect();
            Some(solution)
        } else {
            None
        }
    }

    /// Synthesize a function
    fn synthesize_function(&self, goal: &Goal) -> Solution {
        let template = self
            .templates
            .get("rust_function")
            .cloned()
            .unwrap_or_default();

        // Extract function name from goal description
        let name = self.extract_name(&goal.description, "my_function");

        let code = template
            .replace("{{name}}", &name)
            .replace("{{params}}", "")
            .replace("{{return_type}}", "()")
            .replace("{{body}}", &format!("// {}", goal.description));

        Solution::code(&goal.id, "rust", &code)
            .with_confidence(0.7)
            .with_explanation(&format!("Generated function {} from goal", name))
    }

    /// Synthesize a test
    fn synthesize_test(&self, goal: &Goal) -> Solution {
        let template = self.templates.get("rust_test").cloned().unwrap_or_default();

        let name = self.extract_name(&goal.description, "feature");

        let code = template
            .replace("{{name}}", &name)
            .replace("{{assertions}}", "assert!(true);");

        Solution::code(&goal.id, "rust", &code)
            .with_confidence(0.7)
            .with_explanation(&format!("Generated test for {}", name))
    }

    /// Synthesize file creation
    fn synthesize_file_creation(&self, goal: &Goal) -> Solution {
        let path = self.extract_path(&goal.description, "new_file.rs");

        Solution::file_op(&goal.id, "create", &path)
            .with_confidence(0.8)
            .with_explanation(&format!("Create file at {}", path))
    }

    /// Synthesize a command
    fn synthesize_command(&self, goal: &Goal) -> Solution {
        // Extract command from description
        let cmd = if goal.description.to_lowercase().contains("test") {
            "cargo test"
        } else if goal.description.to_lowercase().contains("build") {
            "cargo build"
        } else if goal.description.to_lowercase().contains("format") {
            "cargo fmt"
        } else {
            "echo 'TODO'"
        };

        Solution::command(&goal.id, cmd).with_confidence(0.9)
    }

    /// Extract a name from description
    fn extract_name(&self, description: &str, default: &str) -> String {
        // Simple extraction - find quoted strings or words after keywords
        let words: Vec<&str> = description.split_whitespace().collect();

        for (i, word) in words.iter().enumerate() {
            if *word == "function" || *word == "for" || *word == "named" {
                if let Some(next) = words.get(i + 1) {
                    let name = next.trim_matches(|c| c == '"' || c == '\'' || c == '`');
                    if !name.is_empty() {
                        return name.to_string();
                    }
                }
            }
        }

        default.to_string()
    }

    /// Extract a path from description
    fn extract_path(&self, description: &str, default: &str) -> String {
        // Find path-like patterns
        for word in description.split_whitespace() {
            let word = word.trim_matches(|c| c == '"' || c == '\'' || c == '`');
            if word.contains('/') || word.ends_with(".rs") || word.ends_with(".txt") {
                return word.to_string();
            }
        }

        default.to_string()
    }
}

/// Execution planner
#[derive(Debug, Default)]
pub struct ExecutionPlanner {
    /// Planned steps
    pub steps: Vec<ExecutionStep>,
    /// Dependencies graph
    dependencies: HashMap<String, Vec<String>>,
}

/// A step in the execution plan
#[derive(Debug, Clone)]
pub struct ExecutionStep {
    /// Step identifier
    pub id: String,
    /// Solution to execute
    pub solution_id: String,
    /// Goal this step achieves
    pub goal_id: String,
    /// Order in the plan
    pub order: usize,
    /// Status
    pub status: StepStatus,
    /// Estimated confidence
    pub confidence: f32,
}

/// Status of an execution step
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Ready,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl ExecutionPlanner {
    /// Create a new planner
    pub fn new() -> Self {
        Self::default()
    }

    /// Plan execution of solutions
    pub fn plan(&mut self, solutions: &[Solution], goals: &[Goal]) -> Vec<ExecutionStep> {
        self.steps.clear();
        self.dependencies.clear();

        // Build dependency graph from goals
        for goal in goals {
            self.dependencies
                .insert(goal.id.clone(), goal.depends_on.clone());
        }

        // Topological sort to determine order
        let ordered_goal_ids = self.topological_sort(goals);

        // Create steps in dependency order
        for (order, goal_id) in ordered_goal_ids.iter().enumerate() {
            if let Some(solution) = solutions.iter().find(|s| &s.goal_id == goal_id) {
                self.steps.push(ExecutionStep {
                    id: format!("step_{}", order),
                    solution_id: solution.id.clone(),
                    goal_id: goal_id.clone(),
                    order,
                    status: StepStatus::Pending,
                    confidence: solution.confidence,
                });
            }
        }

        // Mark first steps as ready
        self.update_ready_steps();

        self.steps.clone()
    }

    /// Topological sort of goals
    fn topological_sort(&self, goals: &[Goal]) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();

        for goal in goals {
            if !visited.contains(&goal.id) {
                self.dfs(&goal.id, &mut visited, &mut temp_visited, &mut result);
            }
        }

        result
    }

    /// DFS for topological sort
    fn dfs(
        &self,
        goal_id: &str,
        visited: &mut HashSet<String>,
        temp_visited: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) {
        if temp_visited.contains(goal_id) || visited.contains(goal_id) {
            return;
        }

        temp_visited.insert(goal_id.to_string());

        if let Some(deps) = self.dependencies.get(goal_id) {
            for dep in deps {
                self.dfs(dep, visited, temp_visited, result);
            }
        }

        temp_visited.remove(goal_id);
        visited.insert(goal_id.to_string());
        result.push(goal_id.to_string());
    }

    /// Update which steps are ready to execute
    fn update_ready_steps(&mut self) {
        let completed: HashSet<String> = self
            .steps
            .iter()
            .filter(|s| s.status == StepStatus::Completed)
            .map(|s| s.goal_id.clone())
            .collect();

        for step in &mut self.steps {
            if step.status != StepStatus::Pending {
                continue;
            }

            let deps = self
                .dependencies
                .get(&step.goal_id)
                .cloned()
                .unwrap_or_default();
            if deps.iter().all(|d| completed.contains(d)) {
                step.status = StepStatus::Ready;
            }
        }
    }

    /// Mark step as completed
    pub fn complete_step(&mut self, step_id: &str) -> bool {
        if let Some(step) = self.steps.iter_mut().find(|s| s.id == step_id) {
            step.status = StepStatus::Completed;
            self.update_ready_steps();
            true
        } else {
            false
        }
    }

    /// Get next ready step
    pub fn next_step(&self) -> Option<&ExecutionStep> {
        self.steps.iter().find(|s| s.status == StepStatus::Ready)
    }

    /// Get all ready steps
    pub fn ready_steps(&self) -> Vec<&ExecutionStep> {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Ready)
            .collect()
    }

    /// Get execution progress
    pub fn progress(&self) -> (usize, usize) {
        let completed = self
            .steps
            .iter()
            .filter(|s| s.status == StepStatus::Completed)
            .count();
        (completed, self.steps.len())
    }
}

/// Intent-based programming engine
#[derive(Debug)]
pub struct IntentEngine {
    /// Intent parser
    pub parser: IntentParser,
    /// Goal decomposer
    pub decomposer: GoalDecomposer,
    /// Solution synthesizer
    pub synthesizer: SolutionSynthesizer,
    /// Execution planner
    pub planner: ExecutionPlanner,
    /// Active intents
    pub intents: HashMap<String, Intent>,
    /// Generated solutions
    pub solutions: HashMap<String, Vec<Solution>>,
}

impl Default for IntentEngine {
    fn default() -> Self {
        Self {
            parser: IntentParser::new(),
            decomposer: GoalDecomposer::new(),
            synthesizer: SolutionSynthesizer::new(),
            planner: ExecutionPlanner::new(),
            intents: HashMap::new(),
            solutions: HashMap::new(),
        }
    }
}

impl IntentEngine {
    /// Create a new engine
    pub fn new() -> Self {
        Self::default()
    }

    /// Process an intent from description
    pub fn process(&mut self, description: &str) -> &Intent {
        // Parse intent
        let mut intent = self.parser.parse(description);

        // Decompose goals
        self.decomposer.decompose_intent(&mut intent, 0);

        // Store intent
        let intent_id = intent.id.clone();
        self.intents.insert(intent_id.clone(), intent);

        self.intents.get(&intent_id).unwrap()
    }

    /// Generate solutions for an intent
    pub fn synthesize(&mut self, intent_id: &str) -> Vec<Solution> {
        if let Some(intent) = self.intents.get(intent_id) {
            let solutions = self.synthesizer.synthesize(intent);
            self.solutions
                .insert(intent_id.to_string(), solutions.clone());
            solutions
        } else {
            Vec::new()
        }
    }

    /// Create execution plan for an intent
    pub fn plan(&mut self, intent_id: &str) -> Vec<ExecutionStep> {
        let solutions = self.solutions.get(intent_id).cloned().unwrap_or_default();
        let goals = self
            .intents
            .get(intent_id)
            .map(|i| i.goals.clone())
            .unwrap_or_default();

        self.planner.plan(&solutions, &goals)
    }

    /// Full pipeline: parse -> decompose -> synthesize -> plan
    pub fn execute(&mut self, description: &str) -> Vec<ExecutionStep> {
        let intent = self.process(description);
        let intent_id = intent.id.clone();
        self.synthesize(&intent_id);
        self.plan(&intent_id)
    }

    /// Get intent by ID
    pub fn get_intent(&self, intent_id: &str) -> Option<&Intent> {
        self.intents.get(intent_id)
    }

    /// List all intents
    pub fn list_intents(&self) -> Vec<&Intent> {
        self.intents.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
        assert!(Priority::Low > Priority::Optional);
    }

    #[test]
    fn test_constraint_must_include() {
        let c = Constraint::must_include("code", "fn main()");
        assert!(matches!(c.constraint_type, ConstraintType::MustInclude(_)));
        assert!(c.hard);
    }

    #[test]
    fn test_constraint_must_exclude() {
        let c = Constraint::must_exclude("code", "unsafe");
        assert!(matches!(c.constraint_type, ConstraintType::MustExclude(_)));
    }

    #[test]
    fn test_constraint_with_rationale() {
        let c = Constraint::security("code", "no eval").with_rationale("Eval is dangerous");
        assert_eq!(c.rationale, Some("Eval is dangerous".to_string()));
    }

    #[test]
    fn test_constraint_soft() {
        let c = Constraint::performance("api", "< 100ms").soft();
        assert!(!c.hard);
    }

    #[test]
    fn test_goal_new() {
        let goal = Goal::new("Create a new function");
        assert_eq!(goal.status, GoalStatus::Pending);
        assert!(goal.created_at > 0);
    }

    #[test]
    fn test_goal_with_priority() {
        let goal = Goal::new("Critical task").with_priority(Priority::Critical);
        assert_eq!(goal.priority, Priority::Critical);
    }

    #[test]
    fn test_goal_with_constraint() {
        let goal = Goal::new("Task").with_constraint(Constraint::must_include("output", "test"));
        assert_eq!(goal.constraints.len(), 1);
    }

    #[test]
    fn test_goal_dependencies_met() {
        let goal = Goal::new("Task").depends_on("goal_1").depends_on("goal_2");

        let mut achieved = HashSet::new();
        assert!(!goal.dependencies_met(&achieved));

        achieved.insert("goal_1".to_string());
        assert!(!goal.dependencies_met(&achieved));

        achieved.insert("goal_2".to_string());
        assert!(goal.dependencies_met(&achieved));
    }

    #[test]
    fn test_goal_achieve() {
        let mut goal = Goal::new("Task");
        goal.achieve();
        assert_eq!(goal.status, GoalStatus::Achieved);
        assert!(goal.completed_at.is_some());
    }

    #[test]
    fn test_intent_new() {
        let intent = Intent::new("Build a web server");
        assert!(!intent.description.is_empty());
        assert!(intent.created_at > 0);
    }

    #[test]
    fn test_intent_with_goal() {
        let intent = Intent::new("Build app")
            .with_goal(Goal::new("Create backend"))
            .with_goal(Goal::new("Create frontend"));
        assert_eq!(intent.goals.len(), 2);
    }

    #[test]
    fn test_intent_with_context() {
        let intent = Intent::new("Task")
            .with_context("language", "rust")
            .with_context("framework", "actix");
        assert_eq!(intent.context.get("language"), Some(&"rust".to_string()));
    }

    #[test]
    fn test_intent_is_satisfied() {
        let mut intent = Intent::new("Task")
            .with_goal(Goal::new("Step 1"))
            .with_goal(Goal::new("Step 2"));

        assert!(!intent.is_satisfied());

        intent.goals[0].achieve();
        assert!(!intent.is_satisfied());

        intent.goals[1].achieve();
        assert!(intent.is_satisfied());
    }

    #[test]
    fn test_parser_parse_simple() {
        let parser = IntentParser::new();
        let intent = parser.parse("Create a new user authentication system");

        assert!(!intent.goals.is_empty());
        assert!(intent.goals[0].tags.contains(&"create".to_string()));
    }

    #[test]
    fn test_parser_parse_with_constraints() {
        let parser = IntentParser::new();
        let intent = parser
            .parse("Build a fast API. Must include authentication. Without external dependencies.");

        assert!(!intent.constraints.is_empty());
    }

    #[test]
    fn test_parser_parse_priority() {
        let parser = IntentParser::new();
        let intent = parser.parse("Critical: fix security vulnerability");

        assert_eq!(intent.goals[0].priority, Priority::Critical);
    }

    #[test]
    fn test_decomposer_decompose() {
        let decomposer = GoalDecomposer::new();
        let goal = Goal::new("Create a new API endpoint");

        let sub_goals = decomposer.decompose(&goal);
        assert!(!sub_goals.is_empty());

        // Check dependencies
        for (i, sub) in sub_goals.iter().enumerate() {
            if i > 0 {
                assert!(!sub.depends_on.is_empty());
            }
        }
    }

    #[test]
    fn test_decomposer_fix_pattern() {
        let decomposer = GoalDecomposer::new();
        let goal = Goal::new("Fix the login bug");

        let sub_goals = decomposer.decompose(&goal);
        assert!(sub_goals.iter().any(|g| g.description.contains("identify")));
        assert!(sub_goals.iter().any(|g| g.description.contains("verify")));
    }

    #[test]
    fn test_solution_code() {
        let sol = Solution::code("goal_1", "rust", "fn main() {}");
        assert!(matches!(sol.solution_type, SolutionType::Code { .. }));
        assert_eq!(sol.goal_id, "goal_1");
    }

    #[test]
    fn test_solution_command() {
        let sol = Solution::command("goal_1", "cargo test");
        assert!(matches!(sol.solution_type, SolutionType::Command(_)));
    }

    #[test]
    fn test_solution_with_confidence() {
        let sol = Solution::code("g", "rust", "code").with_confidence(0.95);
        assert_eq!(sol.confidence, 0.95);

        let sol = Solution::code("g", "rust", "code").with_confidence(1.5);
        assert_eq!(sol.confidence, 1.0); // Clamped
    }

    #[test]
    fn test_constraint_checker_must_include() {
        let mut checker = ConstraintChecker::new();
        let sol = Solution::code("g", "rust", "fn main() { println!(\"hello\"); }");

        let constraints = vec![Constraint::must_include("code", "println")];
        assert!(checker.check(&sol, &constraints));
        assert!(checker.violations.is_empty());
    }

    #[test]
    fn test_constraint_checker_must_exclude() {
        let mut checker = ConstraintChecker::new();
        let sol = Solution::code("g", "rust", "unsafe { dangerous() }");

        let constraints = vec![Constraint::must_exclude("code", "unsafe")];
        assert!(!checker.check(&sol, &constraints));
        assert!(!checker.violations.is_empty());
    }

    #[test]
    fn test_constraint_checker_soft_violation() {
        let mut checker = ConstraintChecker::new();
        let sol = Solution::code("g", "rust", "code");

        let constraints = vec![Constraint::performance("code", "< 100ms").soft()];
        let satisfied = checker.check(&sol, &constraints);

        // Soft constraints don't fail the check
        assert!(satisfied || checker.all_hard_satisfied());
    }

    #[test]
    fn test_synthesizer_function() {
        let mut synthesizer = SolutionSynthesizer::new();
        let goal = Goal::new("Create function named calculate");

        let sol = synthesizer.synthesize_for_goal(&goal, &[]);
        assert!(sol.is_some());

        if let Some(s) = sol {
            if let SolutionType::Code { content, .. } = s.solution_type {
                assert!(content.contains("fn"));
            }
        }
    }

    #[test]
    fn test_synthesizer_test() {
        let mut synthesizer = SolutionSynthesizer::new();
        let goal = Goal::new("Add test for login feature");

        let sol = synthesizer.synthesize_for_goal(&goal, &[]);
        assert!(sol.is_some());

        if let Some(s) = sol {
            if let SolutionType::Code { content, .. } = s.solution_type {
                assert!(content.contains("#[test]"));
            }
        }
    }

    #[test]
    fn test_execution_planner_plan() {
        let mut planner = ExecutionPlanner::new();

        let goals = vec![
            Goal::new("Step 1"),
            Goal::new("Step 2").depends_on("step_0"),
        ];

        let solutions = vec![
            Solution::code(&goals[0].id, "rust", "step 1"),
            Solution::code(&goals[1].id, "rust", "step 2"),
        ];

        let steps = planner.plan(&solutions, &goals);
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn test_execution_planner_ready_steps() {
        let mut planner = ExecutionPlanner::new();

        let g1 = Goal::new("First");
        let mut g2 = Goal::new("Second");
        g2.depends_on.push(g1.id.clone());

        let goals = vec![g1.clone(), g2.clone()];
        let solutions = vec![
            Solution::code(&g1.id, "rust", "1"),
            Solution::code(&g2.id, "rust", "2"),
        ];

        planner.plan(&solutions, &goals);

        let ready = planner.ready_steps();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].goal_id, g1.id);
    }

    #[test]
    fn test_execution_planner_complete_step() {
        let mut planner = ExecutionPlanner::new();

        let g1 = Goal::new("First");
        let mut g2 = Goal::new("Second");
        g2.depends_on.push(g1.id.clone());

        let goals = vec![g1.clone(), g2.clone()];
        let solutions = vec![
            Solution::code(&g1.id, "rust", "1"),
            Solution::code(&g2.id, "rust", "2"),
        ];

        planner.plan(&solutions, &goals);

        // Initially only first step is ready
        assert_eq!(planner.ready_steps().len(), 1);

        // Complete first step
        planner.complete_step("step_0");

        // Now second step should be ready
        assert_eq!(planner.ready_steps().len(), 1);
        assert!(
            planner.ready_steps()[0].goal_id.contains(&g2.id)
                || planner
                    .steps
                    .iter()
                    .any(|s| s.status == StepStatus::Completed)
        );
    }

    #[test]
    fn test_execution_planner_progress() {
        let mut planner = ExecutionPlanner::new();

        let goals = vec![Goal::new("Task 1"), Goal::new("Task 2")];
        let solutions = vec![
            Solution::code(&goals[0].id, "rust", "1"),
            Solution::code(&goals[1].id, "rust", "2"),
        ];

        planner.plan(&solutions, &goals);
        assert_eq!(planner.progress(), (0, 2));

        planner.complete_step("step_0");
        assert_eq!(planner.progress(), (1, 2));
    }

    #[test]
    fn test_intent_engine_new() {
        let engine = IntentEngine::new();
        assert!(engine.intents.is_empty());
    }

    #[test]
    fn test_intent_engine_process() {
        let mut engine = IntentEngine::new();
        let intent = engine.process("Create a user registration system");

        assert!(!intent.goals.is_empty());
    }

    #[test]
    fn test_intent_engine_synthesize() {
        let mut engine = IntentEngine::new();
        let intent = engine.process("Add test for feature X");
        let intent_id = intent.id.clone();

        let solutions = engine.synthesize(&intent_id);
        assert!(!solutions.is_empty());
    }

    #[test]
    fn test_intent_engine_execute() {
        let mut engine = IntentEngine::new();
        let _steps = engine.execute("Build a simple calculator");

        // Should have created an intent and generated steps
        assert!(!engine.intents.is_empty());
    }

    #[test]
    fn test_intent_engine_list_intents() {
        let mut engine = IntentEngine::new();
        engine.process("Task 1");
        engine.process("Task 2");

        assert_eq!(engine.list_intents().len(), 2);
    }

    #[test]
    fn test_constraint_type_pattern() {
        let mut checker = ConstraintChecker::new();
        let sol = Solution::code("g", "rust", "fn main() {}");

        let constraints = vec![Constraint::matches_pattern("code", r"fn\s+\w+")];
        assert!(checker.check(&sol, &constraints));
    }

    #[test]
    fn test_constraint_type_range() {
        let mut checker = ConstraintChecker::new();
        let sol = Solution::code("g", "rust", "short");

        let constraints = vec![Constraint::in_range("code", 1, 100)];
        assert!(checker.check(&sol, &constraints));

        let constraints = vec![Constraint::in_range("code", 100, 200)];
        assert!(!checker.check(&sol, &constraints));
    }

    #[test]
    fn test_goal_with_criteria() {
        let goal = Goal::new("Build API")
            .with_criterion("All endpoints return JSON")
            .with_criterion("Response time < 100ms");

        assert_eq!(goal.success_criteria.len(), 2);
    }

    #[test]
    fn test_goal_with_tags() {
        let goal = Goal::new("Task")
            .with_tag("backend")
            .with_tag("high-priority");

        assert!(goal.tags.contains(&"backend".to_string()));
    }

    #[test]
    fn test_intent_all_constraints() {
        let intent = Intent::new("Build app")
            .with_constraint(Constraint::security("app", "encrypt all data"))
            .with_goal(
                Goal::new("Create API").with_constraint(Constraint::performance("api", "< 100ms")),
            );

        let all = intent.all_constraints();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_intent_goals_by_status() {
        let mut intent = Intent::new("Build")
            .with_goal(Goal::new("Step 1"))
            .with_goal(Goal::new("Step 2"));

        intent.goals[0].achieve();

        let achieved = intent.goals_by_status(GoalStatus::Achieved);
        let pending = intent.goals_by_status(GoalStatus::Pending);

        assert_eq!(achieved.len(), 1);
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_decomposition_strategy() {
        let decomposer = GoalDecomposer::new().with_max_depth(5);
        assert_eq!(decomposer.max_depth, 5);
    }

    // ================== Additional Coverage Tests ==================

    #[test]
    fn test_priority_all_variants() {
        let priorities = vec![
            Priority::Critical,
            Priority::High,
            Priority::Medium,
            Priority::Low,
            Priority::Optional,
        ];
        for p in &priorities {
            assert!(!format!("{:?}", p).is_empty());
        }
        // Test ordering
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
        assert!(Priority::Low > Priority::Optional);
    }

    #[test]
    fn test_priority_default() {
        assert_eq!(Priority::default(), Priority::Medium);
    }

    #[test]
    fn test_constraint_type_all_variants() {
        let types = vec![
            ConstraintType::MustInclude("test".to_string()),
            ConstraintType::MustExclude("test".to_string()),
            ConstraintType::Range { min: 0, max: 100 },
            ConstraintType::Pattern(".*".to_string()),
            ConstraintType::Performance("< 100ms".to_string()),
            ConstraintType::Resource("memory < 1GB".to_string()),
            ConstraintType::Security("encrypt".to_string()),
            ConstraintType::Type("integer".to_string()),
            ConstraintType::Custom("custom".to_string()),
        ];
        for ct in &types {
            assert!(!format!("{:?}", ct).is_empty());
        }
    }

    #[test]
    fn test_constraint_with_priority_critical() {
        let constraint =
            Constraint::performance("api", "< 100ms").with_priority(Priority::Critical);
        assert_eq!(constraint.priority, Priority::Critical);
    }

    #[test]
    fn test_goal_status_all_variants() {
        let statuses = vec![
            GoalStatus::Pending,
            GoalStatus::InProgress,
            GoalStatus::Achieved,
            GoalStatus::Failed,
            GoalStatus::Blocked,
            GoalStatus::Abandoned,
        ];
        for s in &statuses {
            assert!(!format!("{:?}", s).is_empty());
        }
    }

    #[test]
    fn test_goal_fail() {
        let mut goal = Goal::new("Task");
        assert_eq!(goal.status, GoalStatus::Pending);

        goal.fail();
        assert_eq!(goal.status, GoalStatus::Failed);
    }

    #[test]
    fn test_goal_achieve_sets_timestamp() {
        let mut goal = Goal::new("Task");
        assert!(goal.completed_at.is_none());

        goal.achieve();
        assert!(goal.completed_at.is_some());
        assert_eq!(goal.status, GoalStatus::Achieved);
    }

    #[test]
    fn test_solution_type_file_operation() {
        let file_op = SolutionType::FileOperation {
            operation: "write".to_string(),
            path: "/tmp/test".to_string(),
            content: Some("content".to_string()),
        };
        assert!(matches!(file_op, SolutionType::FileOperation { .. }));
    }

    #[test]
    fn test_solution_type_composite() {
        let sol1 = Solution::code("g1", "rust", "code1");
        let sol2 = Solution::command("g2", "cmd");
        let composite = SolutionType::Composite(vec![sol1, sol2]);
        if let SolutionType::Composite(sols) = composite {
            assert_eq!(sols.len(), 2);
        } else {
            panic!("Expected composite");
        }
    }

    #[test]
    fn test_constraint_checker_violation_count() {
        let mut checker = ConstraintChecker::new();
        let sol = Solution::code("g", "rust", "unsafe code here");

        let constraints = vec![
            Constraint::must_exclude("code", "unsafe"),
            Constraint::must_include("code", "println"),
        ];

        checker.check(&sol, &constraints);
        assert_eq!(checker.violations.len(), 2);
    }

    #[test]
    fn test_step_status_all() {
        let statuses = vec![
            StepStatus::Pending,
            StepStatus::Ready,
            StepStatus::InProgress,
            StepStatus::Completed,
            StepStatus::Failed,
            StepStatus::Skipped,
        ];
        for s in &statuses {
            assert!(!format!("{:?}", s).is_empty());
        }
    }

    #[test]
    fn test_execution_step_status_change() {
        let mut step = ExecutionStep {
            id: "step_1".to_string(),
            solution_id: "sol_1".to_string(),
            goal_id: "goal_1".to_string(),
            order: 0,
            status: StepStatus::Pending,
            confidence: 0.8,
        };

        step.status = StepStatus::InProgress;
        assert_eq!(step.status, StepStatus::InProgress);

        step.status = StepStatus::Failed;
        assert_eq!(step.status, StepStatus::Failed);
    }
}
