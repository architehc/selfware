use super::*;

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
fn test_semantic_linker_debug() {
    let linker = SemanticLinker::new();
    let debug = format!("{:?}", linker);
    assert!(debug.contains("SemanticLinker"));
}

#[test]
fn test_semantic_linker_default() {
    let _l = SemanticLinker::default();
}
