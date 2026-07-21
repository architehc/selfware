use super::*;

// Explain mode tests
#[test]
fn test_explanation_level() {
    assert!(!ExplanationLevel::Beginner.description().is_empty());
    assert!(ExplanationLevel::Beginner < ExplanationLevel::Expert);
}

#[test]
fn test_line_explanation() {
    let line = LineExplanation::new(1, "let x = 5;", "Declares a variable")
        .with_concept("variable_binding");

    assert_eq!(line.line_number, 1);
    assert_eq!(line.concepts.len(), 1);
}

#[test]
fn test_code_explainer() {
    let mut explainer = CodeExplainer::new();

    let code = r#"
fn main() {
    let x = 5;
    let mut y = 10;
}
        "#;

    let explanation = explainer.explain(code);

    assert!(!explanation.explanation.is_empty());
    assert!(!explanation.line_explanations.is_empty());
}

#[test]
fn test_code_explainer_history_limit() {
    let mut explainer = CodeExplainer::new();

    // Explain more than max_history times (default is 100)
    for i in 0..150 {
        let code = format!("fn test_{}() {{ }}", i);
        explainer.explain(&code);
    }

    // History should be limited to max_history
    assert_eq!(explainer.history.len(), 100);

    // The oldest entries should have been evicted
    // The first explanation in history should be test_50 (indices 50-149)
    assert!(explainer.history[0].code.contains("test_50"));

    // The most recent should be test_149
    assert!(explainer.history[99].code.contains("test_149"));
}

#[test]
fn test_explainer_config() {
    let config = ExplainModeConfig {
        level: ExplanationLevel::Advanced,
        line_by_line: false,
        ..Default::default()
    };

    let explainer = CodeExplainer::new().with_config(config);
    assert_eq!(explainer.config.level, ExplanationLevel::Advanced);
}

// Concept extraction tests
#[test]
fn test_concept_creation() {
    let mut concept = Concept::new("Ownership", "memory")
        .with_description("Rust's ownership system")
        .with_difficulty(Difficulty::Intermediate);

    concept.add_prerequisite("variables");
    concept.add_example("let s = String::from(\"hello\");");
    concept.record_occurrence("src/main.rs");

    assert_eq!(concept.category, "memory");
    assert_eq!(concept.difficulty, Difficulty::Intermediate);
    assert_eq!(concept.occurrence_count, 1);
}

#[test]
fn test_lesson() {
    let mut lesson = Lesson::new("Introduction", 1).with_description("Getting started");

    lesson.add_concept("concept_1");
    lesson.add_objective("Understand basics");
    lesson.add_exercise(Exercise::new("Hello World", "Write hello world"));

    assert_eq!(lesson.concepts.len(), 1);
    assert_eq!(lesson.exercises.len(), 1);
}

#[test]
fn test_curriculum() {
    let mut curriculum = Curriculum::new("Rust Basics");

    curriculum.add_concept(Concept::new("Variables", "basics"));

    let lesson = Lesson::new("Variables", 1);
    curriculum.add_lesson(lesson);

    assert_eq!(curriculum.lessons.len(), 1);
    assert!(curriculum.total_minutes > 0);
}

#[test]
fn test_concept_extractor() {
    let extractor = ConceptExtractor::new();

    let code = r#"
fn main() {
    let x: Result<i32, &str> = Ok(5);
    match x {
        Ok(v) => println!("{}", v),
        Err(e) => println!("{}", e),
    }
}
        "#;

    let concepts = extractor.extract_from_code(code, &PathBuf::from("test.rs"));
    assert!(!concepts.is_empty());
}

#[test]
fn test_curriculum_generation() {
    let extractor = ConceptExtractor::new();

    let concepts = vec![
        Concept::new("Variables", "basics").with_difficulty(Difficulty::Beginner),
        Concept::new("Ownership", "memory").with_difficulty(Difficulty::Intermediate),
    ];

    let curriculum = extractor.generate_curriculum(&concepts, "Test Curriculum");

    assert!(!curriculum.lessons.is_empty());
}

// Quiz tests
#[test]
fn test_multiple_choice_question() {
    let question = QuizQuestion::multiple_choice("What is 2 + 2?")
        .with_options(
            vec!["3".to_string(), "4".to_string(), "5".to_string()],
            vec![1],
        )
        .with_explanation("Basic arithmetic");

    assert!(question.is_correct(&[1]));
    assert!(!question.is_correct(&[0]));
}

#[test]
fn test_true_false_question() {
    let question = QuizQuestion::true_false("Rust has garbage collection", false);

    assert!(question.is_correct(&[1])); // False is index 1
    assert!(!question.is_correct(&[0]));
}

#[test]
fn test_quiz() {
    let mut quiz = Quiz::new("Test Quiz")
        .with_description("A test quiz")
        .with_time_limit(30);

    quiz.add_question(QuizQuestion::true_false("Test", true));
    quiz.add_question(
        QuizQuestion::multiple_choice("Test MC")
            .with_options(vec!["A".to_string(), "B".to_string()], vec![0]),
    );

    assert_eq!(quiz.question_count(), 2);
    assert_eq!(quiz.total_points(), 2);
}

#[test]
fn test_quiz_result() {
    let mut quiz = Quiz::new("Test");
    quiz.add_question(
        QuizQuestion::multiple_choice("Q1")
            .with_options(vec!["A".to_string(), "B".to_string()], vec![0]),
    );

    let q_id = quiz.questions[0].id.clone();
    let mut answers = HashMap::new();
    answers.insert(q_id, vec![0]);

    let result = QuizResult::from_attempt(&quiz, answers, 60);

    assert_eq!(result.score, 1);
    assert!(result.passed);
    assert!(result.incorrect_questions.is_empty());
}

#[test]
fn test_quiz_generator() {
    let generator = QuizGenerator::new();

    let code = r#"
let x = 5;
let mut y = 10;
y.to_string().unwrap();
        "#;

    let quiz = generator.generate_from_code(code, 5);
    assert!(!quiz.questions.is_empty());
}

#[test]
fn test_concept_quiz() {
    let generator = QuizGenerator::new();

    let mut concept = Concept::new("Testing", "basics");
    concept.add_example("assert!(true);");

    let quiz = generator.generate_concept_quiz(&concept, 3);
    assert!(!quiz.questions.is_empty());
}

#[test]
fn test_exercise() {
    let exercise = Exercise::new("Hello World", "Print hello world")
        .with_starter_code("fn main() { }")
        .add_hint("Use println! macro");

    assert!(exercise.starter_code.is_some());
    assert_eq!(exercise.hints.len(), 1);
}

#[test]
fn test_explanation_level_description() {
    assert_eq!(
        ExplanationLevel::Beginner.description(),
        "Detailed explanations with basic concepts"
    );
    assert_eq!(
        ExplanationLevel::Intermediate.description(),
        "Moderate explanations assuming some knowledge"
    );
    assert_eq!(
        ExplanationLevel::Advanced.description(),
        "Concise explanations for experienced developers"
    );
    assert_eq!(
        ExplanationLevel::Expert.description(),
        "Minimal explanations, focus on edge cases"
    );
}

#[test]
fn test_explanation_level_ordering() {
    assert!(ExplanationLevel::Beginner < ExplanationLevel::Intermediate);
    assert!(ExplanationLevel::Intermediate < ExplanationLevel::Advanced);
    assert!(ExplanationLevel::Advanced < ExplanationLevel::Expert);
}

#[test]
fn test_explanation_level_eq() {
    assert_eq!(ExplanationLevel::Beginner, ExplanationLevel::Beginner);
    assert_ne!(ExplanationLevel::Beginner, ExplanationLevel::Expert);
}

#[test]
fn test_explain_mode_config_default() {
    let config = ExplainModeConfig::default();
    assert_eq!(config.level, ExplanationLevel::Beginner);
    assert!(config.line_by_line);
    assert!(config.include_concepts);
    assert_eq!(config.max_length, 2000);
    assert_eq!(config.language, "en");
}

#[test]
fn test_line_explanation_new() {
    let exp = LineExplanation::new(10, "let x = 5;", "Declares a variable");
    assert_eq!(exp.line_number, 10);
    assert_eq!(exp.code, "let x = 5;");
    assert!(exp.concepts.is_empty());
}

#[test]
fn test_line_explanation_with_concepts() {
    let exp = LineExplanation::new(1, "fn main()", "Main function")
        .with_concept("function")
        .with_concept("entry_point");

    assert_eq!(exp.concepts.len(), 2);
    assert!(exp.concepts.contains(&"function".to_string()));
}

#[test]
fn test_code_explainer_known_concepts() {
    let mut explainer = CodeExplainer::new();
    explainer.add_known_concept("variables");
    explainer.add_known_concept("functions");

    assert_eq!(explainer.known_concepts.len(), 2);
}

#[test]
fn test_difficulty_ordering() {
    assert!(Difficulty::Beginner < Difficulty::Elementary);
    assert!(Difficulty::Elementary < Difficulty::Intermediate);
    assert!(Difficulty::Intermediate < Difficulty::Advanced);
    assert!(Difficulty::Advanced < Difficulty::Expert);
}

#[test]
fn test_difficulty_all_variants() {
    let variants = [
        Difficulty::Beginner,
        Difficulty::Elementary,
        Difficulty::Intermediate,
        Difficulty::Advanced,
        Difficulty::Expert,
    ];
    for v in variants {
        let _ = format!("{:?}", v);
    }
}

#[test]
fn test_concept_with_example() {
    let mut concept = Concept::new("Ownership", "memory");
    concept.add_example("let s = String::new();");
    concept.add_example("drop(s);");

    assert_eq!(concept.examples.len(), 2);
}

#[test]
fn test_concept_record_occurrence() {
    let mut concept = Concept::new("Test", "test");
    assert_eq!(concept.occurrence_count, 0);

    concept.record_occurrence("src/main.rs");
    assert_eq!(concept.occurrence_count, 1);

    concept.record_occurrence("src/lib.rs");
    assert_eq!(concept.occurrence_count, 2);
}

#[test]
fn test_concept_prerequisites() {
    let mut concept = Concept::new("Borrowing", "memory").with_difficulty(Difficulty::Intermediate);

    concept.add_prerequisite("ownership");
    concept.add_prerequisite("references");

    assert_eq!(concept.prerequisites.len(), 2);
}

#[test]
fn test_curriculum_new() {
    let curriculum = Curriculum::new("Rust Fundamentals");
    assert_eq!(curriculum.title, "Rust Fundamentals");
    assert!(curriculum.lessons.is_empty());
    assert_eq!(curriculum.total_minutes, 0);
}

#[test]
fn test_lesson_estimated_minutes_default() {
    let lesson = Lesson::new("Basics", 1).with_description("Basic concepts");

    // Default is 30 minutes
    assert_eq!(lesson.estimated_minutes, 30);
}

#[test]
fn test_question_type_variants() {
    let types = [
        QuestionType::MultipleChoice,
        QuestionType::TrueFalse,
        QuestionType::FillInBlank,
        QuestionType::CodeCompletion,
        QuestionType::BugFix,
        QuestionType::CodeExplanation,
    ];
    for t in types {
        let _ = format!("{:?}", t);
    }
}

#[test]
fn test_quiz_default_passing_score() {
    let quiz = Quiz::new("Test Quiz");

    // Default passing score is 70%
    assert_eq!(quiz.passing_score, 70);
}

#[test]
fn test_quiz_result_partial_answers() {
    let mut quiz = Quiz::new("Test");

    quiz.add_question(
        QuizQuestion::multiple_choice("Q1")
            .with_options(vec!["A".to_string(), "B".to_string()], vec![0]),
    );
    quiz.add_question(
        QuizQuestion::multiple_choice("Q2")
            .with_options(vec!["A".to_string(), "B".to_string()], vec![1]),
    );

    let q_id = quiz.questions[0].id.clone();
    let mut answers = HashMap::new();
    answers.insert(q_id, vec![0]); // Only answer first question

    let result = QuizResult::from_attempt(&quiz, answers, 60);

    // Score should be 1 (only one correct answer)
    assert_eq!(result.score, 1);
}

#[test]
fn test_code_explanation_clone() {
    let explanation = CodeExplanation {
        code: "let x = 1;".to_string(),
        explanation: "Declares variable".to_string(),
        line_explanations: vec![],
        concepts: vec!["variable".to_string()],
        related_topics: vec![],
        level: ExplanationLevel::Beginner,
    };

    let cloned = explanation.clone();
    assert_eq!(explanation.code, cloned.code);
    assert_eq!(explanation.level, cloned.level);
}

#[test]
fn test_exercise_hints() {
    let exercise = Exercise::new("Sum", "Calculate the sum")
        .add_hint("First hint")
        .add_hint("Second hint");

    assert_eq!(exercise.hints.len(), 2);
}

#[test]
fn test_concept_clone() {
    let concept = Concept::new("Variables", "basics")
        .with_difficulty(Difficulty::Beginner)
        .with_description("Variable declaration");

    let cloned = concept.clone();
    assert_eq!(concept.name, cloned.name);
    assert_eq!(concept.category, cloned.category);
}

#[test]
fn test_lesson_clone() {
    let lesson = Lesson::new("Intro", 1).with_description("Introduction");

    let cloned = lesson.clone();
    assert_eq!(lesson.title, cloned.title);
    assert_eq!(lesson.order, cloned.order);
}

#[test]
fn test_quiz_question_default_points() {
    let question = QuizQuestion::true_false("Test?", true);
    assert_eq!(question.points, 1);
}

#[test]
fn test_quiz_question_default_difficulty() {
    let question = QuizQuestion::true_false("Hard?", false);
    assert_eq!(question.difficulty, Difficulty::Beginner);
}

#[test]
fn test_explain_struct_line() {
    let mut explainer = CodeExplainer::new();
    let code = "pub struct MyStruct { field: i32 }";
    let explanation = explainer.explain(code);

    assert!(!explanation.concepts.is_empty());
}

#[test]
fn test_explain_impl_line() {
    let mut explainer = CodeExplainer::new();
    let code = "impl MyTrait for MyType { }";
    let explanation = explainer.explain(code);

    assert!(explanation.concepts.contains(&"implementation".to_string()));
}

#[test]
fn test_explain_async_code() {
    let mut explainer = CodeExplainer::new();
    let code = "async fn fetch() { data.await }";
    let explanation = explainer.explain(code);

    assert!(explanation.concepts.contains(&"async".to_string()));
}
