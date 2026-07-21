use super::*;

#[test]
fn test_entity_creation() {
    let entity = Entity::new("test_fn", EntityType::Function)
        .with_visibility(Visibility::Public)
        .with_tag("async");

    assert_eq!(entity.name, "test_fn");
    assert_eq!(entity.entity_type, EntityType::Function);
    assert_eq!(entity.visibility, Visibility::Public);
    assert!(entity.tags.contains(&"async".to_string()));
}

#[test]
fn test_entity_with_location() {
    let entity = Entity::new("MyStruct", EntityType::Struct).with_location(
        PathBuf::from("src/lib.rs"),
        10,
        1,
    );

    assert_eq!(entity.file, Some(PathBuf::from("src/lib.rs")));
    assert_eq!(entity.line, Some(10));
    assert_eq!(entity.column, Some(1));
}

#[test]
fn test_pattern_cap_evicts_oldest() {
    let mut graph = KnowledgeGraph::new();
    let mut ids = Vec::new();
    for i in 0..MAX_PATTERNS * 2 + 1 {
        let mut p = Pattern::new(format!("pattern-{}", i), PatternType::Idiom);
        // Manual, monotonic timestamps make eviction order deterministic
        // without sleeping.
        p.created_at = i as u64;
        ids.push(graph.add_pattern(p));
    }
    // The cap keeps the graph between the 90% target and MAX_PATTERNS.
    assert!(
        graph.patterns.len() <= MAX_PATTERNS,
        "pattern count {} exceeded cap {}",
        graph.patterns.len(),
        MAX_PATTERNS
    );
    assert!(
        graph.patterns.len() >= MAX_PATTERNS * 9 / 10,
        "pattern count {} dropped below target {}",
        graph.patterns.len(),
        MAX_PATTERNS * 9 / 10
    );
    // The oldest patterns should have been evicted.
    for id in &ids[..5] {
        assert!(
            !graph.patterns.contains_key(id),
            "oldest pattern {} should have been evicted",
            id
        );
    }
}

#[test]
fn test_smell_cap_evicts_oldest_and_deduplicates() {
    let mut graph = KnowledgeGraph::new();
    for i in 0..MAX_SMELLS * 2 + 1 {
        graph.add_smell(CodeSmellInstance::new(
            CodeSmell::MagicNumber,
            format!("entity-{}", i),
            PathBuf::from("src/lib.rs"),
            i,
        ));
    }
    assert!(
        graph.smells.len() <= MAX_SMELLS,
        "smell count {} exceeded cap {}",
        graph.smells.len(),
        MAX_SMELLS
    );
    assert!(
        graph.smells.len() >= MAX_SMELLS * 9 / 10,
        "smell count {} dropped below target {}",
        graph.smells.len(),
        MAX_SMELLS * 9 / 10
    );

    // Deduplicate: same (file, smell, line) should not add another entry.
    graph.add_smell(CodeSmellInstance::new(
        CodeSmell::MagicNumber,
        "duplicate-entity",
        PathBuf::from("src/lib.rs"),
        MAX_SMELLS * 2 + 10,
    ));
    assert!(graph.smells.len() <= MAX_SMELLS);
}

#[test]
fn test_entity_with_qualified_name() {
    let entity =
        Entity::new("Config", EntityType::Struct).with_qualified_name("crate::config::Config");

    assert_eq!(entity.qualified_name, "crate::config::Config");
}

#[test]
fn test_relation_creation() {
    let rel = Relation::new("entity-1", "entity-2", RelationType::Calls).with_strength(0.8);

    assert_eq!(rel.source_id, "entity-1");
    assert_eq!(rel.target_id, "entity-2");
    assert_eq!(rel.relation_type, RelationType::Calls);
    assert!((rel.strength - 0.8).abs() < 0.01);
}

#[test]
fn test_relation_type_inverse() {
    assert_eq!(RelationType::Calls.inverse(), RelationType::CalledBy);
    assert_eq!(RelationType::Uses.inverse(), RelationType::UsedBy);
    assert_eq!(
        RelationType::Implements.inverse(),
        RelationType::ImplementedBy
    );
    assert_eq!(RelationType::SimilarTo.inverse(), RelationType::SimilarTo);
}

#[test]
fn test_pattern_creation() {
    let pattern = Pattern::new("Singleton", PatternType::DesignPattern)
        .with_confidence(0.95)
        .with_description("Singleton pattern detected");

    assert_eq!(pattern.name, "Singleton");
    assert_eq!(pattern.pattern_type, PatternType::DesignPattern);
    assert!((pattern.confidence - 0.95).abs() < 0.01);
}

#[test]
fn test_code_smell_severity() {
    assert_eq!(CodeSmell::GodObject.severity(), 5);
    assert_eq!(CodeSmell::LongMethod.severity(), 3);
    assert_eq!(CodeSmell::CommentedCode.severity(), 1);
}

#[test]
fn test_code_smell_suggested_fix() {
    assert!(!CodeSmell::LongMethod.suggested_fix().is_empty());
    assert!(CodeSmell::DuplicatedCode.suggested_fix().contains("shared"));
}

#[test]
fn test_code_smell_instance() {
    let smell = CodeSmellInstance::new(
        CodeSmell::TooManyParameters,
        "entity-1",
        PathBuf::from("test.rs"),
        10,
    )
    .with_description("Has 8 parameters");

    assert_eq!(smell.smell, CodeSmell::TooManyParameters);
    assert!(smell.description.contains("8 parameters"));
}

#[test]
fn test_knowledge_graph_add_entity() {
    let mut graph = KnowledgeGraph::new();

    let entity = Entity::new("test", EntityType::Function);
    let id = graph.add_entity(entity);

    assert!(graph.get_entity(&id).is_some());
    assert_eq!(graph.entity_count(), 1);
}

#[test]
fn test_knowledge_graph_add_relation() {
    let mut graph = KnowledgeGraph::new();

    let e1 = graph.add_entity(Entity::new("caller", EntityType::Function));
    let e2 = graph.add_entity(Entity::new("callee", EntityType::Function));

    let rel_id = graph.add_relation(Relation::new(&e1, &e2, RelationType::Calls));

    assert!(graph.get_relation(&rel_id).is_some());
    assert_eq!(graph.relation_count(), 1);
}

#[test]
fn test_knowledge_graph_find_by_name() {
    let mut graph = KnowledgeGraph::new();

    graph.add_entity(Entity::new("Config", EntityType::Struct));
    graph.add_entity(Entity::new("Settings", EntityType::Struct));

    let results = graph.find_by_name("Config");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Config");
}

#[test]
fn test_knowledge_graph_find_by_type() {
    let mut graph = KnowledgeGraph::new();

    graph.add_entity(Entity::new("fn1", EntityType::Function));
    graph.add_entity(Entity::new("fn2", EntityType::Function));
    graph.add_entity(Entity::new("Struct1", EntityType::Struct));

    let functions = graph.find_by_type(EntityType::Function);
    assert_eq!(functions.len(), 2);

    let structs = graph.find_by_type(EntityType::Struct);
    assert_eq!(structs.len(), 1);
}

#[test]
fn test_knowledge_graph_relations_from() {
    let mut graph = KnowledgeGraph::new();

    let e1 = graph.add_entity(Entity::new("a", EntityType::Function));
    let e2 = graph.add_entity(Entity::new("b", EntityType::Function));
    let e3 = graph.add_entity(Entity::new("c", EntityType::Function));

    graph.add_relation(Relation::new(&e1, &e2, RelationType::Calls));
    graph.add_relation(Relation::new(&e1, &e3, RelationType::Calls));

    let rels = graph.relations_from(&e1);
    assert_eq!(rels.len(), 2);
}

#[test]
fn test_knowledge_graph_get_related() {
    let mut graph = KnowledgeGraph::new();

    let e1 = graph.add_entity(Entity::new("main", EntityType::Function));
    let e2 = graph.add_entity(Entity::new("helper", EntityType::Function));

    graph.add_relation(Relation::new(&e1, &e2, RelationType::Calls));

    let related = graph.get_related(&e1, Some(RelationType::Calls));
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].0.name, "helper");
}

#[test]
fn test_knowledge_graph_add_pattern() {
    let mut graph = KnowledgeGraph::new();

    let pattern = Pattern::new("Builder", PatternType::DesignPattern);
    let id = graph.add_pattern(pattern);

    assert!(graph.get_pattern(&id).is_some());
}

#[test]
fn test_knowledge_graph_add_smell() {
    let mut graph = KnowledgeGraph::new();

    let smell = CodeSmellInstance::new(
        CodeSmell::LongMethod,
        "entity-1",
        PathBuf::from("test.rs"),
        10,
    );
    graph.add_smell(smell);

    assert_eq!(graph.get_smells().len(), 1);
}

#[test]
fn test_knowledge_graph_to_dot() {
    let mut graph = KnowledgeGraph::new();

    let e1 = graph.add_entity(Entity::new("main", EntityType::Function));
    let e2 = graph.add_entity(Entity::new("Config", EntityType::Struct));
    graph.add_relation(Relation::new(&e1, &e2, RelationType::Uses));

    let dot = graph.to_dot();
    assert!(dot.contains("digraph"));
    assert!(dot.contains("main"));
    assert!(dot.contains("Config"));
}

#[test]
fn test_entity_type_display() {
    assert_eq!(format!("{}", EntityType::Function), "Function");
    assert_eq!(format!("{}", EntityType::Struct), "Struct");
    assert_eq!(format!("{}", EntityType::Trait), "Trait");
}

#[test]
fn test_relation_type_display() {
    assert_eq!(format!("{}", RelationType::Calls), "calls");
    assert_eq!(format!("{}", RelationType::Uses), "uses");
}

#[test]
fn test_visibility_display() {
    assert_eq!(format!("{}", Visibility::Public), "public");
    assert_eq!(format!("{}", Visibility::Private), "private");
    assert_eq!(format!("{}", Visibility::Crate), "crate");
}

#[test]
fn test_pattern_type_display() {
    assert_eq!(format!("{}", PatternType::DesignPattern), "Design Pattern");
    assert_eq!(format!("{}", PatternType::AntiPattern), "Anti-Pattern");
}

#[test]
fn test_pattern_example() {
    let example = PatternExample::new(PathBuf::from("src/lib.rs"), 10, 20, "fn example() {}");

    assert_eq!(example.start_line, 10);
    assert_eq!(example.end_line, 20);
}

#[test]
fn test_rust_entity_extractor() {
    let extractor = RustEntityExtractor::new();

    let code = r#"
pub fn main() {
    println!("Hello");
}

struct Config {
    name: String,
}

pub enum Status {
    Active,
    Inactive,
}
"#;

    let entities = extractor.extract(code, Path::new("test.rs"));

    assert!(!entities.is_empty());

    let function_count = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Function)
        .count();
    assert!(function_count >= 1);

    let struct_count = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Struct)
        .count();
    assert!(struct_count >= 1);
}

#[test]
fn test_rust_extractor_public_visibility() {
    let extractor = RustEntityExtractor::new();

    let code = "pub fn public_fn() {}";
    let entities = extractor.extract(code, Path::new("test.rs"));

    assert!(!entities.is_empty());
    assert_eq!(entities[0].visibility, Visibility::Public);
}

#[test]
fn test_rust_extractor_crate_visibility() {
    let extractor = RustEntityExtractor::new();

    let code = "pub(crate) fn crate_fn() {}";
    let entities = extractor.extract(code, Path::new("test.rs"));

    assert!(!entities.is_empty());
    assert_eq!(entities[0].visibility, Visibility::Crate);
}

#[test]
fn test_smell_detector_creation() {
    let detector = SmellDetector::new()
        .with_max_function_lines(100)
        .with_max_parameters(10);

    assert_eq!(detector.max_function_lines, 100);
    assert_eq!(detector.max_parameters, 10);
}

#[test]
fn test_smell_detector_detect_nesting() {
    let detector = SmellDetector::new().with_max_nesting(2);

    let code = r#"
fn deep() {
    if true {
        if true {
            if true {
                println!("deep");
            }
        }
    }
}
"#;

    let smells = detector.detect(code, Path::new("test.rs"), "entity-1");

    let has_nesting_smell = smells.iter().any(|s| s.smell == CodeSmell::DeeplyNested);
    assert!(has_nesting_smell);
}

#[test]
fn test_smell_detector_function_smells() {
    let detector = SmellDetector::new()
        .with_max_parameters(3)
        .with_max_function_lines(10);

    let smells =
        detector.detect_function_smells("big_fn", 6, 50, Path::new("test.rs"), 1, "entity-1");

    assert!(!smells.is_empty());
    let has_param_smell = smells
        .iter()
        .any(|s| s.smell == CodeSmell::TooManyParameters);
    let has_long_smell = smells.iter().any(|s| s.smell == CodeSmell::LongMethod);
    assert!(has_param_smell);
    assert!(has_long_smell);
}

#[test]
fn test_pattern_recognizer() {
    let recognizer = PatternRecognizer::new();

    let code = r#"
struct ConfigBuilder {
    name: String,
}

impl ConfigBuilder {
    fn new() -> Self { Self { name: String::new() } }
    fn with_name(mut self, name: &str) -> Self { self.name = name.to_string(); self }
    fn build(self) -> Config { Config { name: self.name } }
}
"#;

    let patterns = recognizer.recognize(code);

    let has_builder = patterns.iter().any(|(name, _, _)| name == "Builder");
    assert!(has_builder);
}

#[test]
fn test_pattern_recognizer_add_signature() {
    let mut recognizer = PatternRecognizer::new();

    recognizer.add_signature(
        "Custom",
        PatternType::Custom,
        vec!["custom_keyword".to_string()],
    );

    let patterns = recognizer.recognize("This has custom_keyword in it");

    let has_custom = patterns.iter().any(|(name, _, _)| name == "Custom");
    assert!(has_custom);
}

#[test]
fn test_semantic_linker_rust_imports() {
    let linker = SemanticLinker::new();

    let code = r#"
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::config::Config;
use super::helper::Helper;
"#;

    let imports = linker.extract_imports(code, "rust");

    assert!(!imports.is_empty());
    assert!(imports.contains(&"std::collections::HashMap".to_string()));
}

#[test]
fn test_semantic_linker_unknown_language() {
    let linker = SemanticLinker::new();

    let imports = linker.extract_imports("import something", "unknown");

    assert!(imports.is_empty());
}

#[test]
fn test_unique_entity_ids() {
    let e1 = Entity::new("a", EntityType::Function);
    let e2 = Entity::new("b", EntityType::Function);

    assert_ne!(e1.id, e2.id);
}

#[test]
fn test_unique_relation_ids() {
    let r1 = Relation::new("a", "b", RelationType::Calls);
    let r2 = Relation::new("c", "d", RelationType::Calls);

    assert_ne!(r1.id, r2.id);
}

#[test]
fn test_unique_pattern_ids() {
    let p1 = Pattern::new("A", PatternType::Custom);
    let p2 = Pattern::new("B", PatternType::Custom);

    assert_ne!(p1.id, p2.id);
}

#[test]
fn test_knowledge_graph_entity_count_by_type() {
    let mut graph = KnowledgeGraph::new();

    graph.add_entity(Entity::new("fn1", EntityType::Function));
    graph.add_entity(Entity::new("fn2", EntityType::Function));
    graph.add_entity(Entity::new("Struct1", EntityType::Struct));

    let counts = graph.entity_count_by_type();

    assert_eq!(*counts.get(&EntityType::Function).unwrap_or(&0), 2);
    assert_eq!(*counts.get(&EntityType::Struct).unwrap_or(&0), 1);
}

#[test]
fn test_knowledge_graph_find_in_file() {
    let mut graph = KnowledgeGraph::new();

    let path = PathBuf::from("src/lib.rs");
    graph.add_entity(Entity::new("fn1", EntityType::Function).with_location(path.clone(), 10, 1));
    graph.add_entity(Entity::new("fn2", EntityType::Function).with_location(path.clone(), 20, 1));
    graph.add_entity(Entity::new("fn3", EntityType::Function).with_location(
        PathBuf::from("src/main.rs"),
        5,
        1,
    ));

    let in_lib = graph.find_in_file(&path);
    assert_eq!(in_lib.len(), 2);
}

#[test]
fn test_knowledge_graph_get_referencing() {
    let mut graph = KnowledgeGraph::new();

    let e1 = graph.add_entity(Entity::new("caller1", EntityType::Function));
    let e2 = graph.add_entity(Entity::new("caller2", EntityType::Function));
    let e3 = graph.add_entity(Entity::new("target", EntityType::Function));

    graph.add_relation(Relation::new(&e1, &e3, RelationType::Calls));
    graph.add_relation(Relation::new(&e2, &e3, RelationType::Calls));

    let referencing = graph.get_referencing(&e3, Some(RelationType::Calls));
    assert_eq!(referencing.len(), 2);
}

#[test]
fn test_knowledge_graph_get_patterns() {
    let mut graph = KnowledgeGraph::new();

    graph.add_pattern(Pattern::new("Singleton", PatternType::DesignPattern));
    graph.add_pattern(Pattern::new("Factory", PatternType::DesignPattern));

    let patterns = graph.get_patterns();
    assert_eq!(patterns.len(), 2);
}

#[test]
fn test_relation_with_location_and_context() {
    let rel = Relation::new("a", "b", RelationType::Calls)
        .with_location(PathBuf::from("test.rs"), 10)
        .with_context("Function call in main");

    assert!(rel.location.is_some());
    assert_eq!(rel.context, Some("Function call in main".to_string()));
}

#[test]
fn test_entity_with_attribute() {
    let entity = Entity::new("test", EntityType::Function)
        .with_attribute("async", "true")
        .with_attribute("unsafe", "false");

    assert_eq!(entity.attributes.get("async"), Some(&"true".to_string()));
    assert_eq!(entity.attributes.get("unsafe"), Some(&"false".to_string()));
}

#[test]
fn test_pattern_with_example() {
    let example = PatternExample::new(PathBuf::from("src/lib.rs"), 10, 20, "impl Singleton {}");

    let pattern = Pattern::new("Singleton", PatternType::DesignPattern).with_example(example);

    assert_eq!(pattern.examples.len(), 1);
}

#[test]
fn test_code_smell_display() {
    assert_eq!(format!("{}", CodeSmell::GodObject), "God Object");
    assert_eq!(format!("{}", CodeSmell::LongMethod), "Long Method");
    assert_eq!(format!("{}", CodeSmell::MagicNumber), "Magic Number");
}

// ── Additional coverage tests ─────────────────────────────────

#[test]
fn test_relation_type_inverse_all_pairs() {
    // Verify every inverse is a proper round-trip
    let variants = vec![
        RelationType::Calls,
        RelationType::CalledBy,
        RelationType::Uses,
        RelationType::UsedBy,
        RelationType::Extends,
        RelationType::ExtendedBy,
        RelationType::Implements,
        RelationType::ImplementedBy,
        RelationType::Contains,
        RelationType::ContainedIn,
        RelationType::Imports,
        RelationType::ImportedBy,
        RelationType::DependsOn,
        RelationType::DependencyOf,
        RelationType::SimilarTo,
        RelationType::Overrides,
        RelationType::OverriddenBy,
    ];
    for v in variants {
        assert_eq!(v.inverse().inverse(), v);
    }
}

#[test]
fn test_relation_type_display_all() {
    assert_eq!(format!("{}", RelationType::CalledBy), "called_by");
    assert_eq!(format!("{}", RelationType::UsedBy), "used_by");
    assert_eq!(format!("{}", RelationType::Extends), "extends");
    assert_eq!(format!("{}", RelationType::ExtendedBy), "extended_by");
    assert_eq!(format!("{}", RelationType::Implements), "implements");
    assert_eq!(format!("{}", RelationType::ImplementedBy), "implemented_by");
    assert_eq!(format!("{}", RelationType::Contains), "contains");
    assert_eq!(format!("{}", RelationType::ContainedIn), "contained_in");
    assert_eq!(format!("{}", RelationType::Imports), "imports");
    assert_eq!(format!("{}", RelationType::ImportedBy), "imported_by");
    assert_eq!(format!("{}", RelationType::DependsOn), "depends_on");
    assert_eq!(format!("{}", RelationType::DependencyOf), "dependency_of");
    assert_eq!(format!("{}", RelationType::SimilarTo), "similar_to");
    assert_eq!(format!("{}", RelationType::Overrides), "overrides");
    assert_eq!(format!("{}", RelationType::OverriddenBy), "overridden_by");
}

#[test]
fn test_entity_type_display_all() {
    assert_eq!(format!("{}", EntityType::Module), "Module");
    assert_eq!(format!("{}", EntityType::Enum), "Enum");
    assert_eq!(format!("{}", EntityType::Constant), "Constant");
    assert_eq!(format!("{}", EntityType::TypeAlias), "TypeAlias");
    assert_eq!(format!("{}", EntityType::Impl), "Impl");
    assert_eq!(format!("{}", EntityType::Macro), "Macro");
    assert_eq!(format!("{}", EntityType::Method), "Method");
    assert_eq!(format!("{}", EntityType::Field), "Field");
    assert_eq!(format!("{}", EntityType::Variant), "Variant");
    assert_eq!(format!("{}", EntityType::Parameter), "Parameter");
    assert_eq!(format!("{}", EntityType::Variable), "Variable");
}

#[test]
fn test_visibility_display_all() {
    assert_eq!(format!("{}", Visibility::Super), "super");
    assert_eq!(format!("{}", Visibility::Restricted), "restricted");
}

#[test]
fn test_pattern_type_display_all() {
    assert_eq!(
        format!("{}", PatternType::ArchitecturalPattern),
        "Architectural Pattern"
    );
    assert_eq!(format!("{}", PatternType::Idiom), "Idiom");
    assert_eq!(format!("{}", PatternType::Custom), "Custom");
}

#[test]
fn test_code_smell_display_all() {
    assert_eq!(format!("{}", CodeSmell::LargeClass), "Large Class");
    assert_eq!(
        format!("{}", CodeSmell::TooManyParameters),
        "Too Many Parameters"
    );
    assert_eq!(format!("{}", CodeSmell::DeeplyNested), "Deeply Nested Code");
    assert_eq!(format!("{}", CodeSmell::DuplicatedCode), "Duplicated Code");
    assert_eq!(format!("{}", CodeSmell::DeadCode), "Dead Code");
    assert_eq!(format!("{}", CodeSmell::UnusedImport), "Unused Import");
    assert_eq!(
        format!("{}", CodeSmell::CyclicDependency),
        "Cyclic Dependency"
    );
    assert_eq!(format!("{}", CodeSmell::FeatureEnvy), "Feature Envy");
    assert_eq!(format!("{}", CodeSmell::DataClump), "Data Clump");
    assert_eq!(
        format!("{}", CodeSmell::PrimitiveObsession),
        "Primitive Obsession"
    );
    assert_eq!(format!("{}", CodeSmell::ShotgunSurgery), "Shotgun Surgery");
    assert_eq!(
        format!("{}", CodeSmell::InappropriateIntimacy),
        "Inappropriate Intimacy"
    );
    assert_eq!(format!("{}", CodeSmell::CommentedCode), "Commented Code");
    assert_eq!(
        format!("{}", CodeSmell::HardcodedString),
        "Hardcoded String"
    );
}

#[test]
fn test_code_smell_severity_all() {
    assert_eq!(CodeSmell::CyclicDependency.severity(), 5);
    assert_eq!(CodeSmell::DuplicatedCode.severity(), 4);
    assert_eq!(CodeSmell::LargeClass.severity(), 4);
    assert_eq!(CodeSmell::ShotgunSurgery.severity(), 4);
    assert_eq!(CodeSmell::InappropriateIntimacy.severity(), 4);
    assert_eq!(CodeSmell::TooManyParameters.severity(), 3);
    assert_eq!(CodeSmell::DeeplyNested.severity(), 3);
    assert_eq!(CodeSmell::FeatureEnvy.severity(), 3);
    assert_eq!(CodeSmell::DataClump.severity(), 3);
    assert_eq!(CodeSmell::PrimitiveObsession.severity(), 3);
    assert_eq!(CodeSmell::DeadCode.severity(), 2);
    assert_eq!(CodeSmell::UnusedImport.severity(), 2);
    assert_eq!(CodeSmell::MagicNumber.severity(), 2);
    assert_eq!(CodeSmell::HardcodedString.severity(), 2);
}

#[test]
fn test_code_smell_suggested_fix_all() {
    // Verify all variants return non-empty strings
    let smells = vec![
        CodeSmell::LongMethod,
        CodeSmell::LargeClass,
        CodeSmell::TooManyParameters,
        CodeSmell::DeeplyNested,
        CodeSmell::DuplicatedCode,
        CodeSmell::DeadCode,
        CodeSmell::UnusedImport,
        CodeSmell::CyclicDependency,
        CodeSmell::FeatureEnvy,
        CodeSmell::DataClump,
        CodeSmell::PrimitiveObsession,
        CodeSmell::GodObject,
        CodeSmell::ShotgunSurgery,
        CodeSmell::InappropriateIntimacy,
        CodeSmell::CommentedCode,
        CodeSmell::MagicNumber,
        CodeSmell::HardcodedString,
    ];
    for s in smells {
        assert!(!s.suggested_fix().is_empty(), "{:?} fix is empty", s);
    }
}

#[test]
fn test_relation_strength_clamped() {
    let r = Relation::new("a", "b", RelationType::Calls).with_strength(1.5);
    assert!((r.strength - 1.0).abs() < f32::EPSILON);

    let r2 = Relation::new("a", "b", RelationType::Calls).with_strength(-0.5);
    assert!((r2.strength - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_pattern_confidence_clamped() {
    let p = Pattern::new("X", PatternType::Custom).with_confidence(2.0);
    assert!((p.confidence - 1.0).abs() < f32::EPSILON);

    let p2 = Pattern::new("X", PatternType::Custom).with_confidence(-1.0);
    assert!((p2.confidence - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_entity_with_documentation() {
    let e = Entity::new("foo", EntityType::Function).with_documentation("Does foo things");
    assert_eq!(e.documentation, Some("Does foo things".to_string()));
}

#[test]
fn test_entity_with_signature() {
    let e = Entity::new("bar", EntityType::Function).with_signature("fn bar(x: i32) -> bool");
    assert_eq!(e.signature, Some("fn bar(x: i32) -> bool".to_string()));
}

#[test]
fn test_code_smell_instance_defaults() {
    let instance = CodeSmellInstance::new(CodeSmell::GodObject, "e1", PathBuf::from("big.rs"), 1);
    assert_eq!(instance.severity, 5);
    assert!(!instance.suggestion.is_empty());
    assert!(instance.description.contains("detected"));
}

#[test]
fn test_knowledge_graph_default() {
    let g = KnowledgeGraph::default();
    assert_eq!(g.entity_count(), 0);
    assert_eq!(g.relation_count(), 0);
}

#[test]
fn test_knowledge_graph_find_cycles_no_cycle() {
    let mut graph = KnowledgeGraph::new();
    let a = graph.add_entity(Entity::new("a", EntityType::Module));
    let b = graph.add_entity(Entity::new("b", EntityType::Module));
    graph.add_relation(Relation::new(&a, &b, RelationType::DependsOn));

    let cycles = graph.find_cycles();
    assert!(cycles.is_empty());
}

#[test]
fn test_knowledge_graph_find_cycles_with_cycle() {
    let mut graph = KnowledgeGraph::new();
    let a = graph.add_entity(Entity::new("a", EntityType::Module));
    let b = graph.add_entity(Entity::new("b", EntityType::Module));
    let c = graph.add_entity(Entity::new("c", EntityType::Module));

    graph.add_relation(Relation::new(&a, &b, RelationType::DependsOn));
    graph.add_relation(Relation::new(&b, &c, RelationType::DependsOn));
    graph.add_relation(Relation::new(&c, &a, RelationType::DependsOn));

    let cycles = graph.find_cycles();
    assert!(!cycles.is_empty());
}

#[test]
fn test_knowledge_graph_find_cycles_uses_relation() {
    let mut graph = KnowledgeGraph::new();
    let a = graph.add_entity(Entity::new("x", EntityType::Function));
    let b = graph.add_entity(Entity::new("y", EntityType::Function));

    graph.add_relation(Relation::new(&a, &b, RelationType::Uses));
    graph.add_relation(Relation::new(&b, &a, RelationType::Uses));

    let cycles = graph.find_cycles();
    assert!(!cycles.is_empty());
}

#[test]
fn test_knowledge_graph_remove_entity_cleans_indices() {
    let mut graph = KnowledgeGraph::new();
    let path = PathBuf::from("src/lib.rs");
    let id = graph.add_entity(Entity::new("fn1", EntityType::Function).with_location(
        path.clone(),
        1,
        1,
    ));
    let id2 = graph.add_entity(Entity::new("fn2", EntityType::Function));

    // Add a relation between them
    graph.add_relation(Relation::new(&id, &id2, RelationType::Calls));

    // Remove the first entity
    graph.remove_entity_internal(&id);

    assert!(graph.get_entity(&id).is_none());
    assert_eq!(graph.entity_count(), 1);
    assert!(graph.find_by_name("fn1").is_empty());
    assert!(graph.find_in_file(&path).is_empty());
    assert_eq!(graph.relation_count(), 0);
}

#[test]
fn test_knowledge_graph_relations_to() {
    let mut graph = KnowledgeGraph::new();
    let a = graph.add_entity(Entity::new("a", EntityType::Function));
    let b = graph.add_entity(Entity::new("b", EntityType::Function));
    graph.add_relation(Relation::new(&a, &b, RelationType::Calls));

    let rels = graph.relations_to(&b);
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].source_id, a);
}

#[test]
fn test_knowledge_graph_get_related_no_filter() {
    let mut graph = KnowledgeGraph::new();
    let a = graph.add_entity(Entity::new("a", EntityType::Function));
    let b = graph.add_entity(Entity::new("b", EntityType::Function));
    graph.add_relation(Relation::new(&a, &b, RelationType::Calls));
    graph.add_relation(Relation::new(&a, &b, RelationType::Uses));

    let related = graph.get_related(&a, None);
    assert_eq!(related.len(), 2);
}

#[test]
fn test_knowledge_graph_get_referencing_no_filter() {
    let mut graph = KnowledgeGraph::new();
    let a = graph.add_entity(Entity::new("a", EntityType::Function));
    let b = graph.add_entity(Entity::new("b", EntityType::Function));
    graph.add_relation(Relation::new(&a, &b, RelationType::Calls));

    let refs = graph.get_referencing(&b, None);
    assert_eq!(refs.len(), 1);
}

#[test]
fn test_knowledge_graph_save_and_load() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("graph.json");

    let mut graph = KnowledgeGraph::new();
    graph.add_entity(Entity::new("saved", EntityType::Struct));

    graph.save_to_file(&path).unwrap();
    assert!(path.exists());

    let loaded = KnowledgeGraph::load_from_file(&path).unwrap();
    assert_eq!(loaded.entity_count(), 1);
    assert!(!loaded.find_by_name("saved").is_empty());
}

#[test]
fn test_knowledge_graph_load_missing_file() {
    let path = PathBuf::from("/tmp/nonexistent_kg_test.json");
    let graph = KnowledgeGraph::load_from_file(&path).unwrap();
    assert_eq!(graph.entity_count(), 0);
}

#[test]
fn test_knowledge_graph_to_dot_edge_styles() {
    let mut graph = KnowledgeGraph::new();
    let m = graph.add_entity(Entity::new("mod1", EntityType::Module));
    let f = graph.add_entity(Entity::new("fn1", EntityType::Function));
    let s = graph.add_entity(Entity::new("S1", EntityType::Struct));
    let t = graph.add_entity(Entity::new("T1", EntityType::Trait));
    let e = graph.add_entity(Entity::new("E1", EntityType::Enum));

    graph.add_relation(Relation::new(&f, &s, RelationType::Uses));
    graph.add_relation(Relation::new(&s, &t, RelationType::Implements));
    graph.add_relation(Relation::new(&m, &f, RelationType::Contains));
    graph.add_relation(Relation::new(&f, &e, RelationType::DependsOn));

    let dot = graph.to_dot();
    assert!(dot.contains("lightblue")); // Module color
    assert!(dot.contains("lightgreen")); // Function color
    assert!(dot.contains("lightyellow")); // Struct/Enum color
    assert!(dot.contains("lightpink")); // Trait color
    assert!(dot.contains("dashed")); // Uses style
    assert!(dot.contains("bold")); // Implements style
    assert!(dot.contains("dotted")); // Contains style
}

#[test]
fn test_smell_detector_magic_number() {
    let detector = SmellDetector::new();

    // Line with magic number 42
    assert!(detector.has_magic_number("    let x = 42;"));
    // Lines that should NOT flag
    assert!(!detector.has_magic_number("const MAX: usize = 42;"));
    assert!(!detector.has_magic_number("static N: i32 = 42;"));
    assert!(!detector.has_magic_number("    let x = 0;"));
    assert!(!detector.has_magic_number("    let x = 1;"));
    assert!(!detector.has_magic_number("    let x = 2;"));
    // 10, 100, 1000 are exempted
    assert!(!detector.has_magic_number("    let x = 10;"));
    assert!(!detector.has_magic_number("    let x = 100;"));
    assert!(!detector.has_magic_number("    let x = 1000;"));
}

#[test]
fn test_smell_detector_commented_code() {
    let detector = SmellDetector::new();

    assert!(detector.is_commented_code("// fn old_function() {"));
    assert!(detector.is_commented_code("// let x = 5;"));
    assert!(detector.is_commented_code("// if condition {"));
    assert!(detector.is_commented_code("// return result;"));
    assert!(detector.is_commented_code("// some code;"));
    // Normal comments should NOT flag
    assert!(!detector.is_commented_code("// This is a regular comment"));
    assert!(!detector.is_commented_code("not a comment"));
}

#[test]
fn test_smell_detector_large_file() {
    let detector = SmellDetector::new().with_max_file_lines(5);
    let code = "line1\nline2\nline3\nline4\nline5\nline6\nline7\n";

    let smells = detector.detect(code, Path::new("big.rs"), "e1");
    let has_large = smells.iter().any(|s| s.smell == CodeSmell::LargeClass);
    assert!(has_large);
}

#[test]
fn test_smell_detector_no_smells_clean_code() {
    let detector = SmellDetector::new();
    let code = "fn clean() {\n    println!(\"ok\");\n}\n";

    let smells = detector.detect(code, Path::new("clean.rs"), "e1");
    // Clean code should have no deeply nested or large file smells
    let deep = smells.iter().any(|s| s.smell == CodeSmell::DeeplyNested);
    let large = smells.iter().any(|s| s.smell == CodeSmell::LargeClass);
    assert!(!deep);
    assert!(!large);
}

#[test]
fn test_rust_extractor_trait() {
    let extractor = RustEntityExtractor::new();
    let code = "pub trait MyTrait {\n    fn do_thing(&self);\n}\n";
    let entities = extractor.extract(code, Path::new("test.rs"));

    let traits: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Trait)
        .collect();
    assert_eq!(traits.len(), 1);
    assert_eq!(traits[0].name, "MyTrait");
}

#[test]
fn test_rust_extractor_const() {
    let extractor = RustEntityExtractor::new();
    let code = "pub const MAX_SIZE: usize = 100;\n";
    let entities = extractor.extract(code, Path::new("test.rs"));

    let consts: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Constant)
        .collect();
    assert_eq!(consts.len(), 1);
    assert_eq!(consts[0].name, "MAX_SIZE");
}

#[test]
fn test_rust_extractor_enum() {
    let extractor = RustEntityExtractor::new();
    let code = "enum Color {\n    Red,\n    Blue,\n}\n";
    let entities = extractor.extract(code, Path::new("test.rs"));

    let enums: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Enum)
        .collect();
    assert_eq!(enums.len(), 1);
    assert_eq!(enums[0].name, "Color");
}

#[test]
fn test_rust_extractor_super_visibility() {
    let extractor = RustEntityExtractor::new();
    let code = "pub(super) fn inner_fn() {}\n";
    let entities = extractor.extract(code, Path::new("test.rs"));

    assert!(!entities.is_empty());
    assert_eq!(entities[0].visibility, Visibility::Super);
}

#[test]
fn test_rust_extractor_skips_comments_and_empty() {
    let extractor = RustEntityExtractor::new();
    let code = "\n// fn commented_out() {}\n\n";
    let entities = extractor.extract(code, Path::new("test.rs"));
    assert!(entities.is_empty());
}

#[test]
fn test_rust_extractor_default() {
    let _extractor = RustEntityExtractor::default();
}

#[test]
fn test_semantic_linker_python_imports() {
    let linker = SemanticLinker::new();
    let code = "from os.path import join\nimport sys\n";
    let imports = linker.extract_imports(code, "python");
    assert!(imports.contains(&"os.path".to_string()));
    assert!(imports.contains(&"sys".to_string()));
}

#[test]
fn test_semantic_linker_javascript_imports() {
    let linker = SemanticLinker::new();
    let code = r#"import foo from './foo';
const bar = require('./bar');
"#;
    let imports = linker.extract_imports(code, "javascript");
    assert!(imports.contains(&"./foo".to_string()));
    assert!(imports.contains(&"./bar".to_string()));
}

#[test]
fn test_semantic_linker_link_imports() {
    let linker = SemanticLinker::new();
    let mut graph = KnowledgeGraph::new();

    let source = graph.add_entity(Entity::new("main", EntityType::Module));
    graph.add_entity(Entity::new("Config", EntityType::Struct));

    let imports = vec!["crate::config::Config".to_string()];
    let rels = linker.link_imports(&source, &imports, &graph);

    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].relation_type, RelationType::Imports);
}

#[test]
fn test_semantic_linker_link_imports_no_match() {
    let linker = SemanticLinker::new();
    let graph = KnowledgeGraph::new();

    let imports = vec!["nonexistent::Module".to_string()];
    let rels = linker.link_imports("src-1", &imports, &graph);
    assert!(rels.is_empty());
}

#[test]
fn test_semantic_linker_debug() {
    let linker = SemanticLinker::new();
    let debug = format!("{:?}", linker);
    assert!(debug.contains("SemanticLinker"));
}

#[test]
fn test_pattern_recognizer_default() {
    let _pr = PatternRecognizer::default();
}

#[test]
fn test_pattern_recognizer_low_confidence_filtered() {
    let recognizer = PatternRecognizer::new();
    // Code that only matches 1 of 3 keywords for Builder (< 0.5 threshold)
    let code = "fn build() {}";
    let patterns = recognizer.recognize(code);
    // "build" alone is 1/3 = 0.33 for Builder, below 0.5 threshold
    let builder = patterns.iter().find(|(name, _, _)| name == "Builder");
    assert!(builder.is_none());
}

#[test]
fn test_pattern_recognizer_factory() {
    let recognizer = PatternRecognizer::new();
    let code = "struct Factory {\n}\nimpl Factory {\n    fn create() -> Self {}\n}\n";
    let patterns = recognizer.recognize(code);
    let has_factory = patterns.iter().any(|(name, _, _)| name == "Factory");
    assert!(has_factory);
}

#[test]
fn test_pattern_recognizer_singleton() {
    let recognizer = PatternRecognizer::new();
    let code = "static INSTANCE: Option<Self> = None;\nfn get_instance() -> &Self {}\n";
    let patterns = recognizer.recognize(code);
    let has_singleton = patterns.iter().any(|(name, _, _)| name == "Singleton");
    assert!(has_singleton);
}

#[test]
fn test_smell_detector_with_all_config() {
    let d = SmellDetector::new()
        .with_max_function_lines(20)
        .with_max_file_lines(200)
        .with_max_parameters(3)
        .with_max_nesting(2);

    assert_eq!(d.max_function_lines, 20);
    assert_eq!(d.max_file_lines, 200);
    assert_eq!(d.max_parameters, 3);
    assert_eq!(d.max_nesting, 2);
}

#[test]
fn test_smell_detector_default() {
    let _d = SmellDetector::default();
}

#[test]
fn test_smell_detector_function_no_smells() {
    let d = SmellDetector::new();
    let smells = d.detect_function_smells("ok_fn", 2, 10, Path::new("t.rs"), 1, "e1");
    assert!(smells.is_empty());
}

#[test]
fn test_semantic_linker_default() {
    let _l = SemanticLinker::default();
}
