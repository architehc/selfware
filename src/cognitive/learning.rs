//! Learning & Education Mode
//!
//! Provides explain-as-you-code functionality and concept extraction for curriculum
//! generation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static CONCEPT_COUNTER: AtomicU64 = AtomicU64::new(1);
static LESSON_COUNTER: AtomicU64 = AtomicU64::new(1);

// ============================================================================
// Explain-as-You-Code Mode
// ============================================================================

/// Explanation detail level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ExplanationLevel {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

impl ExplanationLevel {
    pub fn description(&self) -> &'static str {
        match self {
            ExplanationLevel::Beginner => "Detailed explanations with basic concepts",
            ExplanationLevel::Intermediate => "Moderate explanations assuming some knowledge",
            ExplanationLevel::Advanced => "Concise explanations for experienced developers",
            ExplanationLevel::Expert => "Minimal explanations, focus on edge cases",
        }
    }
}

/// Code explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExplanation {
    /// Code snippet being explained
    pub code: String,
    /// Explanation text
    pub explanation: String,
    /// Line-by-line explanations
    pub line_explanations: Vec<LineExplanation>,
    /// Concepts involved
    pub concepts: Vec<String>,
    /// Related topics to learn more
    pub related_topics: Vec<String>,
    /// Explanation level
    pub level: ExplanationLevel,
}

/// Line-by-line explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineExplanation {
    /// Line number
    pub line_number: u32,
    /// Code on this line
    pub code: String,
    /// Explanation
    pub explanation: String,
    /// Concepts used
    pub concepts: Vec<String>,
}

impl LineExplanation {
    pub fn new(line_number: u32, code: impl Into<String>, explanation: impl Into<String>) -> Self {
        Self {
            line_number,
            code: code.into(),
            explanation: explanation.into(),
            concepts: Vec::new(),
        }
    }

    pub fn with_concept(mut self, concept: impl Into<String>) -> Self {
        self.concepts.push(concept.into());
        self
    }
}

/// Explanation mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainModeConfig {
    /// Default explanation level
    pub level: ExplanationLevel,
    /// Show line-by-line explanations
    pub line_by_line: bool,
    /// Include related concepts
    pub include_concepts: bool,
    /// Maximum explanation length
    pub max_length: usize,
    /// Language for explanations
    pub language: String,
}

impl Default for ExplainModeConfig {
    fn default() -> Self {
        Self {
            level: ExplanationLevel::Beginner,
            line_by_line: true,
            include_concepts: true,
            max_length: 2000,
            language: "en".to_string(),
        }
    }
}

/// Code explainer
#[derive(Debug, Clone)]
pub struct CodeExplainer {
    /// Configuration
    pub config: ExplainModeConfig,
    /// Known concepts for context
    known_concepts: Vec<String>,
    /// Explanation history (bounded with LRU eviction)
    history: Vec<CodeExplanation>,
    /// Maximum history size to prevent unbounded growth
    max_history: usize,
}

impl CodeExplainer {
    pub fn new() -> Self {
        Self {
            config: ExplainModeConfig::default(),
            known_concepts: Vec::new(),
            history: Vec::new(),
            max_history: 100, // Limit history to 100 entries
        }
    }

    pub fn with_config(mut self, config: ExplainModeConfig) -> Self {
        self.config = config;
        self
    }

    pub fn add_known_concept(&mut self, concept: impl Into<String>) {
        self.known_concepts.push(concept.into());
    }

    pub fn explain(&mut self, code: &str) -> CodeExplanation {
        let lines: Vec<&str> = code.lines().collect();
        let mut line_explanations = Vec::new();
        let mut all_concepts = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            let (explanation, concepts) = self.explain_line(trimmed);
            let mut line_exp = LineExplanation::new((i + 1) as u32, *line, explanation);

            for concept in &concepts {
                line_exp = line_exp.with_concept(concept);
                if !all_concepts.contains(concept) {
                    all_concepts.push(concept.clone());
                }
            }

            line_explanations.push(line_exp);
        }

        let overall_explanation = self.generate_overall_explanation(code, &all_concepts);

        let explanation = CodeExplanation {
            code: code.to_string(),
            explanation: overall_explanation,
            line_explanations,
            concepts: all_concepts,
            related_topics: self.suggest_related_topics(code),
            level: self.config.level,
        };

        self.history.push(explanation.clone());

        // Enforce max_history limit with LRU eviction (remove oldest entries)
        while self.history.len() > self.max_history {
            self.history.remove(0);
        }

        explanation
    }

    fn explain_line(&self, line: &str) -> (String, Vec<String>) {
        let mut concepts = Vec::new();

        // Pattern matching for common code constructs
        let explanation = if line.starts_with("fn ") || line.starts_with("pub fn ") {
            concepts.push("function".to_string());
            self.explain_function(line)
        } else if line.starts_with("let ") {
            concepts.push("variable_binding".to_string());
            if line.starts_with("let mut ") {
                concepts.push("mutability".to_string());
            }
            self.explain_variable(line)
        } else if line.starts_with("struct ") || line.starts_with("pub struct ") {
            concepts.push("struct".to_string());
            self.explain_struct(line)
        } else if line.starts_with("impl ") {
            concepts.push("implementation".to_string());
            self.explain_impl(line)
        } else if line.starts_with("if ") {
            concepts.push("conditional".to_string());
            "Conditional statement that executes code based on a condition".to_string()
        } else if line.starts_with("for ")
            || line.starts_with("while ")
            || line.starts_with("loop ")
        {
            concepts.push("loop".to_string());
            self.explain_loop(line)
        } else if line.contains("->") {
            concepts.push("return_type".to_string());
            "Specifies the return type of a function".to_string()
        } else if line.contains("::") {
            concepts.push("path".to_string());
            "Namespace path to access items".to_string()
        } else if line.contains(".unwrap()") || line.contains(".expect(") {
            concepts.push("error_handling".to_string());
            "Unwraps a Result or Option, panicking if it's an error".to_string()
        } else if line.contains("async ") || line.contains(".await") {
            concepts.push("async".to_string());
            "Asynchronous code that can be paused and resumed".to_string()
        } else {
            "Executes an operation or expression".to_string()
        };

        (explanation, concepts)
    }

    fn explain_function(&self, line: &str) -> String {
        let visibility = if line.starts_with("pub ") {
            "public"
        } else {
            "private"
        };
        format!(
            "Defines a {} function. Functions encapsulate reusable blocks of code.",
            visibility
        )
    }

    fn explain_variable(&self, line: &str) -> String {
        if line.starts_with("let mut ") {
            "Declares a mutable variable. Its value can be changed after initialization."
                .to_string()
        } else {
            "Declares an immutable variable. Its value cannot be changed after assignment."
                .to_string()
        }
    }

    fn explain_struct(&self, line: &str) -> String {
        let visibility = if line.starts_with("pub ") {
            "public"
        } else {
            "private"
        };
        format!(
            "Defines a {} struct. Structs group related data together.",
            visibility
        )
    }

    fn explain_impl(&self, line: &str) -> String {
        if line.contains(" for ") {
            "Implements a trait for a type, adding functionality.".to_string()
        } else {
            "Implements methods for a type.".to_string()
        }
    }

    fn explain_loop(&self, line: &str) -> String {
        if line.starts_with("for ") {
            "Iterates over a collection or range of values.".to_string()
        } else if line.starts_with("while ") {
            "Loops while a condition is true.".to_string()
        } else {
            "Infinite loop that runs until explicitly broken.".to_string()
        }
    }

    fn generate_overall_explanation(&self, _code: &str, concepts: &[String]) -> String {
        let mut explanation = String::new();

        match self.config.level {
            ExplanationLevel::Beginner => {
                explanation.push_str("This code demonstrates several programming concepts:\n\n");
                for concept in concepts {
                    explanation.push_str(&format!(
                        "- **{}**: {}\n",
                        concept,
                        self.describe_concept(concept)
                    ));
                }
            }
            ExplanationLevel::Intermediate => {
                explanation.push_str("Key concepts used: ");
                explanation.push_str(&concepts.join(", "));
            }
            ExplanationLevel::Advanced | ExplanationLevel::Expert => {
                explanation.push_str("Uses: ");
                explanation.push_str(&concepts.join(", "));
            }
        }

        explanation
    }

    fn describe_concept(&self, concept: &str) -> &'static str {
        match concept {
            "function" => "A reusable block of code that performs a specific task",
            "variable_binding" => "Assigning a value to a name for later use",
            "mutability" => "The ability to modify a value after it's created",
            "struct" => "A custom data type that groups related values",
            "implementation" => "Adding methods and behavior to a type",
            "conditional" => "Making decisions based on conditions",
            "loop" => "Repeating code multiple times",
            "return_type" => "The type of value a function returns",
            "path" => "The location of an item in the code hierarchy",
            "error_handling" => "Managing and recovering from errors",
            "async" => "Non-blocking code that can wait for operations",
            _ => "A programming concept",
        }
    }

    fn suggest_related_topics(&self, _code: &str) -> Vec<String> {
        vec![
            "Ownership and borrowing".to_string(),
            "Error handling with Result".to_string(),
            "Pattern matching".to_string(),
        ]
    }

    pub fn history(&self) -> &[CodeExplanation] {
        &self.history
    }
}

impl Default for CodeExplainer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Concept Extraction & Curriculum
// ============================================================================

/// Concept difficulty
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Beginner,
    Elementary,
    Intermediate,
    Advanced,
    Expert,
}

/// Programming concept
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    /// Concept ID
    pub id: String,
    /// Concept name
    pub name: String,
    /// Description
    pub description: String,
    /// Difficulty level
    pub difficulty: Difficulty,
    /// Category
    pub category: String,
    /// Prerequisites (concept IDs)
    pub prerequisites: Vec<String>,
    /// Example code
    pub examples: Vec<String>,
    /// Files where this concept appears
    pub occurrences: Vec<PathBuf>,
    /// Occurrence count
    pub occurrence_count: u32,
}

impl Concept {
    pub fn new(name: impl Into<String>, category: impl Into<String>) -> Self {
        let id = format!("concept_{}", CONCEPT_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            name: name.into(),
            description: String::new(),
            difficulty: Difficulty::Beginner,
            category: category.into(),
            prerequisites: Vec::new(),
            examples: Vec::new(),
            occurrences: Vec::new(),
            occurrence_count: 0,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_difficulty(mut self, difficulty: Difficulty) -> Self {
        self.difficulty = difficulty;
        self
    }

    pub fn add_prerequisite(&mut self, prerequisite: impl Into<String>) {
        self.prerequisites.push(prerequisite.into());
    }

    pub fn add_example(&mut self, example: impl Into<String>) {
        self.examples.push(example.into());
    }

    pub fn record_occurrence(&mut self, file: impl Into<PathBuf>) {
        self.occurrences.push(file.into());
        self.occurrence_count += 1;
    }
}

/// Lesson in the curriculum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    /// Lesson ID
    pub id: String,
    /// Lesson title
    pub title: String,
    /// Description
    pub description: String,
    /// Concepts covered
    pub concepts: Vec<String>,
    /// Estimated time (minutes)
    pub estimated_minutes: u32,
    /// Order in curriculum
    pub order: u32,
    /// Learning objectives
    pub objectives: Vec<String>,
    /// Exercises
    pub exercises: Vec<Exercise>,
}

impl Lesson {
    pub fn new(title: impl Into<String>, order: u32) -> Self {
        let id = format!("lesson_{}", LESSON_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            title: title.into(),
            description: String::new(),
            concepts: Vec::new(),
            estimated_minutes: 30,
            order,
            objectives: Vec::new(),
            exercises: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn add_concept(&mut self, concept_id: impl Into<String>) {
        self.concepts.push(concept_id.into());
    }

    pub fn add_objective(&mut self, objective: impl Into<String>) {
        self.objectives.push(objective.into());
    }

    pub fn add_exercise(&mut self, exercise: Exercise) {
        self.exercises.push(exercise);
    }
}

/// Exercise in a lesson
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    /// Exercise title
    pub title: String,
    /// Instructions
    pub instructions: String,
    /// Starter code
    pub starter_code: Option<String>,
    /// Expected solution pattern
    pub solution_pattern: Option<String>,
    /// Hints
    pub hints: Vec<String>,
}

impl Exercise {
    pub fn new(title: impl Into<String>, instructions: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            instructions: instructions.into(),
            starter_code: None,
            solution_pattern: None,
            hints: Vec::new(),
        }
    }

    pub fn with_starter_code(mut self, code: impl Into<String>) -> Self {
        self.starter_code = Some(code.into());
        self
    }

    pub fn add_hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }
}

/// Curriculum generated from codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Curriculum {
    /// Curriculum title
    pub title: String,
    /// Description
    pub description: String,
    /// Concepts
    pub concepts: HashMap<String, Concept>,
    /// Lessons in order
    pub lessons: Vec<Lesson>,
    /// Total estimated time (minutes)
    pub total_minutes: u32,
    /// Target audience
    pub target_audience: ExplanationLevel,
}

impl Curriculum {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: String::new(),
            concepts: HashMap::new(),
            lessons: Vec::new(),
            total_minutes: 0,
            target_audience: ExplanationLevel::Beginner,
        }
    }

    pub fn add_concept(&mut self, concept: Concept) {
        self.concepts.insert(concept.id.clone(), concept);
    }

    pub fn add_lesson(&mut self, lesson: Lesson) {
        self.total_minutes += lesson.estimated_minutes;
        self.lessons.push(lesson);
    }

    pub fn get_concept(&self, id: &str) -> Option<&Concept> {
        self.concepts.get(id)
    }

    pub fn suggested_order(&self) -> Vec<&Lesson> {
        let mut sorted: Vec<_> = self.lessons.iter().collect();
        sorted.sort_by_key(|l| l.order);
        sorted
    }
}

/// Concept extractor
#[derive(Debug, Clone)]
pub struct ConceptExtractor {
    /// Known concept patterns
    patterns: HashMap<String, ConceptPattern>,
}

/// Pattern for detecting concepts
#[derive(Debug, Clone)]
pub struct ConceptPattern {
    /// Pattern name
    pub name: String,
    /// Keywords to look for
    pub keywords: Vec<String>,
    /// Category
    pub category: String,
    /// Difficulty
    pub difficulty: Difficulty,
}

impl ConceptExtractor {
    pub fn new() -> Self {
        let mut extractor = Self {
            patterns: HashMap::new(),
        };
        extractor.add_default_patterns();
        extractor
    }

    fn add_default_patterns(&mut self) {
        // Add common Rust patterns
        self.patterns.insert(
            "ownership".to_string(),
            ConceptPattern {
                name: "Ownership".to_string(),
                keywords: vec!["move".to_string(), "borrow".to_string(), "&".to_string()],
                category: "memory".to_string(),
                difficulty: Difficulty::Intermediate,
            },
        );

        self.patterns.insert(
            "pattern_matching".to_string(),
            ConceptPattern {
                name: "Pattern Matching".to_string(),
                keywords: vec![
                    "match".to_string(),
                    "if let".to_string(),
                    "while let".to_string(),
                ],
                category: "control_flow".to_string(),
                difficulty: Difficulty::Intermediate,
            },
        );

        self.patterns.insert(
            "error_handling".to_string(),
            ConceptPattern {
                name: "Error Handling".to_string(),
                keywords: vec![
                    "Result".to_string(),
                    "Option".to_string(),
                    "?".to_string(),
                    "unwrap".to_string(),
                ],
                category: "error_handling".to_string(),
                difficulty: Difficulty::Intermediate,
            },
        );

        self.patterns.insert(
            "traits".to_string(),
            ConceptPattern {
                name: "Traits".to_string(),
                keywords: vec!["trait".to_string(), "impl".to_string(), "dyn".to_string()],
                category: "abstraction".to_string(),
                difficulty: Difficulty::Advanced,
            },
        );

        self.patterns.insert(
            "generics".to_string(),
            ConceptPattern {
                name: "Generics".to_string(),
                keywords: vec!["<T>".to_string(), "where".to_string()],
                category: "abstraction".to_string(),
                difficulty: Difficulty::Advanced,
            },
        );
    }

    pub fn extract_from_code(&self, code: &str, file_path: &Path) -> Vec<Concept> {
        let mut found_concepts = Vec::new();

        for pattern in self.patterns.values() {
            let mut matches = 0;
            for keyword in &pattern.keywords {
                if code.contains(keyword) {
                    matches += 1;
                }
            }

            if matches > 0 {
                let mut concept = Concept::new(&pattern.name, &pattern.category)
                    .with_difficulty(pattern.difficulty);
                concept.record_occurrence(file_path.to_path_buf());
                found_concepts.push(concept);
            }
        }

        found_concepts
    }

    pub fn generate_curriculum(&self, concepts: &[Concept], title: &str) -> Curriculum {
        let mut curriculum = Curriculum::new(title);

        // Group concepts by difficulty
        let mut by_difficulty: HashMap<Difficulty, Vec<&Concept>> = HashMap::new();
        for concept in concepts {
            by_difficulty
                .entry(concept.difficulty)
                .or_default()
                .push(concept);
        }

        // Create lessons for each difficulty level
        let difficulties = [
            Difficulty::Beginner,
            Difficulty::Elementary,
            Difficulty::Intermediate,
            Difficulty::Advanced,
            Difficulty::Expert,
        ];

        let mut order = 1;
        for difficulty in difficulties {
            if let Some(concepts_at_level) = by_difficulty.get(&difficulty) {
                let lesson_title = format!("{:?} Concepts", difficulty);
                let mut lesson = Lesson::new(&lesson_title, order);
                lesson.description =
                    format!("Learn {:?} level concepts from the codebase", difficulty);

                for concept in concepts_at_level {
                    curriculum.add_concept((*concept).clone());
                    lesson.add_concept(&concept.id);
                }

                lesson.estimated_minutes = (concepts_at_level.len() * 15) as u32;
                curriculum.add_lesson(lesson);
                order += 1;
            }
        }

        curriculum
    }
}

impl Default for ConceptExtractor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/cognitive/learning/learning_test.rs"]
mod tests;
