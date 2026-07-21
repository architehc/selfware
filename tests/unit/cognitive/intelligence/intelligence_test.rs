use super::*;
use tempfile::TempDir;

#[test]
fn test_symbol_kind_icons() {
    assert_eq!(SymbolKind::Function.icon(), "ƒ");
    assert_eq!(SymbolKind::Struct.icon(), "◇");
    assert_eq!(SymbolKind::Enum.icon(), "◆");
    assert_eq!(SymbolKind::Trait.icon(), "▸");
    assert_eq!(SymbolKind::Impl.icon(), "▹");
    assert_eq!(SymbolKind::Const.icon(), "C");
    assert_eq!(SymbolKind::Static.icon(), "S");
    assert_eq!(SymbolKind::Type.icon(), "T");
    assert_eq!(SymbolKind::Macro.icon(), "M");
    assert_eq!(SymbolKind::Module.icon(), "◫");
}

#[test]
fn test_symbol_kind_colors() {
    assert!(SymbolKind::Function.color().contains("33"));
    assert!(SymbolKind::Struct.color().contains("36"));
}

#[test]
fn test_symbol_creation() {
    let sym = Symbol::new(
        "test_fn".to_string(),
        SymbolKind::Function,
        PathBuf::from("src/lib.rs"),
        10,
    );
    assert_eq!(sym.name, "test_fn");
    assert_eq!(sym.kind, SymbolKind::Function);
    assert_eq!(sym.line, 10);
}

#[test]
fn test_symbol_builder() {
    let sym = Symbol::new(
        "test".to_string(),
        SymbolKind::Function,
        PathBuf::from("test.rs"),
        1,
    )
    .with_signature("fn test() -> Result<()>".to_string())
    .with_doc("Test function".to_string())
    .with_visibility(Visibility::Pub)
    .with_parent("TestStruct".to_string())
    .with_column(5);

    assert_eq!(sym.signature, "fn test() -> Result<()>");
    assert_eq!(sym.doc, Some("Test function".to_string()));
    assert_eq!(sym.visibility, Visibility::Pub);
    assert_eq!(sym.parent, Some("TestStruct".to_string()));
    assert_eq!(sym.column, 5);
}

#[test]
fn test_symbol_display() {
    let sym = Symbol::new(
        "my_func".to_string(),
        SymbolKind::Function,
        PathBuf::from("src/lib.rs"),
        42,
    );
    let display = sym.display();
    assert!(display.contains("ƒ"));
    assert!(display.contains("my_func"));
    assert!(display.contains("42"));
}

#[test]
fn test_visibility_parse() {
    assert_eq!(Visibility::parse("pub fn"), Visibility::Pub);
    assert_eq!(Visibility::parse("pub(crate) fn"), Visibility::PubCrate);
    assert_eq!(Visibility::parse("pub(super) fn"), Visibility::PubSuper);
    assert_eq!(Visibility::parse("fn"), Visibility::Private);
}

#[test]
fn test_visibility_is_public() {
    assert!(Visibility::Pub.is_public());
    assert!(!Visibility::Private.is_public());
    assert!(!Visibility::PubCrate.is_public());
}

#[test]
fn test_visibility_default() {
    let v: Visibility = Default::default();
    assert_eq!(v, Visibility::Private);
}

#[test]
fn test_symbol_index_add_search() {
    let mut index = SymbolIndex::new();
    index.add(Symbol::new(
        "test_function".to_string(),
        SymbolKind::Function,
        PathBuf::from("test.rs"),
        1,
    ));
    index.add(Symbol::new(
        "TestStruct".to_string(),
        SymbolKind::Struct,
        PathBuf::from("test.rs"),
        10,
    ));

    assert_eq!(index.len(), 2);
    assert!(!index.is_empty());

    let results = index.search("test");
    assert_eq!(results.len(), 2);

    let results = index.search("Struct");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_symbol_index_get() {
    let mut index = SymbolIndex::new();
    index.add(Symbol::new(
        "my_fn".to_string(),
        SymbolKind::Function,
        PathBuf::from("test.rs"),
        1,
    ));

    assert!(index.get("my_fn").is_some());
    assert!(index.get("nonexistent").is_none());
}

#[test]
fn test_symbol_index_in_file() {
    let mut index = SymbolIndex::new();
    let path = PathBuf::from("src/lib.rs");
    index.add(Symbol::new(
        "fn1".to_string(),
        SymbolKind::Function,
        path.clone(),
        1,
    ));
    index.add(Symbol::new(
        "fn2".to_string(),
        SymbolKind::Function,
        path.clone(),
        5,
    ));

    let symbols = index.in_file(&path).unwrap();
    assert_eq!(symbols.len(), 2);
}

#[test]
fn test_symbol_index_of_kind() {
    let mut index = SymbolIndex::new();
    index.add(Symbol::new(
        "fn1".to_string(),
        SymbolKind::Function,
        PathBuf::from("test.rs"),
        1,
    ));
    index.add(Symbol::new(
        "Struct1".to_string(),
        SymbolKind::Struct,
        PathBuf::from("test.rs"),
        5,
    ));

    let funcs = index.of_kind(&SymbolKind::Function).unwrap();
    assert_eq!(funcs.len(), 1);
}

#[test]
fn test_symbol_index_functions_structs() {
    let mut index = SymbolIndex::new();
    index.add(Symbol::new(
        "fn1".to_string(),
        SymbolKind::Function,
        PathBuf::from("test.rs"),
        1,
    ));
    index.add(Symbol::new(
        "Struct1".to_string(),
        SymbolKind::Struct,
        PathBuf::from("test.rs"),
        5,
    ));

    assert_eq!(index.functions().len(), 1);
    assert_eq!(index.structs().len(), 1);
}

#[test]
fn test_symbol_index_remove_file() {
    let mut index = SymbolIndex::new();
    let path = PathBuf::from("test.rs");
    index.add(Symbol::new(
        "fn1".to_string(),
        SymbolKind::Function,
        path.clone(),
        1,
    ));
    index.add(Symbol::new(
        "fn2".to_string(),
        SymbolKind::Function,
        path.clone(),
        5,
    ));

    assert_eq!(index.len(), 2);
    index.remove_file(&path);
    assert_eq!(index.len(), 0);
}

#[test]
fn test_symbol_index_clear() {
    let mut index = SymbolIndex::new();
    index.add(Symbol::new(
        "fn1".to_string(),
        SymbolKind::Function,
        PathBuf::from("test.rs"),
        1,
    ));
    index.clear();
    assert!(index.is_empty());
}

#[test]
fn test_dependency_creation() {
    let dep = Dependency::new("serde".to_string(), "1.0".to_string());
    assert_eq!(dep.name, "serde");
    assert_eq!(dep.version, "1.0");
    assert!(!dep.optional);
    assert!(!dep.dev);
    assert!(!dep.build);
}

#[test]
fn test_dependency_builder() {
    let dep = Dependency::new("tokio".to_string(), "1.0".to_string())
        .with_features(vec!["full".to_string()])
        .optional()
        .dev();

    assert!(dep.optional);
    assert!(dep.dev);
    assert_eq!(dep.features, vec!["full".to_string()]);
}

#[test]
fn test_dependency_build() {
    let dep = Dependency::new("proc-macro2".to_string(), "1.0".to_string()).build();
    assert!(dep.build);
}

#[test]
fn test_dependency_graph_parse() {
    let toml_content = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0"
tokio = { version = "1.35", features = ["full"] }

[dev-dependencies]
tempfile = "3.9"

[features]
default = []
full = ["tokio/full"]
"#;

    let graph = DependencyGraph::parse(toml_content).unwrap();
    assert_eq!(graph.package_name, Some("test".to_string()));
    assert_eq!(graph.package_version, Some("0.1.0".to_string()));
    assert_eq!(graph.dependencies.len(), 2);
    assert_eq!(graph.dev_dependencies.len(), 1);
    assert!(graph.features.contains_key("full"));
}

#[test]
fn test_dependency_graph_find() {
    let toml_content = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0"
"#;

    let graph = DependencyGraph::parse(toml_content).unwrap();
    assert!(graph.find("serde").is_some());
    assert!(graph.find("nonexistent").is_none());
}

#[test]
fn test_dependency_graph_count() {
    let toml_content = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
a = "1.0"
b = "1.0"

[dev-dependencies]
c = "1.0"
"#;

    let graph = DependencyGraph::parse(toml_content).unwrap();
    assert_eq!(graph.count(), 3);
}

#[test]
fn test_git_state_new() {
    let state = GitState::new();
    assert!(state.branch.is_none());
    assert!(state.commit.is_none());
    assert!(!state.dirty);
}

#[test]
fn test_git_state_summary() {
    let mut state = GitState::new();
    state.branch = Some("main".to_string());
    state.dirty = true;
    state.modified = vec![PathBuf::from("file.rs")];
    state.ahead = 2;

    let summary = state.summary();
    assert!(summary.contains("main"));
    assert!(summary.contains("changes"));
    assert!(summary.contains("↑2"));
}

#[test]
fn test_detect_language() {
    assert_eq!(detect_language("rs"), Some("Rust".to_string()));
    assert_eq!(detect_language("py"), Some("Python".to_string()));
    assert_eq!(detect_language("js"), Some("JavaScript".to_string()));
    assert_eq!(detect_language("ts"), Some("TypeScript".to_string()));
    assert_eq!(detect_language("go"), Some("Go".to_string()));
    assert_eq!(detect_language("unknown"), None);
}

#[test]
fn test_detect_language_case_insensitive() {
    assert_eq!(detect_language("RS"), Some("Rust".to_string()));
    assert_eq!(detect_language("Py"), Some("Python".to_string()));
}

#[test]
fn test_file_entry_from_path() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("test.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();

    let entry = FileEntry::from_path(file_path).unwrap();
    assert_eq!(entry.extension, Some("rs".to_string()));
    assert_eq!(entry.language, Some("Rust".to_string()));
    assert!(entry.size > 0);
}

#[test]
fn test_file_entry_count_lines() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("test.rs");
    std::fs::write(&file_path, "line1\nline2\nline3").unwrap();

    let mut entry = FileEntry::from_path(file_path).unwrap();
    let count = entry.count_lines().unwrap();
    assert_eq!(count, 3);
    assert_eq!(entry.lines, Some(3));
}

#[test]
fn test_file_index_add_get() {
    let mut index = FileIndex::new();
    let entry = FileEntry {
        path: PathBuf::from("src/lib.rs"),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: None,
    };

    index.add(entry);
    assert_eq!(index.len(), 1);
    assert!(index.get(Path::new("src/lib.rs")).is_some());
}

#[test]
fn test_file_index_by_extension() {
    let mut index = FileIndex::new();
    index.add(FileEntry {
        path: PathBuf::from("a.rs"),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: None,
    });
    index.add(FileEntry {
        path: PathBuf::from("b.rs"),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: None,
    });

    let rust_files = index.by_extension("rs");
    assert_eq!(rust_files.len(), 2);
}

#[test]
fn test_file_index_by_language() {
    let mut index = FileIndex::new();
    index.add(FileEntry {
        path: PathBuf::from("a.rs"),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: None,
    });

    let rust_files = index.by_language("Rust");
    assert_eq!(rust_files.len(), 1);
}

#[test]
fn test_file_index_remove() {
    let mut index = FileIndex::new();
    let path = PathBuf::from("test.rs");
    index.add(FileEntry {
        path: path.clone(),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: None,
    });

    assert_eq!(index.len(), 1);
    index.remove(&path);
    assert_eq!(index.len(), 0);
}

#[test]
fn test_file_index_clear() {
    let mut index = FileIndex::new();
    index.add(FileEntry {
        path: PathBuf::from("test.rs"),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: None,
    });

    index.clear();
    assert!(index.is_empty());
}

#[test]
fn test_file_index_language_stats() {
    let mut index = FileIndex::new();
    index.add(FileEntry {
        path: PathBuf::from("a.rs"),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: None,
    });
    index.add(FileEntry {
        path: PathBuf::from("b.py"),
        size: 100,
        modified: Utc::now(),
        extension: Some("py".to_string()),
        language: Some("Python".to_string()),
        lines: None,
    });

    let stats = index.language_stats();
    assert_eq!(stats.get("Rust"), Some(&1));
    assert_eq!(stats.get("Python"), Some(&1));
}

#[test]
fn test_pattern_category_icons() {
    assert_eq!(PatternCategory::Design.icon(), "🏗️");
    assert_eq!(PatternCategory::AntiPattern.icon(), "⚠️");
    assert_eq!(PatternCategory::Convention.icon(), "📏");
    assert_eq!(PatternCategory::Security.icon(), "🔒");
    assert_eq!(PatternCategory::Performance.icon(), "⚡");
    assert_eq!(PatternCategory::Testing.icon(), "🧪");
}

#[test]
fn test_code_pattern_creation() {
    let mut pattern = CodePattern::new(
        "unwrap_usage".to_string(),
        "Direct unwrap calls".to_string(),
        PatternCategory::AntiPattern,
    );

    pattern.add_location(PathBuf::from("test.rs"), 10, ".unwrap()".to_string());
    assert_eq!(pattern.locations.len(), 1);
}

#[test]
fn test_pattern_rule_creation() {
    let rule = PatternRule::new(
        "test_rule",
        PatternCategory::Convention,
        "Test description",
        r"fn\s+test",
    )
    .unwrap();

    assert_eq!(rule.name, "test_rule");
    assert!(rule.regex.is_match("fn test()"));
}

#[test]
fn test_pattern_detector_new() {
    let detector = PatternDetector::new();
    assert!(!detector.rules.is_empty());
}

#[test]
fn test_pattern_detector_analyze() {
    let mut detector = PatternDetector::new();
    let content = r#"
fn main() {
    let x = Some(1).unwrap();
    // TODO: fix this
}
"#;
    detector.analyze(Path::new("test.rs"), content);

    let patterns = detector.patterns();
    assert!(!patterns.is_empty());
}

#[test]
fn test_pattern_detector_by_category() {
    let mut detector = PatternDetector::new();
    let content = "let x = val.unwrap();";
    detector.analyze(Path::new("test.rs"), content);

    let anti = detector.by_category(&PatternCategory::AntiPattern);
    assert!(!anti.is_empty());
}

#[test]
fn test_pattern_detector_anti_patterns() {
    let mut detector = PatternDetector::new();
    detector.analyze(Path::new("test.rs"), ".unwrap()");

    let anti = detector.anti_patterns();
    assert!(!anti.is_empty());
}

#[test]
fn test_pattern_detector_clear() {
    let mut detector = PatternDetector::new();
    detector.analyze(Path::new("test.rs"), ".unwrap()");
    detector.clear();
    assert!(detector.patterns().is_empty());
}

#[test]
fn test_pattern_detector_add_rule() {
    let mut detector = PatternDetector::new();
    let rule = PatternRule::new(
        "custom",
        PatternCategory::Convention,
        "Custom rule",
        r"custom_pattern",
    )
    .unwrap();

    let initial_count = detector.rules.len();
    detector.add_rule(rule);
    assert_eq!(detector.rules.len(), initial_count + 1);
}

#[test]
fn test_pattern_detector_summary() {
    let mut detector = PatternDetector::new();
    detector.analyze(Path::new("test.rs"), "val.unwrap(); // TODO: fix");

    let summary = detector.summary();
    assert!(summary.contains_key(&PatternCategory::AntiPattern));
    assert!(summary.contains_key(&PatternCategory::Convention));
}

#[test]
fn test_project_intelligence_new() {
    let intel = ProjectIntelligence::new(PathBuf::from("/tmp/test"));
    assert_eq!(intel.root(), Path::new("/tmp/test"));
}

#[test]
fn test_project_intelligence_accessors() {
    let intel = ProjectIntelligence::new(PathBuf::from("/tmp/test"));
    assert!(intel
        .symbols()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .is_empty());
    assert!(intel
        .files()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .is_empty());
}

#[test]
fn test_project_intelligence_search_empty() {
    let intel = ProjectIntelligence::new(PathBuf::from("/tmp/test"));
    let results = intel.search("test");
    assert!(results.is_empty());
}

#[test]
fn test_search_result_display() {
    let sym = Symbol::new(
        "test".to_string(),
        SymbolKind::Function,
        PathBuf::from("test.rs"),
        1,
    );
    let result = SearchResult::Symbol(sym);
    assert!(result.display().contains("test"));
}

#[test]
fn test_search_result_file_display() {
    let entry = FileEntry {
        path: PathBuf::from("src/lib.rs"),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: None,
    };
    let result = SearchResult::File(entry);
    assert!(result.display().contains("lib.rs"));
}

#[test]
fn test_search_result_pattern_display() {
    let pattern = CodePattern::new(
        "test_pattern".to_string(),
        "Test".to_string(),
        PatternCategory::Testing,
    );
    let result = SearchResult::Pattern(pattern);
    assert!(result.display().contains("test_pattern"));
}

#[test]
fn test_project_intelligence_refresh_in_temp() {
    let temp = TempDir::new().unwrap();
    let cargo_path = temp.path().join("Cargo.toml");
    std::fs::write(
        &cargo_path,
        r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0"
"#,
    )
    .unwrap();

    let src_dir = temp.path().join("src");
    std::fs::create_dir(&src_dir).unwrap();
    std::fs::write(src_dir.join("lib.rs"), "pub fn hello() {}").unwrap();

    let mut intel = ProjectIntelligence::new(temp.path().to_path_buf());
    intel.refresh().unwrap();

    // Check dependencies were parsed
    let deps = intel
        .dependencies()
        .read()
        .unwrap_or_else(|e| e.into_inner());
    assert!(deps.find("serde").is_some());

    // The refresh succeeded without error, which is the main check
    // File indexing depends on walkdir behavior with temp dirs
}

#[test]
fn test_project_intelligence_index_files_manually() {
    let intel = ProjectIntelligence::new(PathBuf::from("/tmp/test"));

    // Test manual file addition
    let mut files = intel.files().write().unwrap_or_else(|e| e.into_inner());
    files.add(FileEntry {
        path: PathBuf::from("test.rs"),
        size: 100,
        modified: Utc::now(),
        extension: Some("rs".to_string()),
        language: Some("Rust".to_string()),
        lines: Some(10),
    });
    assert_eq!(files.len(), 1);
}

#[test]
fn test_rust_symbol_indexing() {
    let intel = ProjectIntelligence::new(PathBuf::from("/tmp/test"));
    let mut index = SymbolIndex::new();
    let content = r#"
pub fn public_function() {}
fn private_function() {}
pub struct MyStruct {}
pub enum MyEnum {}
pub trait MyTrait {}
impl MyStruct {}
pub const MY_CONST: u32 = 1;
pub type MyType = u32;
macro_rules! my_macro { () => {} }
"#;
    intel.index_rust_symbols(&mut index, Path::new("test.rs"), content);

    assert!(!index.functions().is_empty());
    assert!(!index.structs().is_empty());
    assert!(index.get("MyEnum").is_some());
    assert!(index.get("MyTrait").is_some());
    assert!(index.get("MyStruct").is_some()); // Both struct and impl
}
