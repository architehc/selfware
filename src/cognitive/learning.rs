//! Learning & Education Mode
//!
//! Provides explain-as-you-code functionality, concept extraction for curriculum
//! generation, and quiz generation from code for testing understanding.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::util::current_timestamp_secs;

static CONCEPT_COUNTER: AtomicU64 = AtomicU64::new(1);
static QUIZ_COUNTER: AtomicU64 = AtomicU64::new(1);
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
        let explanation;

        // Pattern matching for common code constructs
        if line.starts_with("fn ") || line.starts_with("pub fn ") {
            concepts.push("function".to_string());
            explanation = self.explain_function(line);
        } else if line.starts_with("let ") || line.starts_with("let mut ") {
            concepts.push("variable_binding".to_string());
            if line.contains("mut ") {
                concepts.push("mutability".to_string());
            }
            explanation = self.explain_variable(line);
        } else if line.starts_with("struct ") || line.starts_with("pub struct ") {
            concepts.push("struct".to_string());
            explanation = self.explain_struct(line);
        } else if line.starts_with("impl ") {
            concepts.push("implementation".to_string());
            explanation = self.explain_impl(line);
        } else if line.starts_with("if ") {
            concepts.push("conditional".to_string());
            explanation =
                "Conditional statement that executes code based on a condition".to_string();
        } else if line.starts_with("for ")
            || line.starts_with("while ")
            || line.starts_with("loop ")
        {
            concepts.push("loop".to_string());
            explanation = self.explain_loop(line);
        } else if line.contains("->") {
            concepts.push("return_type".to_string());
            explanation = "Specifies the return type of a function".to_string();
        } else if line.contains("::") {
            concepts.push("path".to_string());
            explanation = "Namespace path to access items".to_string();
        } else if line.contains(".unwrap()") || line.contains(".expect(") {
            concepts.push("error_handling".to_string());
            explanation = "Unwraps a Result or Option, panicking if it's an error".to_string();
        } else if line.contains("async ") || line.contains(".await") {
            concepts.push("async".to_string());
            explanation = "Asynchronous code that can be paused and resumed".to_string();
        } else {
            explanation = "Executes an operation or expression".to_string();
        }

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
        if line.contains("mut ") {
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
// Quiz Generation
// ============================================================================

/// Quiz question type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestionType {
    MultipleChoice,
    TrueFalse,
    FillInBlank,
    CodeCompletion,
    BugFix,
    CodeExplanation,
}

/// Quiz question
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizQuestion {
    /// Question ID
    pub id: String,
    /// Question type
    pub question_type: QuestionType,
    /// Question text
    pub question: String,
    /// Code snippet (if applicable)
    pub code_snippet: Option<String>,
    /// Answer options (for multiple choice)
    pub options: Vec<String>,
    /// Correct answer index(es)
    pub correct_answers: Vec<usize>,
    /// Explanation of correct answer
    pub explanation: String,
    /// Concepts tested
    pub concepts: Vec<String>,
    /// Difficulty
    pub difficulty: Difficulty,
    /// Points
    pub points: u32,
}

impl QuizQuestion {
    pub fn multiple_choice(question: impl Into<String>) -> Self {
        let id = format!("q_{}", QUIZ_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            question_type: QuestionType::MultipleChoice,
            question: question.into(),
            code_snippet: None,
            options: Vec::new(),
            correct_answers: Vec::new(),
            explanation: String::new(),
            concepts: Vec::new(),
            difficulty: Difficulty::Beginner,
            points: 1,
        }
    }

    pub fn true_false(question: impl Into<String>, answer: bool) -> Self {
        let id = format!("q_{}", QUIZ_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            question_type: QuestionType::TrueFalse,
            question: question.into(),
            code_snippet: None,
            options: vec!["True".to_string(), "False".to_string()],
            correct_answers: vec![if answer { 0 } else { 1 }],
            explanation: String::new(),
            concepts: Vec::new(),
            difficulty: Difficulty::Beginner,
            points: 1,
        }
    }

    pub fn code_completion(question: impl Into<String>, code: impl Into<String>) -> Self {
        let id = format!("q_{}", QUIZ_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            question_type: QuestionType::CodeCompletion,
            question: question.into(),
            code_snippet: Some(code.into()),
            options: Vec::new(),
            correct_answers: Vec::new(),
            explanation: String::new(),
            concepts: Vec::new(),
            difficulty: Difficulty::Intermediate,
            points: 2,
        }
    }

    pub fn with_options(mut self, options: Vec<String>, correct: Vec<usize>) -> Self {
        self.options = options;
        self.correct_answers = correct;
        self
    }

    pub fn with_explanation(mut self, explanation: impl Into<String>) -> Self {
        self.explanation = explanation.into();
        self
    }

    pub fn with_concept(mut self, concept: impl Into<String>) -> Self {
        self.concepts.push(concept.into());
        self
    }

    pub fn with_difficulty(mut self, difficulty: Difficulty) -> Self {
        self.difficulty = difficulty;
        self
    }

    pub fn is_correct(&self, answer_indices: &[usize]) -> bool {
        if answer_indices.len() != self.correct_answers.len() {
            return false;
        }
        let mut sorted_given = answer_indices.to_vec();
        let mut sorted_correct = self.correct_answers.clone();
        sorted_given.sort();
        sorted_correct.sort();
        sorted_given == sorted_correct
    }
}

/// Quiz
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quiz {
    /// Quiz ID
    pub id: String,
    /// Quiz title
    pub title: String,
    /// Description
    pub description: String,
    /// Questions
    pub questions: Vec<QuizQuestion>,
    /// Time limit (minutes, None for no limit)
    pub time_limit_mins: Option<u32>,
    /// Passing score percentage
    pub passing_score: u32,
    /// Created timestamp
    pub created_at: u64,
}

impl Quiz {
    pub fn new(title: impl Into<String>) -> Self {
        let id = format!("quiz_{}", QUIZ_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            title: title.into(),
            description: String::new(),
            questions: Vec::new(),
            time_limit_mins: None,
            passing_score: 70,
            created_at: current_timestamp_secs(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_time_limit(mut self, minutes: u32) -> Self {
        self.time_limit_mins = Some(minutes);
        self
    }

    pub fn add_question(&mut self, question: QuizQuestion) {
        self.questions.push(question);
    }

    pub fn total_points(&self) -> u32 {
        self.questions.iter().map(|q| q.points).sum()
    }

    pub fn question_count(&self) -> usize {
        self.questions.len()
    }
}

/// Quiz attempt result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizResult {
    /// Quiz ID
    pub quiz_id: String,
    /// Answers given (question ID -> selected indices)
    pub answers: HashMap<String, Vec<usize>>,
    /// Score
    pub score: u32,
    /// Total possible points
    pub total_points: u32,
    /// Percentage
    pub percentage: f64,
    /// Passed
    pub passed: bool,
    /// Time taken (seconds)
    pub time_taken_secs: u64,
    /// Completed timestamp
    pub completed_at: u64,
    /// Incorrect questions for review
    pub incorrect_questions: Vec<String>,
}

impl QuizResult {
    pub fn from_attempt(
        quiz: &Quiz,
        answers: HashMap<String, Vec<usize>>,
        time_taken_secs: u64,
    ) -> Self {
        let mut score = 0;
        let mut incorrect = Vec::new();

        for question in &quiz.questions {
            if let Some(given) = answers.get(&question.id) {
                if question.is_correct(given) {
                    score += question.points;
                } else {
                    incorrect.push(question.id.clone());
                }
            } else {
                incorrect.push(question.id.clone());
            }
        }

        let total_points = quiz.total_points();
        let percentage = if total_points > 0 {
            (score as f64 / total_points as f64) * 100.0
        } else {
            0.0
        };

        QuizResult {
            quiz_id: quiz.id.clone(),
            answers,
            score,
            total_points,
            percentage,
            passed: percentage >= quiz.passing_score as f64,
            time_taken_secs,
            completed_at: current_timestamp_secs(),
            incorrect_questions: incorrect,
        }
    }
}

/// Quiz generator
#[derive(Debug, Clone)]
pub struct QuizGenerator {
    /// Question templates
    templates: Vec<QuestionTemplate>,
}

/// Template for generating questions
#[derive(Debug, Clone)]
pub struct QuestionTemplate {
    /// Template name
    pub name: String,
    /// Question pattern
    pub pattern: String,
    /// Question type
    pub question_type: QuestionType,
    /// Concept
    pub concept: String,
}

impl QuizGenerator {
    pub fn new() -> Self {
        let mut generator = Self {
            templates: Vec::new(),
        };
        generator.add_default_templates();
        generator
    }

    fn add_default_templates(&mut self) {
        self.templates.push(QuestionTemplate {
            name: "ownership_move".to_string(),
            pattern: "What happens to the variable after this line?".to_string(),
            question_type: QuestionType::MultipleChoice,
            concept: "ownership".to_string(),
        });

        self.templates.push(QuestionTemplate {
            name: "mutability".to_string(),
            pattern: "Can this variable be modified?".to_string(),
            question_type: QuestionType::TrueFalse,
            concept: "mutability".to_string(),
        });

        self.templates.push(QuestionTemplate {
            name: "error_handling".to_string(),
            pattern: "What will happen if this returns an error?".to_string(),
            question_type: QuestionType::MultipleChoice,
            concept: "error_handling".to_string(),
        });
    }

    pub fn generate_from_code(&self, code: &str, num_questions: usize) -> Quiz {
        let mut quiz = Quiz::new("Code Understanding Quiz")
            .with_description("Test your understanding of this code");

        // Generate questions based on code analysis
        let lines: Vec<&str> = code.lines().collect();

        for (i, line) in lines.iter().take(num_questions).enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            if let Some(question) = self.generate_question_for_line(trimmed, i + 1) {
                quiz.add_question(question);
            }
        }

        quiz
    }

    fn generate_question_for_line(&self, line: &str, line_num: usize) -> Option<QuizQuestion> {
        if line.contains("let mut ") {
            Some(
                QuizQuestion::true_false(
                    format!(
                        "Line {}: The variable declared here can be modified later.",
                        line_num
                    ),
                    true,
                )
                .with_concept("mutability")
                .with_explanation(
                    "Variables declared with 'let mut' are mutable and can be changed.",
                ),
            )
        } else if line.contains("let ") {
            Some(
                QuizQuestion::true_false(
                    format!("Line {}: This variable is immutable.", line_num),
                    true,
                )
                .with_concept("immutability")
                .with_explanation("Variables declared with 'let' without 'mut' are immutable."),
            )
        } else if line.contains(".unwrap()") {
            Some(
                QuizQuestion::multiple_choice(format!(
                    "Line {}: What happens if unwrap() is called on None or Err?",
                    line_num
                ))
                .with_options(
                    vec![
                        "The program panics".to_string(),
                        "Returns a default value".to_string(),
                        "Returns None".to_string(),
                        "Silently fails".to_string(),
                    ],
                    vec![0],
                )
                .with_concept("error_handling")
                .with_explanation("unwrap() causes a panic if called on None or Err."),
            )
        } else if line.contains("fn ") {
            Some(
                QuizQuestion::true_false(
                    format!("Line {}: This declares a function.", line_num),
                    true,
                )
                .with_concept("functions"),
            )
        } else {
            None
        }
    }

    pub fn generate_concept_quiz(&self, concept: &Concept, num_questions: usize) -> Quiz {
        let mut quiz = Quiz::new(format!("{} Quiz", concept.name))
            .with_description(format!("Test your understanding of {}", concept.name));

        // Generate questions about the concept
        let q1 = QuizQuestion::true_false(
            format!(
                "{} is a {:?} level concept.",
                concept.name, concept.difficulty
            ),
            true,
        )
        .with_concept(&concept.name);
        quiz.add_question(q1);

        // Add more questions based on examples
        for example in concept.examples.iter().take(num_questions - 1) {
            let q = QuizQuestion::code_completion(
                format!("Identify the {} usage in this code:", concept.name),
                example.clone(),
            )
            .with_concept(&concept.name)
            .with_difficulty(concept.difficulty);
            quiz.add_question(q);
        }

        quiz
    }
}

impl Default for QuizGenerator {
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
