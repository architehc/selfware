use super::*;
use std::collections::HashMap;
use tempfile::TempDir;

// -- Template rendering -------------------------------------------------

#[test]
fn test_render_template_basic() {
    let mut vars = HashMap::new();
    vars.insert("name".into(), "hello-world".into());
    vars.insert("desc".into(), "A test project".into());

    let result = TemplateEngine::render_template("name={{name}}, desc={{desc}}", &vars);
    assert_eq!(result, "name=hello-world, desc=A test project");
}

#[test]
fn test_render_template_missing_placeholder_kept() {
    let vars = HashMap::new();
    let result = TemplateEngine::render_template("{{unknown}}", &vars);
    assert_eq!(result, "{{unknown}}");
}

#[test]
fn test_render_template_multiple_occurrences() {
    let mut vars = HashMap::new();
    vars.insert("x".into(), "42".into());
    let result = TemplateEngine::render_template("a={{x}} b={{x}}", &vars);
    assert_eq!(result, "a=42 b=42");
}

#[test]
fn test_render_template_empty_value() {
    let mut vars = HashMap::new();
    vars.insert("project_name".into(), "".into());
    let result = TemplateEngine::render_template("name={{project_name}}", &vars);
    assert_eq!(result, "name=");
}

#[test]
fn test_render_template_no_placeholders() {
    let vars = HashMap::new();
    let result = TemplateEngine::render_template("plain text", &vars);
    assert_eq!(result, "plain text");
}

// -- Rust scaffolding ---------------------------------------------------

#[test]
fn test_scaffold_rust_project() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions {
        description: "Test Rust project".into(),
        framework: None,
        with_ci: true,
        with_tests: true,
        qa_profile: "standard".into(),
    };

    let files = engine
        .scaffold_project("rust", "my-app", dir.path(), &opts)
        .unwrap();

    // Verify Cargo.toml was created with correct name
    let cargo_path = dir.path().join("Cargo.toml");
    assert!(cargo_path.exists(), "Cargo.toml should exist");
    let cargo_content = std::fs::read_to_string(&cargo_path).unwrap();
    assert!(
        cargo_content.contains("name = \"my-app\""),
        "Cargo.toml should contain project name"
    );
    assert!(
        cargo_content.contains("Test Rust project"),
        "Cargo.toml should contain description"
    );

    // Verify src/main.rs
    assert!(dir.path().join("src/main.rs").exists());
    let main_content = std::fs::read_to_string(dir.path().join("src/main.rs")).unwrap();
    assert!(main_content.contains("my-app"));

    // Verify src/lib.rs
    assert!(dir.path().join("src/lib.rs").exists());

    // Verify tests/
    assert!(dir.path().join("tests/integration_test.rs").exists());

    // Verify CI workflow
    assert!(dir.path().join(".github/workflows/rust-qa.yml").exists());

    // All expected files present
    assert!(files.contains(&"Cargo.toml".to_string()));
    assert!(files.contains(&"src/main.rs".to_string()));
    assert!(files.contains(&"src/lib.rs".to_string()));
    assert!(files.contains(&"tests/integration_test.rs".to_string()));
    assert!(files.contains(&".github/workflows/rust-qa.yml".to_string()));
}

// -- Python scaffolding -------------------------------------------------

#[test]
fn test_scaffold_python_project() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions {
        description: "Test Python project".into(),
        framework: None,
        with_ci: true,
        with_tests: true,
        qa_profile: "standard".into(),
    };

    let files = engine
        .scaffold_project("python", "my-api", dir.path(), &opts)
        .unwrap();

    // Verify pyproject.toml
    let pyproject_path = dir.path().join("pyproject.toml");
    assert!(pyproject_path.exists(), "pyproject.toml should exist");
    let pyproject_content = std::fs::read_to_string(&pyproject_path).unwrap();
    assert!(
        pyproject_content.contains("name = \"my-api\""),
        "pyproject.toml should contain project name"
    );

    // Verify module directory
    assert!(dir.path().join("src/my_api/__init__.py").exists());
    assert!(dir.path().join("src/my_api/cli.py").exists());

    // Verify tests
    assert!(dir.path().join("tests/__init__.py").exists());
    assert!(dir.path().join("tests/test_main.py").exists());

    // Verify CI
    assert!(dir.path().join(".github/workflows/python-qa.yml").exists());

    assert!(files.contains(&"pyproject.toml".to_string()));
    assert!(files.contains(&"src/my_api/__init__.py".to_string()));
}

// -- Node.js scaffolding ------------------------------------------------

#[test]
fn test_scaffold_nodejs_project() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions {
        description: "Test Node project".into(),
        framework: None,
        with_ci: true,
        with_tests: true,
        qa_profile: "standard".into(),
    };

    let files = engine
        .scaffold_project("nodejs", "my-service", dir.path(), &opts)
        .unwrap();

    // Verify all 5 config files
    assert!(dir.path().join("package.json").exists());
    assert!(dir.path().join("tsconfig.json").exists());
    assert!(dir.path().join("eslint.config.mjs").exists());
    assert!(dir.path().join(".prettierrc").exists());
    assert!(dir.path().join("vitest.config.ts").exists());

    // Verify source and test
    assert!(dir.path().join("src/index.ts").exists());
    assert!(dir.path().join("tests/index.test.ts").exists());

    // Verify package.json has correct name
    let pkg_content = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
    assert!(pkg_content.contains("\"name\": \"my-service\""));

    // CI
    assert!(dir.path().join(".github/workflows/nodejs-qa.yml").exists());

    // Check all 5 config files are in the list
    assert!(files.contains(&"package.json".to_string()));
    assert!(files.contains(&"tsconfig.json".to_string()));
    assert!(files.contains(&"eslint.config.mjs".to_string()));
    assert!(files.contains(&".prettierrc".to_string()));
    assert!(files.contains(&"vitest.config.ts".to_string()));
}

#[test]
fn test_scaffold_nodejs_aliases() {
    // All aliases should work
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions::default();

    for alias in &["nodejs", "node", "typescript", "node.js", "ts"] {
        let sub = dir.path().join(alias);
        std::fs::create_dir_all(&sub).unwrap();
        let result = engine.scaffold_project(alias, "test", &sub, &opts);
        assert!(
            result.is_ok(),
            "alias '{}' should succeed: {:?}",
            alias,
            result.err()
        );
    }
}

// -- QA schema ----------------------------------------------------------

#[test]
fn test_load_qa_schema_embedded() {
    let config = load_qa_schema(None).unwrap();
    assert_eq!(config.qa_profile.name, "standard");
    assert!(!config.qa_profile.stages.is_empty());
    assert!(!config.qa_profile.quality_gates.is_empty());
}

#[test]
fn test_load_qa_schema_from_disk() {
    let dir = TempDir::new().unwrap();
    let schema_path = dir.path().join("qa-schema.yaml");
    std::fs::write(&schema_path, QA_SCHEMA_YAML).unwrap();

    let config = load_qa_schema_profile(Some(&schema_path), "standard").unwrap();
    assert_eq!(config.qa_profile.name, "standard");
}

#[test]
fn test_load_qa_schema_profile_standard() {
    let config = load_qa_schema_profile(None, "standard").unwrap();
    assert_eq!(config.qa_profile.name, "standard");
}

#[test]
fn test_load_qa_schema_profile_strict() {
    let config = load_qa_schema_profile(None, "strict").unwrap();
    assert_eq!(config.qa_profile.name, "strict");
}

#[test]
fn test_load_qa_schema_profile_minimal() {
    let config = load_qa_schema_profile(None, "minimal").unwrap();
    assert_eq!(config.qa_profile.name, "minimal");
}

#[test]
fn test_load_qa_schema_profile_unknown() {
    let result = load_qa_schema_profile(None, "nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_qa_schema_to_weights() {
    let config = load_qa_schema(None).unwrap();
    let weights = qa_schema_to_weights(&config);
    // syntax weight = 0.10 * 100 = 10.0
    assert!((weights.syntax - 10.0).abs() < 0.01);
    // test weight = 0.30 * 100 = 30.0
    assert!((weights.test - 30.0).abs() < 0.01);
    assert!(weights.total() > 0.0);
}

// -- Embedded template validity -----------------------------------------

#[test]
fn test_embedded_rust_cargo_toml_is_valid_toml() {
    // Render with dummy vars first so placeholders don't break parsing
    let mut vars = HashMap::new();
    vars.insert("project_name".into(), "test_project".into());
    vars.insert("module_name".into(), "test_project".into());
    vars.insert("project_description".into(), "test".into());
    vars.insert("repository_url".into(), "".into());
    vars.insert("project_url".into(), "".into());
    vars.insert("keywords".into(), "test".into());
    vars.insert("categories".into(), "test".into());

    let rendered = TemplateEngine::render_template(RUST_CARGO_TOML, &vars);
    let parsed: Result<toml::Value, _> = toml::from_str(&rendered);
    assert!(
        parsed.is_ok(),
        "Rendered Cargo.toml should be valid TOML: {:?}",
        parsed.err()
    );
}

#[test]
fn test_embedded_nodejs_package_json_is_valid_json() {
    let mut vars = HashMap::new();
    vars.insert("project_name".into(), "test-project".into());
    vars.insert("project_description".into(), "test".into());
    vars.insert("repository_url".into(), "".into());
    vars.insert("project_url".into(), "".into());
    vars.insert("keywords".into(), "test".into());

    let rendered = TemplateEngine::render_template(NODEJS_PACKAGE_JSON, &vars);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&rendered);
    assert!(
        parsed.is_ok(),
        "Rendered package.json should be valid JSON: {:?}",
        parsed.err()
    );
}

#[test]
fn test_embedded_prettierrc_is_valid_json() {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(NODEJS_PRETTIERRC);
    assert!(
        parsed.is_ok(),
        ".prettierrc should be valid JSON: {:?}",
        parsed.err()
    );
}

#[test]
fn test_embedded_tsconfig_is_parseable() {
    // tsconfig.json uses JSON-with-comments (JSONC) which TypeScript supports
    // but serde_json does not. Verify it at least contains the expected keys.
    assert!(NODEJS_TSCONFIG_JSON.contains("compilerOptions"));
    assert!(NODEJS_TSCONFIG_JSON.contains("\"strict\": true"));
    assert!(NODEJS_TSCONFIG_JSON.contains("\"outDir\""));
}

#[test]
fn test_embedded_qa_schema_is_valid_yaml() {
    // The schema is a multi-document YAML. Verify each document parses
    // individually via the iterator API.
    let mut count = 0;
    for document in serde_yaml::Deserializer::from_str(QA_SCHEMA_YAML) {
        let val = serde_yaml::Value::deserialize(document);
        assert!(
            val.is_ok(),
            "YAML document {} should parse: {:?}",
            count,
            val.err()
        );
        count += 1;
    }
    assert!(
        count >= 3,
        "Should have at least 3 YAML documents (standard, strict, minimal)"
    );
}

// -- CI workflow generation ---------------------------------------------

#[test]
fn test_ci_workflow_generation_rust() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions {
        with_ci: true,
        ..Default::default()
    };

    let files = engine
        .scaffold_project("rust", "ci-test", dir.path(), &opts)
        .unwrap();
    assert!(files.contains(&".github/workflows/rust-qa.yml".to_string()));

    let wf_content =
        std::fs::read_to_string(dir.path().join(".github/workflows/rust-qa.yml")).unwrap();
    assert!(wf_content.contains("cargo check"));
    assert!(wf_content.contains("cargo clippy"));
}

#[test]
fn test_ci_workflow_generation_python() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions {
        with_ci: true,
        ..Default::default()
    };

    let files = engine
        .scaffold_project("python", "ci-test", dir.path(), &opts)
        .unwrap();
    assert!(files.contains(&".github/workflows/python-qa.yml".to_string()));
}

#[test]
fn test_ci_workflow_generation_nodejs() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions {
        with_ci: true,
        ..Default::default()
    };

    let files = engine
        .scaffold_project("nodejs", "ci-test", dir.path(), &opts)
        .unwrap();
    assert!(files.contains(&".github/workflows/nodejs-qa.yml".to_string()));
}

#[test]
fn test_no_ci_when_disabled() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions {
        with_ci: false,
        ..Default::default()
    };

    let files = engine
        .scaffold_project("rust", "no-ci", dir.path(), &opts)
        .unwrap();
    assert!(!files.iter().any(|f| f.contains(".github")));
}

#[test]
fn test_no_tests_when_disabled() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions {
        with_tests: false,
        ..Default::default()
    };

    let files = engine
        .scaffold_project("rust", "no-tests", dir.path(), &opts)
        .unwrap();
    assert!(!files.iter().any(|f| f.contains("tests/")));
}

// -- Interview integration ----------------------------------------------

#[test]
fn test_scaffold_from_context_rust() {
    use crate::interview::{InterviewContext, ProjectType, TestingPreference};

    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join("my-rust-app");
    std::fs::create_dir_all(&project_dir).unwrap();

    let ctx = InterviewContext {
        language: Some("Rust".into()),
        framework: Some("axum".into()),
        project_type: Some(ProjectType::WebApi),
        testing_preference: Some(TestingPreference::TestsAfter),
        output_dir: None,
        scope: None,
        extra_notes: vec![],
        task: "Build a REST API".into(),
    };

    let files = scaffold_from_context(&ctx, &project_dir).unwrap();
    assert!(files.contains(&"Cargo.toml".to_string()));
    assert!(files.contains(&"src/main.rs".to_string()));
}

#[test]
fn test_scaffold_from_context_python() {
    use crate::interview::{InterviewContext, TestingPreference};

    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join("my-python-app");
    std::fs::create_dir_all(&project_dir).unwrap();

    let ctx = InterviewContext {
        language: Some("Python".into()),
        framework: None,
        project_type: None,
        testing_preference: Some(TestingPreference::Tdd),
        output_dir: None,
        scope: None,
        extra_notes: vec![],
        task: "A Python service".into(),
    };

    let files = scaffold_from_context(&ctx, &project_dir).unwrap();
    assert!(files.contains(&"pyproject.toml".to_string()));
}

#[test]
fn test_scaffold_from_context_no_tests() {
    use crate::interview::{InterviewContext, TestingPreference};

    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join("no-tests-app");
    std::fs::create_dir_all(&project_dir).unwrap();

    let ctx = InterviewContext {
        language: Some("Rust".into()),
        framework: None,
        project_type: None,
        testing_preference: Some(TestingPreference::None),
        output_dir: None,
        scope: None,
        extra_notes: vec![],
        task: "Quick script".into(),
    };

    let files = scaffold_from_context(&ctx, &project_dir).unwrap();
    assert!(!files.iter().any(|f| f.contains("tests/")));
}

#[test]
fn scaffold_from_context_writes_rust_project() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = crate::interview::InterviewContext {
        language: Some("rust".into()),
        framework: None,
        project_type: None,
        testing_preference: Some(crate::interview::TestingPreference::Tdd),
        output_dir: None,
        scope: None,
        extra_notes: vec![],
        task: "test scaffold".into(),
    };
    let files = scaffold_from_context(&ctx, dir.path()).unwrap();
    assert!(!files.is_empty());
    assert!(files.iter().any(|f| f.ends_with("Cargo.toml")));
}

// -- Overwrite refusal ---------------------------------------------------

#[test]
fn test_scaffold_refuses_to_overwrite_existing_files() {
    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join("app");
    std::fs::create_dir_all(&project_dir).unwrap();
    // Pre-existing user file that a scaffold would silently clobber.
    std::fs::write(
        project_dir.join("Cargo.toml"),
        "[package]\nname = \"mine\"\n",
    )
    .unwrap();

    let engine = TemplateEngine::new();
    let opts = ScaffoldOptions::default();
    let result = engine.scaffold_project("rust", "app", &project_dir, &opts);

    let err = result.expect_err("scaffolding over existing files must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("Cargo.toml"),
        "error must list the conflicting file(s): {}",
        msg
    );
    assert!(
        msg.contains("already exist") || msg.contains("refusing"),
        "error must explain the refusal: {}",
        msg
    );
    // Nothing else may be written: no half-scaffold left behind.
    assert!(
        !project_dir.join("src/main.rs").exists(),
        "no files should be written when the scaffold is refused"
    );
    // The existing file must be untouched.
    let content = std::fs::read_to_string(project_dir.join("Cargo.toml")).unwrap();
    assert!(content.contains("name = \"mine\""));
}

#[test]
fn test_scaffold_force_overwrites_existing_files() {
    let dir = TempDir::new().unwrap();
    let project_dir = dir.path().join("app");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        project_dir.join("Cargo.toml"),
        "[package]\nname = \"mine\"\n",
    )
    .unwrap();

    let engine = TemplateEngine::new();
    let opts = ScaffoldOptions::default();
    let files = engine
        .scaffold_project_force("rust", "app", &project_dir, &opts)
        .expect("force scaffold should succeed");
    assert!(files.contains(&"Cargo.toml".to_string()));
    // The forced scaffold actually replaced the file.
    let content = std::fs::read_to_string(project_dir.join("Cargo.toml")).unwrap();
    assert!(
        content.contains("name = \"app\""),
        "force should clobber, got: {}",
        content
    );
}

#[test]
fn test_scaffold_from_context_honors_output_dir_subdirectory() {
    use crate::interview::InterviewContext;

    let dir = TempDir::new().unwrap();
    let ctx = InterviewContext {
        language: Some("Rust".into()),
        framework: None,
        project_type: None,
        testing_preference: None,
        output_dir: Some("my-new-app".into()),
        scope: None,
        extra_notes: vec![],
        task: "test scaffold".into(),
    };

    let files = scaffold_from_context(&ctx, dir.path()).unwrap();
    assert!(files.contains(&"Cargo.toml".to_string()));
    // Files must land in the chosen subdirectory, NOT the caller's dir.
    assert!(
        dir.path().join("my-new-app/Cargo.toml").exists(),
        "scaffold should honor the interview's output-dir answer"
    );
    assert!(
        !dir.path().join("Cargo.toml").exists(),
        "nothing should be written into the caller's directory"
    );
}

#[test]
fn test_resolve_output_dir_rejects_path_traversal() {
    use crate::interview::InterviewContext;

    let base = std::path::Path::new("/tmp/scaffold-root");
    let ctx = |dir: &str| InterviewContext {
        language: None,
        framework: None,
        project_type: None,
        testing_preference: None,
        output_dir: Some(dir.to_string()),
        scope: None,
        extra_notes: vec![],
        task: String::new(),
    };

    for bad in ["../escape", "/abs/path", "a/b", "..", "x\\y"] {
        assert!(
            resolve_output_dir(&ctx(bad), base).is_err(),
            "'{}' must be rejected as an output-dir answer",
            bad
        );
    }
    // Plain names and the current-dir answers are accepted.
    assert_eq!(
        resolve_output_dir(&ctx("my-app"), base).unwrap(),
        base.join("my-app")
    );
    assert_eq!(resolve_output_dir(&ctx("."), base).unwrap(), base);
}

// -- Available templates ------------------------------------------------

#[test]
fn test_available_templates() {
    let templates = TemplateEngine::available_templates();
    assert_eq!(templates.len(), 3);
    assert!(templates.iter().any(|t| t.language == "rust"));
    assert!(templates.iter().any(|t| t.language == "python"));
    assert!(templates.iter().any(|t| t.language == "nodejs"));
}

// -- Unsupported language -----------------------------------------------

#[test]
fn test_scaffold_unsupported_language() {
    let dir = TempDir::new().unwrap();
    let engine = TemplateEngine::with_override_dir(None);
    let opts = ScaffoldOptions::default();

    let result = engine.scaffold_project("haskell", "test", dir.path(), &opts);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Unsupported language"));
}

// -- Runtime override ---------------------------------------------------

#[test]
fn test_runtime_override() {
    let dir = TempDir::new().unwrap();
    let override_dir = dir.path().join("overrides");
    let rust_dir = override_dir.join("rust");
    std::fs::create_dir_all(&rust_dir).unwrap();

    // Write a custom Cargo.toml override
    std::fs::write(
        rust_dir.join("Cargo.toml"),
        "[package]\nname = \"{{project_name}}\"\nversion = \"99.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();

    let engine = TemplateEngine::with_override_dir(Some(override_dir));
    let opts = ScaffoldOptions::default();
    let project_dir = dir.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let _files = engine
        .scaffold_project("rust", "overridden", &project_dir, &opts)
        .unwrap();

    let cargo_content = std::fs::read_to_string(project_dir.join("Cargo.toml")).unwrap();
    assert!(
        cargo_content.contains("99.0.0"),
        "Should use the overridden template"
    );
}
